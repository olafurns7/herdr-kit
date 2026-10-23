//! Current-run read projection only; acceptance policy belongs to store::work.
use super::*;

const LIMIT: i64 = 100;
const SELECTED: &str = "with selected as (select * from work_items where root_id=? order by priority desc,id limit ?) ";

fn available(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "select exists(select 1 from sqlite_master where type='table' and name='work_items')",
        [],
        |r| r.get(0),
    )?)
}

pub(super) fn snapshot(db: &Connection, root: i64) -> Result<Value> {
    if !available(db)? {
        return Ok(Value::Null);
    }
    let total: i64 = db.query_row(
        "select count(*) from work_items where root_id=?",
        [root],
        |r| r.get(0),
    )?;
    let mut items = rows(
        db,
        &format!(
            "{SELECTED}select w.id as work_id,w.title,w.priority,w.revision,w.state,w.current_run,substr(w.reason,1,512) as reason,r.revision as run_revision,r.round,r.action,json_array_length(v.contract,'$.profile.checks') as required_checks,(select id from work_acceptances x where x.work_id=w.id and x.revision=w.revision and x.run_id=w.current_run) as acceptance_id from selected w left join work_runs r on r.id=w.current_run left join work_revisions v on v.work_id=w.id and v.revision=w.revision order by w.priority desc,w.id"
        ),
        vec![root.into(), LIMIT.into()],
    )?;
    // The run/round uniqueness index bounds this join to four stages per selected item.
    let assignments = rows(
        db,
        &format!(
            "{SELECTED}select w.id as work_id,a.lane_id as task_id,a.launch_id,a.stage,a.state,a.process_state,a.stop_event,x.id as result_id,json_extract(x.payload,'$.kind') as result_kind,json_extract(x.payload,'$.result.verdict') as verdict,json_array_length(x.payload,'$.result.findings') as finding_count,json_array_length(x.payload,'$.result.checks') as check_count,(select count(*) from json_each(x.payload,'$.result.checks') j where json_extract(j.value,'$.exit') is not 0 or json_extract(j.value,'$.signal') is not null or json_extract(j.value,'$.timed_out')=1 or json_extract(j.value,'$.error') is not null or exists(select 1 from json_each(j.value,'$.cases') c where json_extract(c.value,'$.status') is not 'passed')) as failed_checks from selected w join work_runs r on r.id=w.current_run join work_assignments a on a.run_id=r.id and a.round=r.round left join work_results x on x.launch_id=a.launch_id order by a.launch_id"
        ),
        vec![root.into(), LIMIT.into()],
    )?;
    for w in &mut items {
        let current: Vec<Value> = assignments
            .iter()
            .filter(|a| a["work_id"] == w["work_id"])
            .cloned()
            .collect();
        let stage = |name: &str| current.iter().find(|a| a["stage"] == name);
        let stale = !w["run_revision"].is_null() && w["run_revision"] != w["revision"];
        let accepted = w["acceptance_id"].is_i64();
        let gate = stage("gate").filter(|a| a["result_kind"] == "proof");
        let proof = if stale {
            "stale; recheck required"
        } else if w["required_checks"].is_null() {
            "unavailable"
        } else if w["required_checks"] == 0 {
            "not required"
        } else if accepted && w["reason"].is_string() {
            "receipt recorded; blocked/recheck"
        } else if accepted {
            "passed (acceptance receipt)"
        } else if gate.is_some_and(|a| n(a, "failed_checks") > 0) {
            "failed"
        } else if gate.is_some() {
            "recorded; validation pending"
        } else {
            "pending"
        };
        let review = if stale {
            "stale; recheck required"
        } else {
            stage("reviewer")
                .filter(|a| a["result_kind"] == "review")
                .and_then(|a| a["verdict"].as_str())
                .unwrap_or("pending")
        };
        w["proof"] = json!(proof);
        w["review"] = json!(review);
        w["assignment"] = current.last().cloned().unwrap_or(Value::Null);
        w["assignments"] = json!(current);
    }
    let truncated = total > items.len() as i64;
    Ok(json!({"items":items,"total":total,"limit":LIMIT,"truncated":truncated}))
}

