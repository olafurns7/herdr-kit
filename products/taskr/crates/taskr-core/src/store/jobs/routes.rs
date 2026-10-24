use super::*;

/// Call in the work acceptance transaction; true replaces only its final inbox message.
pub fn accepted(
    db: &Connection,
    root: i64,
    work: i64,
    revision: i64,
    run: i64,
    result: i64,
) -> Result<bool> {
    let id:Option<String>=db.query_row("select id from jobs_operations where root_id=? and kind='await' and reference_id=? and revision=?",params![root,work,revision],|r|r.get(0)).optional()?;
    let Some(id) = id else { return Ok(false) };
    attach(db, &id, run, result)?;
    reconcile_operation(db, &id)?;
    Ok(true)
}
fn offered(db: &Connection, o: &Operation, result: i64) -> Result<bool> {
    Ok(db.query_row("select exists(select 1 from events where recipient_task_id=? and kind='message' and json_extract(data,'$.notice.reason')='accepted' and json_extract(data,'$.notice.work')=? and json_extract(data,'$.notice.result')=?)",params![o.root,o.reference,result],|r|r.get(0))?)
}
fn attach(db: &Connection, id: &str, run: i64, result: i64) -> Result<()> {
    let o = operation(db, id)?;
    let prior: (Option<i64>, Option<i64>) = db.query_row(
        "select result_id,run_id from jobs_routes where operation_id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if prior.0.is_some() && prior != (Some(result), Some(run)) {
        return Err(reject("conflicting immutable jobs result"));
    }
    if prior.0.is_some() {
        return Ok(());
    }
    db.execute(
        "update jobs_routes set result_id=?,run_id=? where operation_id=?",
        params![result, run, id],
    )?;
    let payload: String = db.query_row(
        "select payload from work_results where id=?",
        [result],
        |r| r.get(0),
    )?;
    let value: Value = serde_json::from_str(&payload)?;
    finish(
        db,
        id,
        "completed",
        Some(tool(
            json!({"result_id":result,"work":o.reference,"revision":o.revision,"run":run,"result":value,"instruction":format!("After processing this result call result_receipt with {{\"handle\":\"{id}\",\"result\":{result}}}. Polling is not a receipt.")}),
        )),
    )?;
    if offered(db, &o, result)? {
        db.execute("update jobs_routes set state='released',fallback_sent=1 where operation_id=? and state!='receipted'",[id])?;
    }
    Ok(())
}
pub(super) fn refresh(db: &Connection, id: &str) -> Result<()> {
    let o = operation(db, id)?;
    let found: Option<(i64, i64)> = db
        .query_row(
            "select run_id,result_id from work_acceptances where work_id=? and revision=?",
            params![o.reference, o.revision],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((run, result)) = found {
        attach(db, id, run, result)?;
    } else if o.status == "working" {
        let (revision, state): (i64, String) = db.query_row(
            "select revision,state from work_items where id=?",
            [o.reference],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if revision != o.revision || state == "cancelled" {
            finish(
                db,
                id,
                "failed",
                Some(
                    json!({"code":-32000,"message":"Work revision was superseded or explicitly cancelled; inspect work_get and choose a current revision."}),
                ),
            )?;
            db.execute(
                "update jobs_routes set state='released' where operation_id=? and state='active'",
                [id],
            )?;
        }
    }
    Ok(())
}
pub(super) fn release_tx(db: &Connection, id: &str) -> Result<()> {
    db.execute(
        "update jobs_routes set state='released' where operation_id=? and state='active'",
        [id],
    )?;
    fallback(db, id)
}
fn fallback(db: &Connection, id: &str) -> Result<()> {
    let row:Option<(i64,i64)>=db.query_row("select result_id,run_id from jobs_routes where operation_id=? and state='released' and fallback_sent=0 and result_id is not null",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((result, run)) = row else {
        return Ok(());
    };
    let o = operation(db, id)?;
    if !offered(db, &o, result)? {
        let parked:bool=db.query_row("select coalesce((select json_extract(data,'$.value') from events where task_id=? and kind='ref' and json_extract(data,'$.key')='glance.state' order by id desc limit 1)='parked',0)",[o.root],|r|r.get(0))?;
        if parked {
            return Ok(());
        }
        let generation = consumer::push_generation(db, &task(db, o.root)?)?;
        let text = format!("work {}: accepted", o.reference);
        event(
            db,
            Event {
                task: o.root,
                to: Some(o.root),
                kind: "message",
                summary: &text,
                data: Some(
                    json!({"text":text,"gen":generation,"notice":{"reason":"accepted","work":o.reference,"revision":o.revision,"run":run,"result":result}}),
                ),
                ..Event::default()
            },
        )?;
    }
    db.execute(
        "update jobs_routes set fallback_sent=1 where operation_id=?",
        [id],
    )?;
    Ok(())
}
pub(super) fn reconcile_operation(db: &Connection, id: &str) -> Result<usize> {
    let o = operation(db, id)?;
    let (tracked, expired): (bool, bool) = if o.kind == "await" {
        db.query_row("select state='active' or (state='released' and fallback_sent=0 and ?!='failed'),state='active' and deadline<=? from jobs_routes where operation_id=?",params![o.status,now(),id],|r|Ok((r.get(0)?,r.get(1)?)))?
    } else {
        (o.status == "input_required", false)
    };
    if !tracked {
        return Ok(0);
    }
    let t = task(db, o.root)?;
    let ended = delivery::generation(db, o.root)? != o.generation
        || t.status == "closed"
        || machine_name(t.machine.as_deref()) != o.host
        || t.role != "orchestrator";
    if o.kind == "owner" {
        if ended {
            finish(
                db,
                id,
                "completed",
                Some(tool(
                    json!({"outcome":"no_decision","ask":o.reference,"reason":"owner presentation session ended"}),
                )),
            )?;
        } else {
            owner::refresh(db, &o)?;
        }
        return Ok(0);
    }
    let mut released = 0;
    if expired || ended {
        released = db.execute(
            "update jobs_routes set state='released' where operation_id=? and state='active'",
            [id],
        )?;
        finish(
            db,
            id,
            "completed",
            Some(tool(json!({"outcome":"released_to_channel"}))),
        )?;
    }
    refresh(db, id)?;
    fallback(db, id)?;
    Ok(released)
}
fn reconcile_scope(db: &Connection, session: Option<&Session>) -> Result<usize> {
    let filter = if session.is_some() {
        "o.root_id=? and o.generation=? and o.binding=? and o.host=? and"
    } else {
        ""
    };
    let mut stmt=db.prepare(&format!("select o.id from jobs_operations o left join jobs_routes r on r.operation_id=o.id where {filter} (r.state='active' or (r.state='released' and r.fallback_sent=0 and o.status!='failed') or (o.kind='owner' and o.status='input_required'))"))?;
    let ids = if let Some(s) = session {
        stmt.query_map(params![s.root, s.generation, s.binding, host()], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut released = 0;
    for id in ids {
        released += reconcile_operation(db, &id)?;
    }
    Ok(released)
}
pub(super) fn reconcile_session(db: &Connection, s: &Session) -> Result<usize> {
    reconcile_scope(db, Some(s))
}
pub(super) fn reconcile_tx(db: &Connection) -> Result<usize> {
    reconcile_scope(db, None)
}
/// Run before jobs startup and at most five seconds apart in the authoritative service.
/// The 95-second fallback bound excludes service downtime.
pub fn reconcile(db: &mut Connection) -> Result<Value> {
    transaction(db, |tx| Ok(json!({"ok":true,"released":reconcile_tx(tx)?})))
}
