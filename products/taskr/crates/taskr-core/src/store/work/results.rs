use super::*;

fn result(db: &Connection, r: &Run, stage: &str) -> Result<Option<(i64, Submission)>> {
    let row:Option<(i64,String)>=db.query_row("select x.id,x.payload from work_results x join work_assignments a on a.launch_id=x.launch_id where a.run_id=? and a.round=? and a.stage=?",params![r.id,r.round,stage],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    row.map(|(id, p)| Ok((id, parse(&p)?))).transpose()
}
pub(super) fn execution(db: &Connection, r: &Run) -> Result<(i64, Execution)> {
    match result(db, r, "executor")? {
        Some((id, Submission::Execution(e))) => Ok((id, e)),
        _ => Err(reject("execution result missing")),
    }
}
fn review(db: &Connection, r: &Run) -> Result<(i64, Review)> {
    match result(db, r, "reviewer")? {
        Some((id, Submission::Review(e))) => Ok((id, e)),
        _ => Err(reject("review result missing")),
    }
}
pub fn submit(db: &mut Connection, launch: i64, input: Submission) -> Result<Value> {
    let payload = encoded(&input)?;
    let sha = digest(&payload);
    transaction(db, |tx| {
        let (worker, current) = delivery::worker(tx)?;
        if current != launch {
            return Err(reject("submission must use current TASKR_LAUNCH"));
        }
        let mut ass = assignment(tx, launch)?;
        if ass["task_id"] != worker.id {
            return Err(reject("assignment worker mismatch"));
        }
        let r = run_row(tx, ass["run_id"].as_i64().unwrap())?;
        let w = item(tx, r.work)?;
        if w.run != Some(r.id) || w.revision != r.revision || w.state == "cancelled" {
            return Err(reject("stale or cancelled result"));
        }
        let c = contract(tx, &w)?;
        if ass["round"] != r.round || dependencies(tx, &c)? != r.deps {
            return Err(reject("superseded round or dependency acceptance"));
        }
        let prior: Option<(i64, String)> = tx
            .query_row(
                "select id,digest from work_results where launch_id=?",
                [launch],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, old)) = prior {
            if old != sha {
                return Err(reject("conflicting immutable result replay"));
            }
            return Ok(
                json!({"ok":true,"result_id":id,"digest":sha,"work_id":w.id,"revision":r.revision,"run_id":r.id,"launch_id":launch,"duplicate":true}),
            );
        }
        // A fast runner may beat the controller tick; a matching start/got proves launch start, not native identity.
        if ass["process_state"] == "starting" {
            start_tx(tx, launch)?;
            ass = assignment(tx, launch)?;
        }
        if r.state != "active" || ass["state"] != "running" || ass["process_state"] != "running" {
            return Err(reject("only running assignments may submit"));
        }
        match &input {
            Submission::Execution(e) => {
                if ass["stage"] != "executor" {
                    return Err(reject("execution requires executor assignment"));
                }
                source(&e.source)?;
                text(&e.report, 65536, "execution report")?;
                artifacts(&e.artifacts)?;
                if e.source.input_digest != c.base.input_digest {
                    return Err(reject("execution input digest differs from contract"));
                }
            }
            Submission::Review(rev) => {
                if ass["stage"] != "reviewer" {
                    return Err(reject("review requires independent reviewer assignment"));
                }
                let (execution_id, e) = execution(tx, &r)?;
                if rev.execution_result != execution_id || rev.source != e.source {
                    return Err(reject("review source/result mismatch"));
                }
                if rev.findings.len() > 128 {
                    return Err(usage("too many findings"));
                }
                unique(rev.findings.iter().map(|f| f.id.as_str()), "finding ID")?;
                for f in &rev.findings {
                    text(&f.reason, 4096, "finding reason")?;
                    text(&f.evidence, 8192, "finding evidence")?;
                }
                if (rev.verdict == Verdict::Clear) != rev.findings.is_empty() {
                    return Err(usage(
                        "clear review requires no findings; other verdicts require findings",
                    ));
                }
            }
            Submission::Proof(gate) => {
                if ass["stage"] != "gate" {
                    return Err(reject("proof requires assigned gate runner launch"));
                }
                validate_gate(tx, &r, &c, launch, gate)?;
            }
            Submission::Adjudication(adj) => {
                if ass["stage"] != "adjudicator" {
                    return Err(reject(
                        "adjudication requires distinct adjudicator assignment",
                    ));
                }
                let (review_id, rev) = review(tx, &r)?;
                if adj.review_result != review_id
                    || adj.source != rev.source
                    || adj.verdict == Verdict::Disputed
                {
                    return Err(reject("adjudication source/review/verdict mismatch"));
                }
                unique(adj.finding_ids.iter().map(String::as_str), "finding ID")?;
                let actual: std::collections::BTreeSet<_> =
                    adj.finding_ids.iter().map(String::as_str).collect();
                let want: std::collections::BTreeSet<_> =
                    rev.findings.iter().map(|f| f.id.as_str()).collect();
                if actual != want {
                    return Err(reject("adjudication must resolve exact review finding set"));
                }
                text(&adj.reason, 8192, "adjudication reason")?;
            }
        }
        tx.execute(
            "insert into work_results(launch_id,digest,payload,created_at) values(?,?,?,?)",
            params![launch, sha, payload, now()],
        )?;
        let id = tx.last_insert_rowid();
        if let Submission::Proof(gate) = &input {
            for check in &gate.checks {
                tx.execute("insert into work_proofs(result_id,checker_launch,check_id,payload) values(?,?,?,?)",params![id,launch,check.check,encoded(check)?])?;
            }
        }
        tx.execute(
            "update work_assignments set state='settled' where launch_id=?",
            [launch],
        )?;
        tx.execute("update work_items set state='verifying' where id=?", [w.id])?;
        audit(
            tx,
            r.root,
            "submitted",
            Some(w.id),
            Some(r.id),
            Some(id),
            false,
        )?;
        Ok(
            json!({"ok":true,"result_id":id,"digest":sha,"work_id":w.id,"revision":r.revision,"run_id":r.id,"launch_id":launch}),
        )
    })
}
fn action(db: &Connection, r: &Run, name: &str, reason: Option<&str>) -> Result<Value> {
    db.execute(
        "update work_runs set action=? where id=?",
        params![name, r.id],
    )?;
    if let Some(reason) = reason {
        db.execute(
            "update work_items set reason=? where id=?",
            params![reason, r.work],
        )?;
    }
    let mut out = run_output(db, r.id)?;
    if let Some(reason) = reason {
        out["reason"] = json!(reason);
    }
    Ok(out)
}
fn rework(db: &Connection, r: &Run, c: &Contract, reason: &str) -> Result<Value> {
    if !all_stopped(db, r.id)? {
        return stop_action(db, r);
    }
    if r.round >= c.max_rework {
        return escalate(db, r, "rework budget exhausted");
    }
    db.execute(
        "update work_runs set round=round+1,action='rework' where id=?",
        [r.id],
    )?;
    db.execute(
        "update work_items set state='active',reason=? where id=?",
        params![reason, r.work],
    )?;
    audit(db, r.root, "rework", Some(r.work), Some(r.id), None, false)?;
    run_output(db, r.id)
}
pub fn advance(db: &mut Connection, a: Authority, run: i64) -> Result<Value> {
    transaction(db, |tx| {
        let r = run_row(tx, run)?;
        authority(tx, a, r.root, false)?;
        if r.state == "accepted" {
            return run_output(tx, run);
        }
        let (r, w, c) = current_run(tx, run)?;
        if dependencies(tx, &c)? != r.deps {
            return Err(reject("dependency acceptance changed"));
        }
        let blocked: bool = tx.query_row(
            "select exists(select 1 from work_assignments where run_id=? and process_state='uncertain')",
            [run],
            |r| r.get(0),
        )?;
        if blocked {
            let mut out = stop_action(tx, &r)?;
            out["reason"] =
                json!("uncertain process; explicit identity and stopped observation required");
            return Ok(out);
        }
        if r.action == "escalation" {
            return run_output(tx, run);
        }
        let Some((execution_id, Submission::Execution(exec))) = result(tx, &r, "executor")? else {
            return run_output(tx, run);
        };
        if let Some(out) = need_stop(tx, &r, "executor")? {
            return Ok(out);
        }
        if !c.profile.checks.is_empty() {
            let gate = result(tx, &r, "gate")?;
            let Some((_, Submission::Proof(gate))) = gate else {
                return action(tx, &r, "check", None);
            };
            if let Some(out) = need_stop(tx, &r, "gate")? {
                return Ok(out);
            }
            let launch:i64=tx.query_row("select launch_id from work_assignments where run_id=? and round=? and stage='gate'",params![r.id,r.round],|r|r.get(0))?;
            if !gate_passes(tx, &r, &c, launch, &gate)? {
                return rework(tx, &r, &c, "gate check failed; rework");
            }
        }
        let Some((review_id, Submission::Review(rev))) = result(tx, &r, "reviewer")? else {
            return action(tx, &r, "review", None);
        };
        if rev.source != exec.source || rev.execution_result != execution_id {
            return Err(reject("review no longer matches execution"));
        }
        if let Some(out) = need_stop(tx, &r, "reviewer")? {
            return Ok(out);
        }
        match rev.verdict {
            Verdict::Rework => return rework(tx, &r, &c, "review requires rework"),
            Verdict::Disputed if c.adjudicator.is_none() => {
                return escalate(
                    tx,
                    &r,
                    "disputed findings require lead or distinct adjudicator",
                );
            }
            _ => {}
        }
        if c.require_adjudicator || rev.verdict == Verdict::Disputed {
            let Some((_, Submission::Adjudication(adj))) = result(tx, &r, "adjudicator")? else {
                return action(tx, &r, "adjudicate", None);
            };
            if adj.review_result != review_id || adj.source != exec.source {
                return Err(reject("adjudication no longer matches review"));
            }
            match adj.verdict {
                Verdict::Clear => {}
                Verdict::Rework => return rework(tx, &r, &c, "review requires rework"),
                Verdict::Disputed => {
                    return escalate(tx, &r, "adjudication unresolved");
                }
            }
        }
        if !all_stopped(tx, run)? {
            return stop_action(tx, &r);
        }
        tx.execute("insert into work_acceptances(work_id,revision,run_id,result_id,dependencies,created_at) values(?,?,?,?,?,?)",params![w.id,w.revision,run,execution_id,r.deps,now()])?;
        tx.execute(
            "update work_runs set state='accepted',action='accepted' where id=?",
            [run],
        )?;
        tx.execute(
            "update work_items set state='done',reason=null where id=?",
            [w.id],
        )?;
        tx.execute("delete from work_checkouts where run_id=?", [run])?;
        invalidate_dependents(tx, w.id)?;
        let native_routed =
            super::super::jobs::accepted(tx, r.root, w.id, w.revision, run, execution_id)?;
        audit(
            tx,
            r.root,
            "accepted",
            Some(w.id),
            Some(run),
            Some(execution_id),
            !native_routed,
        )?;
        run_output(tx, run)
    })
}
/// Quiet all enabled live assignments, including reports before submit; urgent kinds keep routes.
pub fn quiet(db: &Connection, launch: Option<i64>) -> Result<bool> {
    Ok(db.query_row("select exists(select 1 from work_assignments a join work_runs r on r.id=a.run_id join work_campaigns c on c.root_id=r.root_id where a.launch_id=? and r.state='active' and c.enabled=1)",[launch],|r|r.get(0))?)
}
fn need_stop(db: &Connection, r: &Run, stage: &str) -> Result<Option<Value>> {
    let launch:Option<i64>=db.query_row("select launch_id from work_assignments where run_id=? and round=? and stage=? and process_state!='stopped'",params![r.id,r.round,stage],|r|r.get(0)).optional()?;
    if let Some(launch) = launch {
        let mut out = action(db, r, "stop", None)?;
        out["launch_id"] = json!(launch);
        return Ok(Some(out));
    }
    Ok(None)
}
fn stop_action(db: &Connection, r: &Run) -> Result<Value> {
    let launch:i64=db.query_row("select launch_id from work_assignments where run_id=? and process_state!='stopped' order by launch_id limit 1",[r.id],|r|r.get(0))?;
    let mut out = action(db, r, "stop", None)?;
    out["launch_id"] = json!(launch);
    Ok(out)
}

fn escalate(db: &Connection, r: &Run, reason: &str) -> Result<Value> {
    if r.action != "escalation" {
        audit(
            db,
            r.root,
            "escalated",
            Some(r.work),
            Some(r.id),
            None,
            true,
        )?;
    }
    action(db, r, "escalation", Some(reason))
}
