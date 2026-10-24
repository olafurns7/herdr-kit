use super::*;

pub fn configuration(db: &Connection, campaign: i64) -> Result<Value> {
    type ConfigurationRow = (bool, u32, u32, Option<String>, Option<String>, bool);
    let row:Option<ConfigurationRow>=db.query_row("select enabled,max_active,max_review_pending,workspace,host,no_slotr from work_campaigns where root_id=?",[campaign],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let (enabled, max_active, max_review_pending, workspace, host, no_slotr) =
        row.unwrap_or((false, 1, 1, None, None, false));
    Ok(
        json!({"enabled":enabled,"max_active":max_active,"max_review_pending":max_review_pending,"workspace":workspace,"host":host,"no_slotr":no_slotr}),
    )
}
/// Admitted read on the authenticated caller host. Never substitutes a root/worker identity.
pub fn campaigns(db: &Connection) -> Result<Value> {
    const LIMIT: usize = 1000;
    let mut s=db.prepare("select t.id from work_campaigns c join tasks t on t.id=c.root_id where c.enabled=1 and t.status!='closed' and t.parent_id is null and t.current_launch_id is null and t.machine is ? order by t.id limit 1001")?;
    let mut ids = s
        .query_map([caller_machine()], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let truncated = ids.len() > LIMIT;
    ids.truncate(LIMIT);
    let rows=ids.into_iter().map(|id|Ok(json!({"campaign":id,"host":machine_name(task(db,id)?.machine.as_deref()),"generation":delivery::generation(db,id)?,"parked":parked(db,id)?,"configuration":configuration(db,id)?}))).collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"host":machine_name(caller_machine().as_deref()),"limit":LIMIT,"returned":rows.len(),"truncated":truncated,"campaigns":rows}),
    )
}
/// Thin nonterminal scan. The bound also keeps worst-case escaped reasons within the RPC envelope.
pub fn dispatch(db: &Connection, campaign: i64, limit: u32) -> Result<Value> {
    if !(1..=64).contains(&limit) {
        return Err(usage("dispatch limit must be 1..64"));
    }
    let mut s=db.prepare("select w.id,w.revision,w.state,w.priority,w.current_run,w.reason from work_items w where w.root_id=? and (w.state in ('queued','active','verifying') or w.state='cancelled' and exists(select 1 from work_runs r where r.id=w.current_run and r.work_id=w.id and r.state='active')) order by case when w.state='queued' then 1 else 0 end,w.priority desc,w.id limit ?")?;
    let mut rows=s.query_map(params![campaign,limit+1],|r|Ok(json!({"work_id":r.get::<_,i64>(0)?,"revision":r.get::<_,i64>(1)?,"state":r.get::<_,String>(2)?,"priority":r.get::<_,i32>(3)?,"current_run":r.get::<_,Option<i64>>(4)?,"reason":r.get::<_,Option<String>>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let truncated = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    Ok(
        json!({"campaign":campaign,"limit":limit,"returned":rows.len(),"truncated":truncated,"work":rows}),
    )
}

pub fn block(db: &mut Connection, a: Authority, id: i64, input: Reason) -> Result<Value> {
    text(&input.reason, 4096, "block reason")?;
    transaction(db, |tx| {
        let w = item(tx, id)?;
        authority(tx, a, w.root, false)?;
        let prior: Option<String> =
            tx.query_row("select reason from work_items where id=?", [id], |r| {
                r.get(0)
            })?;
        if prior.as_deref() == Some(&input.reason) {
            let mut out = show(tx, id)?;
            out["duplicate"] = json!(true);
            return Ok(out);
        }
        tx.execute(
            "update work_items set reason=? where id=?",
            params![input.reason, id],
        )?;
        audit(tx, w.root, "blocked", Some(id), w.run, None, true)?;
        show(tx, id)
    })
}
fn observation_time(db: &Connection, launch: i64, at: &str) -> Result<()> {
    let time = parse_time(at).ok_or_else(|| usage("observed_at must be RFC3339"))?;
    let recorded: String = db.query_row(
        "select recorded_at from launches where id=?",
        [launch],
        |r| r.get(0),
    )?;
    let start =
        parse_time(&recorded).ok_or_else(|| reject("launch has invalid recorded timestamp"))?;
    let now = frozen_now().expect("clock");
    if time < start || time > now || now - time > time::Duration::seconds(60) {
        return Err(reject("stale/future process observation"));
    }
    Ok(())
}
/// Pins launcher-observed native identity before prompting; does not establish start.
pub fn observe(db: &mut Connection, a: Authority, launch: i64, input: Started) -> Result<Value> {
    native_observation(db, a, launch, input, false)
}
pub fn started_observed(
    db: &mut Connection,
    a: Authority,
    launch: i64,
    input: Started,
) -> Result<Value> {
    native_observation(db, a, launch, input, true)
}
fn native_observation(
    db: &mut Connection,
    a: Authority,
    launch: i64,
    input: Started,
    start: bool,
) -> Result<Value> {
    encoded(&input)?;
    location_id(&input.pane)?;
    text(&input.native_session, 256, "native session")?;
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let (r, _, _) = current_run(tx, ass["run_id"].as_i64().unwrap())?;
        authority(tx, a, r.root, false)?;
        bound_host(tx, launch)?;
        if ass["stage"] == "gate" {
            return Err(reject("shell gate has no native session"));
        }
        if ass["dispatch"]["pane"] != input.pane {
            return Err(reject("native observation pane differs from dispatch"));
        }
        if ass["process_state"] == "stopped" {
            return Err(reject("cannot bind native identity after stopped"));
        }
        let (recorded, pane): (Option<String>, Option<String>) = tx.query_row(
            "select session_ref,pane_id from launches where id=?",
            [launch],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if task(tx, ass["task_id"].as_i64().unwrap())?.launch != Some(launch)
            || pane.as_deref() != Some(input.pane.as_str())
        {
            return Err(reject(
                "native observation differs from current launch/pane",
            ));
        }
        if (ass["process_state"] == "running" || ass["process_state"] == "uncertain")
            && recorded.as_deref() != Some(input.native_session.as_str())
        {
            return Err(reject(
                "late native binding requires exact stored launch session",
            ));
        }
        if recorded
            .as_deref()
            .is_some_and(|s| s != input.native_session)
            || ass["native_session"]
                .as_str()
                .is_some_and(|s| s != input.native_session)
        {
            return Err(reject("native identity changed"));
        }
        observation_time(tx, launch, &input.observed_at)?;
        if start {
            if ass["process_state"] != "uncertain" {
                start_tx(tx, launch)?;
            }
        } else {
            if ass["process_state"] != "starting"
                && ass["native_session"].as_str() != Some(input.native_session.as_str())
            {
                return Err(reject(
                    "initial native observation requires bound starting launch",
                ));
            }
            tx.execute("update launches set session_kind=case when session_ref is null then case when provider='codex' then 'thread_id' else 'id' end else session_kind end,session_source=case when session_ref is null then 'work:observe' else session_source end,session_ref=coalesce(session_ref,?) where id=?",params![input.native_session,launch])?;
        }
        tx.execute("update work_assignments set native_session=?,identity_observation=coalesce(identity_observation,?) where launch_id=?",params![input.native_session,encoded(&input)?,launch])?;
        assignment(tx, launch)
    })
}
pub(super) fn all_stopped(db: &Connection, run: i64) -> Result<bool> {
    Ok(!db.query_row(
        "select exists(select 1 from work_assignments where run_id=? and process_state!='stopped')",
        [run],
        |r| r.get::<_, bool>(0),
    )?)
}
pub(super) fn release_cancelled(db: &Connection, r: &Run) -> Result<()> {
    let w = item(db, r.work)?;
    if r.state == "active"
        && w.run == Some(r.id)
        && w.state == "cancelled"
        && all_stopped(db, r.id)?
    {
        db.execute("delete from work_checkouts where run_id=?", [r.id])?;
        db.execute(
            "update work_runs set state='cancelled',action='escalation' where id=?",
            [r.id],
        )?;
    }
    Ok(())
}
fn process_ids(pids: &[u32]) -> Result<()> {
    let unique: std::collections::BTreeSet<_> = pids.iter().copied().collect();
    if pids.is_empty() || pids.len() > 1024 || unique.contains(&0) || unique.len() != pids.len() {
        return Err(usage(
            "process evidence requires 1..1024 positive unique PIDs",
        ));
    }
    Ok(())
}
fn process_scan(db: &Connection, root: i64, launch: i64, input: &Stopped) -> Result<()> {
    let scan = &input.process_scan;
    let foreground = &input.foreground;
    if scan.shell_pid == 0
        || scan.shell_pid != foreground.pid
        || scan.session_id == 0
        || !(1..=65536).contains(&scan.scanned)
        || scan.deleted_cwd > scan.scanned
        || scan.session_remaining.len() > 1024
        || scan.checkout_remaining.len() > 1024
        || scan.exempt.len() > 1024
    {
        return Err(usage("invalid process scan identity/count bounds"));
    }
    for process in &scan.session_remaining {
        if process.pid == 0 {
            return Err(usage("process scan PID must be positive"));
        }
        text(&process.command, 4096, "process command")?;
    }
    for process in &scan.checkout_remaining {
        if process.pid == 0 {
            return Err(usage("checkout process PID must be positive"));
        }
        text(&process.cwd, 4096, "process cwd")?;
    }
    let workspace: Option<String> = db.query_row(
        "select workspace_id from launches where id=?",
        [launch],
        |r| r.get(0),
    )?;
    let mut exempt = std::collections::BTreeSet::new();
    for process in &scan.exempt {
        let pid = match process {
            ProcessExemption::WorkspaceShell {
                pid,
                pane,
                session_id,
                foreground_pgid,
                session_pids,
            } => {
                location_id(pane)?;
                if pane.split_once(':').map(|(workspace, _)| workspace) != workspace.as_deref()
                    || *session_id == 0
                    || *foreground_pgid == 0
                    || session_pids.as_slice() != [*pid]
                    || (pane == &input.pane
                        && (*pid != foreground.pid || *session_id != scan.session_id))
                    || (*pid == foreground.pid && pane != &input.pane)
                {
                    return Err(reject(
                        "workspace shell exemption identity/singleton mismatch",
                    ));
                }
                *pid
            }
            ProcessExemption::Observer {
                pid,
                observer_pid,
                parent_chain,
            } => {
                process_ids(parent_chain)?;
                if *observer_pid == 0
                    || parent_chain.first() != Some(pid)
                    || parent_chain.last() != Some(observer_pid)
                {
                    return Err(reject("observer exemption parent chain mismatch"));
                }
                *pid
            }
            ProcessExemption::CampaignRoot {
                pid,
                pane,
                foreground_pgid,
                foreground_pids,
                observed_at,
            } => {
                location_id(pane)?;
                process_ids(foreground_pids)?;
                if *foreground_pgid == 0
                    || !foreground_pids.contains(pid)
                    || pane != &task(db, root)?.pane
                    || pane == &input.pane
                {
                    return Err(reject("campaign root exemption pane/foreground mismatch"));
                }
                observation_time(db, launch, observed_at)?;
                *pid
            }
        };
        if pid == 0 || !exempt.insert(pid) {
            return Err(usage(
                "process scan exemption PIDs must be positive and unique",
            ));
        }
    }
    if !scan.session_remaining.is_empty() || !scan.checkout_remaining.is_empty() {
        return Err(reject(
            "process scan reports remaining session/checkout processes",
        ));
    }
    Ok(())
}
pub fn stopped(db: &mut Connection, a: Authority, launch: i64, input: Stopped) -> Result<Value> {
    encoded(&input)?;
    location_id(&input.pane)?;
    text(&input.foreground.command, 4096, "foreground command")?;
    if !input.agent_absent
        || input.foreground.pid == 0
        || !matches!(
            std::path::Path::new(&input.foreground.command)
                .file_name()
                .and_then(|p| p.to_str()),
            Some("sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "nu")
        )
    {
        return Err(reject(
            "stop requires agent absence and observed interactive shell foreground",
        ));
    }
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let r = run_row(tx, ass["run_id"].as_i64().unwrap())?;
        authority(tx, a, r.root, false)?;
        bound_host(tx, launch)?;
        if task(tx, ass["task_id"].as_i64().unwrap())?.launch != Some(launch)
            || ass["dispatch"]["pane"] != input.pane
        {
            return Err(reject("stop pane/launch differs from immutable dispatch"));
        }
        let expected = ass["native_session"].as_str();
        let current_native: Option<String> = tx.query_row(
            "select session_ref from launches where id=?",
            [launch],
            |r| r.get(0),
        )?;
        if current_native
            .as_deref()
            .is_some_and(|s| Some(s) != expected)
        {
            return Err(reject("native launch identity changed before stop"));
        }
        if ass["stage"] != "gate" && expected.is_none() {
            return Err(reject(
                "explicit native identity observation is required before stop",
            ));
        }
        if expected != input.native_session.as_deref() {
            return Err(reject("stop native identity mismatch"));
        }
        if let Some(event) = ass["stop_event"].as_i64() {
            let raw: String =
                tx.query_row("select data from events where id=?", [event], |r| r.get(0))?;
            let v: Value = serde_json::from_str(&raw)?;
            if v["observation"] != serde_json::to_value(&input)? {
                return Err(reject("stop receipt is immutable"));
            }
            return assignment(tx, launch);
        }
        process_scan(tx, r.root, launch, &input)?;
        observation_time(tx, launch, &input.observed_at)?;
        if ass["process_state"] == "reserved" {
            return Err(reject("undispatched reservation has no process to observe"));
        }
        let event = event(
            tx,
            Event {
                task: ass["task_id"].as_i64().unwrap(),
                launch: Some(launch),
                kind: "work_process_stopped",
                data: Some(json!({"run":r.id,"origin":"controller","observation":input})),
                ..Event::default()
            },
        )?;
        tx.execute(
            "update work_assignments set process_state='stopped',stop_event=? where launch_id=?",
            params![event, launch],
        )?;
        release_cancelled(tx, &r)?;
        assignment(tx, launch)
    })
}

pub(super) fn bound_host(db: &Connection, launch: i64) -> Result<()> {
    let host: Option<String> =
        db.query_row("select machine from launches where id=?", [launch], |r| {
            r.get(0)
        })?;
    if host != caller_machine() {
        return Err(reject(
            "process observation/dispatch requires the reserved launch host",
        ));
    }
    Ok(())
}
