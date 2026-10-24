//! Synthetic ledger integration; no dialogs, worker processes, or live adapter setup.
#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::Harness;
use taskr_core::{
    db::params,
    store::{self, jobs as api},
};

const BINDING: &str = "synthetic-binding-1";
struct Jobs {
    h: Harness,
    root: i64,
}
impl Jobs {
    fn new() -> Self {
        let h = Harness::new();
        let root = h.ok(&[], &["new", "jobs-root", "--role", "orchestrator"])["task_id"]
            .as_i64()
            .unwrap();
        Self { h, root }
    }
    fn argv(&self, verb: &str, data: Value, generation: i64, binding: &str) -> Vec<String> {
        vec![
            "jobs".into(),
            verb.into(),
            "--as".into(),
            self.root.to_string(),
            "--generation".into(),
            generation.to_string(),
            "--binding".into(),
            binding.into(),
            "--data".into(),
            data.to_string(),
        ]
    }
    fn call_at(
        &self,
        verb: &str,
        data: Value,
        generation: i64,
        binding: &str,
        env: &[(String, String)],
    ) -> Value {
        let args = self.argv(verb, data, generation, binding);
        self.h
            .ok(env, &args.iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn call(&self, verb: &str, data: Value) -> Value {
        self.call_at(verb, data, 0, BINDING, &[])
    }
    fn rejected(
        &self,
        verb: &str,
        data: Value,
        generation: i64,
        binding: &str,
        env: &[(String, String)],
        exit: i32,
    ) {
        let mut args = vec!["--json".to_string()];
        args.extend(self.argv(verb, data, generation, binding));
        let out = self.h.output(
            env,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            b"",
        );
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
    }
    fn work(&self) -> i64 {
        self.call(
            "add",
            json!({"key":"one","campaign":self.root,"title":"synthetic work"}),
        )["work_id"]
            .as_i64()
            .unwrap()
    }
    fn pending(&self, work: i64) -> String {
        self.call("await", json!({"work":work,"revision":1}))["taskId"]
            .as_str()
            .unwrap()
            .into()
    }
    fn accepted(&self, work: i64) -> i64 {
        let mut db = self.h.conn();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        store::transaction(&mut db,|tx|{
            // Seed a domain-accepted result; these tests exercise the routing hook,
            // while work-domain tests own execution/review acceptance provenance.
            tx.execute("insert into tasks(parent_id,name,role,created_at,updated_at) values(?,'synthetic-executor','implementer',?,?)",params![self.root,store::now(),store::now()])?;
            let lane=tx.last_insert_rowid();
            tx.execute("insert into launches(task_id,recorded_at) values(?,?)",params![lane,store::now()])?;
            let launch=tx.last_insert_rowid();
            tx.execute("update tasks set current_launch_id=? where id=?",params![launch,lane])?;
            tx.execute("insert into work_runs(work_id,revision,root_id,generation,claim_key,claim_input,dependencies,action,state,created_at) values(?,1,?,?,?,'{}','[]','accepted','accepted',?)",params![work,self.root,store::delivery::generation(tx,self.root)?,format!("fixture-{work}"),store::now()])?;
            let run=tx.last_insert_rowid();
            let stop=store::event(tx,store::Event{task:lane,launch:Some(launch),kind:"work_process_stopped",summary:"synthetic routing fixture",data:Some(json!({"origin":"controller"})),..store::Event::default()})?;
            tx.execute("insert into work_assignments(launch_id,run_id,stage,round,lane_id,state,process_state,stop_event,lane_snapshot) values(?,?,'executor',0,?,'settled','stopped',?,'{}')",params![launch,run,lane,stop])?;
            tx.execute("insert into work_results(launch_id,digest,payload,created_at) values(?,'synthetic','{\"kind\":\"execution\",\"report\":\"fixture\"}',?)",params![launch,store::now()])?;
            let result=tx.last_insert_rowid();
            tx.execute("insert into work_acceptances(work_id,revision,run_id,result_id,dependencies,created_at) values(?,1,?,?, '[]',?)",params![work,run,result,store::now()])?;
            tx.execute("update work_items set state='done',current_run=? where id=?",params![run,work])?;
            api::accepted(tx,self.root,work,1,run,result)?;
            assert_eq!(tx.query_row("select count(*) from pragma_foreign_key_check",[],|r|r.get::<_,i64>(0))?,0);
            Ok(result)
        }).unwrap()
    }
    fn notices(&self) -> i64 {
        self.h.count("select count(*) from events where recipient_task_id=? and kind='message' and json_extract(data,'$.notice.reason')='accepted'",self.root)
    }
    fn park(&self, parked: bool) {
        store::event(
            &self.h.conn(),
            store::Event {
                task: self.root,
                kind: "ref",
                data: Some(json!({"key":"glance.state","value":if parked {"parked"}else{""}})),
                ..store::Event::default()
            },
        )
        .unwrap();
    }
    fn ask(&self) -> i64 {
        self.h.ok(&[],&["ask","--owner","--as",&self.root.to_string(),"--question",&json!({"question":"Synthetic approval?","options":[{"label":"Proceed"},{"label":"Hold"}]}).to_string()])["ask_id"].as_i64().unwrap()
    }
    fn owner(&self, ask: i64) -> (String, String) {
        let p = self.call("allow-ask", json!({"ask":ask}));
        let o = self.call("owner", json!({"ask":ask}));
        (
            o["taskId"].as_str().unwrap().into(),
            p["version"].as_str().unwrap().into(),
        )
    }
}
#[test]
fn explicit_receipt_retires_parked_pending_fallback_before_or_after_unpark() {
    for unpark_first in [false, true] {
        let j = Jobs::new();
        let w = j.work();
        let id = j.pending(w);
        let result = j.accepted(w);
        j.park(true);
        j.call("release", json!({"handle":id}));
        assert_eq!(j.notices(), 0);
        assert_eq!(
            j.h.conn()
                .query_row("select fallback_sent from jobs_routes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        if unpark_first {
            j.park(false);
        }
        for _ in 0..2 {
            j.call("receipt", json!({"handle":id,"result":result}));
        }
        j.park(false);
        for _ in 0..3 {
            j.call("reconcile", json!({}));
        }
        assert_eq!(j.notices(), 0);
        assert_eq!(
            j.h.conn()
                .query_row("select state,fallback_sent from jobs_routes", [], |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?
                )))
                .unwrap(),
            ("receipted".into(), 0)
        );
    }
}
#[test]
fn parked_cli_release_before_or_after_result_offers_once_only_after_unpark() {
    for result_first in [false, true] {
        let j = Jobs::new();
        let w = j.work();
        let id = j.pending(w);
        j.park(true);
        if result_first {
            j.accepted(w);
        }
        let terminal = j.call("release", json!({"handle":id}));
        if !result_first {
            j.accepted(w);
        }
        for _ in 0..2 {
            j.call("reconcile", json!({}));
        }
        assert_eq!(j.notices(), 0);
        assert_eq!(
            j.h.conn()
                .query_row("select state,fallback_sent from jobs_routes", [], |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?
                )))
                .unwrap(),
            ("released".into(), 0)
        );
        j.park(false);
        for _ in 0..3 {
            j.call("reconcile", json!({}));
        }
        assert_eq!(j.notices(), 1);
        assert_eq!(j.call("poll", json!({"handle":id})), terminal);
    }
}
#[test]
fn add_key_and_await_handle_are_durable_and_never_repeat_execution() {
    let j = Jobs::new();
    let w = j.work();
    assert_eq!(w, j.work());
    let id = j.pending(w);
    assert_eq!(id, j.pending(w));
    let v = j.call("recover", json!({}));
    assert_eq!(v["operations"][0]["taskId"], id);
    assert_eq!(
        j.h.count("select count(*) from work_runs where root_id=?", j.root),
        0
    );
    j.rejected(
        "add",
        json!({"key":"one","campaign":j.root,"title":"different"}),
        0,
        BINDING,
        &[],
        6,
    );
    j.rejected(
        "add",
        json!({"campaign":j.root,"title":"missing key"}),
        0,
        BINDING,
        &[],
        2,
    );
    j.rejected("await", json!({"work":w,"revision":2}), 0, BINDING, &[], 6);
    j.rejected(
        "await",
        json!({"work":w,"revision":1}),
        0,
        "different-binding",
        &[],
        6,
    );
}
#[test]
fn handle_effects_are_isolated_and_session_validate_precedes_global_reconciliation() {
    let j = Jobs::new();
    let work = j.work();
    let id = j.pending(work);
    let other =
        j.h.ok(&[], &["new", "other-root", "--role", "orchestrator"])["task_id"]
            .as_i64()
            .unwrap();
    let other_call = |verb: &str, data: Value| {
        j.h.ok(
            &[],
            &[
                "jobs",
                verb,
                "--as",
                &other.to_string(),
                "--generation",
                "0",
                "--binding",
                "other-session-binding",
                "--data",
                &data.to_string(),
            ],
        )
    };
    let other_work = other_call(
        "add",
        json!({"key":"other","campaign":other,"title":"other work"}),
    )["work_id"]
        .as_i64()
        .unwrap();
    let other_id = other_call("await", json!({"work":other_work,"revision":1}))["taskId"]
        .as_str()
        .unwrap()
        .to_string();
    let ask=j.h.ok(&[],&["ask","--owner","--as",&other.to_string(),"--question",&json!({"question":"Other synthetic choice?","options":[{"label":"Proceed"},{"label":"Hold"}]}).to_string()])["ask_id"].as_i64().unwrap();
    other_call("allow-ask", json!({"ask":ask}));
    let owner = other_call("owner", json!({"ask":ask}))["taskId"]
        .as_str()
        .unwrap()
        .to_string();
    let sibling = j.call(
        "add",
        json!({"key":"sibling","campaign":j.root,"title":"sibling work"}),
    )["work_id"]
        .as_i64()
        .unwrap();
    let sibling = j.pending(sibling);
    j.h.conn().execute("update jobs_routes set deadline='2000-01-01T00:00:00.000Z' where operation_id in (?,?)",params![other_id,sibling]).unwrap();
    j.h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [other],
        )
        .unwrap();
    let route = |id: &str| {
        j.h.conn()
            .query_row(
                "select state from jobs_routes where operation_id=?",
                [id],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    };
    let status = || {
        j.h.conn()
            .query_row(
                "select status from jobs_operations where id=?",
                [&owner],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    };
    let untouched = || {
        assert_eq!(route(&other_id), "active");
        assert_eq!(route(&sibling), "active");
        assert_eq!(status(), "input_required");
    };
    let result = j.accepted(work);
    untouched();
    j.call("poll", json!({"handle":id}));
    untouched();
    assert_eq!(j.pending(work), id);
    untouched();
    j.call("release", json!({"handle":id}));
    untouched();
    j.call("receipt", json!({"handle":id,"result":result}));
    untouched();
    j.call("validate", json!({}));
    assert_eq!(route(&sibling), "released");
    assert_eq!(route(&other_id), "active");
    assert_eq!(status(), "input_required");
    j.call("reconcile", json!({}));
    assert_eq!(route(&other_id), "released");
    assert_eq!(status(), "completed");
    assert_eq!(
        j.h.count(
            "select count(*) from events where id=? and answered_by is null",
            ask
        ),
        1
    );
}
#[test]
fn recover_pages_use_rowid_bounds_and_reconcile_only_returned_session_rows() {
    let j = Jobs::new();
    let db = j.h.conn();
    let host = store::local_machine();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    let mut expected = Vec::new();
    for n in 0..67 {
        db.execute("insert into work_items(root_id,title,priority,revision,state,creator,created_at) values(?,'page fixture',0,1,'backlog','{}','t')",[j.root]).unwrap();
        let work = db.last_insert_rowid();
        db.execute(
            "insert into work_revisions(work_id,revision,creator,created_at) values(?,1,'{}','t')",
            [work],
        )
        .unwrap();
        let id = format!("{:032x}", 1000 - n);
        let binding = if n == 1 {
            "other-session-binding"
        } else {
            BINDING
        };
        db.execute("insert into jobs_operations(id,root_id,generation,binding,host,kind,reference_id,revision,status,created_at,updated_at) values(?,?,0,?,?,'await',?,1,'working','t','t')",params![id,j.root,binding,host,work]).unwrap();
        let cursor = db.last_insert_rowid();
        db.execute("insert into jobs_routes(operation_id,state,deadline) values(?,'active','2000-01-01T00:00:00.000Z')",[&id]).unwrap();
        if n != 1 {
            expected.push((cursor, id));
        }
    }
    let untouched = expected[32].1.clone();
    let first = j.h.ok(
        &[],
        &[
            "jobs",
            "recover",
            "--as",
            &j.root.to_string(),
            "--generation",
            "0",
            "--binding",
            BINDING,
        ],
    );
    assert_eq!(first["returned"], 32);
    assert_eq!(first["limit"], 32);
    assert_eq!(first["truncated"], true);
    assert_eq!(first["next_cursor"], expected[31].0);
    assert_eq!(first["operations"][0]["taskId"], expected[0].1);
    assert_eq!(
        db.query_row(
            "select state from jobs_routes where operation_id=?",
            [&untouched],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "active"
    );
    let second = j.call("recover", json!({"cursor":first["next_cursor"]}));
    assert_eq!(second["returned"], 32);
    assert_eq!(second["truncated"], true);
    assert_eq!(second["next_cursor"], expected[63].0);
    let third = j.call("recover", json!({"cursor":second["next_cursor"]}));
    assert_eq!(third["returned"], 2);
    assert_eq!(third["truncated"], false);
    assert_eq!(third["next_cursor"], Value::Null);
    let ids = first["operations"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["operations"].as_array().unwrap())
        .chain(third["operations"].as_array().unwrap())
        .map(|o| o["taskId"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        expected
            .iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>()
    );
    let maximum = j.call("recover", json!({"limit":64}));
    assert_eq!(maximum["returned"], 64);
    assert_eq!(maximum["limit"], 64);
    assert_eq!(maximum["truncated"], true);
    let empty = j.call("recover", json!({"cursor":expected.last().unwrap().0}));
    assert_eq!(empty["returned"], 0);
    assert_eq!(empty["truncated"], false);
    assert_eq!(empty["next_cursor"], Value::Null);
    let foreign_state:String=db.query_row("select r.state from jobs_routes r join jobs_operations o on o.id=r.operation_id where o.binding='other-session-binding'",[],|r|r.get(0)).unwrap();
    assert_eq!(foreign_state, "active");
    assert_eq!(
        db.query_row("select count(*) from pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    for data in [
        json!({"limit":0}),
        json!({"limit":65}),
        json!({"limit":-1}),
        json!({"limit":"32"}),
        json!({"cursor":-1}),
        json!({"cursor":"1"}),
        json!({"unknown":true}),
    ] {
        j.rejected("recover", data, 0, BINDING, &[], 2);
    }
}
#[test]
fn repeated_polls_receipts_and_late_cancel_keep_terminal_result_immutable() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    let result = j.accepted(w);
    let first = j.call("poll", json!({"handle":id}));
    assert_eq!(first["status"], "completed");
    assert_eq!(j.notices(), 0);
    for _ in 0..3 {
        assert_eq!(j.call("poll", json!({"handle":id})), first);
    }
    j.rejected(
        "receipt",
        json!({"handle":id,"result":result+1}),
        0,
        BINDING,
        &[],
        6,
    );
    for _ in 0..2 {
        j.call("receipt", json!({"handle":id,"result":result}));
    }
    j.h.conn()
        .execute(
            "update jobs_routes set deadline='2000-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    j.call("reconcile", json!({}));
    assert_eq!(j.notices(), 0);
    let next = j.call("cancel", json!({"handle":id}));
    assert_eq!(next["result"], first["result"]);
    assert_eq!(next["status"], "completed");
}
#[test]
fn cancelled_await_preserves_work_and_one_future_fallback() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    let a = j.call("cancel", json!({"handle":id}));
    let b = j.call("cancel", json!({"handle":id}));
    assert_eq!(a, b);
    assert_eq!(a["status"], "cancelled");
    assert_eq!(j.call("get", json!({"work":w}))["state"], "backlog");
    assert_eq!(j.notices(), 0);
    let result = j.accepted(w);
    for _ in 0..3 {
        j.call("reconcile", json!({}));
    }
    assert_eq!(j.notices(), 1);
    assert_eq!(j.pending(w), id);
    assert_eq!(j.call("poll", json!({"handle":id}))["status"], "cancelled");
    j.call("receipt", json!({"handle":id,"result":result}));
    assert_eq!(j.notices(), 1);
}
#[test]
fn explicit_release_before_result_returns_released_to_channel() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    let released = j.call("release", json!({"handle":id}));
    assert_eq!(
        released["result"]["structuredContent"]["outcome"],
        "released_to_channel"
    );
    j.accepted(w);
    assert_eq!(j.notices(), 1);
    assert_eq!(
        j.call("poll", json!({"handle":id}))["result"],
        released["result"]
    );
}
#[test]
fn stale_generation_cannot_renew_or_rebind_old_operations() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    j.accepted(w);
    let db = j.h.conn();
    db.execute(
        "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
        [j.root],
    )
    .unwrap();
    let generation = db.last_insert_rowid();
    drop(db);
    let deadline: String =
        j.h.conn()
            .query_row("select deadline from jobs_routes", [], |r| r.get(0))
            .unwrap();
    let old = j.call("poll", json!({"handle":id}));
    for _ in 0..3 {
        assert_eq!(j.call("poll", json!({"handle":id})), old);
    }
    assert_eq!(j.call("validate", json!({}))["current"], false);
    assert_eq!(j.call("recover", json!({}))["operations"][0]["taskId"], id);
    assert_eq!(
        j.h.conn()
            .query_row("select deadline from jobs_routes", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        deadline
    );
    assert_eq!(j.notices(), 1);
    j.rejected(
        "poll",
        json!({"handle":id}),
        generation,
        "new-synthetic-session",
        &[],
        6,
    );
    assert_eq!(old["routeState"], "released");
    j.rejected(
        "add",
        json!({"key":"new","campaign":j.root,"title":"refused"}),
        0,
        BINDING,
        &[],
        6,
    );
    j.rejected("await", json!({"work":w,"revision":1}), 0, BINDING, &[], 6);
    j.rejected(
        "receipt",
        json!({"handle":id,"result":1}),
        0,
        BINDING,
        &[],
        6,
    );
    j.rejected(
        "receipt",
        json!({"handle":id,"result":1}),
        generation,
        "new-synthetic-session",
        &[],
        6,
    );
}
#[test]
fn stale_generation_repeated_unfinished_polls_return_released_without_new_route() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    j.h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [j.root],
        )
        .unwrap();
    let first = j.call("poll", json!({"handle":id}));
    assert_eq!(
        first["result"]["structuredContent"]["outcome"],
        "released_to_channel"
    );
    for _ in 0..3 {
        assert_eq!(j.call("poll", json!({"handle":id})), first);
    }
    j.accepted(w);
    assert_eq!(j.notices(), 1);
    assert_eq!(j.call("poll", json!({"handle":id})), first);
    assert_eq!(
        j.h.count(
            "select count(*) from jobs_operations where root_id=?",
            j.root
        ),
        1
    );
}
#[cfg(feature = "contract")]
#[test]
fn authoritative_frozen_clock_releases_at_ninety_seconds_within_ninety_five() {
    let j = Jobs::new();
    let w = j.work();
    let time = |s: &str| vec![("TASKR_FROZEN_NOW".into(), format!("2026-01-01T00:{s}Z"))];
    let a = j.call_at(
        "await",
        json!({"work":w,"revision":1}),
        0,
        BINDING,
        &time("00:00"),
    );
    let id = a["taskId"].as_str().unwrap();
    j.call_at("poll", json!({"handle":id}), 0, BINDING, &time("00:05"));
    j.call_at("validate", json!({}), 0, BINDING, &time("01:34"));
    assert_eq!(
        j.h.conn()
            .query_row("select state from jobs_routes", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "active"
    );
    let expired = j.call_at("poll", json!({"handle":id}), 0, BINDING, &time("01:35"));
    assert_eq!(expired["routeState"], "released");
    j.accepted(w);
    assert_eq!(j.notices(), 1);
    j.call_at("poll", json!({"handle":id}), 0, BINDING, &time("01:40"));
    assert_eq!(j.notices(), 1);
}
#[test]
fn cross_campaign_host_worker_and_suborchestrator_authority_is_refused() {
    let j = Jobs::new();
    let w = j.work();
    let id = j.pending(w);
    let other = j.h.ok(&[], &["new", "other", "--role", "orchestrator"])["task_id"]
        .as_i64()
        .unwrap();
    let other = other.to_string();
    let data = json!({"work":w,"revision":1}).to_string();
    let args = vec![
        "jobs",
        "await",
        "--as",
        &other,
        "--generation",
        "0",
        "--binding",
        BINDING,
        "--data",
        &data,
    ];
    let out = j.h.output(&[], &args, b"");
    assert_eq!(out.status.code(), Some(6));
    j.rejected(
        "poll",
        json!({"handle":id}),
        0,
        BINDING,
        &[
            ("TASKR_TASK".into(), j.root.to_string()),
            ("TASKR_LAUNCH".into(), "123".into()),
        ],
        6,
    );
    j.h.conn()
        .execute(
            "update tasks set machine='different.example' where id=?",
            [j.root],
        )
        .unwrap();
    j.rejected("validate", json!({}), 0, BINDING, &[], 6);
    j.h.conn()
        .execute(
            "update tasks set machine=null,role='sub-orchestrator' where id=?",
            [j.root],
        )
        .unwrap();
    j.rejected("validate", json!({}), 0, BINDING, &[], 6);
}
#[test]
fn owner_opt_in_is_per_ask_and_accepts_human_or_hook_shaped_identical_reply_once() {
    let j = Jobs::new();
    let ask = j.ask();
    j.rejected("owner", json!({"ask":ask}), 0, BINDING, &[], 6);
    let (handle, _version) = j.owner(ask);
    let v = j.call("poll", json!({"handle":handle}));
    assert_eq!(v["status"], "input_required");
    assert_eq!(
        v["inputRequests"]["owner_choice"]["method"],
        "elicitation/create"
    );
    let reply = json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}});
    let human = j.call("update", reply.clone());
    let hook = j.call("update", reply);
    assert_eq!(human, hook);
    assert_eq!(
        human["result"]["structuredContent"]["provenance"],
        format!("native:{BINDING}")
    );
    assert_eq!(
        j.h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            ask
        ),
        1
    );
    j.rejected("update",json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Hold"}}}}),0,BINDING,&[],6);
    let second = j.ask();
    j.rejected("owner", json!({"ask":second}), 0, BINDING, &[], 6);
}
#[test]
fn owner_decline_cancel_stale_choices_and_unknown_keys_never_answer() {
    for action in ["decline", "cancel"] {
        let j = Jobs::new();
        let ask = j.ask();
        let (handle, _version) = j.owner(ask);
        j.rejected(
            "update",
            json!({"handle":handle,"responses":{"other":{"action":"decline"}}}),
            0,
            BINDING,
            &[],
            2,
        );
        j.rejected("update",json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed","unknown":"stale"}}}}),0,BINDING,&[],2);
        j.rejected("update",json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Unknown"}}}}),0,BINDING,&[],6);
        let reply = json!({"handle":handle,"responses":{"owner_choice":{"action":action}}});
        let a = j.call("update", reply.clone());
        assert_eq!(a, j.call("update", reply));
        assert_eq!(a["result"]["structuredContent"]["outcome"], "no_decision");
        assert_eq!(
            j.h.count(
                "select count(*) from events where id=? and answered_by is null",
                ask
            ),
            1
        );
    }
}
#[test]
fn answer_from_another_channel_wins_without_implicit_supersession() {
    let j = Jobs::new();
    let ask = j.ask();
    let (handle, _version) = j.owner(ask);
    j.h.ok(&[], &["answer", &ask.to_string(), "Hold"]);
    j.rejected("update",json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}}),0,BINDING,&[],6);
    assert_eq!(
        j.call("poll", json!({"handle":handle}))["result"]["structuredContent"]["outcome"],
        "answered_elsewhere"
    );
    assert_eq!(
        j.h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            ask
        ),
        1
    );
}
#[test]
fn stale_owner_question_and_old_generation_reply_preserve_the_ask() {
    let j = Jobs::new();
    let ask = j.ask();
    let (handle, _) = j.owner(ask);
    let reply = json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}});
    j.h.conn().execute("update events set data=json_set(data,'$.question.question','Changed question?') where id=?",[ask]).unwrap();
    j.rejected("update", reply, 0, BINDING, &[], 6);
    assert_eq!(
        j.h.count(
            "select count(*) from events where id=? and answered_by is null",
            ask
        ),
        1
    );
    let j = Jobs::new();
    let ask = j.ask();
    let (handle, _) = j.owner(ask);
    j.h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [j.root],
        )
        .unwrap();
    j.rejected("update",json!({"handle":handle,"responses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}}),0,BINDING,&[],6);
    let ended = j.call("poll", json!({"handle":handle}));
    assert_eq!(
        ended["result"]["structuredContent"]["outcome"],
        "no_decision"
    );
    assert_eq!(
        j.h.count(
            "select count(*) from events where id=? and answered_by is null",
            ask
        ),
        1
    );
}
