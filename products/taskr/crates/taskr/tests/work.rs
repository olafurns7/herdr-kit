#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::Harness;
use taskr_core::db::{OptionalExtension, params};
use taskr_core::store::work as api;

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
    fn call(&self, verb: &str, id: Option<&str>, mut data: Option<Value>) -> Value {
        if let Some(data) = data.as_mut() {
            if let Some(contract) = data.get_mut("contract").filter(|c| !c.is_null()) {
                self.seal(contract);
            }
            if verb == "configure" && data["enabled"] == true && data.get("workspace").is_none() {
                data["workspace"] = json!("w1");
            }
        }
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
        let digest:Option<String>=self.h.conn().query_row("select json_extract(contract,'$.base.input_digest') from work_revisions where contract is not null order by rowid desc limit 1",[],|r|r.get(0)).optional().unwrap();
        json!({"commit":"a".repeat(40),"tree":"b".repeat(40),"input_digest":digest.unwrap_or("c".repeat(64))})
    }
    fn seal(&self, c: &mut Value) {
        let mut profile: api::Profile = serde_json::from_value(c["profile"].clone()).unwrap();
        profile.digest = api::profile_digest(&profile).unwrap();
        let deps=c["dependencies"].as_array().unwrap().iter().map(|d| {
            let digest:String=self.h.conn().query_row("select json_extract(contract,'$.base.input_digest') from work_revisions where work_id=? and revision=?",params![d["work"].as_i64().unwrap(),d["revision"].as_i64().unwrap()],|r|r.get(0)).unwrap();
            api::DependencyInput{work:d["work"].as_i64().unwrap(),revision:d["revision"].as_i64().unwrap(),input_digest:digest}
        }).collect::<Vec<_>>();
        c["base"]["input_digest"] = json!(api::input_digest(&profile, &deps).unwrap());
        c["profile"] = serde_json::to_value(profile).unwrap();
    }
    fn contract(&self, checks: bool) -> Value {
        let lane = |t: &str, model: &str| json!({"task":t.parse::<i64>().unwrap(),"name":format!("lane-{model}"),"provider":"fixture","model":model,"effort":"medium"});
        json!({"intent":"produce the requested change","acceptance":"independent review is clear","scope":["src"],"authority":"local work only","checkout":{"host":taskr_core::store::local_machine(),"cwd":self.h.home.canonicalize().unwrap().to_str().unwrap()},"base":self.source(),"write_scope":["src"],"dependencies":[],"executor":lane(&self.executor,"executor-model"),"reviewer":lane(&self.reviewer,"review-model"),"adjudicator":lane(&self.adjudicator,"adjudicator-model"),"max_rework":1,"require_adjudicator":false,"profile":{"id":"fixture","version":1,"digest":"d".repeat(64),"checks":if checks{json!([{"id":"unit","kind":"cases","argv":["checker"],"timeout_ms":1000,"cases":["first"],"receipt_file":"cases.json","admission":"direct"}])}else{json!([])}}})
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
        if ass["stage"] == "gate" {
            self.call("started", Some(&launch), None);
        } else {
            self.call("started",Some(&launch),Some(json!({"pane":"w1:p2","native_session":format!("native-{launch}"),"observed_at":taskr_core::store::now()})));
        }
        env
    }
    fn stop(&self, ass: &Value) -> Value {
        let launch = ass["launch_id"].to_string();
        self.call("stopped",Some(&launch),Some(json!({"pane":"w1:p2","native_session":if ass["stage"]=="gate"{Value::Null}else{json!(format!("native-{launch}"))},"foreground":{"pid":123,"command":"/bin/sh"},"agent_absent":true,"observed_at":taskr_core::store::now()})))
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
        let result=self.submit(ass,json!({"kind":"execution","result":{"source":self.source(),"report":"implemented","artifacts":[]}}));
        self.stop(ass);
        result
    }
    fn proof(&self, run: &Value, gate: &Value, exit: i32) -> Value {
        let snapshot = self
            .h
            .ok(&[], &["work", "inputs", &run["work_id"].to_string()]);
        json!({"kind":"proof","result":{"source":self.source(),"profile_digest":snapshot["profile"]["digest"],"dependencies":snapshot["dependency_receipts"],"host":taskr_core::store::local_machine(),"runner_version":"fixture-v1","environment":[{"name":"tool","value":"synthetic"}],"checks":[{"run":run["run_id"],"revision":run["revision"],"source":self.source(),"profile_digest":snapshot["profile"]["digest"],"check":"unit","checker_launch":gate["launch_id"],"argv":["checker"],"exit":exit,"signal":null,"timed_out":false,"error":null,"admission":{"method":"direct","ticket":null,"reason":null},"stdout":{"sha256":"e".repeat(64),"bytes":4,"truncated":false},"stderr":{"sha256":"e".repeat(64),"bytes":0,"truncated":false},"cases":[{"id":"first","status":"passed"}],"artifacts":[{"name":"receipt","sha256":"e".repeat(64)}]}]}})
    }
    fn review(&self, run: &str, execution: &Value, verdict: &str) -> Value {
        assert_eq!(self.call("advance", Some(run), None)["action"], "review");
        let ass = self.call("reserve", Some(run), Some(json!({"stage":"reviewer"})));
        self.running(&ass);
        let result=self.submit(&ass,json!({"kind":"review","result":{"source":self.source(),"execution_result":execution["result_id"],"verdict":verdict,"findings":if verdict=="clear"{json!([])}else{json!([{"id":"F1","reason":"repair required","evidence":"src/file:1"}])}}}));
        self.stop(&ass);
        result
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
fn assigned_gate_proof_precedes_review_and_requires_stage_stop() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(true));
    let run = w.claim(&id, "proof");
    let ass = &run["assignments"][0];
    w.running(ass);
    let exec=w.submit(ass,json!({"kind":"execution","result":{"source":w.source(),"report":"implemented","artifacts":[]}}));
    let rid = run["run_id"].to_string();
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "stop");
    w.reject("reserve", &rid, json!({"stage":"gate"}), 6);
    w.stop(ass);
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "check");
    w.reject("reserve", &rid, json!({"stage":"reviewer"}), 6);
    let gate = w.call("reserve", Some(&rid), Some(json!({"stage":"gate"})));
    assert_eq!(gate["lane"]["provider"], "shell");
    assert_eq!(gate["lane"]["model"], "none");
    w.running(&gate);
    w.submit(&gate, w.proof(&run, &gate, 0));
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "stop");
    w.stop(&gate);
    w.review(&rid, &exec, "clear");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "accepted");
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
    let backlog = w.add(Some(w.contract(false)));
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
    let id = w.add(None);
    w.call("block", Some(&id), Some(json!({"reason":"fixture block"})));
    let (sender,to,reason):(i64,i64,String)=w.h.conn().query_row("select task_id,recipient_task_id,json_extract(data,'$.notice.reason') from events where kind='message' order by id desc limit 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(sender, to);
    assert_eq!(reason, "blocked");
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
    w.stop(&adj);
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
    let id = w.add(None);
    w.call("block", Some(&id), Some(json!({"reason":"fixture block"})));
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

#[test]
fn reused_lane_cannot_move_to_another_active_run_after_submission() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "owns-lane");
    w.execute(&run["assignments"][0]);
    let mut second = w.contract(false);
    second["checkout"]["cwd"] = json!(
        w.h.home
            .canonicalize()
            .unwrap()
            .join("isolated")
            .to_str()
            .unwrap()
    );
    let id = w.queued(second);
    w.reject(
        "claim",
        &id,
        json!({"expected_revision":1,"key":"steal-settled-lane"}),
        6,
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn failed_gate_is_durable_rework_before_review_and_old_gate_cannot_submit() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(true));
    let run = w.claim(&id, "failed-gate");
    w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "check");
    let gate = w.call("reserve", Some(&rid), Some(json!({"stage":"gate"})));
    w.running(&gate);
    let mut proof = w.proof(&run, &gate, 1);
    proof["result"]["environment"] = json!([{"name":"claimed_stdout","value":"PASS"}]);
    w.submit(&gate, proof.clone());
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "stop");
    assert_eq!(
        w.h.conn()
            .query_row("select round from work_runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    w.stop(&gate);
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "rework");
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from work_assignments where stage='reviewer'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        w.h.output(
            &w.env(&gate),
            &[
                "work",
                "submit",
                &gate["launch_id"].to_string(),
                "--data",
                &proof.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='message'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "submitted and bounded rework are audit-only"
    );
}

#[test]
fn gate_case_failures_cannot_accept_and_share_the_total_rework_budget() {
    for cases in [
        json!([]),
        json!([{"id":"first","status":"skipped"}]),
        json!([{"id":"first","status":"failed"}]),
        json!([{"id":"first","status":"passed"},{"id":"first","status":"passed"}]),
        json!([{"id":"wrong","status":"passed"}]),
    ] {
        let w = Work::new();
        w.configure();
        let mut c = w.contract(true);
        c["max_rework"] = json!(0);
        let id = w.queued(c);
        let run = w.claim(&id, "cases");
        w.execute(&run["assignments"][0]);
        let rid = run["run_id"].to_string();
        w.call("advance", Some(&rid), None);
        let gate = w.call("reserve", Some(&rid), Some(json!({"stage":"gate"})));
        w.running(&gate);
        let mut proof = w.proof(&run, &gate, 0);
        proof["result"]["checks"][0]["cases"] = cases;
        w.submit(&gate, proof);
        w.stop(&gate);
        assert_eq!(w.call("advance", Some(&rid), None)["action"], "escalation");
        assert_eq!(
            w.h.conn()
                .query_row("select count(*) from work_acceptances", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            w.h.conn()
                .query_row("select count(*) from work_checkouts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn gate_proof_rejects_wrong_role_source_profile_dependencies_and_input_drift() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(true));
    let run = w.claim(&id, "proof-identity");
    let exec = &run["assignments"][0];
    w.running(exec);
    let forged = w.proof(&run, exec, 0);
    assert_eq!(
        w.h.output(
            &w.env(exec),
            &[
                "work",
                "submit",
                &exec["launch_id"].to_string(),
                "--data",
                &forged.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    w.submit(exec,json!({"kind":"execution","result":{"source":w.source(),"report":"implemented","artifacts":[]}}));
    w.stop(exec);
    let rid = run["run_id"].to_string();
    w.call("advance", Some(&rid), None);
    let gate = w.call("reserve", Some(&rid), Some(json!({"stage":"gate"})));
    w.running(&gate);
    let good = w.proof(&run, &gate, 0);
    for field in ["tree", "input_digest", "profile_digest", "dependencies"] {
        let mut bad = good.clone();
        match field {
            "tree" => bad["result"]["source"]["tree"] = json!("f".repeat(40)),
            "input_digest" => bad["result"]["source"]["input_digest"] = json!("f".repeat(64)),
            "profile_digest" => bad["result"]["profile_digest"] = json!("f".repeat(64)),
            _ => bad["result"]["dependencies"] = json!([{"work":1,"revision":1,"acceptance":1}]),
        }
        assert_eq!(
            w.h.output(
                &w.env(&gate),
                &[
                    "work",
                    "submit",
                    &gate["launch_id"].to_string(),
                    "--data",
                    &bad.to_string()
                ],
                b""
            )
            .status
            .code(),
            Some(6),
            "{field}"
        );
    }
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_proofs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    w.submit(&gate, good);
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_proofs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn settled_result_can_be_uncertain_and_normal_stop_keeps_checkout_and_run() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "stop-safety");
    let ass = &run["assignments"][0];
    w.running(ass);
    w.submit(ass,json!({"kind":"execution","result":{"source":w.source(),"report":"implemented","artifacts":[]}}));
    let uncertain = w.call(
        "uncertain",
        Some(&ass["launch_id"].to_string()),
        Some(json!({"reason":"observation lost after submit"})),
    );
    assert_eq!(uncertain["state"], "settled");
    assert_eq!(uncertain["process_state"], "uncertain");
    let rid = run["run_id"].to_string();
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "stop");
    w.stop(ass);
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "review");
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        w.h.conn()
            .query_row("select state from work_runs", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "active"
    );
}

#[test]
fn late_normal_reconcile_never_reverts_accepted_work_or_duplicates_acceptance() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "late-stop");
    let ass = &run["assignments"][0];
    let exec = w.execute(ass);
    let rid = run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    w.call("advance", Some(&rid), None);
    let event: i64 =
        w.h.conn()
            .query_row(
                "select stop_event from work_assignments where launch_id=?",
                [ass["launch_id"].as_i64().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
    w.call(
        "reconcile",
        Some(&ass["launch_id"].to_string()),
        Some(json!({"stopped_event":event})),
    );
    assert_eq!(w.h.ok(&[], &["work", "show", &id])["state"], "done");
    assert_eq!(w.call("advance", Some(&rid), None)["state"], "accepted");
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_acceptances", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    w.reject(
        "reconcile",
        &ass["launch_id"].to_string(),
        json!({"stopped_event":event,"abandon":true}),
        6,
    );
}

#[test]
fn undispatched_uncertainty_remains_cancellable_and_live_lane_close_is_guarded() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "no-dispatch");
    let ass = &run["assignments"][0];
    let tid = ass["task_id"].to_string();
    assert_eq!(
        w.h.output(&[], &["close", &tid], b"").status.code(),
        Some(6)
    );
    w.call(
        "uncertain",
        Some(&ass["launch_id"].to_string()),
        Some(json!({"reason":"dispatch never happened"})),
    );
    w.call(
        "cancel",
        Some(&id),
        Some(json!({"reason":"cancel undispatched"})),
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    w.h.ok(&[], &["close", &tid]);
}

#[test]
fn block_is_idempotent_and_reports_before_submit_stay_quiet() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='message'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "root verbs need no self-notices"
    );
    w.call(
        "block",
        Some(&id),
        Some(json!({"reason":"missing capacity"})),
    );
    let before: i64 =
        w.h.conn()
            .query_row("select count(*) from events", [], |r| r.get(0))
            .unwrap();
    assert_eq!(
        w.call(
            "block",
            Some(&id),
            Some(json!({"reason":"missing capacity"}))
        )["duplicate"],
        true
    );
    assert_eq!(
        before,
        w.h.conn()
            .query_row("select count(*) from events", [], |r| r.get::<_, i64>(0))
            .unwrap()
    );
    let run = w.claim(&id, "quiet-before-submit");
    let ass = &run["assignments"][0];
    w.running(ass);
    let report = w.h.home.join("report.md");
    std::fs::write(&report, "synthetic progress").unwrap();
    w.h.ok(
        &w.env(ass),
        &["ready", "progress", "--report", report.to_str().unwrap()],
    );
    w.h.ok(&w.env(ass), &["done", "routine"]);
    assert_eq!(w.h.conn().query_row("select count(*) from events where kind in ('ready','done') and recipient_task_id is not null",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    w.h.ok(&w.env(ass), &["fail", "urgent exception"]);
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='fail' and recipient_task_id is not null",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn creation_key_is_canonical_durable_and_conflicting_reuse_is_refused() {
    let w = Work::new();
    let data =
        json!({"campaign":w.root.parse::<i64>().unwrap(),"title":"keyed","key":"native-add-1"});
    let first = w.call("add", None, Some(data.clone()));
    let replay = w.call("add", None, Some(data));
    assert_eq!(first["work_id"], replay["work_id"]);
    let changed =
        json!({"campaign":w.root.parse::<i64>().unwrap(),"title":"changed","key":"native-add-1"});
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
                &changed.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_items", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn stopped_requires_bound_native_pane_shell_and_root_authority() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "stop-observation");
    let ass = &run["assignments"][0];
    w.running(ass);
    let lid = ass["launch_id"].to_string();
    let good = json!({"pane":"w1:p2","native_session":format!("native-{lid}"),"foreground":{"pid":123,"command":"/bin/sh"},"agent_absent":true,"observed_at":taskr_core::store::now()});
    for field in ["pane", "native_session", "foreground", "agent_absent"] {
        let mut bad = good.clone();
        bad[field] = match field {
            "pane" => json!("w1:p9"),
            "native_session" => json!("replacement"),
            "foreground" => json!({"pid":123,"command":"model-process"}),
            _ => json!(false),
        };
        w.reject("stopped", &lid, bad, 6);
    }
    assert_eq!(
        w.h.output(
            &w.env(ass),
            &[
                "work",
                "stopped",
                &lid,
                "--as",
                &w.root,
                "--generation",
                "0",
                "--data",
                &good.to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6)
    );
    w.h.conn()
        .execute(
            "update launches set session_ref='replacement-session' where id=?",
            [ass["launch_id"].as_i64().unwrap()],
        )
        .unwrap();
    w.reject("stopped", &lid, good, 6);
    assert_eq!(
        w.h.conn()
            .query_row(
                "select count(*) from events where kind='work_process_stopped'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn campaigns_read_lists_only_enabled_roots_on_authenticated_host() {
    let w = Work::new();
    assert!(
        w.h.ok(&[], &["work", "campaigns"])["campaigns"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    w.configure();
    let snapshot = w.h.ok(&[], &["work", "campaigns"]);
    assert_eq!(snapshot["campaigns"][0]["campaign"].to_string(), w.root);
    assert_eq!(snapshot["campaigns"][0]["configuration"]["workspace"], "w1");
    w.h.conn()
        .execute(
            "update tasks set machine='other-host' where id=?",
            [w.root.parse::<i64>().unwrap()],
        )
        .unwrap();
    assert!(
        w.h.ok(&[], &["work", "campaigns"])["campaigns"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn hash_command_and_store_reject_profile_input_drift() {
    let w = Work::new();
    let c = w.contract(false);
    let hashed = w.h.ok(
        &[],
        &[
            "work",
            "hash",
            "--data",
            &json!({"profile":c["profile"],"dependencies":[]}).to_string(),
        ],
    );
    assert_eq!(
        hashed["profile_digest"],
        api::profile_digest(&serde_json::from_value(c["profile"].clone()).unwrap()).unwrap()
    );
    let mut good = c;
    w.seal(&mut good);
    for field in ["profile", "input"] {
        let mut bad = good.clone();
        if field == "profile" {
            bad["profile"]["digest"] = json!("f".repeat(64));
        } else {
            bad["base"]["input_digest"] = json!("f".repeat(64));
        }
        let data =
            json!({"campaign":w.root.parse::<i64>().unwrap(),"title":"drift","contract":bad});
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
            Some(6)
        );
    }
}

#[test]
fn fast_gate_submission_derives_only_its_recorded_start_before_controller_tick() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(true));
    let run = w.claim(&id, "fast-gate");
    let exec = w.execute(&run["assignments"][0]);
    let rid = run["run_id"].to_string();
    w.call("advance", Some(&rid), None);
    let gate = w.call("reserve", Some(&rid), Some(json!({"stage":"gate"})));
    let lid = gate["launch_id"].to_string();
    w.call("bind",Some(&lid),Some(json!({"workspace":"w1","tab":"w1:t2","pane":"w1:p2","dispatch_key":"fast-gate-dispatch"})));
    assert_eq!(
        w.h.output(
            &w.env(&gate),
            &[
                "work",
                "submit",
                &lid,
                "--data",
                &w.proof(&run, &gate, 0).to_string()
            ],
            b""
        )
        .status
        .code(),
        Some(6),
        "no matching receipt cannot start a gate"
    );
    w.h.ok(&w.env(&gate), &["start"]);
    w.submit(&gate, w.proof(&run, &gate, 0));
    let ass = w.h.ok(&[], &["work", "show", &id])["runs"][0]["assignments"][1].clone();
    assert_eq!(ass["state"], "settled");
    assert_eq!(ass["process_state"], "running");
    assert!(ass["native_session"].is_null());
    w.stop(&gate);
    w.review(&rid, &exec, "clear");
    assert_eq!(w.call("advance", Some(&rid), None)["action"], "accepted");
}

#[test]
fn acceptance_uses_native_route_transactionally_even_when_parked_and_keeps_audit() {
    for parked in [false, true] {
        let w = Work::new();
        w.configure();
        let id = w.queued(w.contract(false));
        let run = w.claim(&id, "native-route");
        let exec = w.execute(&run["assignments"][0]);
        let rid = run["run_id"].to_string();
        w.review(&rid, &exec, "clear");
        let db = w.h.conn();
        let now = taskr_core::store::now();
        let deadline = taskr_core::store::stamp(
            taskr_core::store::parse_time(&now).unwrap() + time::Duration::seconds(90),
        );
        db.execute("insert into jobs_operations(id,root_id,generation,binding,host,kind,reference_id,revision,status,created_at,updated_at) values('native-op',?,0,'synthetic-binding-token',?,'await',?,1,'working',?,?)",params![w.root.parse::<i64>().unwrap(),taskr_core::store::local_machine(),id.parse::<i64>().unwrap(),now,now]).unwrap();
        db.execute(
            "insert into jobs_routes(operation_id,state,deadline) values('native-op','active',?)",
            [deadline],
        )
        .unwrap();
        if parked {
            w.h.ok(&[], &["set", &w.root, "glance.state=parked"]);
        }
        assert_eq!(w.call("advance", Some(&rid), None)["action"], "accepted");
        assert_eq!(
            db.query_row(
                "select result_id from jobs_routes where operation_id='native-op'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            exec["result_id"].as_i64().unwrap()
        );
        assert_eq!(
            db.query_row(
                "select status from jobs_operations where id='native-op'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "completed"
        );
        assert_eq!(
            db.query_row(
                "select count(*) from events where kind='work' and summary='accepted'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(db.query_row("select count(*) from events where kind='message' and json_extract(data,'$.notice.reason')='accepted'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
}

#[test]
fn daemon_reads_stay_thin_bounded_and_leave_detail_history_compatible() {
    let w = Work::new();
    w.configure();
    let old = w.queued(w.contract(false));
    let run = w.claim(&old, "historical-report");
    let ass = &run["assignments"][0];
    w.running(ass);
    let report = "x".repeat(65536);
    let exec = w.submit(
        ass,
        json!({"kind":"execution","result":{"source":w.source(),"report":report,"artifacts":[]}}),
    );
    w.stop(ass);
    let rid = run["run_id"].to_string();
    w.review(&rid, &exec, "clear");
    w.call("advance", Some(&rid), None);
    w.add(None); // unassigned backlog is not dispatchable
    let mut queued = Vec::new();
    for priority in [1, 3, 2] {
        let id = w.queued(w.contract(false));
        w.call(
            "revise",
            Some(&id),
            Some(json!({"expected_revision":1,"priority":priority})),
        );
        queued.push(id);
    }
    let campaigns = w.h.ok(&[], &["work", "campaigns"]);
    assert!(campaigns.to_string().len() < 8192);
    assert!(campaigns["campaigns"][0].get("work").is_none());
    assert!(campaigns["campaigns"][0].get("configuration").is_some());
    let thin = w.h.ok(&[], &["work", "dispatch", &w.root, "--limit", "2"]);
    assert_eq!(thin["returned"], 2);
    assert_eq!(thin["limit"], 2);
    assert_eq!(thin["truncated"], true);
    assert_eq!(thin["work"][0]["work_id"].to_string(), queued[1]);
    assert_eq!(thin["work"][1]["work_id"].to_string(), queued[2]);
    assert!(thin.to_string().len() < 4096);
    assert!(thin["work"][0].get("runs").is_none());
    w.call(
        "cancel",
        Some(&queued[0]),
        Some(json!({"reason":"cancel queued fixture"})),
    );
    let thin = w.h.ok(&[], &["work", "dispatch", &w.root]);
    assert_eq!(thin["returned"], 2);
    assert_eq!(thin["truncated"], false);
    let detail = w.h.ok(&[], &["work", "show", &old]);
    assert_eq!(
        detail["runs"][0]["results"][0]["payload"]["result"]["report"]
            .as_str()
            .unwrap()
            .len(),
        65536
    );
    assert!(
        w.h.ok(&[], &["work", "list", &w.root])["work"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["work_id"].to_string() == old)
    );
    for limit in ["0", "65", "-1"] {
        assert_eq!(
            w.h.output(&[], &["work", "dispatch", &w.root, "--limit", limit], b"")
                .status
                .code(),
            Some(2)
        );
    }
}

#[test]
fn restarted_dispatch_finds_cancelled_cleanup_without_relaunching_and_omits_it_after_stop() {
    let w = Work::new();
    w.configure();
    let id = w.queued(w.contract(false));
    let run = w.claim(&id, "cancel-restart");
    let ass = &run["assignments"][0];
    w.running(ass);
    w.call(
        "cancel",
        Some(&id),
        Some(json!({"reason":"owner cancelled while process was live"})),
    );
    let queued = w.queued(w.contract(false));
    let launches_before: i64 =
        w.h.conn()
            .query_row("select count(*) from launches", [], |r| r.get(0))
            .unwrap();
    // A new CLI process has no controller memory: only the durable projection can recover cleanup.
    let fresh = w.h.ok(&[], &["work", "dispatch", &w.root, "--limit", "1"]);
    assert_eq!(fresh["returned"], 1);
    assert_eq!(fresh["truncated"], true);
    assert_eq!(fresh["work"][0]["work_id"].to_string(), id);
    assert_eq!(fresh["work"][0]["state"], "cancelled");
    assert_eq!(fresh["work"][0]["current_run"], run["run_id"]);
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from launches", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        launches_before
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    w.stop(ass);
    let fresh = w.h.ok(&[], &["work", "dispatch", &w.root]);
    assert_eq!(fresh["returned"], 1);
    assert_eq!(fresh["work"][0]["work_id"].to_string(), queued);
    assert_eq!(w.h.ok(&[], &["work", "show", &id])["state"], "cancelled");
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from work_checkouts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        w.h.conn()
            .query_row("select count(*) from launches", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        launches_before
    );
}
