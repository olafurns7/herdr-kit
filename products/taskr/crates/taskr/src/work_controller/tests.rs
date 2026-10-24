//! Controlled fixtures only. These tests never address a live daemon or ledger.
use super::*;
use std::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
pub(super) struct Scratch(pub(super) PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
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
    scan_error: Cell<bool>,
    scan_blocker: Cell<bool>,
    scan_native_drift: Cell<bool>,
    scan_privileged_unknown: Cell<bool>,
    root_host: RefCell<Value>,
    client_mode: Cell<bool>,
    observe_reject: Cell<bool>,
    observe_unconfirmed: Cell<bool>,
    observe_drift: Cell<bool>,
    block_events: Cell<usize>,
    pane_cwd: RefCell<Option<String>>,
    herdr_once: RefCell<Option<(String, usize)>>,
    herdr_reject: Cell<bool>,
    observe_transport: Cell<bool>,
    native_init: Cell<&'static str>,
    native_init_receipt: Cell<bool>,
    exit_observation: Cell<&'static str>,
    claims: RefCell<std::collections::BTreeMap<String, Value>>,
    claim_keys: RefCell<Vec<String>>,
    next_run: Cell<i64>,
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
            scan_error: Cell::new(false),
            scan_blocker: Cell::new(false),
            scan_native_drift: Cell::new(false),
            scan_privileged_unknown: Cell::new(false),
            root_host: RefCell::new(json!("")),
            client_mode: Cell::new(false),
            observe_reject: Cell::new(false),
            observe_unconfirmed: Cell::new(false),
            observe_drift: Cell::new(false),
            block_events: Cell::new(0),
            pane_cwd: RefCell::new(None),
            herdr_once: RefCell::new(None),
            herdr_reject: Cell::new(false),
            observe_transport: Cell::new(false),
            native_init: Cell::new("refuse"),
            native_init_receipt: Cell::new(false),
            exit_observation: Cell::new("absent"),
            claims: RefCell::new(Default::default()),
            claim_keys: RefCell::new(Vec::new()),
            next_run: Cell::new(9),
        }
    }
    fn run() -> Value {
        json!({"run_id":9,"work_id":1,"revision":1,"round":1,"state":"active","action":"reserve","assignments":[{"launch_id":11,"task_id":12,"run_id":9,"round":1,"stage":"executor","state":"reserved","process_state":"reserved","native_session":null,"dispatch":null,"lane":{"task":null,"name":"synthetic-exec","provider":"claude","model":"synthetic-exec","effort":"medium"}}],"results":[]})
    }
    fn call(&self, s: impl Into<String>) {
        self.calls.borrow_mut().push(s.into());
    }
    fn current(&self) -> Value {
        current_run(&self.snapshot.borrow()["work"][0]).unwrap()["assignments"][0].clone()
    }
    fn work(&self) -> Value {
        self.snapshot.borrow()["work"][0].clone()
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
            json!({"name":name,"pane_id":"w1:p1","agent_status":"idle","interactive_ready":true,"agent_session":{"value":session}}),
        ];
    }
    fn fresh_codex(&self) {
        self.active("starting");
        let reviewer = self.work()["revisions"][0]["contract"]["reviewer"].clone();
        let mut s = self.snapshot.borrow_mut();
        let a = &mut s["work"][0]["runs"][0]["assignments"][0];
        a["stage"] = json!("reviewer");
        a["lane"] = reviewer;
        *self.agents.borrow_mut() = vec![
            json!({"name":"rev-w11","pane_id":"w1:p1","agent":"codex","agent_status":"idle","interactive_ready":true}),
        ];
        fs::create_dir_all(self.scratch.0.join("launch-11")).unwrap();
        fs::write(
            self.scratch.0.join("launch-11/brief.md"),
            "HERDR-BRIEF role=reviewer\n",
        )
        .unwrap();
    }
}
fn checkout_alias(b: &Fake, kind: &str) -> Option<String> {
    let alias = match kind {
        "symlink" => {
            let path = b.scratch.0.join("alias");
            std::os::unix::fs::symlink(&b.scratch.0, &path).unwrap();
            path
        }
        "dot" => PathBuf::from(format!("{}/.", b.scratch.0.display())),
        "case" => {
            let path = b.scratch.0.with_file_name(
                b.scratch
                    .0
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_uppercase(),
            );
            if fs::canonicalize(&path).ok().as_ref() != Some(&b.scratch.0) {
                return None;
            }
            path
        }
        _ => unreachable!(),
    };
    Some(alias.to_str().unwrap().into())
}
impl driver::Backend for Fake {
    fn initialize_native(&self, root: i64, g: i64, cwd: &str, a: &Value) -> Result<Value> {
        self.call("effect:initialize-native");
        assert_eq!((root, g), (3, 7));
        assert_eq!(Path::new(cwd), self.scratch.0.as_path());
        assert_eq!(a["task_id"], 12);
        assert_eq!(a["launch_id"], 11);
        assert_eq!(a["process_state"], "starting");
        match self.native_init.get() {
            "complete"
            | "complete-done"
            | "mismatch"
            | "mismatch-done"
            | "reread-outage"
            | "post-working"
            | "post-blocked"
            | "post-unknown"
            | "post-noninteractive"
            | "post-missing-interactive" => {
                self.call(if self.native_init_receipt.get() {
                    "bootstrap:reuse-completed-receipt"
                } else if self.agents.borrow()[0]["agent_session"]["value"].is_string() {
                    "bootstrap:existing-native-metadata"
                } else {
                    self.native_init_receipt.set(true);
                    "bootstrap:fixed-shell"
                });
                self.native(
                    "rev-w11",
                    if matches!(self.native_init.get(), "mismatch" | "mismatch-done") {
                        "replacement"
                    } else {
                        "native-fixture"
                    },
                );
                self.agents.borrow_mut()[0]["agent"] = json!("codex");
                self.agents.borrow_mut()[0]["agent_status"] = json!(match self.native_init.get() {
                    "complete-done" | "mismatch-done" => "done",
                    "post-working" => "working",
                    "post-blocked" => "blocked",
                    "post-unknown" => "unknown",
                    _ => "idle",
                });
                if self.native_init.get() == "post-noninteractive" {
                    self.agents.borrow_mut()[0]["interactive_ready"] = json!(false);
                }
                if self.native_init.get() == "post-missing-interactive" {
                    self.agents.borrow_mut()[0]
                        .as_object_mut()
                        .unwrap()
                        .remove("interactive_ready");
                }
                if self.native_init.get() == "reread-outage" {
                    let next = self
                        .calls
                        .borrow()
                        .iter()
                        .filter(|c| c.as_str() == "herdr:agent list")
                        .count()
                        + 1;
                    *self.herdr_once.borrow_mut() = Some(("agent list".into(), next));
                }
                Ok(
                    json!({"native_session":"native-fixture","provenance_path":self.scratch.0.join("bootstrap.json")}),
                )
            }
            "malformed" => Ok(json!({"ok":true})),
            "transport" => Err(failure("native shell delivery unknown")),
            "refuse" => Err(store::Error {
                code: ExitCode::Rejected,
                message: "native initialization refused".into(),
            }),
            mode => panic!("unexpected initializer mode {mode}"),
        }
    }
    fn client_mode(&self) -> bool {
        self.client_mode.get()
    }
    fn process_scan(
        &self,
        _cwd: &Path,
        shell: i32,
        trust: &inventory::Trust,
        root: i64,
        launch: i64,
    ) -> Result<Value> {
        self.call("process:scan");
        assert_eq!((root, launch), (3, 11));
        if self.scan_privileged_unknown.get() {
            return Err(failure(format!(
                "{} controlled helper pid/start/argv evidence pending",
                inventory::PRIVILEGED_UNCERTAIN
            )));
        }
        assert_eq!(shell, 400);
        assert_eq!(trust.root.get(&500).map(String::as_str), Some("taskr"));
        assert_eq!(
            trust.shells.get(&400).map(|s| s.command.as_str()),
            Some("zsh")
        );
        if self.scan_error.get() {
            return Err(failure("synthetic incomplete OS inventory"));
        }
        if self.scan_native_drift.get() {
            self.native("replacement", "replacement");
        }
        Ok(
            json!({"method":"local-user-session-cwd-v1","uid":501,"shell_pid":400,"session_id":400,"scanned":3,"deleted_cwd":0,"privileged_cwd":0,
            "session_remaining":if self.scan_blocker.get(){vec![json!({"pid":401,"command":"synthetic-child"})]}else{vec![]},
            "checkout_remaining":[],"exempt":[{"pid":400,"reason":"workspace-shell","pane":"w1:p1","session_id":400,"foreground_pgid":400,"session_pids":[400]},{"pid":500,"reason":"campaign-root","pane":"w2:p1","foreground_pgid":500,"foreground_pids":[500],"observed_at":store::now()},{"pid":600,"reason":"observer","observer_pid":600,"parent_chain":[600]}]}),
        )
    }
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
            let reason = data.unwrap()["reason"].clone();
            let mut snapshot = self.snapshot.borrow_mut();
            if snapshot["work"][0]["reason"] != reason {
                self.block_events.set(self.block_events.get() + 1);
            }
            snapshot["work"][0]["reason"] = reason;
            return Ok(json!({}));
        }
        if verb == "observe" && self.observe_reject.get() {
            return Err(store::Error {
                code: ExitCode::Rejected,
                message: "synthetic rejected observation".into(),
            });
        }
        if verb == "observe" && self.observe_transport.replace(false) {
            return Err(failure("one-shot observe transport outage"));
        }
        if verb == "claim" {
            assert_eq!(
                data.as_ref().unwrap()["expected_revision"],
                self.snapshot.borrow()["work"][0]["revision"]
            );
            let key = data.as_ref().unwrap()["key"].as_str().unwrap().to_string();
            self.claim_keys.borrow_mut().push(key.clone());
            if let Some(run) = self.claims.borrow().get(&key) {
                return Ok(run.clone());
            }
            let id = self.next_run.get();
            self.next_run.set(id + 1);
            let mut run = Self::run();
            run["run_id"] = json!(id);
            run["assignments"][0]["run_id"] = json!(id);
            run["assignments"][0]["launch_id"] = json!(id + 2);
            run["assignments"][0]["task_id"] = json!(id + 3);
            let mut s = self.snapshot.borrow_mut();
            let w = &mut s["work"][0];
            w["state"] = json!("active");
            w["current_run"] = json!(id);
            w["runs"].as_array_mut().unwrap().push(run.clone());
            self.claims.borrow_mut().insert(key, run.clone());
            return Ok(run);
        }
        if verb == "advance" {
            return Ok(self.next.borrow().clone());
        }
        let mut s = self.snapshot.borrow_mut();
        let work = &mut s["work"][0];
        let run = work["runs"]
            .as_array()
            .unwrap()
            .iter()
            .position(|r| r["run_id"] == work["current_run"])
            .unwrap();
        let a = &mut work["runs"][run]["assignments"][0];
        match verb {
            "observe" => {
                let data = data.unwrap();
                assert_eq!(data["pane"], a["dispatch"]["pane"]);
                assert_eq!(a["process_state"], "starting");
                a["native_session"] = data["native_session"].clone();
                if self.observe_drift.get() {
                    self.native("replacement", "replacement");
                }
                if self.observe_unconfirmed.get() {
                    return Ok(json!({"queued":true}));
                }
            }
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
                assert_eq!(
                    data.as_ref().unwrap()["process_scan"]["method"],
                    "local-user-session-cwd-v1"
                );
                assert_eq!(data.as_ref().unwrap()["process_scan"]["shell_pid"], 400);
                assert_eq!(data.as_ref().unwrap()["process_scan"]["deleted_cwd"], 0);
                assert_eq!(data.as_ref().unwrap()["process_scan"]["privileged_cwd"], 0);
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
        if let Some((key, nth)) = &*self.herdr_once.borrow() {
            let current = format!("herdr:{}", args.join(" "));
            if args.join(" ") == *key
                && self
                    .calls
                    .borrow()
                    .iter()
                    .filter(|c| c.as_str() == current.as_str())
                    .count()
                    == *nth
            {
                return Err(store::Error {
                    code: if self.herdr_reject.get() {
                        ExitCode::Rejected
                    } else {
                        ExitCode::Transport
                    },
                    message: "one-shot Herdr observation failure".into(),
                });
            }
        }
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
                json!({"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","cwd":self.pane_cwd.borrow().as_deref().map_or_else(||self.scratch.0.to_string_lossy().into_owned(),str::to_owned)}})
            }
            ["pane", "process-info", "--pane", "w2:p1"] => {
                json!({"process_info":{"shell_pid":499,"foreground_process_group_id":500,"foreground_processes":[{"pid":500,"name":"taskr"}]}})
            }
            ["pane", "process-info", ..] => {
                json!({"process_info":{"shell_pid":400,"foreground_process_group_id":400,"foreground_processes":[{"pid":400,"name":"zsh"}]}})
            }
            ["agent", "list"] => {
                let observed = json!({"agents":*self.agents.borrow()});
                if self.exit_observation.get() == "transient"
                    && self
                        .calls
                        .borrow()
                        .iter()
                        .any(|c| c == "herdr:pane send-keys w1:p1 ctrl+c ctrl+c")
                {
                    self.agents.borrow_mut().clear();
                    self.exit_observation.set("absent");
                }
                observed
            }
            ["pane", "send-keys", _, "ctrl+c", "ctrl+c"] => {
                match self.exit_observation.get() {
                    "absent" => self.agents.borrow_mut().clear(),
                    "transient" | "persistent-gap" => {
                        self.agents.borrow_mut()[0]
                            .as_object_mut()
                            .unwrap()
                            .remove("agent_session");
                    }
                    "replacement" => {
                        self.agents.borrow_mut()[0]["agent_session"]["value"] = json!("replacement")
                    }
                    mode => panic!("unexpected exit observation {mode}"),
                }
                Value::Null
            }
            ["pane", "send-keys", ..] | ["pane", "run", ..] | ["tab", "rename", ..] => Value::Null,
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
            return Ok(
                json!({"root":{"id":3,"host":*self.root_host.borrow(),"pane_id":"w2:p1"},"lanes":[]}),
            );
        }
        if args[0] == "prompt" {
            assert_eq!(self.current()["native_session"], "native-fixture");
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
fn stopped_abandoned_same_revision_requeue_claims_new_run_once_with_shared_retry_key() {
    let b = Fake::new();
    assert_eq!(
        driver::claim_key(3, &b.work()).unwrap(),
        "controller:3:1:1:after-run:0"
    );
    driver::drive(&b, 3).unwrap();
    {
        let mut s = b.snapshot.borrow_mut();
        let w = &mut s["work"][0];
        w["runs"][0]["assignments"][0]["process_state"] = json!("stopped");
        w["runs"][0]["assignments"][0]["stop_event"] = json!(91);
        w["runs"][0]["state"] = json!("abandoned");
        w["current_run"] = Value::Null;
        w["state"] = json!("queued");
    }
    b.agents.borrow_mut().clear();
    let queued = b.work();
    let concurrent = queued.clone();
    let key = driver::claim_key(3, &queued).unwrap();
    assert_eq!(key, "controller:3:1:1:after-run:9");
    assert_eq!(driver::claim_key(3, &concurrent).unwrap(), key);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.work()["revision"], 1);
    assert_eq!(b.work()["current_run"], 10);
    assert_eq!(b.work()["runs"].as_array().unwrap().len(), 2);
    for snapshot in [&queued, &concurrent] {
        let replay = driver::Backend::change(&b, "claim", 1, 3, 7,
            Some(json!({"expected_revision":snapshot["revision"],"key":driver::claim_key(3,snapshot).unwrap()}))).unwrap();
        assert_eq!(replay["run_id"], 10);
    }
    assert_eq!(b.next_run.get(), 11);
    assert_eq!(
        &*b.claim_keys.borrow(),
        &["controller:3:1:1:after-run:0", &key, &key, &key]
    );
    let mut unordered = queued;
    unordered["runs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"run_id":4}));
    assert_eq!(driver::claim_key(3, &unordered).unwrap(), key);
    unordered["runs"][0]["run_id"] = Value::Null;
    assert!(driver::claim_key(3, &unordered).is_err());
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
fn recovery_pins_native_identity_before_exactly_one_prompt_without_starting() {
    let b = Fake::new();
    b.active("starting");
    b.native("impl-w11", "native-fixture");
    let dir = driver::Backend::directory(&b, 3, 11).unwrap();
    fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
    let a = b.current();
    assert!(a["native_session"].is_null());
    driver::recover(&b, 3, 7, &b.work(), &a).unwrap();
    driver::recover(&b, 3, 7, &b.work(), &a).unwrap();
    assert_eq!(b.current()["native_session"], "native-fixture");
    assert_eq!(b.current()["process_state"], "starting");
    let calls = b.calls.borrow();
    assert_eq!(calls.iter().filter(|c| *c == "change:observe").count(), 1);
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.starts_with("command:prompt"))
            .count(),
        1
    );
    assert!(
        calls.iter().position(|c| c == "change:observe").unwrap()
            < calls
                .iter()
                .position(|c| c.starts_with("command:prompt"))
                .unwrap()
    );
    assert!(!calls.iter().any(|c| c == "change:started"));
}
#[test]
fn recovery_observe_transport_outage_retries_without_prompt_or_uncertainty() {
    let b = Fake::new();
    b.active("starting");
    b.native("impl-w11", "native-fixture");
    let dir = driver::Backend::directory(&b, 3, 11).unwrap();
    fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
    b.observe_transport.set(true);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "starting");
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c == "change:uncertain" || c.starts_with("command:prompt"))
    );
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["native_session"], "native-fixture");
    assert_eq!(
        b.calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("command:prompt"))
            .count(),
        1
    );
    assert!(!b.calls.borrow().iter().any(|c| c == "change:uncertain"));
}
#[test]
fn recovery_initializes_missing_codex_identity_then_observes_before_work_prompt() {
    let b = Fake::new();
    b.fresh_codex();
    b.native_init.set("complete");
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["native_session"], "native-fixture");
    assert_eq!(b.current()["process_state"], "starting");
    driver::drive(&b, 3).unwrap();
    let calls = b.calls.borrow();
    let init = calls
        .iter()
        .position(|c| c == "effect:initialize-native")
        .unwrap();
    let observe = calls.iter().position(|c| c == "change:observe").unwrap();
    let prompt = calls
        .iter()
        .position(|c| c.starts_with("command:prompt"))
        .unwrap();
    assert!(init < observe && observe < prompt);
    assert!(
        calls[init + 1..observe]
            .iter()
            .any(|c| c == "herdr:agent list")
    );
    assert!(
        calls[init + 1..observe]
            .iter()
            .any(|c| c == "herdr:pane get w1:p1")
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_str() == "effect:initialize-native")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.starts_with("command:prompt"))
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|c| c == "change:started" || c == "change:uncertain" || c == "effect:start")
    );
}
#[test]
fn recovery_accepts_done_after_initializer_return_before_observe_and_prompt() {
    let b = Fake::new();
    b.fresh_codex();
    b.native_init.set("complete-done");
    driver::drive(&b, 3).unwrap();
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "starting");
    assert_eq!(b.current()["native_session"], "native-fixture");
    assert_eq!(b.agents.borrow()[0]["agent_status"], "done");
    let calls = b.calls.borrow();
    let init = calls
        .iter()
        .position(|c| c == "effect:initialize-native")
        .unwrap();
    let observe = calls.iter().position(|c| c == "change:observe").unwrap();
    let prompt = calls
        .iter()
        .position(|c| c.starts_with("command:prompt"))
        .unwrap();
    assert!(init < observe && observe < prompt);
    assert!(
        calls[init + 1..observe]
            .iter()
            .any(|c| c == "herdr:agent list")
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_str() == "bootstrap:fixed-shell")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_str() == "effect:initialize-native")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.starts_with("command:prompt"))
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|c| c == "change:started" || c == "change:uncertain" || c == "effect:start")
    );
}
#[test]
fn done_completed_receipt_recovery_reuses_adapter_without_shell_resend() {
    for metadata in [false, true] {
        for pinned in [false, true] {
            let b = Fake::new();
            b.fresh_codex();
            b.native_init.set("complete-done");
            b.native_init_receipt.set(true);
            b.agents.borrow_mut()[0]["agent_status"] = json!("done");
            if metadata {
                b.agents.borrow_mut()[0]["agent_session"] = json!({"value":"native-fixture"});
            }
            if pinned {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
                    json!("native-fixture");
            }
            driver::drive(&b, 3).unwrap();
            driver::drive(&b, 3).unwrap();
            assert_eq!(
                b.current()["process_state"],
                "starting",
                "metadata={metadata} pinned={pinned}"
            );
            assert_eq!(b.current()["native_session"], "native-fixture");
            let calls = b.calls.borrow();
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| c.as_str() == "bootstrap:reuse-completed-receipt")
                    .count(),
                1
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| c.as_str() == "effect:initialize-native")
                    .count(),
                1
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| c.starts_with("command:prompt"))
                    .count(),
                1
            );
            assert!(!calls.iter().any(|c| c == "bootstrap:fixed-shell"
                || c == "change:started"
                || c == "change:uncertain"
                || c == "effect:start"));
        }
    }
}
#[test]
fn fresh_idle_and_done_bootstrap_without_completed_receipt_before_work_prompt() {
    for (status, metadata) in [
        ("idle", false),
        ("done", false),
        ("idle", true),
        ("done", true),
    ] {
        let b = Fake::new();
        b.fresh_codex();
        b.native_init.set("complete-done");
        b.agents.borrow_mut()[0]["agent_status"] = json!(status);
        if metadata {
            b.agents.borrow_mut()[0]["agent_session"] = json!({"value":"native-fixture"});
        }
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "starting");
        assert_eq!(b.current()["native_session"], "native-fixture");
        driver::drive(&b, 3).unwrap();
        let calls = b.calls.borrow();
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "effect:initialize-native")
                .count(),
            1
        );
        let bootstrap = if metadata {
            "bootstrap:existing-native-metadata"
        } else {
            "bootstrap:fixed-shell"
        };
        assert_eq!(calls.iter().filter(|c| c.as_str() == bootstrap).count(), 1);
        let observe = calls.iter().position(|c| c == "change:observe").unwrap();
        let prompt = calls
            .iter()
            .position(|c| c.starts_with("command:prompt"))
            .unwrap();
        assert!(observe < prompt);
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.starts_with("command:prompt"))
                .count(),
            1
        );
        assert!(
            !calls
                .iter()
                .any(|c| c == "change:uncertain" || c == "change:started" || c == "effect:start")
        );
    }
}
#[test]
fn idle_completed_receipt_with_pinned_identity_recovers_without_resend() {
    let b = Fake::new();
    b.fresh_codex();
    b.native_init.set("complete");
    b.native_init_receipt.set(true);
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["native_session"], "native-fixture");
    let calls = b.calls.borrow();
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_str() == "bootstrap:reuse-completed-receipt")
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|c| c == "bootstrap:fixed-shell" || c == "change:uncertain")
    );
}
#[test]
fn receipt_bound_done_preserves_uncertain_prior_prompt_and_conflicting_native_refusal() {
    for cause in ["uncertain", "prior-prompt", "conflicting-native"] {
        let b = Fake::new();
        b.fresh_codex();
        b.native_init.set("complete-done");
        b.native_init_receipt.set(true);
        b.agents.borrow_mut()[0]["agent_status"] = json!("done");
        b.agents.borrow_mut()[0]["agent_session"] = json!({"value":"native-fixture"});
        match cause {
            "uncertain" => {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["process_state"] =
                    json!("uncertain")
            }
            "prior-prompt" => b
                .log
                .borrow_mut()
                .push(json!({"launch_id":11,"kind":"prompt"})),
            "conflicting-native" => {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
                    json!("other-native")
            }
            _ => unreachable!(),
        }
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain", "{cause}");
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c == "effect:initialize-native"
                    || c.starts_with("bootstrap:")
                    || c.starts_with("command:prompt")
                    || c == "change:observe"
                    || c == "change:started")
        );
    }
}
#[test]
fn recovery_initializer_mismatch_refusal_transport_or_unknown_reread_never_repeats() {
    for mode in [
        "refuse",
        "mismatch",
        "mismatch-done",
        "post-working",
        "post-blocked",
        "post-unknown",
        "post-noninteractive",
        "post-missing-interactive",
        "malformed",
        "transport",
        "reread-outage",
    ] {
        let b = Fake::new();
        b.fresh_codex();
        b.native_init.set(mode);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain", "{mode}");
        b.native_init.set("complete");
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain");
        let calls = b.calls.borrow();
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "effect:initialize-native")
                .count(),
            1
        );
        assert!(!calls.iter().any(|c| c.starts_with("command:prompt")
            || c == "change:observe"
            || c == "change:started"));
    }
}
#[test]
fn recovery_initializer_preflight_transport_retries_before_any_effect() {
    let b = Fake::new();
    b.fresh_codex();
    b.native_init.set("complete");
    *b.herdr_once.borrow_mut() = Some(("pane get w1:p1".into(), 1));
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "starting");
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c == "effect:initialize-native"
                || c == "change:uncertain"
                || c.starts_with("command:prompt"))
    );
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["native_session"], "native-fixture");
    assert!(!b.calls.borrow().iter().any(|c| c == "change:uncertain"));
}
#[test]
fn recovery_initializer_requires_interactive_ready_codex_without_prior_work_prompt() {
    for cause in [
        "provider",
        "agent",
        "name",
        "pane",
        "blocked",
        "working",
        "unknown",
        "noninteractive",
        "missing-interactive",
        "bound-native",
        "malformed-native",
        "prior-prompt",
        "uncertain",
        "disabled",
        "cancelled",
    ] {
        let b = Fake::new();
        b.fresh_codex();
        b.native_init.set("complete");
        match cause {
            "provider" => {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["lane"]["provider"] =
                    json!("claude")
            }
            "agent" => b.agents.borrow_mut()[0]["agent"] = json!("claude"),
            "name" => b.agents.borrow_mut()[0]["name"] = json!("replacement"),
            "pane" => b.agents.borrow_mut()[0]["pane_id"] = json!("w1:p2"),
            "blocked" | "working" | "unknown" => {
                b.agents.borrow_mut()[0]["agent_status"] = json!(cause)
            }
            "noninteractive" => b.agents.borrow_mut()[0]["interactive_ready"] = json!(false),
            "missing-interactive" => {
                b.agents.borrow_mut()[0]
                    .as_object_mut()
                    .unwrap()
                    .remove("interactive_ready");
            }
            "bound-native" => {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
                    json!("prior-native");
                b.agents.borrow_mut()[0]["agent_session"] = json!({"value":"conflicting-native"});
            }
            "malformed-native" => b.agents.borrow_mut()[0]["agent_session"] = json!({"value":""}),
            "prior-prompt" => b
                .log
                .borrow_mut()
                .push(json!({"launch_id":11,"kind":"prompt"})),
            "uncertain" => {
                b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["process_state"] =
                    json!("uncertain")
            }
            "disabled" => b.snapshot.borrow_mut()["configuration"]["enabled"] = json!(false),
            "cancelled" => b.snapshot.borrow_mut()["work"][0]["state"] = json!("cancelled"),
            _ => unreachable!(),
        }
        driver::drive(&b, 3).unwrap();
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c == "effect:initialize-native"
                    || c == "change:observe"
                    || c.starts_with("command:prompt")),
            "{cause}"
        );
    }
}
#[test]
fn direct_recovery_of_uncertain_identity_does_not_mark_running_even_with_receipt() {
    let b = Fake::new();
    b.fresh_codex();
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["process_state"] =
        json!("uncertain");
    b.native("rev-w11", "native-fixture");
    b.log
        .borrow_mut()
        .push(json!({"launch_id":11,"kind":"got"}));
    driver::recover(&b, 3, 7, &b.work(), &b.current()).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    assert!(b.calls.borrow().is_empty());
}
#[test]
fn recovery_refused_unconfirmed_or_changed_native_observation_withholds_prompt() {
    for cause in ["rejected", "unconfirmed", "changed-native"] {
        let b = Fake::new();
        b.active("starting");
        b.native("impl-w11", "native-fixture");
        b.observe_reject.set(cause == "rejected");
        b.observe_unconfirmed.set(cause == "unconfirmed");
        b.observe_drift.set(cause == "changed-native");
        let dir = driver::Backend::directory(&b, 3, 11).unwrap();
        fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
        driver::recover(&b, 3, 7, &b.work(), &b.current()).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain");
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("command:prompt") || c == "change:started")
        );
    }
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
fn post_exit_metadata_gap_waits_only_for_absence_and_never_sends_more_keys() {
    for (mode, expected) in [
        ("transient", "stopped"),
        ("replacement", "uncertain"),
        ("persistent-gap", "uncertain"),
    ] {
        let b = Fake::new();
        b.active("running");
        b.native("impl-w11", "native-fixture");
        {
            let mut snapshot = b.snapshot.borrow_mut();
            let a = &mut snapshot["work"][0]["runs"][0]["assignments"][0];
            a["native_session"] = json!("native-fixture");
            a["state"] = json!("settled");
        }
        b.exit_observation.set(mode);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], expected, "{mode}");
        let calls = b.calls.borrow();
        assert_eq!(calls.iter().filter(|c| c.contains("send-keys")).count(), 2);
        assert!(!calls.iter().any(|c| c.contains("Enter")));
        assert_eq!(
            calls.iter().any(|c| c == "process:scan"),
            mode == "transient"
        );
        assert_eq!(
            calls.iter().any(|c| c == "change:stopped"),
            mode == "transient"
        );
        if mode == "persistent-gap" {
            assert!(
                calls
                    .iter()
                    .any(|c| c == "herdr:pane read w1:p1 --source visible --lines 30")
            );
        }
    }
}
#[test]
fn missing_native_metadata_before_exit_never_gets_keys_or_stop_receipt() {
    let b = Fake::new();
    b.active("running");
    b.native("impl-w11", "native-fixture");
    b.agents.borrow_mut()[0]
        .as_object_mut()
        .unwrap()
        .remove("agent_session");
    let mut a = b.current();
    a["native_session"] = json!("native-fixture");
    assert!(driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).is_err());
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c.contains("send-keys") || c == "process:scan" || c == "change:stopped")
    );
}
#[test]
fn settled_done_agent_stops_without_changing_its_ready_state() {
    let b = Fake::new();
    b.active("running");
    b.native("impl-w11", "native-fixture");
    b.agents.borrow_mut()[0]["agent_status"] = json!("done");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["state"] = json!("settled");
    let mut a = b.current();
    a["native_session"] = json!("native-fixture");
    driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).unwrap();
    assert!(
        b.calls.borrow().iter().any(|c| {
            c == "herdr:agent wait impl-w11 --until idle --until done --timeout 120000"
        })
    );
    assert_eq!(b.current()["process_state"], "stopped");
    assert_eq!(b.current()["state"], "settled");
    assert!(b.agents.borrow().is_empty());
    assert!(!b.calls.borrow().iter().any(|c| c.contains("Enter")));
}
#[test]
fn non_ready_agent_never_gets_exit_keys() {
    for cause in [
        "blocked",
        "unknown",
        "working",
        "noninteractive",
        "missing-interactive",
    ] {
        let b = Fake::new();
        b.active("running");
        b.native("impl-w11", "native-fixture");
        match cause {
            "noninteractive" => b.agents.borrow_mut()[0]["interactive_ready"] = json!(false),
            "missing-interactive" => {
                b.agents.borrow_mut()[0]
                    .as_object_mut()
                    .unwrap()
                    .remove("interactive_ready");
            }
            _ => b.agents.borrow_mut()[0]["agent_status"] = json!(cause),
        }
        let mut a = b.current();
        a["native_session"] = json!("native-fixture");
        assert!(driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).is_err());
        assert!(!b.calls.borrow().iter().any(|c| c.contains("send-keys")));
        assert_eq!(b.current()["process_state"], "running");
    }
}
#[test]
fn incomplete_blocked_or_changed_process_scan_never_records_stopped() {
    for cause in ["incomplete", "blocker", "native-drift"] {
        let b = Fake::new();
        b.active("running");
        b.native("impl-w11", "native-fixture");
        b.scan_error.set(cause == "incomplete");
        b.scan_blocker.set(cause == "blocker");
        b.scan_native_drift.set(cause == "native-drift");
        let mut a = b.current();
        a["native_session"] = json!("native-fixture");
        let error = driver::stop(&b, 3, 7, "w1", b.scratch.0.to_str().unwrap(), &a).unwrap_err();
        assert!(error.message.starts_with(if cause == "native-drift" {
            "process-stop-identity:"
        } else {
            "process-stop-incomplete:"
        }));
        assert!(!b.calls.borrow().iter().any(|c| c == "change:stopped"));
        assert_eq!(b.current()["process_state"], "running");
    }
}
#[test]
fn process_scan_failure_blocks_once_without_releasing_cancelled_work() {
    let b = Fake::new();
    b.active("running");
    b.snapshot.borrow_mut()["work"][0]["state"] = json!("cancelled");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    b.native("impl-w11", "native-fixture");
    b.scan_error.set(true);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "running");
    assert_eq!(
        b.calls
            .borrow()
            .iter()
            .filter(|c| *c == "change:block")
            .count(),
        1
    );
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c == "change:stopped" || c == "change:reconcile")
    );
}
#[test]
fn one_shot_stop_observation_outages_keep_running_then_stop_on_clean_tick() {
    for (key, nth) in [
        ("pane get w1:p1", 1),
        ("agent list", 1),
        (
            "agent wait impl-w11 --until idle --until done --timeout 120000",
            1,
        ),
        ("pane process-info --pane w1:p1", 1),
        ("agent list", 5),
        ("pane process-info --pane w2:p1", 1),
        ("pane list --workspace w1", 1),
        ("pane process-info --pane w2:p1", 2),
        ("pane process-info --pane w1:p1", 4),
    ] {
        let b = Fake::new();
        b.active("running");
        b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["state"] = json!("settled");
        b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
            json!("native-fixture");
        b.native("impl-w11", "native-fixture");
        *b.herdr_once.borrow_mut() = Some((key.into(), nth));
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "running", "{key}/{nth}");
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c == "change:uncertain" || c == "change:stopped")
        );
        assert!(
            b.snapshot.borrow()["work"][0]["reason"]
                .as_str()
                .unwrap()
                .starts_with("process-stop-incomplete:")
        );
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "stopped", "{key}/{nth}");
        assert!(!b.calls.borrow().iter().any(|c| c == "change:uncertain"));
    }
}
#[test]
fn rejected_stop_identity_observation_remains_uncertain() {
    let b = Fake::new();
    b.active("running");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["state"] = json!("settled");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    b.native("impl-w11", "native-fixture");
    *b.herdr_once.borrow_mut() = Some((
        "agent wait impl-w11 --until idle --until done --timeout 120000".into(),
        1,
    ));
    b.herdr_reject.set(true);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    assert!(!b.calls.borrow().iter().any(|c| c == "change:stopped"));
}
#[test]
fn incomplete_scan_then_cooperative_writer_then_clean_retries_without_uncertainty() {
    for state in ["active", "cancelled"] {
        let b = Fake::new();
        b.active("running");
        {
            let mut snapshot = b.snapshot.borrow_mut();
            snapshot["work"][0]["state"] = json!(state);
            let a = &mut snapshot["work"][0]["runs"][0]["assignments"][0];
            a["state"] = json!("settled");
            a["native_session"] = json!("native-fixture");
        }
        b.native("impl-w11", "native-fixture");
        b.scan_error.set(true);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "running");
        let reason = b.snapshot.borrow()["work"][0]["reason"].clone();
        assert_eq!(b.block_events.get(), 1);
        b.scan_error.set(false);
        b.scan_blocker.set(true);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "running");
        assert_eq!(b.snapshot.borrow()["work"][0]["reason"], reason);
        assert_eq!(b.block_events.get(), 1);
        assert!(
            !b.calls.borrow().iter().any(|c| c == "change:stopped"
                || c == "change:reconcile"
                || c == "change:uncertain")
        );
        b.scan_blocker.set(false);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "stopped");
        assert_eq!(b.block_events.get(), 1);
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c == "change:uncertain" || c == "effect:start" || c == "change:reserve")
        );
        if state == "cancelled" {
            assert!(b.calls.borrow().iter().any(|c| c == "change:reconcile"));
        }
    }
}
#[test]
fn unknown_privileged_helper_cleanup_keeps_uncertainty_and_never_retries_automatic_scan() {
    for state in ["active", "cancelled"] {
        let b = Fake::new();
        b.active("running");
        b.native("impl-w11", "native-fixture");
        {
            let mut s = b.snapshot.borrow_mut();
            s["work"][0]["state"] = json!(state);
            let a = &mut s["work"][0]["runs"][0]["assignments"][0];
            a["state"] = json!("settled");
            a["native_session"] = json!("native-fixture");
        }
        b.scan_privileged_unknown.set(true);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain");
        assert!(
            b.work()["reason"]
                .as_str()
                .unwrap()
                .starts_with(inventory::PRIVILEGED_UNCERTAIN)
        );
        b.scan_privileged_unknown.set(false);
        driver::drive(&b, 3).unwrap();
        assert_eq!(b.current()["process_state"], "uncertain");
        assert_eq!(
            b.calls
                .borrow()
                .iter()
                .filter(|c| c.as_str() == "process:scan")
                .count(),
            1
        );
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c == "change:stopped" || c == "change:reserve" || c == "effect:start")
        );
    }
}
#[test]
fn inspect_stop_reports_privileged_uncertainty_without_ledger_or_pane_effects() {
    let b = Fake::new();
    b.active("running");
    b.scan_privileged_unknown.set(true);
    let result = driver::inspect_stop(&b, 1, 11);
    assert_eq!(result["complete"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .starts_with(inventory::PRIVILEGED_UNCERTAIN)
    );
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("change:") || c.contains("send-keys") || c == "effect:start")
    );
}
#[test]
fn cancelled_uncertain_assignment_never_reenters_stop_even_with_bound_native() {
    let b = Fake::new();
    b.active("uncertain");
    b.native("impl-w11", "native-fixture");
    {
        let mut s = b.snapshot.borrow_mut();
        s["work"][0]["state"] = json!("cancelled");
        s["work"][0]["runs"][0]["assignments"][0]["native_session"] = json!("native-fixture");
    }
    driver::drive(&b, 3).unwrap();
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    assert!(!b.calls.borrow().iter().any(|c| c.contains("send-keys")
        || c.starts_with("herdr:agent wait")
        || c == "process:scan"
        || c == "change:stopped"
        || c == "change:reconcile"
        || c == "effect:start"));
}
#[test]
fn native_absence_drift_still_requires_uncertainty_instead_of_automatic_retry() {
    let b = Fake::new();
    b.active("running");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["state"] = json!("settled");
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    b.native("impl-w11", "native-fixture");
    b.scan_native_drift.set(true);
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "uncertain");
    assert!(!b.calls.borrow().iter().any(|c| c == "change:stopped"));
    let scans = b
        .calls
        .borrow()
        .iter()
        .filter(|c| *c == "process:scan")
        .count();
    driver::drive(&b, 3).unwrap();
    assert_eq!(
        b.calls
            .borrow()
            .iter()
            .filter(|c| *c == "process:scan")
            .count(),
        scans
    );
}
#[test]
fn noncanonical_contract_blocks_before_claim_launch_and_recovery_prompt() {
    for phase in ["claim", "launch", "prompt"] {
        for kind in ["symlink", "dot", "case"] {
            let b = Fake::new();
            let Some(alias) = checkout_alias(&b, kind) else {
                continue;
            };
            b.snapshot.borrow_mut()["work"][0]["revisions"][0]["contract"]["checkout"]["cwd"] =
                json!(alias);
            if phase != "claim" {
                b.active(if phase == "launch" {
                    "reserved"
                } else {
                    "starting"
                });
            }
            if phase == "prompt" {
                b.native("impl-w11", "native-fixture");
                let dir = driver::Backend::directory(&b, 3, 11).unwrap();
                fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
            }
            driver::drive(&b, 3).unwrap();
            assert!(
                b.snapshot.borrow()["work"][0]["reason"]
                    .as_str()
                    .unwrap()
                    .contains("checkout-not-canonical:")
            );
            assert!(!b.calls.borrow().iter().any(|c| c == "change:claim"
                || c == "change:bind"
                || c == "change:observe"
                || c == "effect:start"
                || c.starts_with("command:prompt")
                || c.starts_with("herdr:tab create")
                || c.starts_with("herdr:tab rename")
                || c.starts_with("herdr:pane run")));
        }
    }
}
#[test]
fn alias_pane_spelling_blocks_new_launch_and_recovery_prompt() {
    for phase in ["launch", "prompt"] {
        for kind in ["symlink", "dot", "case"] {
            let b = Fake::new();
            let Some(alias) = checkout_alias(&b, kind) else {
                continue;
            };
            *b.pane_cwd.borrow_mut() = Some(alias);
            b.active(if phase == "launch" {
                "reserved"
            } else {
                "starting"
            });
            if phase == "prompt" {
                b.native("impl-w11", "native-fixture");
                let dir = driver::Backend::directory(&b, 3, 11).unwrap();
                fs::write(dir.join("brief.md"), "HERDR-BRIEF role=implementer\n").unwrap();
            }
            driver::drive(&b, 3).unwrap();
            assert!(
                b.snapshot.borrow()["work"][0]["reason"]
                    .as_str()
                    .unwrap()
                    .contains("checkout-not-canonical:")
            );
            assert!(!b.calls.borrow().iter().any(|c| c == "change:bind"
                || c == "change:observe"
                || c == "effect:start"
                || c.starts_with("command:prompt")
                || c.starts_with("herdr:tab create")
                || c.starts_with("herdr:tab rename")
                || c.starts_with("herdr:pane run")));
        }
    }
}
#[test]
fn approved_check_rejects_alias_checkout_before_artifacts_or_process_start() {
    let b = Fake::new();
    let alias = checkout_alias(&b, "symlink").unwrap();
    let attempt = b.scratch.0.join("attempt");
    fs::create_dir(&attempt).unwrap();
    let check = api::Check {
        id: "fixture".into(),
        kind: "command".into(),
        argv: vec![b.scratch.0.join("missing-command").to_str().unwrap().into()],
        timeout_ms: 1000,
        cases: vec![],
        admission: api::Admission::Direct,
        receipt_file: None,
    };
    let result = runner::execute(&check, &attempt, Path::new(&alias), &AtomicBool::new(false));
    assert!(matches!(result,Err(e) if e.message.starts_with("checkout-not-canonical:")));
    assert_eq!(fs::read_dir(&attempt).unwrap().count(), 0);
}
#[test]
fn old_alias_contract_remains_inspectable_and_cancel_cleanup_can_stop() {
    let b = Fake::new();
    let alias = checkout_alias(&b, "symlink").unwrap();
    b.active("running");
    *b.pane_cwd.borrow_mut() = Some(alias.clone());
    b.snapshot.borrow_mut()["work"][0]["revisions"][0]["contract"]["checkout"]["cwd"] =
        json!(alias);
    b.snapshot.borrow_mut()["work"][0]["runs"][0]["assignments"][0]["native_session"] =
        json!("native-fixture");
    assert_eq!(driver::inspect_stop(&b, 1, 11)["complete"], true);
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("change:") || c.contains("send-keys"))
    );
    b.snapshot.borrow_mut()["work"][0]["state"] = json!("cancelled");
    b.native("impl-w11", "native-fixture");
    driver::drive(&b, 3).unwrap();
    assert_eq!(b.current()["process_state"], "stopped");
    assert!(b.calls.borrow().iter().any(|c| c == "change:reconcile"));
    assert!(
        !b.calls
            .borrow()
            .iter()
            .any(|c| c == "effect:start" || c == "change:claim" || c.starts_with("command:prompt"))
    );
}
#[test]
fn inspect_stop_is_read_only_and_reports_complete_blocked_or_incomplete_scans() {
    for cause in ["clean", "blocker", "incomplete"] {
        let b = Fake::new();
        b.active("running");
        b.scan_blocker.set(cause == "blocker");
        b.scan_error.set(cause == "incomplete");
        let result = driver::inspect_stop(&b, 1, 11);
        assert_eq!(result["complete"], cause != "incomplete");
        if cause == "incomplete" {
            assert!(result["error"].as_str().is_some());
            assert!(result["remaining"].is_null());
        } else {
            assert_eq!(result["remaining"], cause == "blocker");
        }
        assert!(!b.calls.borrow().iter().any(|c| c.starts_with("change:")
            || c.contains("send-keys")
            || c.starts_with("effect:")
            || c.starts_with("command:prompt")));
    }
}
#[test]
fn inspect_stop_uses_selected_revision_and_rejects_unassigned_launch() {
    let b = Fake::new();
    b.active("running");
    b.snapshot.borrow_mut()["work"][0]["revision"] = json!(2);
    assert_eq!(driver::inspect_stop(&b, 1, 11)["complete"], true);
    let result = driver::inspect_stop(&b, 1, 99);
    assert_eq!(result["complete"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("does not belong")
    );
}
#[test]
fn inspect_stop_normalizes_local_empty_root_host_without_enabling_work() {
    for state in ["active", "cancelled"] {
        let b = Fake::new();
        b.active("running");
        b.snapshot.borrow_mut()["configuration"]["enabled"] = json!(false);
        b.snapshot.borrow_mut()["work"][0]["state"] = json!(state);
        assert_eq!(*b.root_host.borrow(), json!(""));
        assert_eq!(driver::inspect_stop(&b, 1, 11)["complete"], true);
        assert!(b.calls.borrow().iter().any(|c| c == "process:scan"));
        assert!(
            !b.calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("change:") || c.contains("send-keys"))
        );
    }
}
#[test]
fn inspect_stop_refuses_wrong_remote_and_hub_local_client_roots_before_scan() {
    for (host, client) in [
        (json!("remote.example"), false),
        (json!(""), true),
        (Value::Null, false),
    ] {
        let b = Fake::new();
        b.active("running");
        *b.root_host.borrow_mut() = host;
        b.client_mode.set(client);
        let result = driver::inspect_stop(&b, 1, 11);
        assert_eq!(result["complete"], false);
        assert!(result["error"].as_str().unwrap().contains("campaign root"));
        assert!(
            !b.calls.borrow().iter().any(|c| c == "process:scan"
                || c.starts_with("change:")
                || c.contains("send-keys"))
        );
    }
    let b = Fake::new();
    b.active("running");
    *b.root_host.borrow_mut() = json!(store::local_machine());
    b.client_mode.set(true);
    assert_eq!(driver::inspect_stop(&b, 1, 11)["complete"], true);
}
#[test]
fn inspect_stop_parser_rejects_missing_or_invalid_launch_before_os_reads() {
    for args in [
        vec!["work", "inspect-stop", "1"],
        vec!["work", "inspect-stop", "1", "--launch", "invalid"],
    ] {
        assert_eq!(
            dispatch(
                false,
                &args.into_iter().map(String::from).collect::<Vec<_>>()
            ),
            Some(ExitCode::Usage)
        );
    }
}
#[test]
fn generated_brief_prohibits_unmanaged_processes_and_states_scan_limits() {
    let b = Fake::new();
    driver::drive(&b, 3).unwrap();
    let brief = fs::read_to_string(b.scratch.0.join("launch-11/brief.md")).unwrap();
    let exe = std::env::current_exe().unwrap();
    assert!(brief.contains(&format!(
        "Ledger CLI: {}",
        driver::quote(exe.to_str().unwrap())
    )));
    for text in [
        "never detach background jobs or create external runtimes",
        "named managed runtime",
        "another effective UID",
        "absolute-path writers with no retained descriptor",
        "Cwd snapshots are not atomic",
        "Model agreement cannot certify containment",
        "Inbox/log/status are non-consuming; only wait offers an event",
        "never ack a raw inbox event",
    ] {
        assert!(brief.contains(text));
    }
}
#[test]
fn generated_ledger_examples_use_selected_absolute_cli_under_hostile_path_with_safe_quotes() {
    use std::os::unix::fs::PermissionsExt;
    let b = Fake::new();
    b.active("reserved");
    let selected = b
        .scratch
        .0
        .join("selected cli 'quote' $SYNTHETIC `not-invoked`")
        .join("taskr");
    let old_bin = b.scratch.0.join("old-bin");
    let old_cli = old_bin.join("taskr");
    fs::create_dir_all(selected.parent().unwrap()).unwrap();
    fs::create_dir(&old_bin).unwrap();
    fs::write(&selected, b"#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\"\n").unwrap();
    fs::write(
        &old_cli,
        b"#!/bin/sh\nprintf '%s\\n' 'OLD PATH CLI' >&2\nexit 77\n",
    )
    .unwrap();
    for file in [&selected, &old_cli] {
        fs::set_permissions(file, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let work = b.work();
    let contract = current_contract(&work).unwrap();
    let run = current_run(&work).unwrap();
    let assignment = &run["assignments"][0];
    let report = b
        .scratch
        .0
        .join("report 'literal' $SYNTHETIC `not-invoked`.md");
    let result = report.with_file_name("result.json");
    let brief = driver::brief_text(
        3, &work, &contract, run, assignment, "w1", "w1:t1", "w1:p1", &report, &selected,
    )
    .unwrap();
    assert!(brief.starts_with("HERDR-BRIEF role=implementer\norchestrator=taskr:3"));
    assert!(brief.contains("task=12 launch=11"));
    assert!(!brief.contains(old_bin.to_str().unwrap()));
    let cli = driver::quote(selected.to_str().unwrap());
    let cases: Vec<(String, Vec<&str>)> = vec![
        ("work source 1".into(), vec!["work", "source", "1"]),
        (
            format!(
                "work submit 11 --file {}",
                driver::quote(result.to_str().unwrap())
            ),
            vec!["work", "submit", "11", "--file", result.to_str().unwrap()],
        ),
        (
            format!(
                "ready 'work result submitted' --report {}",
                driver::quote(report.to_str().unwrap())
            ),
            vec![
                "ready",
                "work result submitted",
                "--report",
                report.to_str().unwrap(),
            ],
        ),
        (
            "done 'work result submitted'".into(),
            vec!["done", "work result submitted"],
        ),
        (
            "ask TEXT --blocking".into(),
            vec!["ask", "TEXT", "--blocking"],
        ),
        ("wait --for answer".into(), vec!["wait", "--for", "answer"]),
        (
            "ack EVENT_ID --as 12".into(),
            vec!["ack", "EVENT_ID", "--as", "12"],
        ),
    ];
    for (suffix, args) in cases {
        let command = format!("{cli} {suffix}");
        let offset = brief
            .find(&command)
            .unwrap_or_else(|| panic!("selected CLI missing for {suffix}"));
        let rendered = &brief[offset..offset + command.len()];
        let out = Command::new("/bin/sh")
            .args(["-c", rendered])
            .env_clear()
            .env("PATH", &old_bin)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{suffix}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        let actual = text.lines().collect::<Vec<_>>();
        assert_eq!(actual[0], selected.to_str().unwrap());
        assert_eq!(&actual[1..], args.as_slice());
    }
    assert!(brief.contains("event ID returned by wait and its --fence FENCE when supplied"));
    assert!(brief.contains(
        "Inbox/log/status are non-consuming; only wait offers an event; never ack a raw inbox event"
    ));
    assert!(
        driver::brief_text(
            3,
            &work,
            &contract,
            run,
            assignment,
            "w1",
            "w1:t1",
            "w1:p1",
            &report,
            Path::new("taskr")
        )
        .is_err()
    );
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
