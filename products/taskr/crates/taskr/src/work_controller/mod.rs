//! Local controller entry points. All authority stays behind normal CLI/RPC operations.
mod admission;
mod driver;
mod process;
mod runner;
mod scheduler;
pub(crate) use scheduler::Scheduler;
#[cfg(test)]
mod tests;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};
use taskr_core::{
    ExitCode,
    store::{self, Result, work as api},
};

fn failure(message: impl std::fmt::Display) -> store::Error {
    store::Error {
        code: ExitCode::Transport,
        message: message.to_string(),
    }
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failure(format!("missing {key}")))
}
fn number(v: &Value, key: &str) -> Result<i64> {
    v[key]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| failure(format!("invalid {key}")))
}
fn scan_rows<'a>(v: &'a Value, key: &str) -> Result<(&'a Vec<Value>, bool)> {
    let rows = v[key]
        .as_array()
        .ok_or_else(|| failure(format!("missing {key} scan rows")))?;
    let limit = number(v, "limit")? as usize;
    let truncated = v["truncated"]
        .as_bool()
        .ok_or_else(|| failure("scan truncation is not explicit"))?;
    if v["returned"].as_u64() != Some(rows.len() as u64) || rows.len() > limit {
        return Err(failure("scan count/limit mismatch"));
    }
    Ok((rows, truncated))
}
fn state_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| failure("HOME is not set"))?;
    let db = std::env::var("TASKR_DB").ok();
    let dir = state_path(
        Path::new(&home),
        db.as_deref(),
        &std::env::current_dir().map_err(failure)?,
    )?;
    private_dir(&dir)?;
    Ok(dir)
}
fn state_path(home: &Path, db: Option<&str>, cwd: &Path) -> Result<PathBuf> {
    let mut dir = home.join(".local/state/taskr/work-controller");
    if let Some(db) = db.filter(|s| !s.is_empty()) {
        let normalized = store::orch::absolute(
            cwd.to_str()
                .ok_or_else(|| failure("non UTF-8 controller cwd"))?,
            db,
        );
        dir.push(format!("db-{:x}", Sha256::digest(normalized.as_bytes())));
    }
    Ok(dir)
}
fn private_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(failure)
}
fn shell_context() -> Result<String> {
    let exe = std::env::current_exe().map_err(failure)?;
    if exe.file_name().and_then(|s| s.to_str()) != Some("taskr") {
        return Err(failure(
            "controller executable must be named taskr so child PATH selects the same CLI",
        ));
    }
    let bin = exe
        .parent()
        .ok_or_else(|| failure("taskr executable directory missing"))?;
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").map_err(failure)?
    );
    let mut setup = format!("unset TASKR_DB && export PATH={}", driver::quote(&path));
    if let Some(db) = std::env::var_os("TASKR_DB") {
        let db = PathBuf::from(db);
        let db = if db.is_absolute() {
            db
        } else {
            std::env::current_dir().map_err(failure)?.join(db)
        };
        setup.push_str(&format!(
            " && export TASKR_DB={}",
            driver::quote(db.to_str().ok_or_else(|| failure("non UTF-8 TASKR_DB"))?)
        ));
    }
    Ok(setup)
}
fn lock(path: &Path) -> Result<Option<fs::File>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(failure)?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
        Err(e) => Err(failure(e)),
    }
}
fn taskr(args: &[String], identity: Option<(i64, i64)>) -> Result<Value> {
    let mut cmd = Command::new(std::env::current_exe().map_err(failure)?);
    cmd.arg("--json").args(args);
    if let Some((task, launch)) = identity {
        cmd.env("TASKR_TASK", task.to_string())
            .env("TASKR_LAUNCH", launch.to_string());
    }
    let output =
        process::run(cmd, Duration::from_secs(75), &AtomicBool::new(false)).map_err(failure)?;
    if output.exit != Some(0) || output.timed_out || output.stdout.digest().truncated {
        return Err(store::Error {
            code: if output.exit == Some(6) {
                ExitCode::Rejected
            } else {
                ExitCode::Transport
            },
            message: format!(
                "taskr {} failed (exit {:?}, deadline {}); inspect ledger before retry",
                args.first().map_or("", String::as_str),
                output.exit,
                output.timed_out
            ),
        });
    }
    if let Ok(value) = serde_json::from_slice(&output.stdout.bytes) {
        return Ok(value);
    }
    let rows = serde_json::Deserializer::from_slice(&output.stdout.bytes)
        .into_iter::<Value>()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(failure)?;
    Ok(Value::Array(rows))
}
fn read(verb: &str, id: i64) -> Result<Value> {
    taskr(&["work".into(), verb.into(), id.to_string()], None)
}
fn mutation(verb: &str, id: i64, root: i64, generation: i64, data: Option<Value>) -> Result<Value> {
    let mut args = vec![
        "work".into(),
        verb.into(),
        id.to_string(),
        "--as".into(),
        root.to_string(),
        "--generation".into(),
        generation.to_string(),
    ];
    if let Some(data) = data {
        args.extend([
            "--data".into(),
            serde_json::to_string(&data).map_err(failure)?,
        ]);
    }
    taskr(&args, None)
}
fn current_contract(work: &Value) -> Result<api::Contract> {
    let revisions = work["revisions"]
        .as_array()
        .ok_or_else(|| failure("missing revisions"))?;
    let revision = revisions
        .iter()
        .find(|r| r["revision"] == work["revision"])
        .ok_or_else(|| failure("missing current revision"))?;
    serde_json::from_value(revision["contract"].clone()).map_err(failure)
}
fn current_run(work: &Value) -> Option<&Value> {
    work["runs"]
        .as_array()?
        .iter()
        .find(|r| r["run_id"] == work["current_run"])
}
pub(crate) fn drive(root: i64) -> Result<Value> {
    let Some(_lock) = lock(&state_dir()?.join(format!("campaign-{root}.lock")))? else {
        return Ok(json!({"busy":true}));
    };
    driver::drive(&driver::Local, root)
}

