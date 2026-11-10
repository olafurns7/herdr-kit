use crate::{
    config::Config,
    manager::Observation,
    state::{Holder, Request, State},
    stats::Stats,
};
use std::collections::BTreeMap;

pub const BACKFILL_LIMIT: u32 = 8;

#[derive(Clone, Debug)]
pub struct Peak {
    pub mib: f64,
    pub samples: usize,
}
pub type Peaks = BTreeMap<String, Peak>;

pub fn class(pool: &str, kind: Option<&str>, cost_mib: u64) -> String {
    let workload = kind.map_or_else(|| format!("cost:{cost_mib}"), str::to_owned);
    format!("{pool}:{workload}")
}
pub fn reservation(q: &Request, cfg: &Config, peaks: &Peaks) -> (f64, usize) {
    let peak = peaks.get(&class(&q.pool, q.kind.as_deref(), q.cost_mib));
    let samples = peak.map_or(0, |p| p.samples);
    let cost = q.cost_mib as f64;
    let reserved = match peak {
        Some(p)
            if cfg.admission.reserve_from == "peak_p95"
                && samples >= cfg.admission.peak_min_samples =>
        {
            cost.min(p.mib)
        }
        _ => cost,
    };
    (reserved, samples)
}
pub fn outstanding(s: &State, obs: &BTreeMap<String, Observation>, cfg: &Config) -> f64 {
    s.holders
        .iter()
        .map(|h| {
            (reservation(&h.request, cfg, &s.scheduling.peaks).0
                - obs.get(&h.run).and_then(|o| o.anon_mib).unwrap_or(0.0))
            .max(0.0)
        })
        .sum()
}
pub fn healthy(sample: &Stats, cfg: &Config) -> bool {
    sample
        .available_mib
        .is_some_and(|v| v >= cfg.watchdog.stop_available_mib as f64)
        && sample
            .psi_full_avg10
            .is_some_and(|v| v < cfg.watchdog.stop_psi_full_avg10_min)
        && sample
            .psi_full_avg60
            .is_some_and(|v| v <= cfg.admission.psi_full_avg60_max)
}
pub fn pressure_stop(s: &State) -> Option<f64> {
    let legacy = s
        .last_stop
        .as_ref()
        .filter(|stop| {
            matches!(
                stop.reason.as_str(),
                "emergency_available_mib" | "stop_available_mib" | "stop_psi_full_avg10"
            )
        })
        .map(|stop| stop.at);
    match (s.scheduling.last_pressure_stop_at, legacy) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
pub fn update_recovery(s: &mut State, sample: &Stats, cfg: &Config, now: f64) {
    if pressure_stop(s).is_some() {
        if !healthy(sample, cfg) {
            s.healthy_since = None;
        } else if s.healthy_since.is_none() {
            s.healthy_since = Some(now);
        }
    }
}
pub fn recovery(s: &State, sample: &Stats, cfg: &Config, now: f64) -> bool {
    pressure_stop(s).is_some_and(|at| {
        !healthy(sample, cfg)
            || s.healthy_since
                .is_none_or(|t| now - t < cfg.admission.recovery_healthy_seconds)
            || now - at < cfg.admission.recovery_healthy_seconds
    })
}
pub fn campaign_held(s: &State, pool: &str, campaign: &str) -> usize {
    s.holders
        .iter()
        .filter(|h| h.request.pool == pool && h.request.campaign == campaign)
        .count()
}
pub fn fresh(q: &Request, cfg: &Config, now: f64) -> bool {
    now - q.seen_at <= 5.0 + 3.0 * cfg.admission.queue_poll_ms as f64 / 1000.0
}
pub fn priority_cap(pool: &crate::config::Pool) -> usize {
    (if pool.campaign_cap == 0 {
        pool.slots
    } else {
        pool.slots.min(pool.campaign_cap)
    }) as usize
}
pub fn cap_blocked(s: &State, q: &Request, cfg: &Config) -> bool {
    let Some(pool) = cfg.pools.get(&q.pool) else {
        return true;
    };
    let held = campaign_held(s, &q.pool, &q.campaign);
    if q.level > 0 {
        return held >= priority_cap(pool);
    }
    pool.campaign_cap > 0
        && held >= pool.campaign_cap as usize
        && s.queue.iter().any(|w| {
            w.pool == q.pool
                && w.campaign != q.campaign
                && s.scheduling.now.is_none_or(|now| fresh(w, cfg, now))
                && campaign_held(s, &w.pool, &w.campaign) < pool.campaign_cap as usize
        })
}
pub fn head<'a>(s: &'a State, pool: &str, cfg: &Config) -> Option<&'a Request> {
    s.queue
        .iter()
        .filter(|q| {
            q.pool == pool
                && !cap_blocked(s, q, cfg)
                && s.scheduling.now.is_none_or(|now| fresh(q, cfg, now))
        })
        .min_by_key(|q| (std::cmp::Reverse(q.level), q.enqueue_seq))
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Scheduling {
    #[serde(skip)]
    pub peaks: Peaks,
    pub activity: Vec<Activity>,
    #[serde(default)]
    pub backfill_admissions: BTreeMap<u64, u32>,
    #[serde(default)]
    pub last_pressure_stop_at: Option<f64>,
    #[serde(skip)]
    pub now: Option<f64>,
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Activity {
    pub pool: String,
    pub campaign: String,
    pub weight: u32,
    #[serde(default)]
    pub task: String,
    pub quiet_since: Option<f64>,
    pub demanding: bool,
}
// The caller holds state.lock; the sidecar survives older schema-1 writers.
pub fn update_activity(s: &mut State, cfg: &Config, now: f64) {
    s.scheduling.now = Some(now);
    // Keep the bound across polls, staleness and changes of the active fair turn.
    s.scheduling
        .backfill_admissions
        .retain(|seq, _| s.queue.iter().any(|q| q.enqueue_seq == *seq));
    for q in &s.queue {
        if cfg.pools.contains_key(&q.pool)
            && fresh(q, cfg, now)
            && !s
                .scheduling
                .activity
                .iter()
                .any(|a| a.pool == q.pool && a.campaign == q.campaign)
        {
            s.scheduling.activity.push(Activity {
                pool: q.pool.clone(),
                campaign: q.campaign.clone(),
                weight: q.level,
                task: q.task.clone(),
                quiet_since: None,
                demanding: false,
            });
        }
    }
    let mut activity = std::mem::take(&mut s.scheduling.activity);
    for a in &mut activity {
        if !cfg.pools.contains_key(&a.pool) {
            continue;
        }
        let requests: Vec<_> = s
            .queue
            .iter()
            .filter(|q| q.pool == a.pool && q.campaign == a.campaign && fresh(q, cfg, now))
            .collect();
        if let Some(q) = requests.iter().max_by_key(|q| q.level) {
            a.weight = q.level;
            a.task = q.task.clone();
        }
        let demanding = requests
            .iter()
            .any(|q| external_blocker(s, q, cfg).is_none());
        if demanding || !requests.is_empty() {
            a.quiet_since = None;
        } else if a.demanding {
            a.quiet_since = Some(now);
        }
        a.demanding = demanding;
    }
    s.scheduling.activity = activity;
    s.scheduling.activity.retain(|a| {
        a.demanding
            || a.quiet_since
                .is_some_and(|at| now - at < cfg.priority.quiet_seconds)
            || s.queue
                .iter()
                .any(|q| q.pool == a.pool && q.campaign == a.campaign)
            || s.holders
                .iter()
                .any(|h| h.request.pool == a.pool && h.request.campaign == a.campaign)
    });
}
pub fn external_blocker(s: &State, q: &Request, cfg: &Config) -> Option<&'static str> {
    let pool = &cfg.pools[&q.pool];
    if !crate::manager::legacy_free(pool) {
        return Some("legacy_lock");
    }
    if let Some(p) = &pool.ports {
        let has_slot = (0..pool.slots).any(|slot| {
            let base = p.base + slot * p.stride;
            // A port in this slot's registered holder is part of slot shortage.
            s.holders
                .iter()
                .any(|h| h.request.pool == q.pool && h.slot == slot && h.port_base == Some(base))
                || (!s.holders.iter().any(|h| {
                    h.port_base
                        .zip(h.port_end)
                        .is_some_and(|(b, e)| base <= e && base + p.stride > b)
                }) && crate::manager::ports_free(base, p.probe))
        });
        if !has_slot {
            return Some("ports_busy");
        }
    }
    None
}
pub fn shared_ports_clear(s: &State, pool_name: &str, cfg: &Config) -> bool {
    let pool = &cfg.pools[pool_name];
    pool.ports.as_ref().is_none_or(|p| {
        (0..pool.slots).any(|slot| {
            let base = p.base + slot * p.stride;
            !s.holders.iter().any(|h| {
                h.request.pool != pool_name
                    && h.port_base
                        .zip(h.port_end)
                        .is_some_and(|(b, e)| base <= e && base + p.stride > b)
            }) && crate::manager::ports_free(base, p.probe)
        })
    })
}
pub fn fair(q: &Request, cfg: &Config, now: f64) -> bool {
    fresh(q, cfg, now) && now - q.since >= cfg.priority.fair_wait_seconds
}
pub fn ordered<'a>(s: &'a State, pool: &str, cfg: &Config, now: f64) -> Vec<&'a Request> {
    let mut queue: Vec<_> = s.queue.iter().filter(|q| q.pool == pool).collect();
    queue.sort_by_key(|q| {
        (
            !fresh(q, cfg, now),
            cap_blocked(s, q, cfg),
            !(fair(q, cfg, now) && external_blocker(s, q, cfg).is_none()),
            if fair(q, cfg, now) && external_blocker(s, q, cfg).is_none() {
                0
            } else {
                u32::MAX - q.level
            },
            q.enqueue_seq,
        )
    });
    queue
}
/// Memory-gated pools with global fair turns share fair turns and priority waits.
fn shares_turns(a: &crate::config::Pool, b: &crate::config::Pool) -> bool {
    a.memory_gated && b.memory_gated && a.fair_turn == "global" && b.fair_turn == "global"
}
pub fn fair_turn<'a>(
    s: &'a State,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> Option<&'a Request> {
    let pool = &cfg.pools[&q.pool];
    let aged = s.queue.iter().filter(|w| {
        cfg.pools.contains_key(&w.pool)
            && fresh(w, cfg, now)
            && fair(w, cfg, now)
            && !cap_blocked(s, w, cfg)
            && external_blocker(s, w, cfg).is_none()
            && (w.pool == q.pool || shared_ports_clear(s, &w.pool, cfg))
    });
    aged.filter(|w| {
        let other = &cfg.pools[&w.pool];
        let why = decide(s, w, cfg, obs, sample, now, |_| true).1;
        (w.pool == q.pool && matches!(why, None | Some("no_slot") | Some("memory_budget")))
            || (shares_turns(pool, other) && matches!(why, None | Some("memory_budget")))
    })
    .min_by_key(|w| w.enqueue_seq)
}
fn backfill_turn<'a>(
    s: &'a State,
    first: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> Option<&'a Request> {
    if backfill_count(s, first.enqueue_seq) >= BACKFILL_LIMIT
        || decide(s, first, cfg, obs, sample, now, |_| true).1 != Some("memory_budget")
    {
        return None;
    }
    ordered(s, &first.pool, cfg, now).into_iter().find(|w| {
        fresh(w, cfg, now)
            && w.cost_mib < first.cost_mib
            && external_blocker(s, w, cfg).is_none()
            && decide(s, w, cfg, obs, sample, now, |slot| {
                cfg.pools[&w.pool]
                    .ports
                    .as_ref()
                    .is_none_or(|p| crate::manager::ports_free(p.base + slot * p.stride, p.probe))
            })
            .1
            .is_none()
    })
}
pub fn backfill_count(s: &State, seq: u64) -> u32 {
    s.scheduling
        .backfill_admissions
        .get(&seq)
        .copied()
        .unwrap_or(0)
}
pub fn backfill_for(
    s: &State,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> Option<u64> {
    let first = fair_turn(s, q, cfg, obs, sample, now)?;
    (first.pool == q.pool
        && backfill_turn(s, first, cfg, obs, sample, now)
            .is_some_and(|w| w.enqueue_seq == q.enqueue_seq))
    .then_some(first.enqueue_seq)
}
pub fn evaluate(
    s: &State,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> (Option<u32>, Option<&'static str>) {
    let pool = &cfg.pools[&q.pool];
    let (slot, reason) = decide(s, q, cfg, obs, sample, now, |slot| {
        pool.ports
            .as_ref()
            .is_none_or(|p| crate::manager::ports_free(p.base + slot * p.stride, p.probe))
    });
    if !fresh(q, cfg, now) {
        return (None, Some("stale_ticket"));
    }
    if let Some(blocker) = external_blocker(s, q, cfg) {
        return (None, Some(blocker));
    }
    if reason.is_some() {
        return (None, reason);
    }
    let reason = preference(s, q, cfg, obs, sample, now);
    (slot.filter(|_| reason.is_none()), reason)
}
pub fn preference(
    s: &State,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> Option<&'static str> {
    let pool = &cfg.pools[&q.pool];
    let reserving = fair_turn(s, q, cfg, obs, sample, now);
    if let Some(first) = reserving {
        if first.enqueue_seq != q.enqueue_seq {
            if first.pool == q.pool
                && decide(s, first, cfg, obs, sample, now, |_| true).1 == Some("memory_budget")
            {
                if backfill_count(s, first.enqueue_seq) >= BACKFILL_LIMIT {
                    return Some("fair_turn_backfill_limit");
                }
                if backfill_turn(s, first, cfg, obs, sample, now)
                    .is_some_and(|w| w.enqueue_seq == q.enqueue_seq)
                {
                    return None;
                }
            }
            return Some("fair_turn");
        }
        return None;
    }
    if ordered(s, &q.pool, cfg, now)
        .first()
        .is_none_or(|w| w.enqueue_seq != q.enqueue_seq)
    {
        return Some("fifo");
    }
    for a in &s.scheduling.activity {
        let Some(other) = cfg.pools.get(&a.pool) else {
            continue;
        };
        if a.weight <= q.level
            || campaign_held(s, &a.pool, &a.campaign) >= priority_cap(other)
            || !(a.pool == q.pool || shares_turns(pool, other))
        {
            continue;
        }
        // An externally blocked pool cannot reserve another pool's capacity.
        if a.pool != q.pool
            && (!crate::manager::legacy_free(other) || !shared_ports_clear(s, &a.pool, cfg))
        {
            continue;
        }
        let demand = s.queue.iter().any(|w| {
            w.pool == a.pool
                && w.campaign == a.campaign
                && fresh(w, cfg, now)
                && !cap_blocked(s, w, cfg)
                && external_blocker(s, w, cfg).is_none()
        });
        if demand
            || a.quiet_since
                .is_some_and(|at| now - at < cfg.priority.quiet_seconds)
        {
            return Some("priority_wait");
        }
    }
    // Equal-weight memory contenders also keep their global enqueue order.
    if s.queue.iter().any(|w| {
        w.pool != q.pool
            && cfg
                .pools
                .get(&w.pool)
                .is_some_and(|p| shares_turns(pool, p))
            && w.level == q.level
            && w.enqueue_seq < q.enqueue_seq
            && fresh(w, cfg, now)
            && !cap_blocked(s, w, cfg)
            && external_blocker(s, w, cfg).is_none()
            && shared_ports_clear(s, &w.pool, cfg)
            && decide(s, w, cfg, obs, sample, now, |_| true).1.is_none()
    }) {
        return Some("priority_wait");
    }
    None
}
pub fn decide(
    s: &State,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
    ports_free: impl Fn(u32) -> bool,
) -> (Option<u32>, Option<&'static str>) {
    let pool = &cfg.pools[&q.pool];
    if cap_blocked(s, q, cfg) {
        return (None, Some("campaign_cap"));
    }
    let slots: Vec<_> = (0..pool.slots)
        .filter(|slot| {
            !s.holders
                .iter()
                .any(|h| h.request.pool == q.pool && h.slot == *slot)
        })
        .collect();
    if slots.is_empty() {
        return (None, Some("no_slot"));
    }
    if pool.memory_gated {
        if recovery(s, sample, cfg, now) {
            return (None, Some("recovery"));
        }
        if sample.available_mib.is_none_or(|v| {
            v - outstanding(s, obs, cfg) - (q.cost_mib as f64) < cfg.admission.reserve_mib as f64
        }) {
            return (None, Some("memory_budget"));
        }
        if sample
            .psi_full_avg60
            .is_none_or(|v| v > cfg.admission.psi_full_avg60_max)
        {
            return (None, Some("psi"));
        }
        if cfg.admission.load1_per_core_max > 0.0
            && sample
                .load1
                .is_none_or(|v| v / sample.cores as f64 > cfg.admission.load1_per_core_max)
        {
            return (None, Some("load"));
        }
    }
    for slot in slots {
        if let Some(p) = &pool.ports {
            let base = p.base + slot * p.stride;
            let end = base + p.stride - 1;
            if s.holders.iter().any(|h| {
                h.port_base
                    .zip(h.port_end)
                    .is_some_and(|(b, e)| base <= e && end >= b)
            }) {
                continue;
            }
            if !ports_free(slot) {
                continue;
            }
        }
        return (Some(slot), None);
    }
    (None, Some("ports_busy"))
}
pub fn overdue(h: &Holder, now: f64) -> bool {
    h.lease_expires_at.is_some_and(|t| now >= t)
}
pub fn holder_state(h: &Holder, _s: &State, _cfg: &Config, now: f64) -> &'static str {
    if h.stopping_at.is_some() {
        "stopping"
    } else if h.warned_at.is_some() {
        "warned"
    } else if overdue(h, now) {
        "overdue"
    } else {
        "running"
    }
}
pub fn fits_without(
    s: &State,
    h: &Holder,
    q: &Request,
    cfg: &Config,
    obs: &BTreeMap<String, Observation>,
    sample: &Stats,
    now: f64,
) -> bool {
    let mut without = s.clone();
    without.holders.retain(|other| other.run != h.run);
    // Releasing a holder may uncap an older ticket. Do not evict it for a
    // waiter that would immediately lose its effective-head position.
    if head(&without, &q.pool, cfg).is_none_or(|first| first.enqueue_seq != q.enqueue_seq) {
        return false;
    }
    let mut sample = sample.clone();
    if let Some(available) = &mut sample.available_mib {
        *available += obs.get(&h.run).and_then(|o| o.anon_mib).unwrap_or(0.0);
    }
    let legacy_ok = cfg.pools[&q.pool].legacy_lock.as_ref().is_none_or(|l| {
        l.mode == "shared" || !without.holders.iter().any(|h| h.request.pool == q.pool)
    });
    legacy_ok
        && decide(&without, q, cfg, obs, &sample, now, |slot| {
            slot == h.slot
                || cfg.pools[&q.pool]
                    .ports
                    .as_ref()
                    .is_none_or(|p| crate::manager::ports_free(p.base + slot * p.stride, p.probe))
        })
        .1
        .is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn request(seq: u64, pool: &str, weight: u32, since: f64) -> Request {
        serde_json::from_value(json!({"enqueue_seq":seq,"pool":pool,"campaign":format!("c{seq}"),"level":weight,"purpose":"synthetic","cwd":"/tmp","cost_mib":1000,"lease_seconds":0,"since":since,"seen_at":1000})).unwrap()
    }
    fn config() -> Config {
        let mut cfg: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
        let pool = cfg.pools.get_mut("default").unwrap();
        pool.slots = 2;
        pool.campaign_cap = 0;
        pool.memory_gated = false;
        cfg
    }
    fn history(events: Vec<serde_json::Value>) -> Peaks {
        let bytes = events
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        crate::manager::read_peaks(&mut std::io::Cursor::new(bytes), 1000.0, 48).unwrap()
    }
    #[test]
    fn stopped_runs_do_not_lower_peaks_but_fast_payload_failures_count() {
        let cfg = config();
        let mut q = request(1, "default", 0, 1000.0);
        q.kind = Some("build".into());
        let mut events = vec![
            json!({"at":1000,"event":"end","pool":"default","kind":"build","memory_peak_bytes":100 * 1048576_u64,"stopped":true});
            100
        ];
        events.extend((0..20).map(|_| {
            json!({"at":1000,"event":"end","pool":"default","kind":"build","memory_peak_bytes":800 * 1048576_u64,"exit_code":1,"stopped":false})
        }));
        assert_eq!(reservation(&q, &cfg, &history(events)), (800.0, 20));
    }
    #[test]
    fn named_peak_classes_follow_the_recorded_pool() {
        let cfg = config();
        let mut q = request(1, "heavy", 0, 1000.0);
        q.kind = Some("build".into());
        let mut events = Vec::new();
        for (pool, peak, count) in [("light", 100, 100), ("heavy", 800, 20)] {
            events.extend((0..count).map(|_| {
                json!({"at":1000,"event":"end","pool":pool,"kind":"build","memory_peak_bytes":peak * 1048576_u64})
            }));
        }
        let peaks = history(events);
        assert_eq!(reservation(&q, &cfg, &peaks), (800.0, 20));
        q.pool = "light".into();
        assert_eq!(reservation(&q, &cfg, &peaks), (100.0, 100));
        q.pool = "redirected".into();
        assert_eq!(reservation(&q, &cfg, &peaks), (1000.0, 0));
    }
    #[test]
    fn unkinded_peak_classes_include_pool_and_declared_cost() {
        let cfg = config();
        let q = request(1, "heavy", 0, 1000.0);
        let mut events = Vec::new();
        for (pool, cost, peak, count) in [
            ("light", 1000, 100, 100),
            ("heavy", 1000, 800, 20),
            ("heavy", 2000, 200, 100),
        ] {
            events.extend((0..count).map(|_| {
                json!({"at":1000,"event":"end","pool":pool,"cost_mib":cost,"memory_peak_bytes":peak * 1048576_u64})
            }));
        }
        assert_eq!(reservation(&q, &cfg, &history(events)), (800.0, 20));
    }
    #[test]
    fn more_than_five_percent_ooms_reserve_the_current_cost() {
        let cfg = config();
        let mut q = request(1, "default", 0, 1000.0);
        q.kind = Some("build".into());
        q.cost_mib = 2000;
        for (ooms, expected) in [(1, 100.0), (2, 2000.0)] {
            let events = (0..20)
                .map(|i| {
                    json!({"at":1000,"event":"end","pool":"default","kind":"build","memory_peak_bytes":100 * 1048576_u64,"oom_killed":i < ooms})
                })
                .collect();
            assert_eq!(reservation(&q, &cfg, &history(events)), (expected, 20));
        }
    }
    #[test]
    fn measured_peaks_release_snapshot_reservations_without_lowering_candidate_cost() {
        let mut cfg = config();
        cfg.admission.reserve_mib = 6144;
        cfg.pools.get_mut("default").unwrap().memory_gated = true;
        cfg.pools.get_mut("default").unwrap().slots = 5;
        let mut s = State::default();
        let mut obs = BTreeMap::new();
        let mut events = String::new();
        for (kind, cost, peak) in [
            (Some("vitest"), 4096, 2867),
            (Some("build"), 9216, 5734),
            (None, 16384, 8499),
        ] {
            // 19 ordinary peaks and one outlier: nearest-rank p95 selects peak.
            for sample in 0..20 {
                let peak = if sample == 19 { peak * 2 } else { peak };
                events.push_str(&json!({"at":1000,"event":"end","pool":"default","kind":kind,"cost_mib":cost,"memory_peak_bytes":peak as u64 * 1048576}).to_string());
                events.push('\n');
            }
        }
        s.scheduling.peaks =
            crate::manager::read_peaks(&mut std::io::Cursor::new(events), 1000.0, 48).unwrap();
        for (i, (kind, cost, anon)) in [
            (Some("vitest"), 4096, 584),
            (Some("vitest"), 4096, 1608),
            (Some("build"), 9216, 727),
            (None, 16384, 3973),
        ]
        .into_iter()
        .enumerate()
        {
            let run = format!("slotr-default-{i}");
            s.holders.push(serde_json::from_value(json!({"enqueue_seq":i,"pool":"default","campaign":"holder","purpose":"synthetic","cwd":"/tmp","cost_mib":cost,"kind":kind,"lease_seconds":0,"since":0,"seen_at":1000,"run":run,"admit_seq":i,"slot":i,"admitted_at":0})).unwrap());
            obs.insert(
                run,
                Observation {
                    live: true,
                    started: true,
                    anon_mib: Some(anon as f64),
                    cpu_usec: None,
                },
            );
        }
        assert_eq!(outstanding(&s, &obs, &cfg), 13075.0);
        assert_eq!(35533.0 - outstanding(&s, &obs, &cfg) - 6144.0, 16314.0);
        let mut candidate = request(99, "default", 0, 1000.0);
        candidate.kind = Some("vitest".into());
        candidate.cost_mib = 16315;
        let sample = Stats {
            available_mib: Some(35533.0),
            psi_full_avg60: Some(0.0),
            ..Stats::default()
        };
        assert_eq!(
            decide(&s, &candidate, &cfg, &obs, &sample, 1000.0, |_| true).1,
            Some("memory_budget")
        );
        candidate.cost_mib = 16314;
        assert_eq!(
            decide(&s, &candidate, &cfg, &obs, &sample, 1000.0, |_| true).1,
            None
        );
        cfg.admission.reserve_from = "cost".into();
        assert_eq!(outstanding(&s, &obs, &cfg), 26900.0);
        assert_eq!(35533.0 - outstanding(&s, &obs, &cfg) - 6144.0, 2489.0);
        cfg.admission.reserve_from = "peak_p95".into();
        // With no readable history, the empty initial cache uses declared costs.
        s.scheduling.peaks.clear();
        assert_eq!(outstanding(&s, &obs, &cfg), 26900.0);
    }
    #[test]
    fn sparse_samples_fall_back_and_reservations_clamp_to_cost() {
        let cfg = config();
        let q = request(1, "default", 0, 1000.0);
        let mut peaks = BTreeMap::from([(
            class(&q.pool, None, q.cost_mib),
            Peak {
                mib: 100.0,
                samples: 19,
            },
        )]);
        assert_eq!(reservation(&q, &cfg, &peaks), (1000.0, 19));
        peaks.get_mut("default:cost:1000").unwrap().samples = 20;
        assert_eq!(reservation(&q, &cfg, &peaks), (100.0, 20));
        peaks.get_mut("default:cost:1000").unwrap().mib = 2000.0;
        assert_eq!(reservation(&q, &cfg, &peaks), (1000.0, 20));
    }
    fn backfill_fixture() -> (Config, State, Stats) {
        let mut cfg = config();
        let pool = cfg.pools.get_mut("default").unwrap();
        pool.memory_gated = true;
        pool.slots = 10;
        cfg.admission.reserve_mib = 6 * 1024;
        let mut first = request(1, "default", 0, 0.0);
        first.cost_mib = 20 * 1024;
        let mut small = request(2, "default", 0, 999.0);
        small.cost_mib = 512;
        let s = State {
            queue: vec![first, small],
            ..State::default()
        };
        let sample = Stats {
            available_mib: Some(20.5 * 1024.0),
            psi_full_avg10: Some(0.0),
            psi_full_avg60: Some(0.0),
            ..Stats::default()
        };
        (cfg, s, sample)
    }
    #[test]
    fn pool_scoped_fair_turn_ignores_other_pools_memory_blocked_turn() {
        let (mut cfg, mut s, sample) = backfill_fixture();
        let mut light = cfg.pools["default"].clone();
        light.slots = 4;
        cfg.pools.insert("light".into(), light);
        s.queue[1].pool = "light".into();
        let obs = BTreeMap::new();
        // A global light pool waits behind the 20 GiB fair turn, with no backfill.
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0),
            (None, Some("fair_turn"))
        );
        cfg.pools.get_mut("light").unwrap().fair_turn = "pool".into();
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0),
            (Some(0), None)
        );
        // An aged, memory-blocked light waiter does not reserve the other pool either.
        s.queue[0].pool = "light".into();
        s.queue[1].pool = "default".into();
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0),
            (Some(0), None)
        );
        // Memory admission still applies inside the scoped pool.
        s.queue[1].pool = "light".into();
        s.queue[1].cost_mib = 15 * 1024;
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0).1,
            Some("memory_budget")
        );
    }
    #[test]
    fn memory_blocked_fair_turn_allows_fitting_512_mib_backfill() {
        let (cfg, s, sample) = backfill_fixture();
        let obs = BTreeMap::new();
        assert_eq!(
            evaluate(&s, &s.queue[0], &cfg, &obs, &sample, 1000.0).1,
            Some("memory_budget")
        );
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0),
            (Some(0), None)
        );
    }
    #[test]
    fn fitting_fair_turn_precedes_small_backfill() {
        let (cfg, s, mut sample) = backfill_fixture();
        sample.available_mib = Some(30.0 * 1024.0);
        let obs = BTreeMap::new();
        assert_eq!(
            evaluate(&s, &s.queue[0], &cfg, &obs, &sample, 1000.0),
            (Some(0), None)
        );
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0).1,
            Some("fair_turn")
        );
    }
    #[test]
    fn backfill_preserves_order_caps_and_memory_safety() {
        let (mut cfg, mut s, mut sample) = backfill_fixture();
        let mut high = s.queue[1].clone();
        high.enqueue_seq = 3;
        high.level = 5;
        s.queue.push(high);
        let obs = BTreeMap::new();
        assert_eq!(
            backfill_for(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0),
            Some(1)
        );
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &obs, &sample, 1000.0).1,
            Some("fair_turn")
        );
        // Positive-weight campaigns retain their hard cap during backfill.
        cfg.pools.get_mut("default").unwrap().campaign_cap = 1;
        let holder: Holder = serde_json::from_value(json!({"enqueue_seq":99,"pool":"default","campaign":"c2","purpose":"synthetic","cwd":"/tmp","cost_mib":512,"lease_seconds":0,"since":0,"seen_at":1000,"run":"synthetic","admit_seq":1,"slot":0,"admitted_at":0})).unwrap();
        s.holders.push(holder);
        assert_eq!(
            evaluate(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0).1,
            Some("campaign_cap")
        );
        s.holders[0].request.campaign = "holder".into();
        s.holders[0].request.cost_mib = 15000;
        assert_eq!(
            evaluate(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0).1,
            Some("memory_budget")
        );
        s.holders.clear();
        // Missing data, reserve, PSI and declared outstanding costs still gate.
        sample.available_mib = None;
        assert_eq!(
            evaluate(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0).1,
            Some("memory_budget")
        );
        sample.available_mib = Some((cfg.admission.reserve_mib + 511) as f64);
        assert_eq!(
            evaluate(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0).1,
            Some("memory_budget")
        );
        sample.available_mib = Some(20.5 * 1024.0);
        sample.psi_full_avg60 = Some(cfg.admission.psi_full_avg60_max + 1.0);
        assert_eq!(
            evaluate(&s, &s.queue[2], &cfg, &obs, &sample, 1000.0).1,
            Some("psi")
        );
    }
    #[test]
    fn backfill_limit_survives_serialization_staleness_and_turn_changes() {
        let (cfg, mut s, sample) = backfill_fixture();
        s.scheduling.backfill_admissions.insert(1, BACKFILL_LIMIT);
        s.scheduling = serde_json::from_slice(&serde_json::to_vec(&s.scheduling).unwrap()).unwrap();
        update_activity(&mut s, &cfg, 1010.0);
        assert_eq!(backfill_count(&s, 1), BACKFILL_LIMIT);
        for q in &mut s.queue {
            q.seen_at = 1010.0;
        }
        let mut older = s.queue[0].clone();
        older.enqueue_seq = 0;
        s.queue.push(older);
        update_activity(&mut s, &cfg, 1010.0);
        assert_eq!(
            fair_turn(&s, &s.queue[1], &cfg, &BTreeMap::new(), &sample, 1010.0)
                .unwrap()
                .enqueue_seq,
            0
        );
        s.queue.pop();
        update_activity(&mut s, &cfg, 1010.0);
        assert_eq!(
            evaluate(&s, &s.queue[1], &cfg, &BTreeMap::new(), &sample, 1010.0).1,
            Some("fair_turn_backfill_limit")
        );
        s.queue.remove(0);
        update_activity(&mut s, &cfg, 1010.0);
        assert_eq!(backfill_count(&s, 1), 0);
    }
    #[test]
    fn manual_and_lifecycle_stops_do_not_restart_pressure_recovery() {
        let cfg = config();
        let sample = Stats {
            available_mib: Some(60000.0),
            psi_full_avg10: Some(0.0),
            psi_full_avg60: Some(0.0),
            ..Stats::default()
        };
        let mut s = State {
            last_stop: Some(crate::state::StopRecord {
                at: 200.0,
                run: "synthetic".into(),
                reason: "manual".into(),
            }),
            ..State::default()
        };
        assert!(!recovery(&s, &sample, &cfg, 200.0));
        s.scheduling.last_pressure_stop_at = Some(100.0);
        s.healthy_since = Some(100.0);
        s.last_stop.as_mut().unwrap().reason = "lease_expired".into();
        assert!(!recovery(&s, &sample, &cfg, 200.0));
        s.last_stop.as_mut().unwrap().reason = "stop_available_mib".into();
        assert!(recovery(&s, &sample, &cfg, 200.0));
    }
    #[test]
    fn twenty_aged_gates_take_successive_turns_before_new_high_demand() {
        let cfg = config();
        let mut s = State {
            queue: (1..=20)
                .map(|seq| request(seq, "default", 0, 0.0))
                .chain([request(21, "default", 1, 999.0)])
                .collect(),
            ..State::default()
        };
        update_activity(&mut s, &cfg, 1000.0);
        for seq in 1..=20 {
            assert_eq!(ordered(&s, "default", &cfg, 1000.0)[0].enqueue_seq, seq);
            assert_eq!(
                evaluate(
                    &s,
                    s.queue.last().unwrap(),
                    &cfg,
                    &BTreeMap::new(),
                    &Stats::default(),
                    1000.0
                )
                .1,
                Some("fair_turn")
            );
            assert!(
                evaluate(
                    &s,
                    &s.queue[0],
                    &cfg,
                    &BTreeMap::new(),
                    &Stats::default(),
                    1000.0
                )
                .0
                .is_some()
            );
            s.queue.remove(0);
        }
        assert!(
            evaluate(
                &s,
                &s.queue[0],
                &cfg,
                &BTreeMap::new(),
                &Stats::default(),
                1000.0
            )
            .0
            .is_some()
        );
    }
    #[test]
    fn slot_shortage_reserves_only_its_pool_and_stale_tickets_keep_sequence() {
        let mut cfg = config();
        cfg.pools
            .insert("other".into(), cfg.pools["default"].clone());
        let holder: Holder = serde_json::from_value(json!({"enqueue_seq":99,"pool":"default","campaign":"holder","purpose":"synthetic","cwd":"/tmp","cost_mib":1000,"lease_seconds":0,"since":0,"seen_at":1000,"run":"slotr-default-1","admit_seq":1,"slot":0,"admitted_at":0})).unwrap();
        cfg.pools.get_mut("default").unwrap().slots = 1;
        let mut s = State {
            holders: vec![holder],
            queue: vec![request(1, "default", 0, 0.0), request(2, "other", 0, 999.0)],
            ..State::default()
        };
        update_activity(&mut s, &cfg, 1000.0);
        assert!(
            evaluate(
                &s,
                &s.queue[1],
                &cfg,
                &BTreeMap::new(),
                &Stats::default(),
                1000.0
            )
            .0
            .is_some()
        );
        s.queue[0].level = 5;
        update_activity(&mut s, &cfg, 1010.0);
        assert!(head(&s, "default", &cfg).is_none());
        s.queue[0].seen_at = 1010.0;
        update_activity(&mut s, &cfg, 1010.0);
        assert_eq!(head(&s, "default", &cfg).unwrap().enqueue_seq, 1);
    }
}
