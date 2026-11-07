#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;

use serde_json::{Value, json};
use std::fs;
use support::*;
use taskr_core::db::params;

fn setup() -> (Harness, i64, i64) {
    let h = Harness::new();
    let (task, launch, _) = h.lane("codex", "implementer");
    h.conn()
        .execute("update tasks set machine='other-host' where id=?", [task])
        .unwrap();
    h.conn()
        .execute("update launches set machine='host-a' where id=?", [launch])
        .unwrap();
    h.script(
        "herdr",
        r#"#!/usr/bin/python3
import json, os, pathlib, sys
home = pathlib.Path(os.environ['HOME'])
with (home / 'herdr-calls').open('a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\n')
if sys.argv[1:] == ['machine', 'list', '--json']:
    print((home / 'machines.json').read_text())
else:
    print('{"result":{"agent":{"agent_status":"working"}}}')
"#,
    );
    machines(
        &h,
        json!([
            {"label":"remote-a","target":"user@HOST-A.example.com","enabled":true},
            {"label":"disabled","target":"host-a","enabled":false},
            {"label":"other","target":"host-b","enabled":true}
        ]),
    );
    (h, task, launch)
}

fn machines(h: &Harness, value: Value) {
    fs::write(h.home.join("machines.json"), value.to_string()).unwrap();
}

