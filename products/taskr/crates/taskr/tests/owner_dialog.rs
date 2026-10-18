#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::*;

fn question() -> Value {
    json!({"question":"Which region?","header":"Region","multiSelect":true,
        "options":[{"label":"North, South","description":"Both"},{"label":"West"}]})
}
fn payload(answer: Option<&str>) -> Value {
    let mut v = json!({"tool_name":"AskUserQuestion","tool_input":{"questions":[question()]}});
    if let Some(answer) = answer {
        v["tool_response"] = json!({"questions":[question()],"answers":{"Which region?":answer},"response":"after lunch"});
    }
    v
}
fn lead(h: &Harness) -> Env {
    h.ok(
        &[],
        &["new", "lead", "--role", "orchestrator", "--pane", "w9:p1"],
    );
    vec![
        ("HERDR_ENV".into(), "1".into()),
        ("HERDR_PANE_ID".into(), "w9:p1".into()),
    ]
}
fn run(h: &Harness, env: &Env, event: &str, v: Value) -> std::process::Output {
    h.output(
        env,
        &["hook", "claude", event],
        &serde_json::to_vec(&v).unwrap(),
    )
}
#[test]
fn dialog_is_visible_before_answer_and_supersedes_in_views_and_inbox() {
    let h = Harness::new();
    let env = lead(&h);
    h.hook(&env, "claude", "PreToolUse", payload(None));
    let open = h.rows(&[], &["asks", "--owner", "--open"]);
    let ask = open[0]["id"].as_i64().unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["data"]["dialog"], true);
    let first = h.ok(&[], &["answer", &ask.to_string(), "A: West"])["answer_id"]
        .as_i64()
        .unwrap();
    h.hook(
        &env,
        "claude",
        "PostToolUse",
        payload(Some("North, South, West")),
    );
    let asks = h.rows(&[], &["asks", "--owner"]);
    assert_eq!(asks[0]["answer"], "A: North, South; B: West — after lunch");
    let newest = asks[0]["answered_by"].as_i64().unwrap();
    assert_ne!(first, newest);
    assert_eq!(
        h.count(
            "select json_extract(data,'$.supersedes_answer_id') from events where id=?",
            newest
        ),
        first
    );
    assert_eq!(
        h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            ask
        ),
        2
    );
    let inbox = h.ok(&[], &["inbox", "--as", "1"]);
    assert!(inbox.to_string().contains(&newest.to_string()));
    let campaign = h.ok(&[], &["campaign", "1"]);
    assert!(
        campaign
            .to_string()
            .contains("A: North, South; B: West — after lunch")
    );
    let log = h.rows(&[], &["log", "1"]);
    assert!(log.iter().any(|e| e["summary"] == "A: West"));
    assert!(
        log.iter()
            .any(|e| e["data"]["supersedes_answer_id"] == first)
    );
    let second = h.output(&[], &["answer", &ask.to_string(), "plain duplicate"], b"");
    assert_eq!(second.status.code(), Some(6));
    let third = h.ok(&[], &["answer", &ask.to_string(), "newest", "--supersede"]);
    assert!(third["answer_id"].as_i64().unwrap() > newest);
}
#[test]
fn hub_reuses_another_roots_open_ask_and_does_not_duplicate_pre() {
    let h = Harness::new();
    let other = h.ok(&[], &["new", "other", "--role", "orchestrator"])["task_id"].to_string();
    let ask = h.ok(
        &[],
        &[
            "ask",
            "--owner",
            "--question",
            &question().to_string(),
            "--as",
            &other,
        ],
    )["ask_id"]
        .as_i64()
        .unwrap();
    let env = lead(&h);
    h.ok(&[], &["set", "2", "hub=on"]);
    for _ in 0..2 {
        h.hook(&env, "claude", "PreToolUse", payload(None));
    }
    assert_eq!(
        h.count(
            "select count(*) from events where kind='ask' and id>=?",
            ask
        ),
        1
    );
    h.hook(&env, "claude", "PostToolUse", payload(Some("West")));
    assert_eq!(
        h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            ask
        ),
        1
    );
}
#[test]
fn workers_are_denied_and_other_tools_are_noops() {
    let h = Harness::new();
    let (_, _, env) = h.lane("claude", "implementer");
    for event in ["PreToolUse", "PostToolUse"] {
        let out = run(&h, &env, event, payload(Some("West")));
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("taskr ask TEXT --blocking"));
        let out = run(&h, &env, event, json!({"tool_name":"Read"}));
        assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    }
    assert_eq!(
        h.count("select count(*) from events where kind='ask' and id>?", 0),
        0
    );
}
#[test]
fn no_root_or_outside_herdr_is_a_noop() {
    let h = Harness::new();
    let mut env = lead(&h);
    env[1].1 = "w99:p1".into();
    h.hook(&env, "claude", "PreToolUse", payload(None));
    env[0].1 = "0".into();
    h.hook(&env, "claude", "PreToolUse", payload(None));
    assert_eq!(
        h.count("select count(*) from events where kind='ask' and id>?", 0),
        0
    );
}
#[test]
fn all_questions_are_recorded_and_partial_answers_leave_the_rest_open() {
    let h = Harness::new();
    let env = lead(&h);
    let mut v = payload(None);
    let mut other = question();
    other["question"] = json!("When?");
    v["tool_input"]["questions"]
        .as_array_mut()
        .unwrap()
        .push(other);
    h.hook(&env, "claude", "PreToolUse", v.clone());
    v["tool_response"] = json!({"answers":{"Which region?":"West"}});
    h.hook(&env, "claude", "PostToolUse", v);
    let open = h.rows(&[], &["asks", "--owner", "--open"]);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["data"]["question"]["question"], "When?");
}
#[test]
fn failures_deny_pre_and_keep_post_answers_visible() {
    let h = Harness::new();
    let env = lead(&h);
    h.hook(&env, "claude", "PreToolUse", payload(None));
    h.ok(&[], &["close", "1"]);
    // A known root with unusable storage must deny, not silently allow the tool.
    let mut broken = env.clone();
    broken.push(("TASKR_DB".into(), h.home.to_str().unwrap().into()));
    let out = run(&h, &broken, "PreToolUse", payload(None));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("plain text"));
    let out = run(&h, &broken, "PostToolUse", payload(Some("West")));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("West"));
}

