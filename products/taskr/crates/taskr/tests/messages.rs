#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::Harness;

fn root(h: &Harness, name: &str) -> String {
    h.ok(&[], &["new", name, "--role", "orchestrator"])["task_id"].to_string()
}
fn lease(h: &Harness, target: &str, holder: &str) -> Value {
    h.ok(
        &[],
        &[
            "consumer",
            "take",
            "--as",
            target,
            "--session",
            "0",
            "--holder",
            holder,
            "--ttl",
            "60000",
        ],
    )
}
fn count(h: &Harness) -> i64 {
    h.conn()
        .query_row("select count(*) from events", [], |r| r.get(0))
        .unwrap()
}
fn refused(h: &Harness, env: &[(String, String)], args: &[&str], code: i32) {
    let before = count(h);
    let out = h.output(env, args, b"");
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(count(h), before, "rejected send wrote an event");
}

#[test]
fn routing_identity_text_status_and_loss_safe_waits() {
    let h = Harness::new();
    let sender = root(&h, "sender");
    let target = root(&h, "receiver");
    let other = root(&h, "other");
    lease(&h, &target, "custom-holder");
    refused(
        &h,
        &[],
        &[
            "prompt", &target, "--as", &sender, "--text", "hello", "--push",
        ],
        6,
    );
    h.ok(
        &[],
        &[
            "consumer",
            "release",
            "--as",
            &target,
            "--session",
            "0",
            "--fence",
            "1",
        ],
    );
    lease(&h, &target, "mcp-channel-test");
    let (worker, _, env) = h.lane("claude", "implementer");
    let worker = worker.to_string();
    refused(
        &h,
        &[],
        &["prompt", &target, "--as", &worker, "--text", "hello"],
        6,
    );
    refused(
        &h,
        &env,
        &["prompt", &target, "--as", &sender, "--text", "hello"],
        6,
    );
    refused(
        &h,
        &[],
        &["prompt", &target, "--as", &target, "--text", "hello"],
        6,
    );
    refused(
        &h,
        &[],
        &["prompt", &worker, "--as", &sender, "--text", "hello"],
        6,
    );
    refused(
        &h,
        &[],
        &[
            "prompt", &target, "--as", &sender, "--text", "hello", "--push", "--pane",
        ],
        2,
    );
    for text in [
        "x".repeat(4097),
        "é".repeat(2049),
        "tab\there".into(),
        "binary\u{7f}".into(),
    ] {
        refused(
            &h,
            &[],
            &["prompt", &target, "--as", &sender, "--text", &text],
            2,
        );
    }
    std::fs::write(h.home.join("body"), b"\xff\x00").unwrap();
    refused(
        &h,
        &[],
        &["prompt", &target, "--as", &sender, "--file", "body"],
        2,
    );
    let pushed = h.ok(
        &[],
        &[
            "prompt",
            &target,
            "--as",
            &sender,
            "--text",
            "hello\npeer",
            "--confirm",
        ],
    );
    assert_eq!(
        pushed,
        json!({"path":"push","event":pushed["event"],"target":target.parse::<i64>().unwrap(),"gen":0,"received":false})
    );
    let e = pushed["event"].to_string();
    let inbox = h.ok(&[], &["inbox", "--as", &target]);
    assert!(inbox.to_string().contains("hello\\npeer"), "{inbox}");
    assert_eq!(
        h.ok(&[], &["message", "status", &e, "--as", &sender])["state"],
        "pending"
    );
    refused(&h, &[], &["message", "status", &e, "--as", &other], 6);
    refused(&h, &env, &["message", "status", &e, "--as", &sender], 6);
    refused(
        &h,
        &[],
        &["wait", "--as", &target, "--for", "ready", "--timeout", "0"],
        6,
    );
    h.ok(
        &[],
        &[
            "consumer",
            "release",
            "--as",
            &target,
            "--session",
            "0",
            "--fence",
            "2",
        ],
    );
    // Both old and updated filters, including a mismatched --from, must preserve E.
    for kinds in [
        "ready,ask,answer,done,fail,prompt_outcome,herdr",
        "message,ready",
    ] {
        let got = h.ok(
            &[],
            &[
                "wait",
                "--as",
                &target,
                "--for",
                kinds,
                "--from",
                &other,
                "--timeout",
                "0",
            ],
        );
        assert_eq!(got["event"]["id"], pushed["event"]);
        assert_eq!(got["event"]["task_id"], sender.parse::<i64>().unwrap());
        assert_eq!(got["event"]["kind"], "message");
    }
    let got = h.ok(
        &[],
        &[
            "wait",
            "--as",
            &target,
            "--for",
            "message",
            "--from",
            &sender,
            "--timeout",
            "0",
        ],
    );
    assert_eq!(got["event"]["id"], pushed["event"]);
    let out = h.output(&[], &["log", &target], b"");
    let out = String::from_utf8(out.stdout).unwrap();
    assert!(out.contains("\"direction\":\"from\""), "{out}");
    assert_eq!(out.matches("\"k\":\"message\"").count(), 1);
    let campaign = h.ok(&[], &["campaign", &target]);
    assert!(
        campaign.to_string().contains("\"direction\":\"from\""),
        "{campaign}"
    );
    h.ok(&[], &["ack", &e, "--as", &target]);
    assert_eq!(
        h.ok(&[], &["message", "status", &e, "--as", &sender])["state"],
        "acked"
    );
    let rounds: i64 = h
        .conn()
        .query_row(
            "select count(*) from events where kind in ('prompt','got') and task_id in (?,?)",
            taskr_core::db::params![sender, target],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rounds, 0);
}