/// One grouped read per glance, never work show/list per row or per rendered frame.
pub(in crate::read) fn counts(db: &Connection) -> Result<Option<BTreeMap<i64, Value>>> {
    if !available(db)? {
        return Ok(None);
    }
    let grouped = rows(
        db,
        "select root_id,count(*) as total,sum(state='backlog') as backlog,sum(state='queued') as queued,sum(state='active') as active,sum(state='verifying') as verifying,sum(state='done') as done,sum(state='cancelled') as cancelled,sum(reason is not null) as blocked from work_items group by root_id",
        vec![],
    )?;
    Ok(Some(
        grouped.into_iter().map(|v| (n(&v, "root_id"), v)).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(upgraded: bool) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(taskr_core::schema::SCHEMA).unwrap();
        if upgraded {
            db.execute_batch(taskr_core::store::work::DDL).unwrap();
        }
        db.execute("insert into tasks(id,name,role,status,created_at,updated_at) values(1,'demo','orchestrator','open','t','t')",[]).unwrap();
        db
    }
    fn item(db: &Connection, id: i64) {
        db.execute("insert into work_items(id,root_id,title,priority,revision,state,creator,created_at) values(?,1,'unassigned',0,1,'backlog','{}','t')",[id]).unwrap();
    }
    fn running(db: &Connection) {
        item(db, 1);
        db.execute("insert into work_revisions(work_id,revision,contract,creator,created_at) values(1,1,?,'{}','t')",[json!({"profile":{"checks":[{"id":"check","cases":["required"]}]}}).to_string()]).unwrap();
        db.execute("insert into work_runs(id,work_id,revision,root_id,generation,claim_key,claim_input,dependencies,round,action,state,created_at) values(1,1,1,1,0,'claim','{}','[]',0,'check','active','t')",[]).unwrap();
        db.execute(
            "update work_items set current_run=1,state='verifying' where id=1",
            [],
        )
        .unwrap();
    }
    fn result(db: &Connection, launch: i64, stage: &str, payload: Value) {
        db.execute(
            "insert into launches(id,task_id,recorded_at) values(?,1,'t')",
            [launch],
        )
        .unwrap();
        db.execute("insert into work_assignments(launch_id,run_id,stage,round,lane_id,state,process_state,lane_snapshot) values(?,1,?,0,1,'settled','uncertain','{}')",params![launch,stage]).unwrap();
        db.execute("insert into work_results(id,launch_id,digest,payload,created_at) values(?,?,'digest',?,'t')",params![launch,launch,payload.to_string()]).unwrap();
    }
    fn first(db: &Connection) -> Value {
        snapshot(db, 1).unwrap()["items"][0].clone()
    }

    #[test]
    fn legacy_empty_and_unassigned_are_distinct_and_truncation_is_explicit() {
        let old = db(false);
        assert!(snapshot(&old, 1).unwrap().is_null());
        assert!(counts(&old).unwrap().is_none());
        let db = db(true);
        assert_eq!(snapshot(&db, 1).unwrap()["total"], 0);
        assert!(
            snapshot(&db, 1).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for id in 1..=LIMIT + 1 {
            item(&db, id);
        }
        let view = snapshot(&db, 1).unwrap();
        assert_eq!(view["total"], LIMIT + 1);
        assert_eq!(view["items"].as_array().unwrap().len(), LIMIT as usize);
        assert_eq!(view["truncated"], true);
        let w = &view["items"][0];
        assert_eq!(w["state"], "backlog");
        assert!(w["assignment"].is_null());
        assert_eq!(w["proof"], "unavailable");
        assert_eq!(counts(&db).unwrap().unwrap()[&1]["backlog"], LIMIT + 1);
        let glance = super::super::super::glance::snapshot(&db).unwrap();
        assert!(
            glance["campaigns"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == 1 && c["work_counts"]["backlog"] == LIMIT + 1),
            "durable backlog must not disappear into quiet campaigns"
        );
    }

    #[test]
    fn gate_receipts_and_source_review_do_not_invent_an_acceptance_or_stop() {
        let db = db(true);
        running(&db);
        result(
            &db,
            2,
            "gate",
            json!({"kind":"proof","result":{"checks":[{"exit":1,"cases":[]}]}}),
        );
        assert_eq!(first(&db)["proof"], "failed");
        assert_eq!(first(&db)["assignment"]["process_state"], "uncertain");
        assert!(first(&db)["acceptance_id"].is_null());
        // No cases or check records never becomes a generic passing proof.
        db.execute(
            "update work_results set payload=? where id=2",
            [json!({"kind":"proof","result":{"checks":[]}}).to_string()],
        )
        .unwrap();
        assert_eq!(first(&db)["proof"], "recorded; validation pending");
        result(
            &db,
            3,
            "reviewer",
            json!({"kind":"review","result":{"verdict":"rework","findings":[{"id":"F1"}]}}),
        );
        assert_eq!(first(&db)["review"], "rework");
        assert_eq!(first(&db)["assignments"].as_array().unwrap().len(), 2);
        // A previous round's result is not the current round's evidence.
        db.execute(
            "update work_runs set round=1,action='rework' where id=1",
            [],
        )
        .unwrap();
        assert_eq!(first(&db)["proof"], "pending");
        assert_eq!(first(&db)["review"], "pending");
        assert!(first(&db)["assignment"].is_null());
    }

    #[test]
    fn accepted_receipt_and_semantic_recheck_keep_their_exact_revision() {
        let db = db(true);
        running(&db);
        result(
            &db,
            2,
            "reviewer",
            json!({"kind":"review","result":{"verdict":"clear"}}),
        );
        db.execute("insert into work_acceptances(id,work_id,revision,run_id,result_id,dependencies,created_at) values(9,1,1,1,2,'[]','t')",[]).unwrap();
        db.execute("update work_items set state='done' where id=1", [])
            .unwrap();
        assert_eq!(first(&db)["acceptance_id"], 9);
        assert_eq!(first(&db)["proof"], "passed (acceptance receipt)");
        db.execute(
            "update work_items set reason='dependency changed; recheck required' where id=1",
            [],
        )
        .unwrap();
        assert_eq!(first(&db)["proof"], "receipt recorded; blocked/recheck");
        db.execute("update work_items set revision=2 where id=1", [])
            .unwrap();
        let w = first(&db);
        assert!(w["acceptance_id"].is_null());
        assert_eq!(w["proof"], "stale; recheck required");
        assert_eq!(w["review"], "stale; recheck required");
        assert_eq!(w["run_revision"], 1);
    }
}