#[test]
fn supersede_keeps_authority_checks_and_delivers_a_new_inbox_event() {
    let h = Harness::new();
    let (worker, _, env) = h.lane("claude", "implementer");
    let other = h.ok(&[], &["new", "other", "--role", "orchestrator"])["task_id"].to_string();
    let ask = h.ok(&env, &["ask", "Proceed?", "--blocking"])["ask_id"].to_string();
    let first = h.ok(&[], &["answer", &ask, "first", "--as", "1"])["answer_id"]
        .as_i64()
        .unwrap();
    let out = h.output(
        &[],
        &["answer", &ask, "wrong root", "--supersede", "--as", &other],
        b"",
    );
    assert_eq!(out.status.code(), Some(6));
    let second = h.ok(&[], &["answer", &ask, "second", "--supersede", "--as", "1"])["answer_id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        h.count(
            "select count(*) from events where kind='answer' and recipient_task_id=?",
            worker
        ),
        2
    );
    assert_eq!(
        h.count(
            "select json_extract(data,'$.supersedes_answer_id') from events where id=?",
            second
        ),
        first
    );
    let out = h.output(
        &[],
        &[
            "answer",
            &ask,
            "cannot withdraw",
            "--supersede",
            "--withdraw",
            "--as",
            &worker.to_string(),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(6));
}

#[test]
fn a_locked_ledger_is_bounded_and_does_not_allow_the_dialog() {
    use std::time::{Duration, Instant};
    let h = Harness::new();
    let env = lead(&h);
    let db = h.conn();
    db.execute_batch("begin exclusive").unwrap();
    let started = Instant::now();
    let out = run(&h, &env, "PreToolUse", payload(None));
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("timed out"));
    db.execute_batch("rollback").unwrap();
}