/// Call BEFORE net::route, including for --json invocations. Never opens a ledger here.
pub(crate) fn dispatch(mut json: bool, args: &[String]) -> Option<ExitCode> {
    let args = if args.first().is_some_and(|s| s == "--json") {
        json = true;
        &args[1..]
    } else {
        args
    };
    if args.first().is_none_or(|s| s != "work")
        || !args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "drive" | "run-checks" | "source" | "_bounded-check"
            )
        })
    {
        return None;
    }
    let result = (|| {
        let verb = args[1].as_str();
        let id = args.get(2).ok_or_else(|| {
            store::usage("work drive ROOT --once | work run-checks WORK --launch LAUNCH")
        })?;
        let id = store::id(id, "work/campaign ID")?;
        match verb {
            "_bounded-check"
                if args.len() == 9
                    && args[3] == "--launch"
                    && args[5] == "--check"
                    && args[7] == "--attempt" =>
            {
                admission::payload(
                    id,
                    store::id(&args[4], "gate launch")?,
                    &args[6],
                    Path::new(&args[8]),
                )
            }
            "source" if args.len() == 3 => {
                let inputs = read("inputs", id)?;
                let profile: api::Profile =
                    serde_json::from_value(inputs["profile"].clone()).map_err(failure)?;
                let deps: Vec<api::DependencyInput> =
                    serde_json::from_value(inputs["dependency_inputs"].clone()).map_err(failure)?;
                let digest = api::input_digest(&profile, &deps)?;
                if inputs["checkout"]["host"] != store::local_machine()
                    || inputs["input_digest"] != digest
                    || api::profile_digest(&profile)? != profile.digest
                {
                    return Err(failure("source checkout host/profile/input mismatch"));
                }
                serde_json::to_value(runner::source(
                    Path::new(field(&inputs["checkout"], "cwd")?),
                    &digest,
                )?)
                .map_err(failure)
            }
            "drive" if args.len() == 4 && args[3] == "--once" => {
                if !store::env("TASKR_LAUNCH").is_empty() {
                    return Err(store::usage("a worker cannot drive a campaign"));
                }
                drive(id)
            }
            "run-checks" if args.len() == 5 && args[3] == "--launch" => {
                runner::run(id, store::id(&args[4], "gate launch")?)
            }
            _ => Err(store::usage(
                "work drive ROOT --once | work run-checks WORK --launch LAUNCH",
            )),
        }
    })();
    Some(match result {
        Ok(value) => {
            crate::cli::emit(json, &value, false);
            ExitCode::Ok
        }
        Err(e) => crate::cli::error(json, "work", &e.message, e.code),
    })
}
