//! Controlled fixtures only. These tests never address a live daemon or ledger.
use super::*;
use std::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "taskr-controller-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(fs::canonicalize(p).unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Fake {
    scratch: Scratch,
    snapshot: RefCell<Value>,
    calls: RefCell<Vec<String>>,
    agents: RefCell<Vec<Value>>,
    log: RefCell<Vec<Value>>,
    unavailable: Cell<bool>,
    crash: Cell<bool>,
    next: RefCell<Value>,
    drift: Cell<bool>,
    pending_truncated: Cell<bool>,
    stale_show: Cell<bool>,
}
impl Fake {
    fn new() -> Self {
        let scratch = Scratch::new();
        let c = json!({"intent":"fix fixture","acceptance":"fixture passes","scope":["src"],"authority":"local only","checkout":{"host":store::local_machine(),"cwd":scratch.0},"base":{"commit":"a".repeat(40),"tree":"b".repeat(40),"input_digest":"c".repeat(64)},"write_scope":["src"],"dependencies":[],"executor":{"task":null,"name":"synthetic-exec","provider":"claude","model":"synthetic-exec","effort":"medium"},"reviewer":{"task":null,"name":"synthetic-review","provider":"codex","model":"synthetic-review","effort":"high"},"adjudicator":null,"max_rework":2,"require_adjudicator":false,"profile":{"id":"fixture","version":1,"digest":"d".repeat(64),"checks":[]}});
        let work = json!({"work_id":1,"campaign":3,"revision":1,"state":"queued","current_run":null,"revisions":[{"revision":1,"contract":c}],"runs":[]});
        Self {
            scratch,
            snapshot: RefCell::new(
                json!({"campaign":3,"host":store::local_machine(),"generation":7,"parked":false,"configuration":{"enabled":true,"workspace":"w1","host":store::local_machine(),"no_slotr":false},"work":[work]}),
            ),
            calls: RefCell::new(Vec::new()),
            agents: RefCell::new(Vec::new()),
            log: RefCell::new(Vec::new()),
            unavailable: Cell::new(false),
            crash: Cell::new(false),
            next: RefCell::new(
                json!({"action":"stop","launch_id":11,"run_id":9,"round":1,"assignments":[],"results":[]}),
            ),
            drift: Cell::new(false),
            pending_truncated: Cell::new(false),
            stale_show: Cell::new(false),
        }
    }
    fn run() -> Value {
        json!({"run_id":9,"work_id":1,"revision":1,"round":1,"state":"active","action":"reserve","assignments":[{"launch_id":11,"task_id":12,"run_id":9,"round":1,"stage":"executor","state":"reserved","process_state":"reserved","native_session":null,"dispatch":null,"lane":{"task":null,"name":"synthetic-exec","provider":"claude","model":"synthetic-exec","effort":"medium"}}],"results":[]})
    }
    fn call(&self, s: impl Into<String>) {
        self.calls.borrow_mut().push(s.into());
    }
    fn current(&self) -> Value {
        self.snapshot.borrow()["work"][0]["runs"][0]["assignments"][0].clone()
    }
    fn active(&self, process: &str) {
        let mut s = self.snapshot.borrow_mut();
        let w = &mut s["work"][0];
        w["state"] = json!("active");
        w["current_run"] = json!(9);
        w["runs"] = json!([Self::run()]);
        let a = &mut w["runs"][0]["assignments"][0];
        a["state"] = json!(process);
        a["process_state"] = json!(process);
        a["dispatch"] =
            json!({"workspace":"w1","tab":"w1:t1","pane":"w1:p1","dispatch_key":"fixture"});
    }
    fn native(&self, name: &str, session: &str) {
        *self.agents.borrow_mut() = vec![
            json!({"name":name,"pane_id":"w1:p1","agent_status":"idle","agent_session":{"value":session}}),
        ];
    }
}
impl driver::Backend for Fake {
    fn read(&self, verb: &str, id: i64) -> Result<Value> {
        self.call(format!("read:{verb}:{id}"));
        let snapshot = self.snapshot.borrow();
        match verb {
            "dispatch" => {
                let work=snapshot["work"].as_array().unwrap().iter().filter(|w|matches!(w["state"].as_str(),Some("queued"|"active"|"verifying"))||w["state"]=="cancelled"&&current_run(w).is_some_and(|r|r["state"]=="active")).map(|w|json!({"work_id":w["work_id"],"revision":w["revision"],"state":w["state"],"priority":w["priority"],"current_run":w["current_run"],"reason":w["reason"]})).collect::<Vec<_>>();
                Ok(
                    json!({"campaign":id,"limit":if self.pending_truncated.get(){work.len().max(1)}else{32},"returned":work.len(),"truncated":self.pending_truncated.get(),"work":work}),
                )
            }
            "show" => {
                let mut work = snapshot["work"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|w| w["work_id"] == id)
                    .unwrap()
                    .clone();
                if self.stale_show.get() {
                    work["revision"] = json!(99);
                }
                Ok(work)
            }
            _ => panic!("unexpected full/history read {verb}"),
        }
    }
    fn change(
        &self,
        verb: &str,
        _id: i64,
        _root: i64,
        g: i64,
        data: Option<Value>,
    ) -> Result<Value> {
        self.call(format!("change:{verb}"));
        assert_eq!(g, 7);
        if verb == "block" {
            return Ok(json!({}));
        }
        if verb == "claim" {
            assert_eq!(
                data.as_ref().unwrap()["expected_revision"],
                self.snapshot.borrow()["work"][0]["revision"]
            );
            let mut s = self.snapshot.borrow_mut();
            let w = &mut s["work"][0];
            w["state"] = json!("active");
            w["current_run"] = json!(9);
            w["runs"] = json!([Self::run()]);
            return Ok(Self::run());
        }
        if verb == "advance" {
            return Ok(self.next.borrow().clone());
        }
        let mut s = self.snapshot.borrow_mut();
        let a = &mut s["work"][0]["runs"][0]["assignments"][0];
        match verb {
            "bind" => {
                a["dispatch"] = data.unwrap();
                a["state"] = json!("starting");
                a["process_state"] = json!("starting");
            }
            "uncertain" => a["process_state"] = json!("uncertain"),
            "started" => {
                a["process_state"] = json!("running");
                if let Some(data) = data {
                    a["native_session"] = data["native_session"].clone();
                }
            }
            "stopped" => {
                assert_eq!(data.as_ref().unwrap()["agent_absent"], true);
                a["process_state"] = json!("stopped");
            }
            "reconcile" => {
                assert_eq!(data.as_ref().unwrap()["abandon"], false);
            }
            _ => panic!("unexpected fake mutation {verb}"),
        }
        Ok(a.clone())
    }
    fn herdr(&self, args: &[&str]) -> Result<Value> {
        self.call(format!("herdr:{}", args.join(" ")));
        if self.unavailable.get() {
            return Err(failure("Herdr unavailable; no server start"));
        }
        Ok(match args {
            ["worktree", "list", "--workspace", "w1"] => {
                json!({"worktrees":[{"path":self.scratch.0,"open_workspace_id":"w1"}]})
            }
            ["pane", "list", ..] => json!({"panes":[{"pane_id":"w1:p1"}]}),
            ["tab", "list", ..] => json!({"tabs":[{"tab_id":"w1:t1"}]}),
            ["pane", "get", ..] => {
                json!({"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","cwd":self.scratch.0}})
            }
            ["pane", "process-info", ..] => {
                json!({"process_info":{"shell_pid":400,"foreground_process_group_id":400,"foreground_processes":[{"pid":400,"name":"zsh"}]}})
            }
            ["agent", "list"] => json!({"agents":*self.agents.borrow()}),
            ["pane", "send-keys", _, "ctrl+c", "ctrl+c"] => {
                self.agents.borrow_mut().clear();
                json!({})
            }
            _ => json!({}),
        })
    }
    fn lanes(&self) -> Result<String> {
        Ok("synthetic-exec\tclaude\tsynthetic-exec\tmedium\timpl\nsynthetic-review\tcodex\tsynthetic-review\thigh\trev\n".into())
    }
    fn command(&self, args: &[String]) -> Result<Value> {
        self.call(format!("command:{}", args.join(" ")));
        if args == ["work", "campaigns"] {
            if self.drift.get()
                && self
                    .calls
                    .borrow()
                    .iter()
                    .filter(|c| *c == "command:work campaigns")
                    .count()
                    > 1
            {
                self.snapshot.borrow_mut()["generation"] = json!(8);
            }
            let s = self.snapshot.borrow();
            let rows = if s["configuration"]["enabled"] == true {
                vec![
                    json!({"campaign":s["campaign"],"host":s["host"],"generation":s["generation"],"parked":s["parked"],"configuration":s["configuration"]}),
                ]
            } else {
                Vec::new()
            };
            return Ok(
                json!({"limit":1000,"returned":rows.len(),"truncated":false,"campaigns":rows}),
            );
        }
        if args[0] == "campaign" {
            return Ok(json!({"root":{"pane_id":"w2:p1"},"lanes":[]}));
        }
        if args[0] == "prompt" {
            self.log
                .borrow_mut()
                .push(json!({"kind":"prompt","launch_id":11,"created_at":store::now()}));
        }
        Ok(json!(*self.log.borrow()))
    }
    fn launch(&self, _args: &[String]) -> Result<()> {
        self.call("effect:start");
        assert_eq!(self.current()["process_state"], "starting");
        if self.crash.get() {
            return Err(failure("injected crash after bind"));
        }
        self.native("impl-w11", "native-fixture");
        Ok(())
    }
    fn directory(&self, _root: i64, launch: i64) -> Result<PathBuf> {
        let p = self.scratch.0.join(format!("launch-{launch}"));
        fs::create_dir_all(&p).unwrap();
        Ok(p)
    }
}
#[test]
fn disabled_and_historical_campaigns_do_nothing() {
    let b = Fake::new();
    b.snapshot.borrow_mut()["configuration"]["enabled"] = json!(false);
    assert_eq!(driver::drive(&b, 3).unwrap()["disabled"], true);
    assert_eq!(&*b.calls.borrow(), &["command:work campaigns"]);
}
#[test]
fn duplicate_ticks_never_launch_or_claim_twice() {
    let b = Fake::new();
    driver::drive(&b, 3).unwrap();
    driver::drive(&b, 3).unwrap();
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    assert_eq!(calls.iter().filter(|c| *c == "effect:start").count(), 1);
    assert_eq!(calls.iter().filter(|c| *c == "change:claim").count(), 1);
    assert!(
        calls.iter().position(|c| c == "change:bind").unwrap()
            < calls.iter().position(|c| c == "effect:start").unwrap()
    );
}
#[test]
fn crash_after_bind_and_unknown_writer_do_not_restart() {
    let b = Fake::new();
    b.crash.set(true);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    driver::drive(&b, 3).unwrap();
    assert_eq!(
        b.calls
            .borrow()
            .iter()
            .filter(|c| *c == "effect:start")
            .count(),
        1
    );
}
#[test]
fn unavailable_server_blocks_without_creating_layout_or_starting() {
    let b = Fake::new();
    b.unavailable.set(true);
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    assert!(calls.iter().any(|c| c == "change:block"));
    assert!(!calls.iter().any(|c| c == "effect:start"
        || c.contains("tab create")
        || c.contains("server start")
        || c == "change:claim"));
}
#[test]
fn lane_drift_is_rejected_before_claim() {
    let b = Fake::new();
    b.snapshot.borrow_mut()["work"][0]["revisions"][0]["contract"]["executor"]["effort"] =
        json!("high");
    driver::drive(&b, 3).unwrap();
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c == "change:claim" || c == "effect:start")
    );
}
#[test]
fn stale_generation_has_no_effect() {
    let b = Fake::new();
    b.active("reserved");
    b.drift.set(true);
    assert_eq!(driver::drive(&b, 3).unwrap_err().code, ExitCode::Rejected);
    assert!(!b.calls.borrow().iter().any(|c| c == "effect:start"));
}
#[test]
fn bound_agent_without_prompt_can_receive_exactly_one_brief() {
    let b = Fake::new();
    b.active("starting");
    b.native("impl-w11", "native-fixture");
    let dir = driver::Backend::directory(&b, 3, 11).unwrap();
    fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
    let a = b.current();
    driver::recover(&b, 3, 7, &json!({}), &a).unwrap();
    driver::recover(&b, 3, 7, &json!({}), &a).unwrap();
    assert_eq!(
        b.calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("command:prompt"))
            .count(),
        1
    );
}
#[test]
fn changed_native_identity_never_gets_exit_keys() {
    let b = Fake::new();
    b.active("running");
    b.native("impl-w11", "replacement");
    let mut a = b.current();
    a["native_session"] = json!("old-session");
    assert!(driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).is_err());
    assert!(!b.calls.borrow().iter().any(|c| c.contains("send-keys")));
}
#[test]
fn stop_requires_owned_identity_absence_and_shell_foreground() {
    let b = Fake::new();
    b.active("running");
    b.native("impl-w11", "native-fixture");
    let mut a = b.current();
    a["native_session"] = json!("native-fixture");
    driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).unwrap();
    assert_eq!(b.current()["process_state"], "stopped");
    assert!(b.agents.borrow().is_empty());
    assert!(!b.calls.borrow().iter().any(|c| c.contains("Enter")));
}
#[test]
fn exact_cases_reject_missing_duplicate_skipped_failed_error_and_extra() {
    let expected = vec!["a".into(), "b".into()];
    for rows in [
        vec![],
        vec![("a", "passed")],
        vec![("a", "passed"), ("a", "passed")],
        vec![("a", "passed"), ("b", "skipped")],
        vec![("a", "passed"), ("b", "failed")],
        vec![("a", "passed"), ("b", "error")],
        vec![("a", "passed"), ("b", "passed"), ("c", "passed")],
    ] {
        let cases = rows
            .into_iter()
            .map(|(id, status)| api::Case {
                id: id.into(),
                status: status.into(),
            })
            .collect::<Vec<_>>();
        assert!(runner::validate_cases(&cases, &expected).is_err());
    }
    assert!(
        runner::validate_cases(
            &[
                api::Case {
                    id: "a".into(),
                    status: "passed".into()
                },
                api::Case {
                    id: "b".into(),
                    status: "passed".into()
                }
            ],
            &expected
        )
        .is_ok()
    );
}
#[test]
fn receipt_traversal_and_symlink_escape_fail() {
    let dir = Scratch::new();
    fs::write(dir.0.join("outside.json"), r#"{"cases":[]}"#).unwrap();
    std::os::unix::fs::symlink(dir.0.join("outside.json"), dir.0.join("link.json")).unwrap();
    for path in ["../outside.json", "/outside.json", "link.json"] {
        assert!(runner::receipt(&dir.0, path, &[]).is_err());
    }
}
#[test]
fn exit_one_despite_pass_text_is_a_failed_check() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "printf PASS; exit 1"]);
    let out = process::run(cmd, Duration::from_secs(2), &AtomicBool::new(false)).unwrap();
    assert_eq!(out.exit, Some(1));
    assert_eq!(out.stdout.bytes, b"PASS");
}
#[test]
fn timeout_kills_children_and_retains_bounded_hashes() {
    let dir = Scratch::new();
    let marker = dir.0.join("escaped");
    let mut cmd = Command::new("sh");
    cmd.args([
        "-c",
        &format!(
            "(sleep 1; touch {}) & while :; do printf 0123456789; done",
            driver::quote(marker.to_str().unwrap())
        ),
    ]);
    let out = process::run(cmd, Duration::from_millis(100), &AtomicBool::new(false)).unwrap();
    assert!(out.timed_out);
    assert!(out.stdout.bytes.len() <= process::LIMIT);
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!marker.exists());
}
#[test]
fn no_slotr_output_text_can_prove_unit_absence() {
    assert!(!admission::unit_absent("slotr: PASS slotr-heavy-1\n"));
    assert!(!admission::unit_absent(
        "LoadState=loaded\nActiveState=active\nMainPID=42\nControlGroup=/unit\n"
    ));
    assert!(admission::unit_absent(
        "LoadState=not-found\nActiveState=inactive\nMainPID=0\nControlGroup=\n"
    ));
}
#[test]
fn argv_spelling_is_preserved_for_multicall_symlink() {
    let dir = Scratch::new();
    let tool = dir.0.join("approved-tool");
    std::os::unix::fs::symlink("/bin/sh", &tool).unwrap();
    let mut cmd = Command::new(&tool);
    cmd.args(["-c", "printf '%s' \"$0\""]);
    let out = process::run(cmd, Duration::from_secs(2), &AtomicBool::new(false)).unwrap();
    assert_eq!(
        String::from_utf8(out.stdout.bytes).unwrap(),
        tool.to_string_lossy()
    );
}
#[test]
fn runner_rejects_wrong_role_launch_task_round_and_settled_assignment() {
    let b = Fake::new();
    b.active("starting");
    let mut work = b.snapshot.borrow()["work"][0].clone();
    work["runs"][0]["assignments"][0]["stage"] = json!("gate");
    assert!(runner::assigned_gate(&work, 11, "12", "11").is_ok());
    for (task, launch) in [("999", "11"), ("12", "999")] {
        assert!(runner::assigned_gate(&work, 11, task, launch).is_err());
    }
    assert!(runner::assigned_gate(&work, 999, "12", "999").is_err());
    for (field, value) in [
        ("stage", json!("executor")),
        ("round", json!(99)),
        ("state", json!("settled")),
        ("process_state", json!("uncertain")),
    ] {
        let mut changed = work.clone();
        changed["runs"][0]["assignments"][0][field] = value;
        assert!(runner::assigned_gate(&changed, 11, "12", "11").is_err());
    }
}
#[test]
fn source_rejects_dirty_and_changed_commits() {
    let dir = Scratch::new();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&dir.0)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Fixture"]);
    git(&["config", "user.email", "fixture@example.com"]);
    fs::write(dir.0.join("source"), "first").unwrap();
    git(&["add", "source"]);
    git(&["commit", "-qm", "first"]);
    let before = runner::source(&dir.0, &"c".repeat(64)).unwrap();
    fs::write(dir.0.join("untracked"), "dirty").unwrap();
    assert!(runner::source(&dir.0, &before.input_digest).is_err());
    fs::remove_file(dir.0.join("untracked")).unwrap();
    fs::write(dir.0.join("source"), "second").unwrap();
    assert!(runner::source(&dir.0, &before.input_digest).is_err());
    git(&["add", "source"]);
    git(&["commit", "-qm", "second"]);
    assert_ne!(
        runner::source(&dir.0, &before.input_digest).unwrap(),
        before
    );
}
#[test]
fn thin_dispatch_never_reads_terminal_history() {
    let b = Fake::new();
    let mut history = b.snapshot.borrow()["work"][0].clone();
    history["work_id"] = json!(99);
    history["state"] = json!("done");
    history["revisions"][0]["contract"] = json!("historical report body must not be parsed");
    b.snapshot.borrow_mut()["work"]
        .as_array_mut()
        .unwrap()
        .push(history);
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    assert!(calls.iter().any(|c| c == "read:dispatch:3"));
    assert!(calls.iter().any(|c| c == "read:show:1"));
    assert!(
        !calls
            .iter()
            .any(|c| c == "read:show:99" || c.starts_with("read:list"))
    );
}
#[test]
fn truncated_dispatch_reports_partial_coverage() {
    let b = Fake::new();
    b.pending_truncated.set(true);
    let result = driver::drive(&b, 3).unwrap();
    assert_eq!(result["complete_scan"], false);
    assert_eq!(result["truncated"], true);
    assert_eq!(result["returned"], 1);
    assert_eq!(result["inspected"], 1);
    assert!(!result["bounded_conditions"].as_array().unwrap().is_empty());
}
#[test]
fn selected_revision_drift_prevents_claim_and_preserves_dispatch_order() {
    let b = Fake::new();
    b.stale_show.set(true);
    let low = b.snapshot.borrow()["work"][0].clone();
    let mut high = low.clone();
    high["work_id"] = json!(2);
    high["priority"] = json!(100);
    b.snapshot.borrow_mut()["work"] = json!([high, low]);
    let result = driver::drive(&b, 3).unwrap();
    assert_eq!(result["stale"], 2);
    let calls = b.calls.borrow();
    assert!(
        calls.iter().position(|c| c == "read:show:2").unwrap()
            < calls.iter().position(|c| c == "read:show:1").unwrap()
    );
    assert!(
        !calls
            .iter()
            .any(|c| c == "change:claim" || c == "effect:start")
    );
}
#[test]
fn thin_scan_requires_explicit_consistent_counts_and_truncation() {
    for value in [
        json!({"work":[],"limit":32,"returned":0}),
        json!({"work":[],"limit":32,"returned":1,"truncated":false}),
        json!({"work":[{}],"limit":0,"returned":1,"truncated":false}),
    ] {
        assert!(scan_rows(&value, "work").is_err());
    }
}
#[test]
fn cancelled_selected_writer_is_stopped_and_reconciled_without_any_launch() {
    let b = Fake::new();
    b.active("running");
    b.native("impl-w11", "native-fixture");
    b.snapshot.borrow_mut()["work"][0]["state"] = json!("cancelled");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    assert!(calls.iter().any(|c| c == "read:show:1"));
    assert!(calls.iter().any(|c| c == "change:stopped"));
    assert!(calls.iter().any(|c| c == "change:reconcile"));
    assert!(!calls.iter().any(|c| matches!(
        c.as_str(),
        "change:claim" | "change:reserve" | "change:bind" | "change:advance" | "effect:start"
    ) || c.starts_with("command:prompt")));
}
#[test]
fn cancelled_unidentified_start_never_recovers_a_prompt_or_restarts() {
    let b = Fake::new();
    b.active("starting");
    b.native("impl-w11", "native-fixture");
    b.snapshot.borrow_mut()["work"][0]["state"] = json!("cancelled");
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    assert!(!b.calls.borrow().iter().any(|c| c == "effect:start"
        || c == "change:reserve"
        || c == "change:claim"
        || c == "change:bind"
        || c.starts_with("command:prompt")));
}
#[test]
fn checkout_verification_selects_configured_workspace_explicitly() {
    let b = Fake::new();
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    assert!(
        calls
            .iter()
            .any(|c| c == "herdr:worktree list --workspace w1")
    );
    assert!(!calls.iter().any(|c| c == "herdr:worktree list"));
}
#[test]
fn explicit_databases_isolate_same_root_launch_locks_and_stale_receipts() {
    let home = Scratch::new();
    let default = home.0.join(".local/state/taskr/work-controller");
    assert_eq!(state_path(&home.0, None, &home.0).unwrap(), default);
    assert_eq!(state_path(&home.0, Some(""), &home.0).unwrap(), default);
    let first = state_path(&home.0, Some("ledger-a.sqlite"), &home.0).unwrap();
    let second = state_path(&home.0, Some("ledger-b.sqlite"), &home.0).unwrap();
    assert_ne!(first, second);
    assert_ne!(first, default);
    assert_eq!(
        first,
        state_path(&home.0, Some("nested/../ledger-a.sqlite"), &home.0).unwrap()
    );
    assert_eq!(
        first,
        state_path(
            &home.0,
            Some(home.0.join("ledger-a.sqlite").to_str().unwrap()),
            &home.0
        )
        .unwrap()
    );
    let namespace = first.file_name().unwrap().to_str().unwrap();
    assert_eq!(namespace.len(), 67);
    assert!(
        namespace
            .strip_prefix("db-")
            .unwrap()
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    );
    private_dir(&first.join("3/launch-11")).unwrap();
    private_dir(&second.join("3/launch-11")).unwrap();
    fs::write(first.join("3/launch-11/shell-exit"), "0\n").unwrap();
    assert!(!second.join("3/launch-11/shell-exit").exists());
    let _first_lock = lock(&first.join("campaign-3.lock")).unwrap().unwrap();
    let _second_lock = lock(&second.join("campaign-3.lock")).unwrap().unwrap();
    assert!(lock(&first.join("campaign-3.lock")).unwrap().is_none());
}