#[test]
fn an_ad_hoc_session_does_not_create_a_ledger() {
    let h = Harness::new();
    let env = vec![
        ("HERDR_ENV".into(), "1".into()),
        ("HERDR_PANE_ID".into(), "w9:p1".into()),
    ];
    h.hook(&env, "claude", "PreToolUse", payload(None));
    h.hook(&env, "claude", "PostToolUse", payload(Some("West")));
    assert!(!h.db.exists());
}

#[test]
fn lead_dialogs_match_only_their_tree_and_ordered_labels() {
    // P2: another lead has identical text, with either the same or reversed options.
    for reverse in [false, true] {
        let h = Harness::new();
        let env = lead(&h);
        h.ok(&[], &["new", "other", "--role", "orchestrator"]);
        let mut other = question();
        if reverse {
            other["options"].as_array_mut().unwrap().reverse();
        }
        let ask = h.ok(
            &[],
            &[
                "ask",
                "--owner",
                "--question",
                &other.to_string(),
                "--as",
                "2",
            ],
        )["ask_id"]
            .as_i64()
            .unwrap();
        h.hook(&env, "claude", "PreToolUse", payload(None));
        let open = h.rows(&[], &["asks", "--owner", "--open"]);
        assert_eq!(open.len(), 2);
        assert!(open.iter().any(|a| a["task_id"] == 1));
        h.hook(&env, "claude", "PostToolUse", payload(Some("West")));
        assert_eq!(
            h.count(
                "select count(*) from events where kind='answer' and related_event_id=?",
                ask
            ),
            0
        );
    }
    // Option changes within the same tree also require a distinct ask.
    let h = Harness::new();
    let env = lead(&h);
    let mut other = question();
    other["options"].as_array_mut().unwrap().reverse();
    h.ok(
        &[],
        &[
            "ask",
            "--owner",
            "--question",
            &other.to_string(),
            "--as",
            "1",
        ],
    );
    h.hook(&env, "claude", "PreToolUse", payload(None));
    assert_eq!(h.rows(&[], &["asks", "--owner", "--open"]).len(), 2);
}

#[test]
fn post_never_supersedes_another_leads_newer_answer() {
    // P6: both roots' asks have answers before Post runs.
    let h = Harness::new();
    let env = lead(&h);
    h.hook(&env, "claude", "PreToolUse", payload(None));
    let own = h.rows(&[], &["asks", "--owner", "--open"])[0]["id"]
        .as_i64()
        .unwrap();
    h.ok(&[], &["new", "other", "--role", "orchestrator"]);
    let other = h.ok(
        &[],
        &[
            "ask",
            "--owner",
            "--question",
            &question().to_string(),
            "--as",
            "2",
        ],
    )["ask_id"]
        .as_i64()
        .unwrap();
    h.ok(&[], &["answer", &own.to_string(), "own answer"]);
    h.ok(&[], &["answer", &other.to_string(), "other answer"]);
    h.hook(&env, "claude", "PostToolUse", payload(Some("West")));
    let asks = h.rows(&[], &["asks", "--owner"]);
    assert_eq!(
        asks.iter().find(|a| a["id"] == own).unwrap()["answer"],
        "B: West — after lunch"
    );
    assert_eq!(
        asks.iter().find(|a| a["id"] == other).unwrap()["answer"],
        "other answer"
    );
    // No own match must refuse instead of falling back across roots.
    h.ok(&[], &["close", "1"]);
    let env = lead(&h);
    let out = run(&h, &env, "PostToolUse", payload(Some("West")));
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            other
        ),
        1
    );
}