fn calls(h: &Harness) -> Vec<Value> {
    fs::read_to_string(h.home.join("herdr-calls"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn event_data(h: &Harness, attempt: i64) -> Value {
    let data: String = h
        .conn()
        .query_row("select data from events where id=?", [attempt], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str(&data).unwrap()
}

fn refused(h: &Harness, task: i64, args: &[&str], code: i32, message: &str) {
    let before = h.count("select count(*) from events where task_id=?", task);
    let output = h.output(&[], &[&["--json"][..], args].concat(), b"");
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["error"].as_str().unwrap().contains(message),
        "{value}"
    );
    assert_eq!(
        h.count("select count(*) from events where task_id=?", task),
        before
    );
}

#[test]
fn ledger_host_forwards_with_the_normal_attempt_deadline_and_outcome() {
    let (h, task, launch) = setup();
    let result = h.ok(&[], &["prompt", &task.to_string(), "--text", "work"]);
    let attempt = result["attempt_id"].as_i64().unwrap();
    assert_eq!(result["outcome"], "activity_observed");
    assert_eq!(result["agent_status"], "working");
    assert_eq!(event_data(&h, attempt)["machine"], "remote-a");
    assert_eq!(
        calls(&h),
        vec![
            json!(["machine", "list", "--json"]),
            json!([
                "--machine",
                "remote-a",
                "agent",
                "prompt",
                "w9:p1",
                format!("First taskr got {attempt}. work"),
                "--wait",
                "--until",
                "working",
                "--until",
                "blocked",
                "--timeout",
                "20000"
            ])
        ]
    );
    assert_eq!(
        h.count(
            "select count(*) from meta where key='receipt_due:'||?",
            attempt
        ),
        1
    );
    assert_eq!(
        h.count(
            "select count(*) from events where kind='prompt_outcome' and related_event_id=?",
            attempt
        ),
        1
    );
    assert_eq!(h.got(attempt), 0, "Herdr activity is not a worker receipt");
    assert_eq!(
        h.count("select count(*) from meta where key='capacity:'||?", launch),
        0
    );
    assert_eq!(
        h.count(
            "select count(*) from launches where id=? and observed_status is not null",
            launch
        ),
        0
    );
}

#[test]
fn zero_or_multiple_enabled_matches_refuse_before_an_attempt() {
    for (profiles, message) in [
        (json!([]), "matches 0 enabled"),
        (
            json!([
                {"label":"first","target":"host-a","enabled":true},
                {"label":"second","target":"another@host-a.example.com","enabled":true}
            ]),
            "matches 2 enabled",
        ),
        (
            json!([{"label":"disabled","target":"host-a","enabled":false}]),
            "matches 0 enabled",
        ),
    ] {
        let (h, task, _) = setup();
        machines(&h, profiles);
        refused(
            &h,
            task,
            &["prompt", &task.to_string(), "--text", "work"],
            5,
            message,
        );
        assert_eq!(calls(&h), vec![json!(["machine", "list", "--json"])]);
    }
}

#[test]
fn ip_targets_do_not_map_to_a_lane_host() {
    let (h, task, launch) = setup();
    h.conn()
        .execute("update launches set machine='192' where id=?", [launch])
        .unwrap();
    machines(
        &h,
        json!([{"label":"ip","target":"user@192.0.2.1","enabled":true}]),
    );
    refused(
        &h,
        task,
        &["prompt", &task.to_string(), "--text", "work"],
        5,
        "matches 0 enabled",
    );
}

#[test]
fn no_local_server_means_no_machine_lookup_or_attempt() {
    let (h, task, _) = setup();
    let output = h.output(
        &[(
            "HERDR_SOCKET_PATH".into(),
            h.home.join("absent.sock").to_string_lossy().into_owned(),
        )],
        &["--json", "prompt", &task.to_string(), "--text", "work"],
        b"",
    );
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(calls(&h).is_empty());
    assert_eq!(
        h.count(
            "select count(*) from events where task_id=? and kind='prompt'",
            task
        ),
        0
    );
}

#[test]
fn hub_rpc_children_do_not_forward_third_host_prompts() {
    let (h, task, _) = setup();
    for command in ["prompt", "_prompt"] {
        let id = task.to_string();
        let mut args = vec!["--hub-child", "--json", command];
        if command == "_prompt" {
            args.push("begin");
        }
        args.extend([id.as_str(), "--text", "work"]);
        let output = h.output(
            &[("TASKR_RPC_CALLER".into(), "host-b".into())],
            &args,
            b"{}",
        );
        assert_eq!(output.status.code(), Some(5), "{command}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("prompt it from that host"));
    }
    assert!(calls(&h).is_empty());
    assert_eq!(
        h.count(
            "select count(*) from events where task_id=? and kind='prompt'",
            task
        ),
        0
    );
}

#[test]
fn remote_files_reference_the_captured_document_and_keep_the_contract() {
    let (h, task, _) = setup();
    let file = h.home.join("brief.md");
    fs::write(&file, "HERDR-BRIEF role=implementer\nDo the task.\n").unwrap();
    h.conn()
        .execute(
            "update tasks set brief_path=? where id=?",
            params![file.to_str().unwrap(), task],
        )
        .unwrap();
    for _ in 0..2 {
        let result = h.ok(
            &[],
            &[
                "prompt",
                &task.to_string(),
                "--file",
                file.to_str().unwrap(),
            ],
        );
        let attempt = result["attempt_id"].as_i64().unwrap();
        let doc: i64 = h
            .conn()
            .query_row(
                "select id from documents where task_id=? and kind='brief' and captured=1",
                [task],
                |r| r.get(0),
            )
            .unwrap();
        let all_calls = calls(&h);
        let text = all_calls.last().unwrap()[5].as_str().unwrap();
        assert!(text.starts_with(&format!("First taskr got {attempt}; run taskr doc get {doc} and execute that text exactly. Worker:")), "{text}");
        assert!(text.contains("run that got even if a hook did (dup = success)"));
        assert!(!text.contains(file.to_str().unwrap()));
        let captured = h.output(&[], &["doc", "get", &doc.to_string()], b"");
        assert!(captured.status.success(), "{captured:?}");
        assert_eq!(
            captured.stdout,
            b"HERDR-BRIEF role=implementer\nDo the task.\n"
        );
    }
    assert_eq!(
        h.count(
            "select count(*) from documents where task_id=? and kind='brief'",
            task
        ),
        1
    );
}

#[test]
fn uncaptured_files_and_capture_failures_refuse_without_an_attempt() {
    for bytes in [
        b"binary\0brief".to_vec(),
        vec![b'a'; (1 << 20) + 1],
        vec![0xff],
    ] {
        let (h, task, _) = setup();
        let file = h.home.join("brief.md");
        fs::write(&file, bytes).unwrap();
        refused(
            &h,
            task,
            &[
                "prompt",
                &task.to_string(),
                "--file",
                file.to_str().unwrap(),
            ],
            2,
            "must be captured",
        );
    }
    let (h, task, _) = setup();
    let file = h.home.join("brief.md");
    refused(
        &h,
        task,
        &[
            "prompt",
            &task.to_string(),
            "--file",
            file.to_str().unwrap(),
        ],
        2,
        "read prompt file",
    );
    assert!(calls(&h).is_empty());
    fs::write(&file, "valid text").unwrap();
    h.conn().execute_batch("create trigger refuse_document before insert on documents begin select raise(abort,'fixture capture failure'); end;").unwrap();
    refused(
        &h,
        task,
        &[
            "prompt",
            &task.to_string(),
            "--file",
            file.to_str().unwrap(),
        ],
        2,
        "could not be captured",
    );
    assert_eq!(
        h.count(
            "select count(*) from meta where key like 'receipt_due:%' and ? > 0",
            task
        ),
        0
    );
}

#[test]
fn local_file_prompts_keep_the_original_path_and_contract() {
    let (h, task, launch) = setup();
    h.conn()
        .execute("update launches set machine=null where id=?", [launch])
        .unwrap();
    let file = h.home.join("brief.md");
    fs::write(&file, "work").unwrap();
    let result = h.ok(
        &[],
        &[
            "prompt",
            &task.to_string(),
            "--file",
            file.to_str().unwrap(),
        ],
    );
    let attempt = result["attempt_id"].as_i64().unwrap();
    let all_calls = calls(&h);
    assert_eq!(all_calls.len(), 1);
    assert_eq!(all_calls[0][0], "agent");
    assert!(all_calls[0][3].as_str().unwrap().starts_with(&format!(
        "First taskr got {attempt}; read {}; execute exactly. Worker:",
        file.display()
    )));
    assert!(event_data(&h, attempt)["machine"].is_null());
}

#[test]
fn answer_prompt_uses_the_same_machine_route() {
    let (h, task, launch) = setup();
    h.conn()
        .execute("update launches set machine=null where id=?", [launch])
        .unwrap();
    let ask = h.ok(
        &[
            ("TASKR_TASK".into(), task.to_string()),
            ("TASKR_LAUNCH".into(), launch.to_string()),
        ],
        &["ask", "What next?", "--blocking"],
    )["event_id"]
        .as_i64()
        .unwrap();
    h.conn()
        .execute("update launches set machine='host-a' where id=?", [launch])
        .unwrap();
    let result = h.ok(
        &[],
        &[
            "answer",
            &ask.to_string(),
            "Continue.",
            "--as",
            "1",
            "--prompt",
        ],
    );
    let attempt = result["attempt_id"].as_i64().unwrap();
    assert_eq!(result["outcome"], "activity_observed");
    assert_eq!(event_data(&h, attempt)["machine"], "remote-a");
    let all_calls = calls(&h);
    assert_eq!(
        all_calls.last().unwrap()[5],
        format!("First taskr got {attempt}. ask {ask}: Continue.")
    );
}

#[test]
fn a_non_push_live_root_uses_its_task_host_and_machine_route() {
    let (h, _, _) = setup();
    let root = h.ok(
        &[],
        &[
            "new",
            "remote-root",
            "--role",
            "orchestrator",
            "--pane",
            "w9:p1",
        ],
    )["task_id"]
        .as_i64()
        .unwrap();
    h.conn()
        .execute("update tasks set machine='host-a' where id=?", [root])
        .unwrap();
    let result = h.ok(
        &[],
        &[
            "prompt",
            &root.to_string(),
            "--as",
            "1",
            "--text",
            "Continue.",
        ],
    );
    assert_eq!(result["path"], "pane");
    assert_eq!(result["outcome"], "activity_observed");
    let attempt = result["attempt_id"].as_i64().unwrap();
    assert_eq!(event_data(&h, attempt)["machine"], "remote-a");
    assert_eq!(calls(&h).last().unwrap()[0], "--machine");
}
