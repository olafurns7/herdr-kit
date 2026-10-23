use super::*;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::fs::OpenOptionsExt,
    sync::{Arc, atomic::AtomicBool},
    time::{SystemTime, UNIX_EPOCH},
};

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0");
    let out =
        process::run(cmd, Duration::from_secs(15), &AtomicBool::new(false)).map_err(failure)?;
    if out.exit != Some(0) || out.timed_out || out.stdout.digest().truncated {
        return Err(failure("git source observation failed"));
    }
    String::from_utf8(out.stdout.bytes)
        .map(|s| s.trim_end().into())
        .map_err(failure)
}
pub(super) fn source(cwd: &Path, input_digest: &str) -> Result<api::Source> {
    let top = git(cwd, &["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(top).map_err(failure)? != fs::canonicalize(cwd).map_err(failure)? {
        return Err(failure("checkout is not canonical Git root"));
    }
    if !git(cwd, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty() {
        return Err(failure("source checkout has tracked or untracked changes"));
    }
    Ok(api::Source {
        commit: git(cwd, &["rev-parse", "HEAD"])?,
        tree: git(cwd, &["rev-parse", "HEAD^{tree}"])?,
        input_digest: input_digest.into(),
    })
}
pub(super) fn artifact(dir: &Path, name: &str, bytes: &[u8]) -> Result<api::Artifact> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .open(dir.join(name))
        .map_err(failure)?;
    f.write_all(bytes).map_err(failure)?;
    f.sync_all().map_err(failure)?;
    Ok(api::Artifact {
        name: name.into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}
pub(super) fn receipt(
    dir: &Path,
    relative: &str,
    expected: &[String],
) -> Result<(Vec<api::Case>, Vec<u8>)> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(failure("receipt must be a relative path without traversal"));
    }
    let mut target = dir.to_path_buf();
    for component in path.components() {
        target.push(component);
        if fs::symlink_metadata(&target)
            .map_err(failure)?
            .file_type()
            .is_symlink()
        {
            return Err(failure("receipt symlinks are forbidden"));
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(&target)
        .map_err(failure)?
        .take((api::MAX_JSON + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > api::MAX_JSON {
        return Err(failure("case receipt exceeds limit"));
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Receipt {
        cases: Vec<api::Case>,
    }
    let parsed: Receipt = serde_json::from_slice(&bytes).map_err(failure)?;
    validate_cases(&parsed.cases, expected)?;
    Ok((parsed.cases, bytes))
}
pub(super) fn validate_cases(cases: &[api::Case], expected: &[String]) -> Result<()> {
    let actual = cases
        .iter()
        .map(|c| c.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let want = expected
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if actual.len() != cases.len() || actual != want || cases.iter().any(|c| c.status != "passed") {
        return Err(failure(
            "case receipt has missing, duplicate, extra, skipped, failed or error cases",
        ));
    }
    Ok(())
}
pub(super) fn check_identity(work: &Value, launch: i64) -> Result<(&Value, &Value, i64)> {
    assigned_gate(
        work,
        launch,
        &store::env("TASKR_TASK"),
        &store::env("TASKR_LAUNCH"),
    )
}
pub(super) fn assigned_gate<'a>(
    work: &'a Value,
    launch: i64,
    task_env: &str,
    launch_env: &str,
) -> Result<(&'a Value, &'a Value, i64)> {
    let run = current_run(work).ok_or_else(|| failure("work has no current run"))?;
    let a = run["assignments"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["launch_id"] == launch))
        .ok_or_else(|| failure("gate assignment missing"))?;
    let task = number(a, "task_id")?;
    if a["stage"] != "gate"
        || a["round"] != run["round"]
        || !matches!(a["process_state"].as_str(), Some("starting" | "running"))
        || a["state"] == "settled"
        || task_env != task.to_string()
        || launch_env != launch.to_string()
    {
        return Err(store::Error {
            code: ExitCode::Rejected,
            message: "run-checks requires the current bound gate task/launch identity".into(),
        });
    }
    if a["dispatch"]["pane"].as_str().is_none() {
        return Err(failure("gate has no immutable dispatch"));
    }
    Ok((run, a, task))
}
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Outcome {
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub error: Option<String>,
    pub stdout: api::OutputDigest,
    pub stderr: api::OutputDigest,
    pub cases: Vec<api::Case>,
    pub artifacts: Vec<api::Artifact>,
}
pub(super) fn execute(
    check: &api::Check,
    attempt: &Path,
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<Outcome> {
    let executable = check
        .argv
        .first()
        .ok_or_else(|| failure("approved argv is empty"))?;
    let context = tool_context(executable, cwd)?;
    let tool = artifact(
        attempt,
        "tool-context.json",
        &serde_json::to_vec(&context).map_err(failure)?,
    )?;
    // Preserve executable spelling: canonicalizing rustc/cargo changes multicall argv[0].
    let mut cmd = Command::new(executable);
    cmd.args(&check.argv[1..])
        .current_dir(cwd)
        .env("TASKR_ATTEMPT_DIR", attempt);
    let out =
        process::run(cmd, Duration::from_millis(check.timeout_ms), cancel).map_err(failure)?;
    let mut artifacts = vec![
        tool,
        artifact(attempt, "stdout", &out.stdout.bytes)?,
        artifact(attempt, "stderr", &out.stderr.bytes)?,
    ];
    let mut error = out
        .cancelled
        .then(|| "check cancelled; process group killed".to_string());
    let cases = match &check.receipt_file {
        Some(path) => match receipt(attempt, path, &check.cases) {
            Ok((cases, bytes)) => {
                artifacts.push(artifact(attempt, "case-receipt", &bytes)?);
                cases
            }
            Err(e) => {
                error = Some(e.message);
                Vec::new()
            }
        },
        None if check.cases.is_empty() => Vec::new(),
        None => {
            error = Some("case-bearing check has no declared receipt file".into());
            Vec::new()
        }
    };
    Ok(Outcome {
        exit: out.exit,
        signal: out.signal,
        timed_out: out.timed_out,
        error,
        stdout: out.stdout.digest(),
        stderr: out.stderr.digest(),
        cases,
        artifacts,
    })
}
fn tool_context(executable: &str, cwd: &Path) -> Result<Value> {
    use std::os::unix::fs::PermissionsExt;
    let path = if executable.contains('/') {
        cwd.join(executable)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into()))
            .map(|p| cwd.join(p).join(executable))
            .find(|p| {
                fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
            .ok_or_else(|| failure("approved check executable is missing"))?
    };
    let resolved = fs::canonicalize(&path).map_err(failure)?;
    let mut file = fs::File::open(&resolved).map_err(failure)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(failure)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > 512 * 1024 * 1024 {
            return Err(failure("check executable exceeds tool context size bound"));
        }
        hash.update(&buf[..n]);
    }
    Ok(
        json!({"requested":executable,"resolved":resolved,"sha256":format!("{:x}",hash.finalize()),"bytes":size,"rustup_toolchain":std::env::var("RUSTUP_TOOLCHAIN").ok()}),
    )
}
pub(super) fn run(work_id: i64, launch: i64) -> Result<Value> {
    let cancel = Arc::new(AtomicBool::new(false));
    let signals = [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]
        .into_iter()
        .map(|s| signal_hook::flag::register(s, cancel.clone()).map_err(failure))
        .collect::<Result<Vec<_>>>()?;
    let result = run_inner(work_id, launch, &cancel);
    for id in signals {
        signal_hook::low_level::unregister(id);
    }
    result
}
fn run_inner(work_id: i64, launch: i64, cancel: &AtomicBool) -> Result<Value> {
    let work = read("show", work_id)?;
    let (run, _a, task) = check_identity(&work, launch)?;
    let root = number(&work, "campaign")?;
    let Some(_lock) = lock(&state_dir()?.join(format!("gate-{launch}.lock")))? else {
        return Err(failure("gate runner already active"));
    };
    let contract = current_contract(&work)?;
    if contract.checkout.host != store::local_machine() {
        return Err(failure("runner checkout belongs to another host"));
    }
    let inputs = read("inputs", work_id)?;
    let dependency_inputs: Vec<api::DependencyInput> =
        serde_json::from_value(inputs["dependency_inputs"].clone()).map_err(failure)?;
    let input_digest = api::input_digest(&contract.profile, &dependency_inputs)?;
    if api::profile_digest(&contract.profile)? != contract.profile.digest
        || input_digest != contract.base.input_digest
        || inputs["input_digest"] != input_digest
    {
        return Err(failure("canonical approved profile/input digest drift"));
    }
    let results = run["results"]
        .as_array()
        .ok_or_else(|| failure("run results missing"))?;
    let execution = results
        .iter()
        .rev()
        .find(|r| r["payload"]["kind"] == "execution")
        .ok_or_else(|| failure("gate has no execution result"))?;
    let expected: api::Source =
        serde_json::from_value(execution["payload"]["result"]["source"].clone())
            .map_err(failure)?;
    let cwd = Path::new(&contract.checkout.cwd);
    if source(cwd, &input_digest)? != expected {
        return Err(failure(
            "actual clean source differs from assigned execution",
        ));
    }
    let snapshot = driver::campaign_snapshot(&driver::Local, root)?
        .ok_or_else(|| failure("campaign no longer enabled on this host"))?;
    if snapshot["configuration"]["enabled"] != true {
        return Err(failure("campaign disabled before checks"));
    }
    // Mark the gate's own launch, never a supplied root identity.
    taskr(&["start".into()], Some((task, launch)))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_nanos();
    let dir = state_dir()?.join(format!(
        "{root}/gate-{launch}-{}-{nonce}",
        std::process::id()
    ));
    private_dir(&dir)?;
    let mut checks = Vec::new();
    for (index, check) in contract.profile.checks.iter().enumerate() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(failure("runner cancelled before next check"));
        }
        let attempt = dir.join(format!("check-{index}"));
        private_dir(&attempt)?;
        let (out, admission) = match check.admission {
            api::Admission::Direct => (
                execute(check, &attempt, cwd, cancel)?,
                api::AdmissionEvidence {
                    method: api::AdmissionMethod::Direct,
                    ticket: None,
                    reason: None,
                },
            ),
            api::Admission::Heavy
                if cfg!(target_os = "macos") && snapshot["configuration"]["no_slotr"] == true =>
            {
                (
                    execute(check, &attempt, cwd, cancel)?,
                    api::AdmissionEvidence {
                        method: api::AdmissionMethod::HostException,
                        ticket: None,
                        reason: Some("explicit macOS no_slotr campaign configuration".into()),
                    },
                )
            }
            api::Admission::Heavy => {
                super::admission::run(work_id, root, launch, task, check, &attempt, cancel)?
            }
        };
        checks.push(api::Proof {
            run: number(run, "run_id")?,
            revision: number(&work, "revision")?,
            source: expected.clone(),
            profile_digest: contract.profile.digest.clone(),
            check: check.id.clone(),
            checker_launch: launch,
            argv: check.argv.clone(),
            exit: out.exit,
            signal: out.signal,
            timed_out: out.timed_out,
            error: out.error,
            admission,
            stdout: out.stdout,
            stderr: out.stderr,
            cases: out.cases,
            artifacts: out.artifacts,
        });
        if source(cwd, &input_digest)? != expected {
            return Err(failure("source changed during check; no proof submitted"));
        }
    }
    let fresh = read("show", work_id)?;
    check_identity(&fresh, launch)?;
    let fresh_inputs = read("inputs", work_id)?;
    if fresh["revision"] != work["revision"]
        || fresh["current_run"] != work["current_run"]
        || fresh_inputs != inputs
        || source(cwd, &input_digest)? != expected
    {
        return Err(failure(
            "source/assignment/dependency inputs changed before proof submission",
        ));
    }
    let dependencies: Vec<api::DependencyReceipt> =
        serde_json::from_value(inputs["dependency_receipts"].clone()).map_err(failure)?;
    let proof = api::GateProof {
        source: expected,
        profile_digest: contract.profile.digest.clone(),
        dependencies,
        host: contract.checkout.host.clone(),
        runner_version: "taskr-work-runner-v1".into(),
        environment: vec![
            api::EnvironmentValue {
                name: "os".into(),
                value: std::env::consts::OS.into(),
            },
            api::EnvironmentValue {
                name: "arch".into(),
                value: std::env::consts::ARCH.into(),
            },
            api::EnvironmentValue {
                name: "taskr-executable".into(),
                value: std::env::current_exe()
                    .map_err(failure)?
                    .to_string_lossy()
                    .into(),
            },
        ],
        checks,
    };
    let submission = serde_json::to_vec(&api::Submission::Proof(proof)).map_err(failure)?;
    artifact(&dir, "proof.json", &submission)?;
    // --file captures immutable local bytes before the normal client/RPC write.
    let result = taskr(
        &[
            "work".into(),
            "submit".into(),
            launch.to_string(),
            "--file".into(),
            dir.join("proof.json").to_string_lossy().into(),
        ],
        Some((task, launch)),
    )?;
    Ok(json!({"submission":result,"artifacts":dir,"launch":launch}))
}
