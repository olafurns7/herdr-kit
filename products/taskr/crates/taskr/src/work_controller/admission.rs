//! The approved payload runs INSIDE slotr's unit. The outer wrapper is not cleanup proof.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{io::Read, sync::atomic::AtomicBool, time::Instant};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Receipt {
    work: i64,
    root: i64,
    launch: i64,
    task: i64,
    run: i64,
    revision: i64,
    check: String,
    profile: String,
    unit: String,
    invocation: String,
    pool: String,
    slot: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Finished {
    admission: Receipt,
    outcome: runner::Outcome,
}
fn bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(failure)?
        .take((api::MAX_JSON + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > api::MAX_JSON {
        return Err(failure("runner receipt exceeds limit"));
    }
    serde_json::from_slice(&bytes).map_err(failure)
}
fn valid_unit(r: &Receipt) -> bool {
    r.pool == "heavy"
        && r.unit
            .strip_prefix("slotr-heavy-")
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        && r.invocation.len() == 32
        && r.invocation
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn attempt_path(attempt: &Path) -> Result<()> {
    if !attempt.is_absolute()
        || fs::canonicalize(attempt).map_err(failure)? != attempt
        || !attempt.starts_with(fs::canonicalize(state_dir()?).map_err(failure)?)
    {
        return Err(failure(
            "bounded payload attempt is outside runner-owned canonical state directory",
        ));
    }
    Ok(())
}
pub(super) fn payload(work_id: i64, launch: i64, check_id: &str, attempt: &Path) -> Result<Value> {
    attempt_path(attempt)?;
    let work = read("show", work_id)?;
    let (run, _a, task) = runner::check_identity(&work, launch)?;
    let c = current_contract(&work)?;
    let check = c
        .profile
        .checks
        .iter()
        .find(|c| c.id == check_id && c.admission == api::Admission::Heavy)
        .ok_or_else(|| failure("bounded payload check is not assigned/approved heavy argv"))?;
    let receipt = Receipt {
        work: work_id,
        root: number(&work, "campaign")?,
        launch,
        task,
        run: number(run, "run_id")?,
        revision: number(&work, "revision")?,
        check: check.id.clone(),
        profile: c.profile.digest.clone(),
        unit: store::env("SLOTR_RUN"),
        invocation: store::env("SLOTR_INVOCATION"),
        pool: store::env("SLOTR_POOL"),
        slot: store::env("SLOTR_SLOT").parse().map_err(failure)?,
    };
    if !valid_unit(&receipt) {
        return Err(failure(
            "missing or malformed slotr unit/invocation environment; payload not run",
        ));
    }
    let inputs = read("inputs", work_id)?;
    let deps: Vec<api::DependencyInput> =
        serde_json::from_value(inputs["dependency_inputs"].clone()).map_err(failure)?;
    let digest = api::input_digest(&c.profile, &deps)?;
    let expected = run["results"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .rev()
                .find(|r| r["payload"]["kind"] == "execution")
        })
        .ok_or_else(|| failure("execution source missing"))?;
    let expected: api::Source =
        serde_json::from_value(expected["payload"]["result"]["source"].clone()).map_err(failure)?;
    if c.checkout.host != store::local_machine()
        || digest != c.base.input_digest
        || api::profile_digest(&c.profile)? != c.profile.digest
        || runner::source(Path::new(&c.checkout.cwd), &digest)? != expected
    {
        return Err(failure("bounded payload source/profile/input changed"));
    }
    // Durable admission identity precedes EVERY approved command effect.
    runner::artifact(
        attempt,
        "admission.json",
        &serde_json::to_vec(&receipt).map_err(failure)?,
    )?;
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let signals = [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT]
        .into_iter()
        .map(|s| signal_hook::flag::register(s, cancel.clone()).map_err(failure))
        .collect::<Result<Vec<_>>>()?;
    let outcome = runner::execute(check, attempt, Path::new(&c.checkout.cwd), &cancel);
    for signal in signals {
        signal_hook::low_level::unregister(signal);
    }
    let finished = Finished {
        admission: receipt,
        outcome: outcome?,
    };
    runner::artifact(
        attempt,
        "finished.json",
        &serde_json::to_vec(&finished).map_err(failure)?,
    )?;
    Ok(json!({"bounded_check":check_id,"launch":launch}))
}
pub(super) fn unit_absent(text: &str) -> bool {
    let fields = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    fields.get("MainPID") == Some(&"0")
        && fields.get("ControlGroup") == Some(&"")
        && (fields.get("LoadState") == Some(&"not-found")
            || matches!(fields.get("ActiveState"), Some(&"inactive" | &"failed")))
}
fn cleanup(unit: &str) -> Result<Value> {
    let mut stop = Command::new("slotr");
    stop.args(["stop", unit, "--reason", "taskr bounded check cleanup"]);
    let stop =
        process::run(stop, Duration::from_secs(15), &AtomicBool::new(false)).map_err(failure)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut cmd = Command::new("systemctl");
        cmd.args([
            "--user",
            "show",
            unit,
            "--property=LoadState",
            "--property=ActiveState",
            "--property=MainPID",
            "--property=ControlGroup",
        ]);
        let observed =
            process::run(cmd, Duration::from_secs(5), &AtomicBool::new(false)).map_err(failure)?;
        let text = String::from_utf8(observed.stdout.bytes).map_err(failure)?;
        if !observed.timed_out && unit_absent(&text) {
            return Ok(json!({"unit":unit,"stop_exit":stop.exit,"absent":true,"observation":text}));
        }
        if Instant::now() >= deadline {
            return Err(failure(
                "slotr unit stop/absence unproved; checkout remains reserved",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub(super) fn run(
    work: i64,
    root: i64,
    launch: i64,
    task: i64,
    check: &api::Check,
    attempt: &Path,
    cancel: &AtomicBool,
) -> Result<(runner::Outcome, api::AdmissionEvidence)> {
    if !cfg!(target_os = "linux") {
        return Err(failure(
            "heavy checks require Linux slotr or an explicit macOS exception",
        ));
    }
    attempt_path(attempt)?;
    let assigned = read("show", work)?;
    let (assigned_run, _, _) = runner::check_identity(&assigned, launch)?;
    let approved = current_contract(&assigned)?;
    let mut cmd = Command::new("slotr");
    cmd.args([
        "run",
        "--pool",
        "heavy",
        "--kind",
        "build",
        "--campaign",
        &format!("taskr-{root}"),
        "--task",
        &task.to_string(),
        "--purpose",
        &format!("work-{work}-launch-{launch}-{}", check.id),
        "--",
    ])
    .arg(std::env::current_exe().map_err(failure)?)
    .args([
        "work",
        "_bounded-check",
        &work.to_string(),
        "--launch",
        &launch.to_string(),
        "--check",
        &check.id,
        "--attempt",
    ])
    .arg(attempt);
    // Queue admission is bounded too. Inner timeout starts only when admission completes.
    let outer = process::run(
        cmd,
        Duration::from_millis(check.timeout_ms).saturating_add(Duration::from_secs(120)),
        cancel,
    );
    let receipt: Receipt = bounded_json(&attempt.join("admission.json")).map_err(|_| {
        failure(
            "missing/late slotr admission receipt; process uncertain, checkout remains reserved",
        )
    })?;
    if !valid_unit(&receipt)
        || receipt.work != work
        || receipt.root != root
        || receipt.launch != launch
        || receipt.task != task
        || receipt.check != check.id
        || receipt.run != number(assigned_run, "run_id")?
        || receipt.revision != number(&assigned, "revision")?
        || receipt.profile != approved.profile.digest
    {
        return Err(failure(
            "slotr receipt identity mismatch; process uncertain",
        ));
    }
    let cleanup = cleanup(&receipt.unit)?;
    runner::artifact(
        attempt,
        "unit-stop.json",
        &serde_json::to_vec(&cleanup).map_err(failure)?,
    )?;
    // Even pipe/capture failures must reconcile the admitted unit before returning.
    if let Ok(out) = &outer {
        runner::artifact(attempt, "slotr-stdout", &out.stdout.bytes)?;
        runner::artifact(attempt, "slotr-stderr", &out.stderr.bytes)?;
    }
    let finished: Finished = bounded_json(&attempt.join("finished.json")).map_err(|_| {
        failure("admitted payload has no machine completion receipt; no proof submitted")
    })?;
    if finished.admission != receipt {
        return Err(failure(
            "slotr invocation changed between admission and completion",
        ));
    }
    let mut outcome = finished.outcome;
    match outer {
        Ok(out) if out.exit == Some(0) && !out.timed_out && !out.cancelled => {}
        Ok(out) => {
            outcome.error = Some(
                "slotr wrapper failed, timed out or was cancelled; unit absence proved".into(),
            );
            outcome.timed_out |= out.timed_out;
        }
        Err(_) => outcome.error = Some("slotr wrapper capture failed; unit absence proved".into()),
    }
    outcome.artifacts.push(api::Artifact {
        name: "slotr-admission".into(),
        sha256: format!(
            "{:x}",
            sha2::Sha256::digest(fs::read(attempt.join("admission.json")).map_err(failure)?)
        ),
    });
    outcome.artifacts.push(api::Artifact {
        name: "slotr-unit-stop".into(),
        sha256: format!(
            "{:x}",
            sha2::Sha256::digest(fs::read(attempt.join("unit-stop.json")).map_err(failure)?)
        ),
    });
    Ok((
        outcome,
        api::AdmissionEvidence {
            method: api::AdmissionMethod::Slotr,
            ticket: Some(format!("{}:{}", receipt.unit, receipt.invocation)),
            reason: Some(
                "private payload receipt before argv; explicit unit stop and absence observed"
                    .into(),
            ),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_replaced_invocation_is_not_a_ticket() {
        let mut r = Receipt {
            work: 1,
            root: 2,
            launch: 3,
            task: 4,
            run: 5,
            revision: 1,
            check: "fixture".into(),
            profile: "a".repeat(64),
            unit: "slotr-heavy-7".into(),
            invocation: "b".repeat(32),
            pool: "heavy".into(),
            slot: 0,
        };
        assert!(valid_unit(&r));
        r.invocation.clear();
        assert!(!valid_unit(&r));
        r.invocation = "B".repeat(32);
        assert!(!valid_unit(&r));
        r.invocation = "b".repeat(32);
        r.unit = "unrelated.service".into();
        assert!(!valid_unit(&r));
    }
    #[test]
    fn missing_late_or_malformed_machine_receipt_never_passes() {
        let dir =
            std::env::temp_dir().join(format!("taskr-admission-receipt-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("admission.json");
        let _ = fs::remove_file(&path);
        assert!(bounded_json::<Receipt>(&path).is_err());
        fs::write(&path, b"slotr: PASS").unwrap();
        assert!(bounded_json::<Receipt>(&path).is_err());
        fs::write(&path, b"{}").unwrap();
        assert!(bounded_json::<Receipt>(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
