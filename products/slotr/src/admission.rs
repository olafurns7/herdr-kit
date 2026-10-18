use crate::{
    config::Config,
    manager::Observation,
    state::{Holder, Request, State},
    stats::Stats,
};
use std::collections::BTreeMap;

pub fn outstanding(s: &State, obs: &BTreeMap<String, Observation>) -> f64 {
    s.holders
        .iter()
        .map(|h| {
            (h.request.cost_mib as f64 - obs.get(&h.run).and_then(|o| o.anon_mib).unwrap_or(0.0))
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
    pub activity: Vec<Activity>,
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
            || (pool.memory_gated
                && other.memory_gated
                && matches!(why, None | Some("memory_budget")))
    })
    .min_by_key(|w| w.enqueue_seq)
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
            || !(a.pool == q.pool || (pool.memory_gated && other.memory_gated))
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
    if pool.memory_gated
        && s.queue.iter().any(|w| {
            w.pool != q.pool
                && cfg.pools.get(&w.pool).is_some_and(|p| p.memory_gated)
                && w.level == q.level
                && w.enqueue_seq < q.enqueue_seq
                && fresh(w, cfg, now)
                && !cap_blocked(s, w, cfg)
                && external_blocker(s, w, cfg).is_none()
                && shared_ports_clear(s, &w.pool, cfg)
                && decide(s, w, cfg, obs, sample, now, |_| true).1.is_none()
        })
    {
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
            v - outstanding(s, obs) - (q.cost_mib as f64) < cfg.admission.reserve_mib as f64
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
