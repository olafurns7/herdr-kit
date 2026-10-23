#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::Harness;
use taskr_core::db::params;

struct Work {
    h: Harness,
    root: String,
    executor: String,
    reviewer: String,
    adjudicator: String,
}
impl Work {
    fn new() -> Self {
        let h = Harness::new();
        let root =
            h.ok(&[], &["new", "work-root", "--role", "orchestrator"])["task_id"].to_string();
        let make = |name: &str, role: &str| {
            h.ok(
                &[],
                &["new", name, "--role", role, "--parent", &root, "--planned"],
            )["task_id"]
                .to_string()
        };
        let executor = make("executor", "implementer");
        let reviewer = make("reviewer", "reviewer");
        let adjudicator = make("adjudicator", "reviewer");
        Self {
            h,
            root,
            executor,
            reviewer,
            adjudicator,
        }
    }
    fn call(&self, verb: &str, id: Option<&str>, data: Option<Value>) -> Value {
        let mut argv = vec!["work".to_string(), verb.into()];
        if let Some(id) = id {
            argv.push(id.into());
        }
        argv.extend([
            "--as".into(),
            self.root.clone(),
            "--generation".into(),
            "0".into(),
        ]);
        if let Some(v) = data {
            argv.extend(["--data".into(), v.to_string()]);
        }
        self.h
            .ok(&[], &argv.iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn source(&self) -> Value {
        json!({"commit":"a".repeat(40),"tree":"b".repeat(40),"input_digest":"c".repeat(64)})
    }
    fn contract(&self, checks: bool) -> Value {
        let lane = |t: &str, model: &str| json!({"task":t.parse::<i64>().unwrap(),"name":format!("lane-{model}"),"provider":"fixture","model":model,"effort":"medium"});
        json!({"intent":"produce the requested change","acceptance":"independent review is clear","scope":["src"],"authority":"local work only","checkout":{"host":taskr_core::store::local_machine(),"cwd":self.h.home.canonicalize().unwrap().to_str().unwrap()},"base":self.source(),"write_scope":["src"],"dependencies":[],"executor":lane(&self.executor,"executor-model"),"reviewer":lane(&self.reviewer,"review-model"),"adjudicator":lane(&self.adjudicator,"adjudicator-model"),"max_rework":1,"require_adjudicator":false,"profile":{"id":"fixture","version":1,"digest":"d".repeat(64),"checks":if checks{json!([{"id":"unit","kind":"cases","argv":["checker"],"timeout_ms":1000,"cases":["first"]}])}else{json!([])}}})
    }
    fn configure(&self) {
        self.call(
            "configure",
            Some(&self.root),
            Some(json!({"enabled":true,"max_active":4,"max_review_pending":4})),
        );
    }
    fn add(&self, c: Option<Value>) -> String {
        self.call("add",None,Some(json!({"campaign":self.root.parse::<i64>().unwrap(),"title":"work item","contract":c})))["work_id"].to_string()
    }
    fn queued(&self, c: Value) -> String {
        let id = self.add(Some(c));
        self.call("queue", Some(&id), Some(json!({"expected_revision":1})));
        id
    }
    fn claim(&self, id: &str, key: &str) -> Value {
        self.call(
            "claim",
            Some(id),
            Some(json!({"expected_revision":1,"key":key})),
        )
    }
    fn reject(&self, verb: &str, id: &str, data: Value, code: i32) {
        let out = self.h.output(
            &[],
            &[
                "work",
                verb,
                id,
                "--as",
                &self.root,
                "--generation",
                "0",
                "--data",
                &data.to_string(),
            ],
            b"",
        );
        assert_eq!(
            out.status.code(),
            Some(code),
            "{verb}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn env(&self, launch: &Value) -> Vec<(String, String)> {
        vec![
            ("TASKR_TASK".into(), launch["task_id"].to_string()),
            ("TASKR_LAUNCH".into(), launch["launch_id"].to_string()),
        ]
    }
    fn running(&self, ass: &Value) -> Vec<(String, String)> {
        let launch = ass["launch_id"].to_string();
        self.call("bind",Some(&launch),Some(json!({"workspace":"w1","tab":"w1:t2","pane":"w1:p2","dispatch_key":format!("dispatch-{launch}")})));
        let env = self.env(ass);
        self.h.ok(&env, &["start"]); // no hook/session reference; a launch-bound receipt is sufficient
        self.call("started", Some(&launch), None);
        env
    }
    fn submit(&self, ass: &Value, payload: Value) -> Value {
        self.h.ok(
            &self.env(ass),
            &[
                "work",
                "submit",
                &ass["launch_id"].to_string(),
                "--data",
                &payload.to_string(),
            ],
        )
    }
    fn execute(&self, ass: &Value) -> Value {
        self.running(ass);
        self.submit(ass,json!({"kind":"execution","result":{"source":self.source(),"report":"implemented","artifacts":[]}}))
    }
    fn review(&self, run: &str, execution: &Value, verdict: &str) -> Value {
        assert_eq!(self.call("advance", Some(run), None)["action"], "review");
        let ass = self.call("reserve", Some(run), Some(json!({"stage":"reviewer"})));
        self.running(&ass);
        self.submit(&ass,json!({"kind":"review","result":{"source":self.source(),"execution_result":execution["result_id"],"verdict":verdict,"findings":if verdict=="clear"{json!([])}else{json!([{"id":"F1","reason":"repair required","evidence":"src/file:1"}])}}}))
    }
}
#[test]
fn backlog_disabled_parked_claim_replay_conflict_and_generation() {
    let w = Work::new();
    let backlog = w.add(None);
    w.reject("queue", &backlog, json!({"expected_revision":1}), 6);
    let id = w.queued(w.contract(false));
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"first"}),
        6,
    );
    w.configure();
    w.h.ok(&[], &["set", &w.root, "glance.state=parked"]);
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"first"}),
        6,
    );
    w.h.ok(&[], &["set", &w.root, "glance.state="]);
    let run = w.claim(&id, "first");
    assert_eq!(run, w.claim(&id, "first"));
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"other"}),
        6,
    );
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":2,"key":"first"}),
        6,
    );
    let second = w.queued(w.contract(false));
    w.reject(
        "claim",
        &second,
        json!({"expected_revision":1,"key":"first"}),
        6,
    );
    w.reject(
        "claim",
        &second,
        json!({"expected_revision":1,"key":"checkout-conflict"}),
        6,
    );
    w.h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [w.root.parse::<i64>().unwrap()],
        )
        .unwrap();
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"first"}),
        6,
    );
}
#[test]
fn independent_review_acceptance_and_immutable_replay_with_quiet_reports() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "one");
    let ass = &run["assignments"][0];
    let rid = run["run_id"].to_string();
    let out = w.h.output(
        &[],
        &[
            "launch",
            &w.executor,
            "--provider",
            "fixture",
            "--model",
            "other",
            "--effort",
            "medium",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(6));
    let exec = w.execute(ass);
    let payload = json!({"kind":"execution","result":{"source":w.source(),"report":"implemented","artifacts":[]}});
    assert_eq!(
        exec["result_id"],
        w.submit(ass, payload.clone())["result_id"]
    );
    let mut conflict = payload;
    conflict["result"]["report"] = json!("changed");
    assert_eq!(
        w.h.output(
            &w.env(ass),
            &[
                "work",
                "submit",
                &ass["launch_id"].to_string(),
                "--data",
                &conflict.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    w.h.ok(&w.env(ass), &["done", "routine report"]);
    let recipient: Option<i64> =
        w.h.conn()
            .query_row(
                "select recipient_task_id from events where kind='done' order by id desc limit 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
    assert!(recipient.is_none());
    w.review(&rid, &exec, "clear");
    let accepted = w.call("advance", Some(&rid), None);
    assert_eq!(accepted["action"], "accepted");
    assert!(accepted["acceptance_id"].is_i64());
    assert_eq!(accepted, w.call("advance", Some(&rid), None));
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_acceptances", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn proof_requires_future_runner_even_after_clear_review() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(true));
    let run = w.claim(&id, "proof");
    let exec = w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "check");
    let bypass = json!({"kind":"proof","result":{"exit":0,"status":"PASS"}});
    let out = w.h.output(
        &w.env(&run["assignments"][0]),
        &[
            "work",
            "submit",
            &run["assignments"][0]["launch_id"].to_string(),
            "--data",
            &bypass.to_string(),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_acceptances", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn rework_is_bounded_and_executor_cannot_dismiss_review_findings() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "rework");
    let rid = run["run_id"].to_string();
    let exec = w.execute(&run["assignments"][0]);
    w.review(&rid, &exec, "rework");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "rework");
    let ass = w.call("reserve", Some(&rid), Some(json!({"stage":"executor"})));
    let exec = w.execute(&ass);
    w.review(&rid, &exec, "rework");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "escalation");
    let forged = json!({"kind":"review","result":{"source":w.source(),"execution_result":exec["result_id"],"verdict":"clear","findings":[]}});
    assert_eq!(
        w.h.output(
            &w.env(&ass),
            &[
                "work",
                "submit",
                &ass["launch_id"].to_string(),
                "--data",
                &forged.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
}
#[test]
fn semantic_revision_keeps_writer_and_rejects_stale_result_and_recovery() {
    let w = Work::new();
    w.configure();
    let mut c = w.contract(false);
    let id = w.queued(c.clone());
    let run = w.claim(&id, "revision");
    let ass = &run["assignments"][0];
    w.running(ass);
    c["intent"] = json!("new intent");
    w.call(
        "revise",
        Some(&id),
        Some(json!({"expected_revision":1,"contract":c})),
    );
    let stale =
        json!({"kind":"execution","result":{"source":w.source(),"report":"stale","artifacts":[]}});
    assert_eq!(
        w.h.output(
            &w.env(ass),
            &[
                "work",
                "submit",
                &ass["launch_id"].to_string(),
                "--data",
                &stale.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    w.call(
        "uncertain",
        Some(&ass["launch_id"].to_string()),
        Some(json!({"reason":"process state unknown"})),
    );
    w.reject(
        "reconcile",
        &ass["launch_id"].to_string(),
        json!({"stopped_event":1}),
        6,
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn wrong_source_and_model_and_dependency_fail_without_results() {
    let w = Work::new();
    w.configure();
    let mut c = w.contract(false);
    c["reviewer"]["model"] = json!("executor-model");
    let data = json!({"campaign":w.root.parse::<i64>().unwrap(),"title":"invalid","contract":c});
    assert_eq!(
        w.h.output(
            &[],
            &[
                "work",
                "add",
                "--as",
                &w.root,
                "--generation",
                "0",
                "--data",
                &data.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(2)
    );
    let backlog = w.add(None);
    let mut c = w.contract(false);
    c["dependencies"] = json!([{"work":backlog.parse::<i64>().unwrap(),"revision":1}]);
    let id = w.queued(c);
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"unaccepted-dep"}),
        6,
    );
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "source");
    let exec = w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    w.call("advance", Some(&rid), None);
    let review = w.call("reserve", Some(&rid), Some(json!({"stage":"reviewer"})));
    w.running(&review);
    let mut source = w.source();
    source["tree"] = json!("e".repeat(40));
    let data = json!({"kind":"review","result":{"source":source,"execution_result":exec["result_id"],"verdict":"clear","findings":[]}});
    assert_eq!(
        w.h.output(
            &w.env(&review),
            &[
                "work",
                "submit",
                &review["launch_id"].to_string(),
                "--data",
                &data.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
}
#[test]
fn compatible_notices_are_self_addressed_and_parked_roots_get_audit_only() {
    let w = Work::new();
    w.add(None);
    let (sender,to,reason):(i64,i64,String)=w.h.conn().query_row("select task_id,recipient_task_id,json_extract(data,'$.notice.reason') from events where kind='message' order by id desc limit 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(sender, to);
    assert_eq!(reason, "added");
    w.h.ok(&[], &["set", &w.root, "glance.state=parked"]);
    let before: i64 =
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='message'",
                [],
                |r| r.get(0),
            )
            .unwrap();
    w.add(None);
    assert_eq!(
        before,
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='message'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
    );
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='work' and summary='added'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
}
#[test]
fn simultaneous_claims_have_one_run_and_launch() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let argv = |key: &str| {
        vec![
            "work".to_string(),
            "claim".into(),
            id.clone(),
            "--as".into(),
            w.root.clone(),
            "--generation".into(),
            "0".into(),
            "--data".into(),
            json!({"expected_revision":1,"key":key}).to_string(),
        ]
    };
    let mut a =
        w.h.command(&[])
            .args(argv("race-a"))
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
    let mut b =
        w.h.command(&[])
            .args(argv("race-b"))
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
    let mut codes = vec![
        a.wait().unwrap().code().unwrap(),
        b.wait().unwrap().code().unwrap(),
    ];
    codes.sort();
    assert_eq!(codes, vec![0, 6]);
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_assignments", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn reducing_limits_does_not_cancel_reservation() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let first = w.claim(&id, "limited");
    w.call(
        "configure",
        Some(&w.root),
        Some(json!({"enabled":true,"max_active":1,"max_review_pending":1})),
    );
    let second = w.queued(w.contract(false));
    w.reject(
        "claim",
        &second,
        json!({"expected_revision":1,"key":"limited-two"}),
        6,
    );
    assert_eq!(
        w.h.conn()
            .query_row(
                "select state from work_runs where id=?",
                params![first["run_id"].as_i64().unwrap()],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "active"
    );
}

#[test]
fn unassigned_contract_creates_fresh_lanes_at_reservation_only() {
    let w = Work::new();
    w.configure();
    let mut c = w.contract(false);
    for role in ["executor", "reviewer", "adjudicator"] {
        c[role]["task"] = Value::Null;
    }
    let count = || {
        w.h.conn()
            .query_row("select count(*) from tasks", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    let before = count();
    let id = w.queued(c);
    assert_eq!(count(), before);
    let run = w.claim(&id, "unassigned");
    assert_eq!(count(), before + 1);
    let executor = &run["assignments"][0];
    assert_ne!(executor["task_id"].to_string(), w.executor);
    let exec = w.execute(executor);
    let rid = run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    assert_eq!(count(), before + 2);
    let generated_role: String = w.h.conn().query_row("select t.role from tasks t join work_assignments a on a.lane_id=t.id where a.run_id=? and a.stage='reviewer'", [run["run_id"].as_i64().unwrap()], |r| r.get(0)).unwrap();
    assert_eq!(generated_role, "reviewer");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "accepted");
}

#[test]
fn dispute_requires_distinct_adjudicator_and_exact_finding_identity() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "disputed");
    let exec = w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    let review = w.review(&rid, &exec, "disputed");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "adjudicate");
    let adj = w.call("reserve", Some(&rid), Some(json!({"stage":"adjudicator"})));
    w.running(&adj);
    let mut data = json!({"kind":"adjudication","result":{"source":w.source(),"review_result":review["result_id"],"finding_ids":["wrong"],"verdict":"clear","reason":"Independent resolution"}});
    assert_eq!(
        w.h.output(
            &w.env(&adj),
            &[
                "work",
                "submit",
                &adj["launch_id"].to_string(),
                "--data",
                &data.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    data["result"]["finding_ids"] = json!(["F1"]);
    w.submit(&adj, data);
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "accepted");
}

#[test]
fn dependency_revision_invalidates_inflight_acceptance_and_completed_dependents() {
    let w = Work::new();
    w.configure();
    let dep = w.queued(w.contract(false));
    let dep_run = w.claim(&dep, "dep");
    let exec = w.execute(&dep_run["assignments"][0]);
    let rid = dep_run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    w.call("advance", Some(&rid), None);
    let mut c = w.contract(false);
    c["dependencies"] = json!([{"work":dep.parse::<i64>().unwrap(),"revision":1}]);
    let dependent = w.queued(c);
    let run = w.claim(&dependent, "dependent");
    let exec = w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    w.call("advance", Some(&rid), None);
    let mut c = w.contract(false);
    c["intent"] = json!("new dependency intent");
    w.call(
        "revise",
        Some(&dep),
        Some(json!({"expected_revision":1,"contract":c})),
    );
    let shown = w.h.ok(&[], &["work", "show", &dependent]);
    assert_eq!(shown["state"], "done");
    assert!(shown["reason"].as_str().unwrap().contains("recheck"));
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_acceptances", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn no_hook_got_receipt_can_be_reconciled_but_wrong_launch_cannot_start() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "got-receipt");
    let ass = &run["assignments"][0];
    let launch = ass["launch_id"].as_i64().unwrap();
    let lid = launch.to_string();
    w.call(
        "bind",
        Some(&lid),
        Some(json!({"workspace":"w1","tab":"w1:t2","pane":"w1:p2","dispatch_key":"got-dispatch"})),
    );
    // A receipt on another launch must not count, even for the same campaign.
    let db = w.h.conn();
    db.execute(
        "insert into events(task_id,kind,created_at) values(?,'start','t')",
        [w.root.parse::<i64>().unwrap()],
    )
    .unwrap();
    assert_eq!(
        w.h.output(
            &[],
            &[
                "work",
                "started",
                &lid,
                "--as",
                &w.root,
                "--generation",
                "0"
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    db.execute(
        "insert into events(task_id,launch_id,kind,created_at) values(?,?,'prompt','t')",
        params![ass["task_id"].as_i64().unwrap(), launch],
    )
    .unwrap();
    let prompt = db.last_insert_rowid();
    w.h.ok(&w.env(ass), &["got", &prompt.to_string()]);
    let reconciled = w.call("reconcile", Some(&lid), Some(json!({})));
    assert_eq!(reconciled["state"], "running");
    assert_eq!(reconciled["start_proof"]["kind"], "receipt");
}

#[test]
fn self_notice_exhaustion_does_not_alarm_and_old_filtered_waits_see_it() {
    let w = Work::new();
    w.add(None);
    let event: i64 =
        w.h.conn()
            .query_row(
                "select id from events where kind='message' order by id desc limit 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
    let out = w.h.output(
        &[],
        &["wait", "--as", &w.root, "--for", "ready", "--timeout", "0"],
        b"",
    );
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\tm\t"));
    w.h.ok(
        &[],
        &[
            "consumer",
            "take",
            "--as",
            &w.root,
            "--session",
            "0",
            "--holder",
            "mcp-channel-fixture",
            "--ttl",
            "60000",
        ],
    );
    for _ in 0..3 {
        w.h.ok(
            &[],
            &[
                "delivery",
                "new",
                &event.to_string(),
                "--as",
                &w.root,
                "--session",
                "0",
                "--adapter",
                "mcp-channel-fixture",
                "--fence",
                "1",
            ],
        );
    }
    w.h.ok(
        &[],
        &[
            "delivery",
            "exhausted",
            &event.to_string(),
            "--as",
            &w.root,
            "--session",
            "0",
            "--fence",
            "1",
        ],
    );
    assert_eq!(w.h.conn().query_row("select count(*) from events where kind='herdr' and json_extract(data,'$.reason')='message_unreceived'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn malformed_local_json_and_unknown_flags_do_not_create_work() {
    let w = Work::new();
    for args in [
        vec![
            "work",
            "add",
            "--as",
            &w.root,
            "--generation",
            "0",
            "--data",
            "{",
        ],
        vec![
            "work",
            "add",
            "--as",
            &w.root,
            "--generation",
            "0",
            "--mystery",
            "value",
        ],
        vec![
            "work",
            "add",
            "--as",
            &w.root,
            "--generation",
            "0",
            "--file",
            "/missing/example.json",
        ],
    ] {
        assert_eq!(w.h.output(&[], &args, b"").status.code(), Some(2));
    }
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_items", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
