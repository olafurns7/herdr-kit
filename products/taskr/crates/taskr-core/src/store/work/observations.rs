use super::*;

pub fn configuration(db: &Connection, campaign: i64) -> Result<Value> {
    let row:Option<(bool,u32,u32,Option<String>,Option<String>,bool)>=db.query_row("select enabled,max_active,max_review_pending,workspace,host,no_slotr from work_campaigns where root_id=?",[campaign],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let (enabled, max_active, max_review_pending, workspace, host, no_slotr) =
        row.unwrap_or((false, 1, 1, None, None, false));
    Ok(
        json!({"enabled":enabled,"max_active":max_active,"max_review_pending":max_review_pending,"workspace":workspace,"host":host,"no_slotr":no_slotr}),
    )
}
/// Admitted read on the authenticated caller host. Never substitutes a root/worker identity.
pub fn campaigns(db: &Connection) -> Result<Value> {
    let mut s=db.prepare("select t.id from work_campaigns c join tasks t on t.id=c.root_id where c.enabled=1 and t.status!='closed' and t.parent_id is null and t.current_launch_id is null and t.machine is ? order by t.id")?;
    let ids = s
        .query_map([caller_machine()], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rows = ids
        .into_iter()
        .map(|id| list(db, id))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"host":machine_name(caller_machine().as_deref()),"campaigns":rows}))
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
pub fn started_observed(
    db: &mut Connection,
    a: Authority,
    launch: i64,
    input: Started,
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
        if ass["process_state"] == "uncertain" || ass["process_state"] == "stopped" {
            return Err(reject(
                "cannot bind native identity after uncertain/stopped",
            ));
        }
        let recorded: Option<String> = tx.query_row(
            "select session_ref from launches where id=?",
            [launch],
            |r| r.get(0),
        )?;
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
        start_tx(tx, launch)?;
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