#[test]
fn previews_are_dropped_before_validation_and_validation_errors_do_not_blame_the_server() {
    // P1: three 1500-byte previews are valid Claude input.
    let h = Harness::new();
    let env = lead(&h);
    let q = json!({"question":"Which layout?","header":"Layout","multiSelect":false,
        "options":[{"label":"Grid","preview":"x".repeat(1500)},
        {"label":"List","preview":"x".repeat(1500)}, {"label":"Cards","preview":"x".repeat(1500)}]});
    let mut v = json!({"tool_name":"AskUserQuestion","tool_input":{"questions":[q]}});
    h.hook(&env, "claude", "PreToolUse", v.clone());
    let asks = h.rows(&[], &["asks", "--owner", "--open"]);
    assert_eq!(asks.len(), 1);
    assert!(
        asks[0]["data"]["question"]["options"][0]
            .get("preview")
            .is_none()
    );
    v["tool_input"]["questions"][0]["options"][0]["label"] = json!("x".repeat(61));
    let out = run(&h, &env, "PreToolUse", v);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("label has 61 characters"));
    assert!(!stderr.contains("when the server is back"));
}

#[test]
fn post_preserves_selection_notes_and_notes_only_answers() {
    // P10: selections and notes-only responses both carry annotations.
    let h = Harness::new();
    let env = lead(&h);
    let mut v = json!({"tool_name":"AskUserQuestion","tool_input":{"questions":[
        {"question":"Merge #34?","options":[{"label":"Merge"},{"label":"Hold"}]},
        {"question":"Release now?","options":[{"label":"Yes"},{"label":"No"}]}]}});
    h.hook(&env, "claude", "PreToolUse", v.clone());
    v["tool_response"] = json!({"answers":{"Merge #34?":"Hold","Release now?":"(notes only)"},
        "annotations":{"Merge #34?":{"notes":"until #35 is green"},
            "Release now?":{"notes":"No - close the PR instead"}}});
    h.hook(&env, "claude", "PostToolUse", v);
    let asks = h.rows(&[], &["asks", "--owner"]);
    assert_eq!(
        asks.iter()
            .find(|a| a["data"]["question"]["question"] == "Merge #34?")
            .unwrap()["answer"],
        "B: Hold — until #35 is green"
    );
    assert_eq!(
        asks.iter()
            .find(|a| a["data"]["question"]["question"] == "Release now?")
            .unwrap()["answer"],
        " — No - close the PR instead"
    );
}

#[test]
fn afk_timeout_keeps_the_ask_open_and_does_not_supersede() {
    // P11: an unsubmitted AFK selection is never an owner answer.
    let h = Harness::new();
    let env = lead(&h);
    h.hook(&env, "claude", "PreToolUse", payload(None));
    let ask = h.rows(&[], &["asks", "--owner", "--open"])[0]["id"]
        .as_i64()
        .unwrap();
    let mut v = payload(Some("West"));
    v["tool_response"]["afkTimeoutMs"] = json!(60000);
    for answered in [false, true] {
        if answered {
            h.ok(&[], &["answer", &ask.to_string(), "submitted answer"]);
        }
        let out = run(&h, &env, "PostToolUse", v.clone());
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("AFK timeout"));
        assert_eq!(
            h.count(
                "select count(*) from events where kind='answer' and related_event_id=?",
                ask
            ),
            i64::from(answered)
        );
        assert_eq!(
            h.rows(&[], &["asks", "--owner", "--open"]).len(),
            usize::from(!answered)
        );
    }
}

#[test]
fn workers_cannot_supersede_owner_answers() {
    // P9: neither plain duplicate nor supersede may replace the owner's Hold.
    let h = Harness::new();
    let (_, _, env) = h.lane("claude", "implementer");
    let ask = h.ok(
        &[],
        &[
            "ask",
            "--owner",
            "--as",
            "1",
            "--question",
            r#"{"question":"Merge #34?","options":[{"label":"Merge"},{"label":"Hold"}]}"#,
        ],
    )["ask_id"]
        .to_string();
    h.ok(&[], &["answer", &ask, "B: Hold"]);
    for args in [
        vec!["answer", &ask, "A: Merge"],
        vec!["answer", &ask, "A: Merge", "--supersede"],
    ] {
        let out = h.output(&env, &args, b"");
        assert_eq!(out.status.code(), Some(6));
        if args.contains(&"--supersede") {
            assert!(String::from_utf8_lossy(&out.stdout).contains("root-only"));
        }
    }
    assert_eq!(h.rows(&[], &["asks", "--owner"])[0]["answer"], "B: Hold");
}