#[test]
fn pane_fallback_overrides_and_hidden_route_share_the_decision() {
    let h = Harness::new();
    let sender = root(&h, "sender");
    let target = root(&h, "receiver");
    // The existing fake observes activity without ever contacting a real Herdr.
    h.script(
        "herdr",
        "#!/bin/sh\necho '{\"result\":{\"agent_status\":\"working\"}}'\n",
    );
    assert!(
        h.output(&[], &["adopt", &target, "--pane", "w1:p1"], b"")
            .status
            .success()
    );
    let pane = h.ok(&[], &["prompt", &target, "--as", &sender, "--text", "pane"]);
    assert_eq!(pane["path"], "pane");
    let (_, _, worker_env) = h.lane("claude", "implementer");
    // P2: the worker environment does not change the existing pane protocol.
    for extra in [
        vec![],
        vec!["--pane"],
        vec!["--as", sender.as_str(), "--pane"],
    ] {
        let mut args = vec!["prompt", target.as_str(), "--text", "worker pane"];
        args.extend(extra);
        assert_eq!(h.ok(&worker_env, &args)["path"], "pane");
    }
    // P3: begin still reopens completed roots on the pane path.
    for status in ["done", "ready", "failed"] {
        h.conn()
            .execute(
                "update tasks set status=? where id=?",
                taskr_core::db::params![status, target],
            )
            .unwrap();
        assert_eq!(
            h.ok(&[], &["prompt", &target, "--text", "reopen"])["path"],
            "pane"
        );
        let status: String = h
            .conn()
            .query_row("select status from tasks where id=?", [&target], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "open");
    }
    let generation = taskr_core::store::delivery::generation(&h.conn(), target.parse().unwrap())
        .unwrap()
        .to_string();
    h.ok(
        &[],
        &[
            "consumer",
            "take",
            "--as",
            &target,
            "--session",
            &generation,
            "--holder",
            "codex-queue-test",
            "--ttl",
            "60000",
        ],
    );
    let pane = h.ok(
        &[],
        &[
            "prompt", &target, "--as", &sender, "--text", "forced", "--pane",
        ],
    );
    assert_eq!(pane["path"], "pane");
    assert_eq!(
        h.ok(
            &worker_env,
            &["prompt", &target, "--text", "legacy live root"]
        )["path"],
        "pane"
    );
    let push = h.ok(
        &[],
        &["prompt", &target, "--as", &sender, "--text", "hidden push"],
    );
    assert_eq!(push["path"], "push");
    assert_eq!(push["gen"], generation.parse::<i64>().unwrap());
    let compact = h.output(
        &[],
        &["prompt", &target, "--as", &sender, "--text", "compact"],
        b"",
    );
    assert!(
        String::from_utf8(compact.stdout)
            .unwrap()
            .starts_with("m1 {")
    );
}

