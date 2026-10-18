use crate::{manager, state};
use anyhow::{Context, Result, ensure};
use rustix::fs::{FlockOperation, flock};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
struct Intent {
    id: String,
    run: String,
    campaign: String,
    caller: serde_json::Value,
    reason_text: Option<String>,
    phase: String,
}
fn read() -> Result<BTreeMap<String, Intent>> {
    match state::read_sidecar(&state::root().join("stops.json"))? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes).context("corrupt stop intents")?),
        None => Ok(BTreeMap::new()),
    }
}
fn event(intent: &Intent) -> Result<()> {
    manager::event(
        json!({"event":"stop","stop_id":intent.id,"phase":intent.phase,"run":intent.run,"campaign":intent.campaign,"reason":"manual","reason_text":intent.reason_text,"caller":intent.caller}),
    )
}
pub fn stop(run: &str, reason: Option<&str>) -> Result<i32> {
    ensure!(
        reason.is_none_or(|r| r.len() <= 1024 && !r.chars().any(char::is_control)),
        "reason must be at most 1024 bytes without control characters"
    );
    // Validate registration before using the unit name in a lock filename.
    ensure!(
        state::read(&state::root())?
            .holders
            .iter()
            .any(|h| h.run == run),
        "unknown run: {run}"
    );
    let lock = state::writable(&state::root().join(format!("stop-{run}.lock")))?;
    flock(&lock, FlockOperation::LockExclusive)?;
    let intent = state::transaction(|s| {
        let holder = s
            .holders
            .iter()
            .find(|h| h.run == run)
            .context("run no longer registered")?
            .clone();
        let mut intents = read()?;
        if let Some(intent) = intents.get(run)
            && intent.phase != "failed"
        {
            return Ok(intent.clone());
        }
        let intent = Intent {
            id: format!("{run}-{}-{}", std::process::id(), manager::now()),
            run: run.into(),
            campaign: holder.request.campaign.clone(),
            caller: manager::caller("cli"),
            reason_text: reason.map(str::to_owned),
            phase: "requested".into(),
        };
        // Audit must succeed before either a durable claim or a signal.
        event(&intent)?;
        intents.retain(|_, i| i.phase != "completed");
        intents.insert(run.into(), intent.clone());
        state::atomic_json(&state::root().join("stops.json"), &intents)?;
        s.holders
            .iter_mut()
            .find(|h| h.run == run)
            .unwrap()
            .stopping_at = Some(manager::now());
        Ok(intent)
    })?;
    if intent.phase == "requested" {
        submit(intent)?;
    } else {
        println!("stop_id={} phase={}", intent.id, intent.phase);
    }
    Ok(0)
}
fn submit(mut intent: Intent) -> Result<()> {
    let output = manager::ctl(&["stop", "--no-block", &intent.run]);
    let succeeded = output.as_ref().is_ok_and(|o| o.status.success());
    intent.phase = if succeeded { "submitted" } else { "failed" }.into();
    let audit_error = state::transaction(|s| {
        let mut intents = read()?;
        intents.insert(intent.run.clone(), intent.clone());
        state::atomic_json(&state::root().join("stops.json"), &intents)?;
        s.scheduling.last_pressure_stop_at = crate::admission::pressure_stop(s);
        s.last_stop = Some(state::StopRecord {
            at: manager::now(),
            run: intent.run.clone(),
            reason: "manual".into(),
        });
        if let Some(h) = s.holders.iter_mut().find(|h| h.run == intent.run) {
            h.stopping_at = succeeded.then(manager::now);
        }
        Ok(event(&intent).err())
    })?;
    if let Some(error) = audit_error {
        return Err(error).context("stop submission outcome audit append failed");
    }
    ensure!(
        succeeded,
        "could not stop {}; stop_id={} phase=failed",
        intent.run,
        intent.id
    );
    println!("stop_id={} phase={}", intent.id, intent.phase);
    Ok(())
}
// Called outside state.lock. A crash may leave a requested intent; systemd stop
// submission is repeatable, not an exactly-once filesystem/systemd transaction.
pub fn reconcile(obs: &BTreeMap<String, manager::Observation>) -> Result<()> {
    for (_, mut intent) in read()? {
        if !matches!(intent.phase.as_str(), "requested" | "submitted") {
            continue;
        }
        let lock = state::writable(&state::root().join(format!("stop-{}.lock", intent.run)))?;
        if flock(&lock, FlockOperation::NonBlockingLockExclusive).is_err() {
            continue;
        }
        let current = read()?;
        let Some(latest) = current.get(&intent.run) else {
            continue;
        };
        intent = latest.clone();
        if !matches!(intent.phase.as_str(), "requested" | "submitted") {
            continue;
        }
        if obs.get(&intent.run).is_some_and(|o| !o.live && o.started) {
            intent.phase = "completed".into();
            state::transaction(|_| {
                let mut intents = read()?;
                event(&intent)?;
                intents.insert(intent.run.clone(), intent);
                state::atomic_json(&state::root().join("stops.json"), &intents)
            })?;
        } else if intent.phase == "requested" && obs.get(&intent.run).is_some_and(|o| o.live) {
            submit(intent)?;
        }
    }
    Ok(())
}

pub fn status() -> Result<serde_json::Value> {
    Ok(serde_json::to_value(read()?)?)
}
