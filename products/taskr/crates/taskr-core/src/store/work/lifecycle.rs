use super::*;

pub(super) struct Run {
    pub id: i64,
    pub work: i64,
    pub revision: i64,
    pub root: i64,
    pub round: u32,
    pub action: String,
    pub state: String,
    pub deps: String,
}
pub(super) fn run_row(db: &Connection, id: i64) -> Result<Run> {
    db.query_row("select id,work_id,revision,root_id,round,action,state,dependencies from work_runs where id=?",[id],|r|Ok(Run{id:r.get(0)?,work:r.get(1)?,revision:r.get(2)?,root:r.get(3)?,round:r.get(4)?,action:r.get(5)?,state:r.get(6)?,deps:r.get(7)?})).optional()?.ok_or_else(||reject("run does not exist"))
}
pub(super) fn current_run(db: &Connection, id: i64) -> Result<(Run, Item, Contract)> {
    let r = run_row(db, id)?;
    let w = item(db, r.work)?;
    if w.run != Some(id)
        || w.revision != r.revision
        || w.state == "cancelled"
        || r.state != "active"
    {
        return Err(reject("stale or terminal work run"));
    }
    let c = contract(db, &w)?;
    Ok((r, w, c))
}
pub(super) fn assignment(db: &Connection, launch: i64) -> Result<Value> {
    let (run,stage,round,lane,state,snapshot,dispatch,proof):(i64,String,u32,i64,String,String,Option<String>,Option<String>)=db.query_row("select run_id,stage,round,lane_id,state,lane_snapshot,dispatch,start_proof from work_assignments where launch_id=?",[launch],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?.ok_or_else(||reject("work assignment does not exist"))?;
    let (process_state,native_session,stop_event):(String,Option<String>,Option<i64>)=db.query_row("select process_state,native_session,stop_event from work_assignments where launch_id=?",[launch],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let identity: Option<String> = db.query_row(
        "select identity_observation from work_assignments where launch_id=?",
        [launch],
        |r| r.get(0),
    )?;
    let stop: Option<String> = stop_event
        .map(|e| {
            db.query_row("select data from events where id=?", [e], |r| {
                r.get::<_, String>(0)
            })
        })
        .transpose()?;
    Ok(
        json!({"identity_observation":identity.map(|s|serde_json::from_str::<Value>(&s)).transpose()?,"stopped":stop.map(|s|serde_json::from_str::<Value>(&s)).transpose()?,"process_state":process_state,"native_session":native_session,"stop_event":stop_event,"launch_id":launch,"run_id":run,"stage":stage,"round":round,"task_id":lane,"state":state,"lane":serde_json::from_str::<Value>(&snapshot)?,"dispatch":dispatch.map(|s|serde_json::from_str::<Value>(&s)).transpose()?,"start_proof":proof.map(|s|serde_json::from_str::<Value>(&s)).transpose()?}),
    )
}
pub(super) fn run_output(db: &Connection, id: i64) -> Result<Value> {
    let r = run_row(db, id)?;
    let mut s = db.prepare(
        "select launch_id from work_assignments where run_id=? order by round,launch_id",
    )?;
    let assignments = s
        .query_map([id], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|l| assignment(db, l))
        .collect::<Result<Vec<_>>>()?;
    let mut s=db.prepare("select r.id,r.launch_id,r.digest,r.payload from work_results r join work_assignments a on a.launch_id=r.launch_id where a.run_id=? order by r.id")?;
    let rows = s
        .query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let results=rows.into_iter().map(|(id,l,d,p)|Ok(json!({"result_id":id,"launch_id":l,"digest":d,"payload":serde_json::from_str::<Value>(&p)?}))).collect::<Result<Vec<_>>>()?;
    let acceptance: Option<i64> = db
        .query_row(
            "select id from work_acceptances where run_id=?",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(
        json!({"ok":true,"work_id":r.work,"revision":r.revision,"run_id":id,"round":r.round,"state":r.state,"action":r.action,"assignments":assignments,"results":results,"acceptance_id":acceptance}),
    )
}
fn lane(c: &Contract, stage: Stage) -> Result<Lane> {
    Ok(match stage {
        Stage::Gate => Lane {
            task: None,
            name: "gate".into(),
            provider: "shell".into(),
            model: "none".into(),
            effort: "none".into(),
        },
        Stage::Executor => c.executor.clone(),
        Stage::Reviewer => c.reviewer.clone(),
        Stage::Adjudicator => c
            .adjudicator
            .clone()
            .ok_or_else(|| reject("no adjudicator configured; escalate"))?,
    })
}
fn reserve_tx(db: &Connection, r: &Run, c: &Contract, stage: Stage) -> Result<Value> {
    let old: Option<i64> = db
        .query_row(
            "select launch_id from work_assignments where run_id=? and stage=? and round=?",
            params![r.id, stage.name(), r.round],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(l) = old {
        return assignment(db, l);
    }
    let allowed = match stage {
        Stage::Executor => matches!(r.action.as_str(), "reserve" | "rework"),
        Stage::Gate => r.action == "check",
        Stage::Reviewer => r.action == "review",
        Stage::Adjudicator => r.action == "adjudicate",
    };
    if !allowed {
        return Err(reject("stage not authorized by durable advance action"));
    }
    if c.checkout.host != machine_name(caller_machine().as_deref()) {
        return Err(reject("checkout/campaign host mismatch"));
    }
    let l = lane(c, stage)?;
    let live: bool = db.query_row(
        "select exists(select 1 from work_assignments where run_id=? and process_state!='stopped')",
        [r.id],
        |r| r.get(0),
    )?;
    if live {
        return Err(reject(
            "previous stage process must stop before reservation",
        ));
    }
    let task_id = if let Some(id) = l.task {
        id
    } else {
        let role = stage.ledger_role();
        db.execute("insert into tasks(parent_id,name,role,status,cwd,created_at,updated_at) values(?,?,?,'planned',?,?,?)", params![r.root,format!("work-{}-{}-{}",r.work,stage.name(),r.round),role,c.checkout.cwd,now(),now()])?;
        db.last_insert_rowid()
    };
    let t = open_task(db, task_id)?;
    if root(db, t.id)? != r.root {
        return Err(reject("lane is no longer in campaign"));
    }
    let busy: bool = db.query_row(
        "select exists(select 1 from work_assignments a join work_runs r on r.id=a.run_id where a.lane_id=? and (a.process_state!='stopped' or r.state='active' and a.run_id!=?))",
        params![t.id,r.id],
        |r| r.get(0),
    )?;
    if busy {
        return Err(reject("lane has a live work assignment"));
    }
    if let Some(old) = t.launch {
        let settled: bool = db.query_row(
            "select exists(select 1 from work_assignments where launch_id=? and process_state='stopped')",
            [old],
            |r| r.get(0),
        )?;
        if !settled {
            return Err(reject(
                "legacy launch requires explicit reconciliation before reuse",
            ));
        }
    }
    let host = orch::machine(db, &c.checkout.host, true, None)?;
    db.execute("insert into launches(task_id,provider,model,effort,machine,recorded_at) values(?,?,?,?,?,?)",params![t.id,l.provider,l.model,l.effort,host,now()])?;
    let launch = db.last_insert_rowid();
    delete_receipts(db, t.id)?;
    db.execute("update tasks set current_launch_id=?,status='open',cwd=?,machine=?,pane_id=null,tab_id=null,workspace_id=null,updated_at=? where id=?",params![launch,c.checkout.cwd,host,now(),t.id])?;
    db.execute("insert into work_assignments(launch_id,run_id,stage,round,lane_id,state,lane_snapshot) values(?,?,?,?,?,'reserved',?)",params![launch,r.id,stage.name(),r.round,t.id,encoded(&l)?])?;
    audit(
        db,
        r.root,
        "reserved",
        Some(r.work),
        Some(r.id),
        None,
        false,
    )?;
    assignment(db, launch)
}
pub fn claim(db: &mut Connection, a: Authority, id: i64, input: Claim) -> Result<Value> {
    encoded(&input)?;
    token(&input.key, "idempotency key")?;
    transaction(db, |tx| {
        let w = item(tx, id)?;
        authority(tx, a, w.root, false)?;
        let raw = encoded(&json!({"work":id,"input":input}))?;
        let old: Option<(i64, String)> = tx
            .query_row(
                "select id,claim_input from work_runs where root_id=? and claim_key=?",
                params![w.root, input.key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((run, prior)) = old {
            if prior != raw {
                return Err(reject("claim key reused with different input"));
            }
            return run_output(tx, run);
        }
        expected(&w, input.expected_revision)?;
        let config: Option<(bool, u32, u32)> = tx
            .query_row(
                "select enabled,max_active,max_review_pending from work_campaigns where root_id=?",
                [w.root],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((true, max_active, max_review)) = config else {
            return Err(reject("campaign not enabled"));
        };
        if parked(tx, w.root)? {
            return Err(reject("campaign is parked"));
        }
        let (active,review):(u32,u32)=tx.query_row("select count(*),coalesce(sum(w.state='verifying'),0) from work_runs r join work_items w on w.current_run=r.id where r.root_id=? and r.state='active'",[w.root],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if active >= max_active || review >= max_review {
            return Err(reject("campaign concurrency limit reached"));
        }
        if w.state != "queued" {
            return Err(reject("work is not queued"));
        }
        let c = contract(tx, &w)?;
        let config = configuration(tx, w.root)?;
        if config["host"] != c.checkout.host
            || machine_name(task(tx, w.root)?.machine.as_deref()) != c.checkout.host
        {
            return Err(reject("checkout/campaign host mismatch"));
        }
        validate_contract(tx, w.root, Some(id), &c)?;
        let deps = dependencies(tx, &c)?;
        let reserved: bool = tx.query_row(
            "select exists(select 1 from work_checkouts where host=? and cwd=?)",
            params![c.checkout.host, c.checkout.cwd],
            |r| r.get(0),
        )?;
        if reserved {
            return Err(reject("checkout already has a work writer reservation"));
        }
        tx.execute("insert into work_runs(work_id,revision,root_id,generation,claim_key,claim_input,dependencies,action,created_at) values(?,?,?,?,?,?,?,'reserve',?)",params![id,w.revision,w.root,a.generation,input.key,raw,deps,now()])?;
        let run = tx.last_insert_rowid();
        tx.execute(
            "insert into work_checkouts(host,cwd,run_id) values(?,?,?)",
            params![c.checkout.host, c.checkout.cwd, run],
        )?;
        tx.execute(
            "update work_items set state='active',current_run=?,reason=null where id=?",
            params![run, id],
        )?;
        reserve_tx(tx, &run_row(tx, run)?, &c, Stage::Executor)?;
        audit(tx, w.root, "claimed", Some(id), Some(run), None, false)?;
        run_output(tx, run)
    })
}
pub fn reserve(db: &mut Connection, a: Authority, run: i64, input: Reserve) -> Result<Value> {
    transaction(db, |tx| {
        let (r, _, c) = current_run(tx, run)?;
        authority(tx, a, r.root, false)?;
        reserve_tx(tx, &r, &c, input.stage)
    })
}
pub fn bind(db: &mut Connection, a: Authority, launch: i64, input: Bind) -> Result<Value> {
    encoded(&input)?;
    token(&input.dispatch_key, "dispatch key")?;
    for v in [&input.workspace, &input.tab, &input.pane] {
        if v.is_empty()
            || v == "null"
            || v.len() > 128
            || !v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":_-".contains(&b))
        {
            return Err(usage("invalid dispatch location"));
        }
    }
    for v in [&input.tab, &input.pane] {
        if v.split_once(':').is_some_and(|(p, _)| p != input.workspace) {
            return Err(usage("dispatch workspace prefix mismatch"));
        }
    }
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let run = ass["run_id"].as_i64().unwrap();
        let (r, _, _) = current_run(tx, run)?;
        authority(tx, a, r.root, false)?;
        bound_host(tx, launch)?;
        let config = configuration(tx, r.root)?;
        if config["workspace"] != input.workspace {
            return Err(reject(
                "dispatch workspace differs from campaign configuration",
            ));
        }
        let tid = ass["task_id"].as_i64().unwrap();
        if task(tx, tid)?.launch != Some(launch) {
            return Err(reject("assignment launch replaced"));
        }
        let raw = encoded(&input)?;
        if ass["state"] != "reserved" {
            if ass["dispatch"] == serde_json::to_value(&input)? {
                return Ok(ass);
            }
            return Err(reject(
                "dispatch intent is immutable; reconcile before another start",
            ));
        }
        tx.execute(
            "update work_assignments set state='starting',process_state='starting',dispatch=? where launch_id=?",
            params![raw, launch],
        )?;
        tx.execute(
            "update launches set workspace_id=?,tab_id=?,pane_id=? where id=?",
            params![input.workspace, input.tab, input.pane, launch],
        )?;
        tx.execute(
            "update tasks set workspace_id=?,tab_id=?,pane_id=?,updated_at=? where id=?",
            params![input.workspace, input.tab, input.pane, now(), tid],
        )?;
        event(
            tx,
            Event {
                task: tid,
                launch: Some(launch),
                kind: "launch",
                summary: "work dispatch bound",
                data: Some(
                    json!({"launch_id":launch,"old":{},"new":{"workspace_id":input.workspace,"tab_id":input.tab,"pane_id":input.pane},"dispatch_key":input.dispatch_key}),
                ),
                ..Event::default()
            },
        )?;
        assignment(tx, launch)
    })
}
pub(super) fn start_tx(db: &Connection, launch: i64) -> Result<Value> {
    let ass = assignment(db, launch)?;
    if ass["process_state"] == "running" {
        return Ok(ass);
    }
    if ass["process_state"] != "starting" {
        return Err(reject(
            "assignment must be starting; uncertain starts need stopped evidence",
        ));
    }
    if task(db, ass["task_id"].as_i64().unwrap())?.launch != Some(launch) {
        return Err(reject("assignment launch replaced"));
    }
    let hook:bool=db.query_row("select session_ref is not null and session_ref!='' and session_source like 'hook:%' from launches where id=?",[launch],|r|r.get(0))?;
    let receipt:Option<i64>=db.query_row("select id from events where launch_id=? and task_id=? and kind in ('got','start') order by id limit 1",params![launch,ass["task_id"].as_i64()],|r|r.get(0)).optional()?;
    let proof = if hook {
        json!({"kind":"hook","launch_id":launch})
    } else if let Some(e) = receipt {
        json!({"kind":"receipt","event_id":e})
    } else {
        return Err(reject("no hook session or launch-bound got/start receipt"));
    };
    db.execute(
        "update work_assignments set state='running',process_state='running',start_proof=? where launch_id=?",
        params![encoded(&proof)?, launch],
    )?;
    if hook {
        db.execute("update work_assignments set native_session=(select session_ref from launches where id=?) where launch_id=?",params![launch,launch])?;
    }
    assignment(db, launch)
}
pub fn started(db: &mut Connection, a: Authority, launch: i64) -> Result<Value> {
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let (r, _, _) = current_run(tx, ass["run_id"].as_i64().unwrap())?;
        authority(tx, a, r.root, false)?;
        bound_host(tx, launch)?;
        start_tx(tx, launch)
    })
}
pub fn uncertain(db: &mut Connection, a: Authority, launch: i64, input: Reason) -> Result<Value> {
    text(&input.reason, 4096, "uncertainty reason")?;
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let r = run_row(tx, ass["run_id"].as_i64().unwrap())?;
        authority(tx, a, r.root, false)?;
        if ass["process_state"] == "stopped" {
            return Err(reject("stopped process cannot become uncertain"));
        }
        let old: Option<String> =
            tx.query_row("select reason from work_items where id=?", [r.work], |r| {
                r.get(0)
            })?;
        if ass["process_state"] == "uncertain" && old.as_deref() == Some(&input.reason) {
            return assignment(tx, launch);
        }
        tx.execute(
            "update work_assignments set process_state='uncertain' where launch_id=?",
            [launch],
        )?;
        tx.execute(
            "update work_items set reason=? where id=?",
            params![input.reason, r.work],
        )?;
        audit(
            tx,
            r.root,
            "uncertain",
            Some(r.work),
            Some(r.id),
            None,
            true,
        )?;
        assignment(tx, launch)
    })
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reconcile {
    pub stopped_event: Option<i64>,
    #[serde(default)]
    pub abandon: bool,
}
/// Stop receipts are produced by the later process controller, never by an executor's prose.
/// Without such a receipt, reconciliation only derives a start from existing receipts.
pub fn reconcile(
    db: &mut Connection,
    a: Authority,
    launch: i64,
    input: Reconcile,
) -> Result<Value> {
    transaction(db, |tx| {
        let ass = assignment(tx, launch)?;
        let r = run_row(tx, ass["run_id"].as_i64().unwrap())?;
        authority(tx, a, r.root, false)?;
        if let Some(e) = input.stopped_event {
            if ass["stop_event"] != e || ass["process_state"] != "stopped" {
                return Err(reject("unproven process stop; checkout remains reserved"));
            }
        } else if ass["process_state"] == "starting" {
            start_tx(tx, launch)?;
        } else if ass["process_state"] == "uncertain" {
            return Err(reject(
                "uncertain process requires an explicit stopped observation",
            ));
        }
        if input.abandon {
            if r.state != "active" || item(tx, r.work)?.run != Some(r.id) {
                return Err(reject("only the current active run can be abandoned"));
            }
            tx.execute("update work_assignments set process_state='stopped' where run_id=? and dispatch is null",[r.id])?;
            if !all_stopped(tx, r.id)? {
                return Err(reject(
                    "abandonment requires all dispatched processes stopped",
                ));
            }
            tx.execute(
                "update work_runs set state='abandoned',action='escalation' where id=?",
                [r.id],
            )?;
            tx.execute("update work_items set state=case when state='cancelled' then state else 'backlog' end,reason='run explicitly abandoned' where id=? and current_run=? and state!='done'",params![r.work,r.id])?;
            tx.execute("delete from work_checkouts where run_id=?", [r.id])?;
        } else {
            release_cancelled(tx, &r)?;
        }
        audit(
            tx,
            r.root,
            "reconciled",
            Some(r.work),
            Some(r.id),
            None,
            false,
        )?;
        assignment(tx, launch)
    })
}
pub fn cancel(db: &mut Connection, a: Authority, id: i64, input: Reason) -> Result<Value> {
    text(&input.reason, 4096, "cancellation reason")?;
    transaction(db, |tx| {
        let w = item(tx, id)?;
        authority(tx, a, w.root, true)?;
        if w.state == "done" {
            return Err(reject("accepted work cannot be cancelled"));
        }
        tx.execute(
            "update work_items set state='cancelled',reason=? where id=?",
            params![input.reason, id],
        )?;
        if let Some(r) = w.run {
            // A reservation with no dispatch intent never started a process.
            tx.execute(
                "update work_assignments set state='settled',process_state='stopped' where run_id=? and dispatch is null",
                [r],
            )?;
            release_cancelled(tx, &run_row(tx, r)?)?;
        }
        invalidate_dependents(tx, id)?;
        audit(
            tx,
            w.root,
            "cancelled",
            Some(id),
            w.run,
            None,
            a.root != w.root,
        )?;
        show(tx, id)
    })
}
pub fn recover(db: &mut Connection, a: Authority, campaign: i64) -> Result<Value> {
    transaction(db, |tx| {
        authority(tx, a, campaign, false)?;
        let mut s=tx.prepare("select a.launch_id from work_assignments a join work_runs r on r.id=a.run_id where r.root_id=? and a.process_state='starting'")?;
        let ids = s
            .query_map([campaign], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for id in ids {
            match start_tx(tx, id) {
                Ok(_) => {}
                Err(e) if e.code == ExitCode::Rejected => {}
                Err(e) => return Err(e),
            }
        }
        audit(tx, campaign, "recovered", None, None, None, false)?;
        list(tx, campaign)
    })
}