#[test]
fn incoming_message_drives_glance_activity_without_owner_badges() {
    let h = Harness::new();
    let sender = root(&h, "sender");
    let target = root(&h, "receiver");
    h.ok(&[], &["set", &sender, "glance.state=parked"]);
    h.ok(&[], &["set", &target, "glance.state=parked"]);
    lease(&h, &target, "mcp-channel-test");
    let sent = h.ok(
        &[],
        &["prompt", &target, "--as", &sender, "--text", "pending peer"],
    );
    let event = sent["event"].as_i64().unwrap();
    h.conn()
        .execute(
            "update events set created_at=? where id=?",
            taskr_core::db::params![
                taskr_core::store::stamp(
                    taskr_core::store::real_now() - taskr_core::Duration::minutes(11)
                ),
                event
            ],
        )
        .unwrap();
    let glance = h.ok(&[], &["glance"]);
    assert!(
        glance["needs_you"].as_array().unwrap().is_empty(),
        "{glance}"
    );
    let attention = glance["attention"].as_array().unwrap();
    assert!(!attention.iter().any(|a|a["kind"]=="parked_active" && a["root_id"]==sender.parse::<i64>().unwrap()), "{glance}");
    assert!(
        attention
            .iter()
            .any(|a| a["kind"] == "message_pending"
                && a["root_id"] == target.parse::<i64>().unwrap()),
        "{glance}"
    );
    assert!(attention.iter().any(|a|a["kind"]=="parked_active" && a["root_id"]==target.parse::<i64>().unwrap()),"{glance}");
    let receiver = glance["campaigns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == target.parse::<i64>().unwrap())
        .unwrap();
    assert_eq!(receiver["parked_active"], true);
    assert!((600_000..720_000).contains(&receiver["activity_age_ms"].as_i64().unwrap()));
    assert_eq!(receiver["spark"].as_array().unwrap().len(), 24);
}

fn release(h: &Harness, target: &str, fence: &str) {
    h.ok(
        &[],
        &[
            "consumer",
            "release",
            "--as",
            target,
            "--session",
            "0",
            "--fence",
            fence,
        ],
    );
}

/// The refusal: exit 6 with its reason, no event and no herdr call.
fn not_push_live(h: &Harness, args: &[&str]) {
    let before = count(h);
    let out = h.output(&[], &[&["--json"], args].concat(), b"");
    assert_eq!(out.status.code(), Some(6), "{args:?}: {out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["outcome"], "rejected", "{v}");
    assert_eq!(v["reason"], "not_push_live", "{v}");
    assert_eq!(v["kind"], "rejected", "{v}");
    let error = v["error"].as_str().unwrap();
    assert!(
        error.contains("is a push lane and is not push-live") && error.contains("--pane"),
        "{v}"
    );
    assert!(v.get("_exit").is_none(), "{v}");
    assert_eq!(count(h), before, "a refused prompt wrote an event");
    assert!(!h.home.join("typed").exists(), "herdr was called");
}

#[test]
fn a_push_lane_that_is_not_push_live_is_not_typed_into() {
    let h = Harness::new();
    let sender = root(&h, "sender");
    let target = h.ok(
        &[],
        &[
            "new",
            "receiver",
            "--role",
            "orchestrator",
            "--pane",
            "w1:p1",
        ],
    )["task_id"]
        .to_string();
    let typed = h.home.join("typed");
    h.script(
        "herdr",
        "#!/bin/sh\necho \"$*\" >> \"$HOME/typed\"\necho '{\"result\":{\"agent\":{\"agent_status\":\"working\"}}}'\n",
    );
    let prompt = ["prompt", &target, "--as", &sender, "--text", "hello"];
    let legacy = ["prompt", &target, "--text", "hello"];
    let pane = [&prompt[..], &["--pane"]].concat();
    let typed_by = |args: &[&str]| {
        let sent = h.ok(&[], args);
        assert_eq!(sent["path"], "pane", "{args:?}");
        assert_eq!(sent["outcome"], "activity_observed", "{args:?}");
        let calls = std::fs::read_to_string(&typed).unwrap();
        assert!(calls.starts_with("agent prompt w1:p1 "), "{calls}");
        std::fs::remove_file(&typed).unwrap();
    };

    // A plain root, then one whose lease is not a push adapter's: the pane, as before.
    typed_by(&prompt);
    typed_by(&legacy);
    lease(&h, &target, "custom-holder");
    typed_by(&prompt);
    release(&h, &target, "1");
    typed_by(&prompt);

    for holder in ["mcp-channel-1", "codex-queue-1"] {
        // A killed adapter leaves its holder in an expired lease.
        let fence = lease(&h, &target, holder)["fence"].to_string();
        h.conn()
            .execute(
                "update consumer_leases set expires_at='2026-01-01T00:00:00.000Z' where task_id=?",
                [&target],
            )
            .unwrap();
        not_push_live(&h, &prompt);
        not_push_live(&h, &legacy);
        typed_by(&pane);
        typed_by(&[&legacy[..], &["--pane"]].concat());
        h.conn()
            .execute(
                "update consumer_leases set expires_at='2999-01-01T00:00:00.000Z' where task_id=?",
                [&target],
            )
            .unwrap();
        assert_eq!(h.ok(&[], &prompt)["path"], "push");
        release(&h, &target, &fence);
    }

    // A released lease keeps no holder; an earlier push to this root marks the lane.
    typed_by(&prompt);
    h.conn()
        .execute(
            "insert into delivery_attempts(event_id,task_id,session_gen,adapter,created_at) select max(id),?,0,'claude-channel','2026-01-01T00:00:00.000Z' from events",
            [&target],
        )
        .unwrap();
    not_push_live(&h, &prompt);
    typed_by(&pane);

    // A restart's adopt outdates the old adapter's unexpired lease.
    let root2 = root(&h, "adopted");
    lease(&h, &root2, "mcp-channel-2");
    assert!(
        h.output(&[], &["adopt", &root2, "--pane", "w1:p1"], b"")
            .status
            .success()
    );
    not_push_live(&h, &["prompt", &root2, "--as", &sender, "--text", "hello"]);

    // A finished root cannot become push-live: the pane still reopens it.
    h.conn()
        .execute("update tasks set status='done' where id=?", [&target])
        .unwrap();
    typed_by(&prompt);
}

#[test]
fn a_remote_push_lane_is_refused_before_the_machine_route() {
    let h = Harness::new();
    let sender = root(&h, "sender");
    let target = root(&h, "receiver");
    h.script("herdr", "#!/bin/sh\necho \"$*\" >> \"$HOME/typed\"\n");
    lease(&h, &target, "mcp-channel-1");
    h.conn()
        .execute(
            "update consumer_leases set machine='host-a',expires_at='2026-01-01T00:00:00.000Z' where task_id=?",
            [&target],
        )
        .unwrap();
    h.conn()
        .execute(
            "update tasks set machine='host-a',pane_id='w1:p1' where id=?",
            [&target],
        )
        .unwrap();
    not_push_live(&h, &["prompt", &target, "--as", &sender, "--text", "hello"]);
}

#[test]
fn a_blocked_agent_reports_that_nothing_was_typed() {
    let h = Harness::new();
    let target = root(&h, "receiver");
    h.script(
        "herdr",
        "#!/bin/sh\necho '{\"error\":{\"code\":\"agent_blocked\",\"message\":\"blocked\"}}' >&2\nexit 1\n",
    );
    assert!(
        h.output(&[], &["adopt", &target, "--pane", "w1:p1"], b"")
            .status
            .success()
    );
    let out = h.output(&[], &["--json", "prompt", &target, "--text", "hello"], b"");
    assert_eq!(out.status.code(), Some(5), "{out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["outcome"], "rejected");
    assert_eq!(v["herdr_error"], "agent_blocked");
    assert_eq!(
        v["herdr_message"],
        "w1:p1 is at a dialog or permission prompt (agent blocked); not typed"
    );
}
