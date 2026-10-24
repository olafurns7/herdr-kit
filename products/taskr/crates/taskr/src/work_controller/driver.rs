use super::*;
use std::sync::atomic::AtomicBool;

pub(super) trait Backend {
    fn read(&self, verb: &str, id: i64) -> Result<Value>;
    fn change(
        &self,
        verb: &str,
        id: i64,
        root: i64,
        generation: i64,
        data: Option<Value>,
    ) -> Result<Value>;
    fn herdr(&self, args: &[&str]) -> Result<Value>;
    fn lanes(&self) -> Result<String>;
    fn command(&self, args: &[String]) -> Result<Value>;
    fn launch(&self, args: &[String]) -> Result<()>;
    fn process_scan(&self, cwd: &Path, shell: i32, trust: &inventory::Trust) -> Result<Value>;
    fn client_mode(&self) -> bool {
        crate::net::client_mode()
    }
    fn directory(&self, root: i64, launch: i64) -> Result<PathBuf> {
        attempt_dir(root, launch)
    }
    fn source(&self, cwd: &Path, digest: &str) -> Result<api::Source> {
        runner::source(cwd, digest)
    }
}
pub(super) struct Local;
impl Backend for Local {
    fn process_scan(&self, cwd: &Path, shell: i32, trust: &inventory::Trust) -> Result<Value> {
        inventory::scan(cwd, shell, trust)
    }
    fn read(&self, v: &str, id: i64) -> Result<Value> {
        read(v, id)
    }
    fn change(&self, v: &str, id: i64, r: i64, g: i64, d: Option<Value>) -> Result<Value> {
        mutation(v, id, r, g, d)
    }
    fn herdr(&self, args: &[&str]) -> Result<Value> {
        let sock = crate::write::herdr::socket();
        if !crate::write::herdr::up(&sock) {
            return Err(failure("Herdr unavailable; no CLI run or server start"));
        }
        let mut cmd = Command::new("herdr");
        cmd.args(args).env("HERDR_SOCKET_PATH", sock);
        let out = process::run(
            cmd,
            Duration::from_secs(if args.starts_with(&["agent", "wait"]) {
                125
            } else {
                20
            }),
            &AtomicBool::new(false),
        )
        .map_err(failure)?;
        if out.exit != Some(0) || out.timed_out || out.stdout.digest().truncated {
            return Err(failure("Herdr command failed or exceeded bounds"));
        }
        let value: Value = serde_json::from_slice(&out.stdout.bytes).map_err(failure)?;
        if !value["error"].is_null() {
            return Err(failure("incompatible Herdr response"));
        }
        Ok(value["result"].clone())
    }
    fn lanes(&self) -> Result<String> {
        let mut cmd = Command::new("herdr-launch");
        cmd.arg("--lanes");
        let out =
            process::run(cmd, Duration::from_secs(10), &AtomicBool::new(false)).map_err(failure)?;
        if out.exit != Some(0) || out.timed_out || out.stdout.digest().truncated {
            return Err(failure("installed lane table unavailable"));
        }
        String::from_utf8(out.stdout.bytes).map_err(failure)
    }
    fn command(&self, args: &[String]) -> Result<Value> {
        taskr(args, None)
    }
    fn launch(&self, args: &[String]) -> Result<()> {
        if !crate::write::herdr::up(&crate::write::herdr::socket()) {
            return Err(failure("Herdr unavailable before launch"));
        }
        let mut cmd = Command::new("herdr-launch");
        cmd.args(args).env(
            "TASKR_WORK_EXECUTABLE",
            std::env::current_exe().map_err(failure)?,
        );
        let out = process::run(cmd, Duration::from_secs(150), &AtomicBool::new(false))
            .map_err(failure)?;
        if out.exit != Some(0) || out.timed_out {
            return Err(failure(
                "reserved launcher failed; reconcile bound intent, never restart blindly",
            ));
        }
        Ok(())
    }
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .ok_or_else(|| failure(format!("missing {key} array")))
}
pub(super) fn lane_matches(table: &str, lane: &api::Lane, stage: &str) -> bool {
    let prefix = if stage == "executor" { "impl" } else { "rev" };
    table.lines().any(|line| {
        let f = line.split('\t').collect::<Vec<_>>();
        f.len() == 5
            && f[0] == lane.name
            && f[1] == lane.provider
            && f[2] == lane.model
            && f[3] == lane.effort
            && f[4].split_whitespace().any(|p| p == prefix)
    })
}
fn same_generation(b: &impl Backend, root: i64, generation: i64) -> Result<()> {
    let Some(s) = campaign_snapshot(b, root)? else {
        return Err(store::Error {
            code: ExitCode::Rejected,
            message: "campaign no longer enabled on this host".into(),
        });
    };
    if s["generation"] != generation
        || s["configuration"]["enabled"] != true
        || s["parked"] != false
    {
        return Err(store::Error {
            code: ExitCode::Rejected,
            message: "campaign generation/configuration changed; no further effect".into(),
        });
    }
    Ok(())
}
pub(super) fn campaign_snapshot(b: &impl Backend, root: i64) -> Result<Option<Value>> {
    let value = b.command(&["work".into(), "campaigns".into()])?;
    let (rows, truncated) = scan_rows(&value, "campaigns")?;
    if let Some(row) = rows.iter().find(|s| s["campaign"] == root) {
        return Ok(Some(row.clone()));
    }
    if truncated {
        return Err(failure(format!(
            "campaign scan bounded at {}; requested campaign not observed",
            value["limit"]
        )));
    }
    Ok(None)
}
fn block(b: &impl Backend, work: i64, root: i64, generation: i64, reason: &str) -> Result<Value> {
    b.change(
        "block",
        work,
        root,
        generation,
        Some(json!({"reason":reason})),
    )
}
fn uncertain(
    b: &impl Backend,
    a: &Value,
    root: i64,
    generation: i64,
    reason: &str,
) -> Result<Value> {
    b.change(
        "uncertain",
        number(a, "launch_id")?,
        root,
        generation,
        Some(json!({"reason":reason})),
    )
}
fn canonical(path: &str) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(failure)
}
fn verify_workspace(b: &impl Backend, workspace: &str, cwd: &str) -> Result<()> {
    let rows = b.herdr(&["worktree", "list", "--workspace", workspace])?;
    let target = canonical(cwd)?;
    let matches = array(&rows, "worktrees")?
        .iter()
        .filter(|w| w["open_workspace_id"] == workspace)
        .filter(|w| w["path"].as_str().and_then(|p| canonical(p).ok()).as_ref() == Some(&target))
        .count();
    if matches != 1 {
        return Err(failure(
            "configured workspace is not bound to the canonical checkout",
        ));
    }
    Ok(())
}
fn pane_location(b: &impl Backend, pane: &str, workspace: &str, cwd: &str) -> Result<String> {
    let info = b.herdr(&["pane", "get", pane])?;
    let p = &info["pane"];
    if p["pane_id"] != pane
        || p["workspace_id"] != workspace
        || canonical(field(p, "cwd")?)? != canonical(cwd)?
    {
        return Err(failure("pane location/checkout drift"));
    }
    Ok(field(p, "tab_id")?.into())
}
fn shell(b: &impl Backend, pane: &str) -> Result<Value> {
    let p = b.herdr(&["pane", "process-info", "--pane", pane])?;
    let p = &p["process_info"];
    let pid = number(p, "shell_pid")?;
    if p["foreground_process_group_id"] != pid {
        return Err(failure("pane foreground is not its interactive shell"));
    }
    let fg = array(p, "foreground_processes")?;
    let process = fg
        .iter()
        .find(|p| p["pid"] == pid)
        .ok_or_else(|| failure("shell foreground process missing"))?;
    let name = field(process, "name")?;
    if !matches!(
        Path::new(name).file_name().and_then(|p| p.to_str()),
        Some("sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "nu")
    ) {
        return Err(failure("foreground command is not a supported shell"));
    }
    Ok(json!({"pid":pid,"command":name}))
}
fn process_trust(b: &impl Backend, root: i64, ws: &str) -> Result<inventory::Trust> {
    let campaign = b.command(&["campaign".into(), root.to_string(), "--all".into()])?;
    let host = campaign["root"]["host"]
        .as_str()
        .ok_or_else(|| failure("registered campaign root host projection missing"))?;
    // Empty legacy projection denotes the ledger host, never a remote client's host.
    if host.is_empty() && b.client_mode() {
        return Err(failure(
            "registered campaign root identity/host mismatch: hub-local root cannot authorize a client scan",
        ));
    }
    let host = store::machine_name((!host.is_empty()).then_some(host));
    if campaign["root"]["id"] != root || host != store::local_machine() {
        return Err(failure("registered campaign root identity/host mismatch"));
    }
    let root_pane = field(&campaign["root"], "pane_id")?.to_string();
    let info = b.herdr(&["pane", "process-info", "--pane", &root_pane])?;
    let fg = array(&info["process_info"], "foreground_processes")?;
    if fg.is_empty() || fg.len() > 128 {
        return Err(failure("root foreground missing or bounded"));
    }
    let mut trust = inventory::Trust {
        root_pane: root_pane.clone(),
        root_group: i32::try_from(number(
            &info["process_info"],
            "foreground_process_group_id",
        )?)
        .map_err(failure)?,
        root_at: timestamp(),
        ..Default::default()
    };
    for p in fg {
        trust.root.insert(
            i32::try_from(number(p, "pid")?).map_err(failure)?,
            field(p, "name")?.into(),
        );
    }
    let panes = b.herdr(&["pane", "list", "--workspace", ws])?;
    let panes = array(&panes, "panes")?;
    if panes.len() > 128 {
        return Err(failure("workspace pane inventory bounded"));
    }
    for p in panes {
        let pane = field(p, "pane_id")?;
        let location = b.herdr(&["pane", "get", pane])?;
        if location["pane"]["workspace_id"] != ws || location["pane"]["pane_id"] != pane {
            return Err(failure("workspace shell pane identity changed"));
        }
        if agent(b, pane)?.is_none()
            && let Ok(s) = shell(b, pane)
        {
            trust.shells.insert(
                i32::try_from(number(&s, "pid")?).map_err(failure)?,
                inventory::WorkspaceShell {
                    command: field(&s, "command")?.into(),
                    pane: pane.into(),
                    group: i32::try_from(number(&s, "pid")?).map_err(failure)?,
                },
            );
        }
    }
    Ok(trust)
}
fn agent(b: &impl Backend, pane: &str) -> Result<Option<Value>> {
    let agents = b.herdr(&["agent", "list"])?;
    let found = array(&agents, "agents")?
        .iter()
        .filter(|a| a["pane_id"] == pane)
        .cloned()
        .collect::<Vec<_>>();
    if found.len() > 1 {
        return Err(failure("multiple native agents in bound pane"));
    }
    Ok(found.into_iter().next())
}
pub(super) fn owns_agent(a: &Value, expected_name: &str, native: &str, pane: &str) -> bool {
    !native.is_empty()
        && a["name"] == expected_name
        && a["pane_id"] == pane
        && a["agent_session"]["value"] == native
}
fn name(a: &Value) -> Result<String> {
    let prefix = if a["stage"] == "executor" {
        "impl"
    } else {
        "rev"
    };
    Ok(format!("{prefix}-w{}", number(a, "launch_id")?))
}
fn assignment(run: &Value, launch: i64) -> Result<&Value> {
    array(run, "assignments")?
        .iter()
        .find(|a| a["launch_id"] == launch)
        .ok_or_else(|| failure("assignment missing"))
}
fn timestamp() -> String {
    store::now()
}

pub(super) fn drive(b: &impl Backend, root: i64) -> Result<Value> {
    let Some(snapshot) = campaign_snapshot(b, root)? else {
        return Ok(json!({"disabled":true}));
    };
    if snapshot["configuration"]["enabled"] != true || snapshot["parked"] != false {
        return Ok(json!({"disabled":true}));
    }
    let generation = snapshot["generation"]
        .as_i64()
        .filter(|g| *g >= 0)
        .ok_or_else(|| failure("missing generation"))?;
    let workspace = field(&snapshot["configuration"], "workspace")?;
    let host = store::local_machine();
    let pending = b.read("dispatch", root)?;
    if pending["campaign"] != root {
        return Err(failure("dispatch campaign identity mismatch"));
    }
    let (rows, truncated) = scan_rows(&pending, "work")?;
    let mut inspected = 0usize;
    let mut stale = 0usize;
    let budget = std::time::Instant::now();
    for candidate in rows {
        if budget.elapsed() > Duration::from_secs(4) {
            break;
        }
        if !matches!(
            candidate["state"].as_str(),
            Some("queued" | "active" | "verifying" | "cancelled")
        ) {
            return Err(failure("dispatch returned a non-pending work state"));
        }
        let id = number(candidate, "work_id")?;
        let work = b.read("show", id)?;
        inspected += 1;
        if work["work_id"] != id || work["campaign"] != root {
            return Err(failure("selected work identity mismatch"));
        }
        if work["revision"] != candidate["revision"]
            || work["state"] != candidate["state"]
            || work["current_run"] != candidate["current_run"]
        {
            stale += 1;
            continue;
        }
        let c = current_contract(&work)?;
        if snapshot["host"] != host
            || snapshot["configuration"]["host"] != host
            || c.checkout.host != host
        {
            block(
                b,
                id,
                root,
                generation,
                "same-host dispatch required: campaign/checkout/local host mismatch",
            )?;
            continue;
        }
        if let Err(e) = verify_workspace(b, workspace, &c.checkout.cwd) {
            block(b, id, root, generation, &e.message)?;
            continue;
        }
        let result = drive_work(b, root, generation, workspace, &work, &c);
        match result {
            Err(e) if e.code == ExitCode::Rejected => return Err(e),
            Err(e) => {
                let reason = if e.message.starts_with("process-stop-incomplete:") {
                    "process-stop-incomplete: inventory incomplete or cooperative writer remains; checkout retained; retry next tick"
                } else {
                    &e.message
                };
                block(b, id, root, generation, reason)?;
            }
            Ok(()) => {}
        }
    }
    let mut conditions = Vec::new();
    if truncated {
        conditions.push(format!(
            "dispatch snapshot bounded at {} rows; additional pending work was not observed",
            pending["limit"]
        ));
    }
    if inspected < rows.len() {
        conditions
            .push("local pass budget reached; returned pending rows remain uninspected".into());
    }
    Ok(
        json!({"campaign":root,"generation":generation,"limit":pending["limit"],"returned":rows.len(),"inspected":inspected,"stale":stale,"truncated":truncated,"complete_scan":!truncated&&inspected==rows.len(),"bounded_conditions":conditions}),
    )
}
fn drive_work(
    b: &impl Backend,
    root: i64,
    g: i64,
    ws: &str,
    work: &Value,
    c: &api::Contract,
) -> Result<()> {
    if work["state"] == "cancelled" {
        return reconcile_cancelled(b, root, g, ws, work, c);
    }
    let id = number(work, "work_id")?;
    let run = if work["state"] == "queued" {
        // Validate policy before the durable claim, so a drifted lane never creates a writer.
        let table = b.lanes()?;
        if !lane_matches(&table, &c.executor, "executor")
            || !lane_matches(&table, &c.reviewer, "reviewer")
            || c.adjudicator
                .as_ref()
                .is_some_and(|l| !lane_matches(&table, l, "adjudicator"))
        {
            return Err(failure(
                "installed lane table/role policy differs from contract",
            ));
        }
        same_generation(b, root, g)?;
        b.change("claim",id,root,g,Some(json!({"expected_revision":work["revision"],"key":format!("controller:{root}:{id}:{}",work["revision"])})))?
    } else {
        current_run(work)
            .ok_or_else(|| failure("active work has no current run"))?
            .clone()
    };
    for a in array(&run, "assignments")? {
        if a["round"] != run["round"] || a["process_state"] == "stopped" {
            continue;
        }
        match a["process_state"].as_str() {
            Some("reserved") => {
                dispatch(b, root, g, ws, work, c, &run, a)?;
                return Ok(());
            }
            Some("starting") => {
                recover(b, root, g, work, a)?;
                return Ok(());
            }
            Some("uncertain") => return Ok(()),
            Some("running") if a["state"] != "settled" => {
                if a["stage"] == "gate"
                    && b.directory(root, number(a, "launch_id")?)?
                        .join("shell-exit")
                        .exists()
                {
                    uncertain(
                        b,
                        a,
                        root,
                        g,
                        "gate shell exited without a settled proof; runner/admission failure requires inspection",
                    )?;
                }
                return Ok(());
            }
            Some("running") => {}
            _ => return Err(failure("unknown assignment process state")),
        }
    }
    same_generation(b, root, g)?;
    if let Some(exec) = array(&run, "results")?
        .iter()
        .rev()
        .find(|r| r["payload"]["kind"] == "execution")
    {
        let expected: api::Source =
            serde_json::from_value(exec["payload"]["result"]["source"].clone()).map_err(failure)?;
        if b.source(Path::new(&c.checkout.cwd), &expected.input_digest)? != expected {
            return Err(failure(
                "source changed after execution/proof; no stage advancement",
            ));
        }
    }
    let next = b.change("advance", number(&run, "run_id")?, root, g, None)?;
    match next["action"].as_str() {
        Some("stop") => {
            let a = assignment(&run, number(&next, "launch_id")?)?;
            if let Err(e) = stop(b, root, g, ws, &c.checkout.cwd, a) {
                if e.code == ExitCode::Rejected || e.message.starts_with("process-stop-incomplete:")
                {
                    return Err(e);
                }
                uncertain(b, a, root, g, &e.message)?;
                if e.message.starts_with("process-stop-") {
                    return Err(e);
                }
            }
        }
        Some("check" | "review" | "adjudicate" | "rework" | "reserve") => {
            let stage = match next["action"].as_str().unwrap() {
                "check" => "gate",
                "review" => "reviewer",
                "adjudicate" => "adjudicator",
                _ => "executor",
            };
            same_generation(b, root, g)?;
            let a = b.change(
                "reserve",
                number(&run, "run_id")?,
                root,
                g,
                Some(json!({"stage":stage})),
            )?;
            if a["process_state"] == "reserved" {
                dispatch(b, root, g, ws, work, c, &next, &a)?;
            }
        }
        Some("accepted" | "escalation" | "wait") => {}
        _ => return Err(failure("unknown durable advance action")),
    }
    Ok(())
}
fn reconcile_cancelled(
    b: &impl Backend,
    root: i64,
    g: i64,
    ws: &str,
    work: &Value,
    c: &api::Contract,
) -> Result<()> {
    let run = current_run(work)
        .ok_or_else(|| failure("cancelled work has no current run to reconcile"))?;
    if run["state"] != "active" {
        return Ok(());
    }
    let assignments = array(run, "assignments")?;
    for a in assignments {
        match a["process_state"].as_str() {
            Some("stopped") => {}
            Some("starting" | "running" | "uncertain") => {
                // Never recover a prompt/start after cancellation; stop only a proven bound identity.
                if a["stage"] != "gate" && a["native_session"].as_str().is_none() {
                    uncertain(
                        b,
                        a,
                        root,
                        g,
                        "cancelled launch has no bound native identity; explicit reconciliation required, no prompt or replacement",
                    )?;
                    return Ok(());
                }
                if let Err(e) = stop(b, root, g, ws, &c.checkout.cwd, a) {
                    if e.code == ExitCode::Rejected
                        || e.message.starts_with("process-stop-incomplete:")
                    {
                        return Err(e);
                    }
                    uncertain(b, a, root, g, &e.message)?;
                    if e.message.starts_with("process-stop-") {
                        return Err(e);
                    }
                    return Ok(());
                }
            }
            _ => {
                return Err(failure(
                    "cancelled run has an unsettled reservation or unknown process state; no launch permitted",
                ));
            }
        }
    }
    let a = assignments.last().ok_or_else(|| {
        failure("cancelled active run has no assignment for cleanup reconciliation")
    })?;
    same_generation(b, root, g)?;
    b.change(
        "reconcile",
        number(a, "launch_id")?,
        root,
        g,
        Some(json!({"stopped_event":a["stop_event"],"abandon":false})),
    )?;
    Ok(())
}
fn new_pane(
    b: &impl Backend,
    root: i64,
    ws: &str,
    cwd: &str,
    name: &str,
) -> Result<(String, String)> {
    let panes = b.herdr(&["pane", "list", "--workspace", ws])?;
    let tabs = b.herdr(&["tab", "list", "--workspace", ws])?;
    if array(&panes, "panes")?.len() == 1 && array(&tabs, "tabs")?.len() == 1 {
        let p = &panes["panes"][0];
        let pane = field(p, "pane_id")?;
        let campaign = b.command(&["campaign".into(), root.to_string(), "--all".into()])?;
        let unused = !array(&campaign, "lanes")?
            .iter()
            .any(|l| l["pane_id"] == pane)
            && campaign["root"]["pane_id"] != pane;
        if unused && agent(b, pane)?.is_none() && shell(b, pane).is_ok() {
            let tab = pane_location(b, pane, ws, cwd)?;
            b.herdr(&["tab", "rename", &tab, name])?;
            return Ok((tab, pane.into()));
        }
    }
    let created = b.herdr(&[
        "tab",
        "create",
        "--workspace",
        ws,
        "--cwd",
        cwd,
        "--label",
        name,
        "--env",
        "FLEET_ROLE=worker",
        "--env",
        "DISABLE_AUTO_UPDATE=true",
        "--no-focus",
    ])?;
    Ok((
        field(&created["tab"], "tab_id")?.into(),
        field(&created["root_pane"], "pane_id")?.into(),
    ))
}
pub(super) fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn attempt_dir(root: i64, launch: i64) -> Result<PathBuf> {
    let p = state_dir()?.join(format!("{root}/launch-{launch}"));
    private_dir(&p)?;
    Ok(p)
}
fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(failure)?;
    f.write_all(bytes).map_err(failure)?;
    f.sync_all().map_err(failure)
}
#[allow(clippy::too_many_arguments)] // The immutable run and assignment are separate ledger snapshots.
fn dispatch(
    b: &impl Backend,
    root: i64,
    g: i64,
    ws: &str,
    work: &Value,
    c: &api::Contract,
    run: &Value,
    a: &Value,
) -> Result<()> {
    let launch = number(a, "launch_id")?;
    let task = number(a, "task_id")?;
    let n = name(a)?;
    let gate = a["stage"] == "gate";
    if !gate {
        let l: api::Lane = serde_json::from_value(a["lane"].clone()).map_err(failure)?;
        if !lane_matches(&b.lanes()?, &l, field(a, "stage")?) {
            return Err(failure("installed lane table/role policy drift"));
        }
    }
    same_generation(b, root, g)?;
    let (tab, pane) = new_pane(b, root, ws, &c.checkout.cwd, &n)?;
    pane_location(b, &pane, ws, &c.checkout.cwd)?;
    shell(b, &pane)?;
    let dir = b.directory(root, launch)?;
    let brief = dir.join("brief.md");
    let report = dir.join("report.md");
    if !gate {
        write_once(
            &brief,
            brief_text(root, work, c, run, a, ws, &tab, &pane, &report)?.as_bytes(),
        )?;
    }
    same_generation(b, root, g)?;
    b.change("bind",launch,root,g,Some(json!({"workspace":ws,"tab":tab,"pane":pane,"dispatch_key":format!("controller:{root}:{launch}")})))?;
    // Anything after bind is recovery work on this exact reservation, never another start.
    let effect: Result<()> = (|| {
        same_generation(b, root, g)?;
        if gate {
            let exit = dir.join("shell-exit");
            let cmd = format!(
                "{} && export FLEET_ROLE=worker DISABLE_AUTO_UPDATE=true TASKR_TASK={task} TASKR_LAUNCH={launch} && {} work run-checks {} --launch {launch}; taskr_check_rc=$?; printf '%s\\n' \"$taskr_check_rc\" > {}",
                shell_context()?,
                quote(
                    std::env::current_exe()
                        .map_err(failure)?
                        .to_str()
                        .ok_or_else(|| failure("non UTF-8 executable"))?
                ),
                number(work, "work_id")?,
                quote(
                    exit.to_str()
                        .ok_or_else(|| failure("non UTF-8 exit path"))?
                )
            );
            b.herdr(&["pane", "run", &pane, &cmd])?;
        } else {
            b.launch(&[
                "--reserved".into(),
                task.to_string(),
                launch.to_string(),
                field(&a["lane"], "name")?.into(),
                n,
                brief.to_string_lossy().into(),
                report.to_string_lossy().into(),
                root.to_string(),
                pane,
            ])?;
        }
        Ok(())
    })();
    if let Err(e) = effect {
        if e.code == ExitCode::Rejected {
            return Err(e);
        }
        uncertain(b, a, root, g, &e.message)?;
    }
    Ok(())
}
pub(super) fn recover(b: &impl Backend, root: i64, g: i64, work: &Value, a: &Value) -> Result<()> {
    let launch = number(a, "launch_id")?;
    let task = number(a, "task_id")?;
    let pane = field(&a["dispatch"], "pane")?;
    if a["stage"] == "gate" {
        // A running shell gate cannot be restarted. The runner's start receipt repairs this tick.
        let log = b.command(&["log".into(), task.to_string(), "--limit".into(), "0".into()])?;
        if has_receipt(&log, launch) {
            b.change("started", launch, root, g, None)?;
            return Ok(());
        }
        uncertain(
            b,
            a,
            root,
            g,
            "gate dispatch has no launch-bound start receipt; inspect process before recovery",
        )?;
        return Ok(());
    }
    let Some(agent) = agent(b, pane)? else {
        uncertain(
            b,
            a,
            root,
            g,
            "bound start has no observed agent; absence alone does not prove writer stopped",
        )?;
        return Ok(());
    };
    let native = match field(&agent["agent_session"], "value") {
        Ok(native) => native,
        Err(e) => {
            uncertain(
                b,
                a,
                root,
                g,
                &format!("bound native identity unavailable: {}", e.message),
            )?;
            return Ok(());
        }
    };
    if !owns_agent(&agent, &name(a)?, native, pane)
        || a["native_session"].as_str().is_some_and(|s| s != native)
    {
        uncertain(
            b,
            a,
            root,
            g,
            "bound pane holds a changed/unknown native identity",
        )?;
        return Ok(());
    }
    let log = b.command(&["log".into(), task.to_string(), "--limit".into(), "0".into()])?;
    if has_receipt(&log, launch) {
        b.change(
            "started",
            launch,
            root,
            g,
            Some(json!({"pane":pane,"native_session":native,"observed_at":timestamp()})),
        )?;
    } else if !events(&log)
        .iter()
        .any(|e| e["launch_id"] == launch && e["kind"] == "prompt")
    {
        let path = b.directory(root, launch)?.join("brief.md");
        if !path.is_file() {
            uncertain(b, a, root, g, "bound start has no captured brief")?;
            return Ok(());
        }
        same_generation(b, root, g)?;
        let observed = (|| -> Result<()> {
            let pinned = b.change(
                "observe",
                launch,
                root,
                g,
                Some(json!({"pane":pane,"native_session":native,"observed_at":timestamp()})),
            )?;
            if pinned["launch_id"] != launch
                || pinned["task_id"] != task
                || pinned["dispatch"]["pane"] != pane
                || pinned["native_session"] != native
            {
                return Err(failure("recovery native observation unconfirmed"));
            }
            let current = agent(b, pane)?
                .ok_or_else(|| failure("recovery native agent disappeared after observation"))?;
            if !owns_agent(&current, &name(a)?, native, pane) {
                return Err(failure(
                    "recovery native identity changed after observation",
                ));
            }
            Ok(())
        })();
        if let Err(e) = observed {
            uncertain(
                b,
                a,
                root,
                g,
                &format!(
                    "recovery native observation failed; no prompt: {}",
                    e.message
                ),
            )?;
            return Ok(());
        }
        same_generation(b, root, g)?;
        b.command(&[
            "prompt".into(),
            task.to_string(),
            "--file".into(),
            path.to_string_lossy().into(),
        ])?;
    } else {
        // Pending prompts are observed, never resent. Unknown/expired attempts block once.
        let history = events(&log);
        let pending = history
            .iter()
            .rev()
            .find(|e| e["launch_id"] == launch && e["kind"] == "prompt");
        let recent = pending
            .and_then(|e| e["created_at"].as_str())
            .and_then(|s| {
                time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
            })
            .is_some_and(|at| time::OffsetDateTime::now_utc() - at < time::Duration::seconds(60));
        let failed = history.iter().any(|e| {
            e["launch_id"] == launch
                && e["kind"] == "prompt_outcome"
                && matches!(
                    e["data"]["outcome"].as_str(),
                    Some("delivery_unknown" | "rejected" | "no_receipt")
                )
        });
        if failed || !recent {
            uncertain(
                b,
                a,
                root,
                g,
                "prompt attempt has unknown/expired delivery and no receipt; never resend",
            )?;
        }
        let _ = work;
    }
    Ok(())
}
fn events(log: &Value) -> Vec<&Value> {
    match log.as_array() {
        Some(a) => a.iter().collect(),
        None => vec![log],
    }
}
fn has_receipt(log: &Value, launch: i64) -> bool {
    events(log)
        .iter()
        .any(|e| e["launch_id"] == launch && matches!(e["kind"].as_str(), Some("got" | "start")))
}
pub(super) fn stop(
    b: &impl Backend,
    root: i64,
    g: i64,
    ws: &str,
    cwd: &str,
    a: &Value,
) -> Result<()> {
    let launch = number(a, "launch_id")?;
    let pane = field(&a["dispatch"], "pane")?;
    pane_location(b, pane, ws, cwd)?;
    let native = a["native_session"].as_str();
    if a["stage"] == "gate" {
        let path = b.directory(root, launch)?.join("shell-exit");
        if fs::read_to_string(path).map_err(failure)?.trim() != "0" {
            return Err(failure(
                "gate shell did not exit successfully; proof alone is not process exit",
            ));
        }
        if agent(b, pane)?.is_some() {
            return Err(failure("gate pane holds an unexpected agent"));
        }
    } else {
        let native =
            native.ok_or_else(|| failure("cannot stop writer without bound native identity"))?;
        let n = name(a)?;
        // Idle and done are ready states; any dialog remains an exception.
        if let Some(current) = agent(b, pane)? {
            if !owns_agent(&current, &n, native, pane) {
                return Err(failure("native identity changed before stop"));
            }
            b.herdr(&[
                "agent",
                "wait",
                &n,
                "--until",
                "idle",
                "--until",
                "done",
                "--timeout",
                "120000",
            ])?;
            for keys in [vec!["esc"], vec!["ctrl+c", "ctrl+c"]] {
                let current = agent(b, pane)?.ok_or_else(|| {
                    failure("owned agent vanished during exit sequence; inspect shell")
                })?;
                if !owns_agent(&current, &n, native, pane)
                    || !matches!(current["agent_status"].as_str(), Some("idle" | "done"))
                {
                    return Err(failure(
                        "native identity or ready state changed before exit input",
                    ));
                }
                same_generation(b, root, g)?;
                let mut args = vec!["pane", "send-keys", pane];
                args.extend(keys);
                b.herdr(&args)?;
            }
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while let Some(current) = agent(b, pane)? {
            if !owns_agent(&current, &n, native, pane) {
                return Err(failure("replacement native identity appeared during stop"));
            }
            if std::time::Instant::now() >= deadline {
                // Read the screen for evidence, but a controller never confirms an exit dialog.
                b.herdr(&["pane", "read", pane, "--source", "visible", "--lines", "30"])?;
                return Err(failure(
                    "agent still present after exit request; inspect dialog, no Enter sent",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let foreground = shell(b, pane)?;
    let process_scan = (|| -> Result<Value> {
        let scan = scan_stop(b, root, ws, cwd, pane, &foreground)?;
        if !array(&scan, "session_remaining")?.is_empty()
            || !array(&scan, "checkout_remaining")?.is_empty()
        {
            return Err(failure(
                "process-stop-incomplete: unexplained session/descendant or checkout cwd process remains",
            ));
        }
        if shell(b, pane)? != foreground || agent(b, pane)?.is_some() {
            return Err(failure(
                "process-stop-identity: bound shell/native absence changed during scan",
            ));
        }
        Ok(scan)
    })()?;
    // Foreground shell and absence are both fresh observations, not a result-settlement inference.
    same_generation(b, root, g)?;
    if shell(b, pane)? != foreground || agent(b, pane)?.is_some() {
        return Err(failure(
            "process-stop-identity: native absence or shell foreground changed before stopped observation",
        ));
    }
    b.change("stopped",launch,root,g,Some(json!({"pane":pane,"native_session":native,"foreground":foreground,"agent_absent":true,"process_scan":process_scan,"observed_at":timestamp()})))?;
    Ok(())
}
fn scan_stop(
    b: &impl Backend,
    root: i64,
    ws: &str,
    cwd: &str,
    pane: &str,
    foreground: &Value,
) -> Result<Value> {
    let started = std::time::Instant::now();
    if agent(b, pane)?.is_some() {
        return Err(failure(
            "process-stop-identity: process scan requires native absence first",
        ));
    }
    let trust = process_trust(b, root, ws)?;
    if started.elapsed() > Duration::from_secs(30) {
        return Err(failure(
            "process-stop-incomplete: process observation deadline",
        ));
    }
    let scan = b
        .process_scan(
            Path::new(cwd),
            i32::try_from(number(foreground, "pid")?).map_err(failure)?,
            &trust,
        )
        .map_err(|e| failure(format!("process-stop-incomplete: {}", e.message)))?;
    let fresh_trust = process_trust(b, root, ws)?;
    if shell(b, pane)? != *foreground || agent(b, pane)?.is_some() {
        return Err(failure(
            "process-stop-identity: bound native absence or shell foreground changed during scan",
        ));
    }
    if !fresh_trust.same_basis(&trust) {
        return Err(failure(
            "process-stop-incomplete: campaign root or workspace shell observation changed during scan",
        ));
    }
    if started.elapsed() > Duration::from_secs(30) {
        return Err(failure(
            "process-stop-incomplete: process observation deadline",
        ));
    }
    Ok(scan)
}
pub(super) fn inspect_stop(b: &impl Backend, work: i64, launch: i64) -> Value {
    let observation = (|| -> Result<(Value, bool)> {
        let item = b.read("show", work)?;
        if item["work_id"] != work {
            return Err(failure("diagnostic work identity mismatch"));
        }
        let run = array(&item, "runs")?
            .iter()
            .find(|run| {
                run["assignments"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|a| a["launch_id"] == launch))
            })
            .ok_or_else(|| failure("launch does not belong to selected work"))?;
        let a = assignment(run, launch)?;
        if a["run_id"] != run["run_id"] {
            return Err(failure("diagnostic launch run mismatch"));
        }
        let revision = array(&item, "revisions")?
            .iter()
            .find(|r| r["revision"] == run["revision"])
            .ok_or_else(|| failure("immutable launch revision missing"))?;
        let c: api::Contract =
            serde_json::from_value(revision["contract"].clone()).map_err(failure)?;
        if c.checkout.host != store::local_machine() {
            return Err(failure("diagnostic requires local checkout host"));
        }
        let ws = field(&a["dispatch"], "workspace")?;
        let pane = field(&a["dispatch"], "pane")?;
        pane_location(b, pane, ws, &c.checkout.cwd)?;
        let foreground = shell(b, pane)?;
        let scan = scan_stop(
            b,
            number(&item, "campaign")?,
            ws,
            &c.checkout.cwd,
            pane,
            &foreground,
        )?;
        let remaining = !array(&scan, "session_remaining")?.is_empty()
            || !array(&scan, "checkout_remaining")?.is_empty();
        Ok((scan, remaining))
    })();
    match observation {
        Ok((scan, remaining)) => {
            json!({"work_id":work,"launch_id":launch,"complete":true,"remaining":remaining,"error":null,"process_scan":scan})
        }
        Err(e) => {
            json!({"work_id":work,"launch_id":launch,"complete":false,"remaining":null,"error":e.message,"process_scan":null})
        }
    }
}
#[allow(clippy::too_many_arguments)] // Include the complete recorded topology in the on-disk brief.
fn brief_text(
    root: i64,
    work: &Value,
    c: &api::Contract,
    run: &Value,
    a: &Value,
    ws: &str,
    tab: &str,
    pane: &str,
    report: &Path,
) -> Result<String> {
    let gate = a["stage"] == "gate";
    if gate {
        return Err(failure("shell gate does not receive a model brief"));
    }
    let executor = a["stage"] == "executor";
    let role = if executor { "implementer" } else { "reviewer" };
    let launch = number(a, "launch_id")?;
    let task = number(a, "task_id")?;
    let id = number(work, "work_id")?;
    let result = report.with_file_name("result.json");
    let schema = if executor {
        json!({"kind":"execution","result":{"source":{"commit":"actual committed HEAD","tree":"actual HEAD tree","input_digest":c.base.input_digest},"report":report,"artifacts":[]}})
    } else if a["stage"] == "reviewer" {
        json!({"kind":"review","result":{"source":"exact execution source below","execution_result":"execution result_id below","verdict":"clear|rework|disputed","findings":[{"id":"stable-id","reason":"concrete defect","evidence":"file:line and scenario"}]}})
    } else {
        json!({"kind":"adjudication","result":{"source":"exact reviewed source below","review_result":"review result_id below","finding_ids":["disputed stable IDs"],"verdict":"clear|rework|disputed","reason":"source-grounded adjudication"}})
    };
    Ok(format!(
        "HERDR-BRIEF role={role}\norchestrator=taskr:{root} report={}\nTask topology: workspace={ws} tab={tab} pane={pane} cwd={}\nObjective: {}\nAcceptance: {}\nOwnership: {}\nAuthority: {}\nValidation: approved deterministic profile only; controller runs checks. {}\nTask identity: work={id} revision={} run={} round={} task={task} launch={launch}\nSource/profile: {}\nResults and prior findings: {}\nResult JSON schema: {}\nProcess boundary: never detach background jobs or create external runtimes. A task needing these requires a named managed runtime and stronger explicit cleanup proof; no automatic handoff under this local profile. The same-user local session/descendant/cwd scan is cooperative evidence, not a sandbox or proof about another UID, containers/remotes, any detached process with cwd outside the checkout (including absolute-path writers with no retained descriptor), or a hostile same-user writer. Model agreement cannot certify containment.\nProgress: write report at report=, write UTF-8 result JSON to {}, then taskr work submit {launch} --file {} && taskr ready 'work result submitted' --report {} && taskr done 'work result submitted'. Use taskr work source {id} for canonical actual source identity. Executor must commit clean owned source before submission. Reviewer/adjudicator is read-only. Never publish, launch helpers, accept yourself, change the contract or weaken checks. Missing consequential decision: taskr ask TEXT --blocking, stop dependent work, taskr wait --for answer, then ack its event.\n",
        report.display(),
        c.checkout.cwd,
        c.intent,
        c.acceptance,
        serde_json::to_string(&c.write_scope).map_err(failure)?,
        c.authority,
        if executor {
            "Implement only the approved scope."
        } else {
            "Independently review the exact source and actual machine proof; preserve stable finding IDs."
        },
        work["revision"],
        run["run_id"],
        run["round"],
        serde_json::to_string(c).map_err(failure)?,
        run["results"],
        schema,
        result.display(),
        quote(&result.to_string_lossy()),
        quote(&report.to_string_lossy())
    ))
}
