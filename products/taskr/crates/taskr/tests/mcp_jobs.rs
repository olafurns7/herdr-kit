#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::Harness;
const BINDING: &str = "synthetic-mcp-session";
fn meta(tasks: bool, form: bool) -> Value {
    let mut caps = json!({});
    if tasks {
        caps["extensions"] = json!({"io.modelcontextprotocol/tasks":{}})
    }
    if form {
        caps["elicitation"] = json!({"form":{}})
    }
    json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"synthetic-client","version":"1"},"io.modelcontextprotocol/clientCapabilities":caps})
}
fn request(id: i64, method: &str, mut params: Value, tasks: bool, form: bool) -> Value {
    params["_meta"] = meta(tasks, form);
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn server(h: &Harness, root: i64, input: &[Value]) -> Vec<Value> {
    let text = input.iter().map(|v| format!("{v}\n")).collect::<String>();
    let out = h.output(
        &[],
        &[
            "mcp",
            "--jobs",
            "--as",
            &root.to_string(),
            "--session",
            "0",
            "--binding",
            BINDING,
        ],
        text.as_bytes(),
    );
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn root(h: &Harness) -> i64 {
    h.ok(&[], &["new", "jobs-root", "--role", "orchestrator"])["task_id"]
        .as_i64()
        .unwrap()
}
#[test]
fn discovery_is_modern_and_each_request_checks_metadata() {
    let h = Harness::new();
    let root = root(&h);
    let mut wrong = request(2, "server/discover", json!({}), true, false);
    wrong["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    let replies = server(
        &h,
        root,
        &[
            request(1, "server/discover", json!({}), false, false),
            wrong,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}),
            request(4, "tools/list", json!({}), true, false),
            request(5, "initialize", json!({}), false, false),
        ],
    );
    assert_eq!(replies.len(), 5);
    assert_eq!(
        replies[0]["result"]["supportedVersions"],
        json!(["2026-07-28"])
    );
    assert_eq!(replies[1]["error"]["code"], -32022);
    assert_eq!(replies[1]["error"]["data"]["requested"], "1900-01-01");
    assert_eq!(
        replies[1]["error"]["data"]["supported"],
        json!(["2026-07-28"])
    );
    assert_eq!(replies[2]["error"]["code"], -32602);
    let tools = replies[3]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 5);
    assert!(!tools.iter().any(|t| t["name"] == "allow-ask"));
    assert!(
        replies[4]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("2026-07-28")
    );
}
#[test]
fn capability_fallback_unknown_and_invalid_calls_never_create_operations() {
    let h = Harness::new();
    let root = root(&h);
    let replies = server(
        &h,
        root,
        &[
            request(
                1,
                "tools/call",
                json!({"name":"work_await","arguments":{"work":1,"revision":1}}),
                false,
                false,
            ),
            request(
                2,
                "tools/call",
                json!({"name":"owner_input","arguments":{"ask":1}}),
                true,
                false,
            ),
            request(
                3,
                "tools/call",
                json!({"name":"allow-ask","arguments":{"ask":1}}),
                true,
                true,
            ),
            request(
                4,
                "tools/call",
                json!({"name":"work_await","arguments":{"work":1,"revision":1,"as":2}}),
                true,
                false,
            ),
            request(5, "tasks/get", json!({"taskId":"bad"}), true, false),
            request(
                6,
                "tasks/get",
                json!({"taskId":"a".repeat(32)}),
                false,
                false,
            ),
            request(7, "unknown", json!({}), true, false),
            request(
                8,
                "tools/call",
                json!({"name":"work_add","arguments":{"key":"k","campaign":root,"title":"x".repeat(513)}}),
                true,
                false,
            ),
        ],
    );
    for reply in &replies[..2] {
        assert_eq!(reply["result"]["resultType"], "complete");
        assert_eq!(
            reply["result"]["structuredContent"]["outcome"],
            "use_ledger"
        );
    }
    for reply in &replies[2..] {
        assert!(reply["error"].is_object(), "{reply}");
    }
    assert_eq!(
        h.count("select count(*) from jobs_operations where root_id=?", root),
        0
    );
    assert_eq!(
        h.count("select count(*) from consumer_leases where task_id=?", root),
        0
    );
}
#[test]
fn immediate_task_handle_survives_adapter_restart_and_duplicate_cancel() {
    let h = Harness::new();
    let root = root(&h);
    let added = server(
        &h,
        root,
        &[request(
            1,
            "tools/call",
            json!({"name":"work_add","arguments":{"key":"one","campaign":root,"title":"synthetic job"}}),
            false,
            false,
        )],
    );
    let work = added[0]["result"]["structuredContent"]["work_id"]
        .as_i64()
        .unwrap();
    let created = server(
        &h,
        root,
        &[request(
            2,
            "tools/call",
            json!({"name":"work_await","arguments":{"work":work,"revision":1}}),
            true,
            false,
        )],
    );
    let task = &created[0]["result"];
    assert_eq!(task["resultType"], "task");
    assert_eq!(task["status"], "working");
    for field in ["taskId", "createdAt", "lastUpdatedAt", "ttlMs"] {
        assert!(task.get(field).is_some(), "{task}");
    }
    assert_eq!(task["pollIntervalMs"], 5000);
    let id = task["taskId"].as_str().unwrap();
    let replies = server(
        &h,
        root,
        &[
            request(3, "tasks/get", json!({"taskId":id}), true, false),
            request(4, "tasks/cancel", json!({"taskId":id}), true, false),
            request(5, "tasks/cancel", json!({"taskId":id}), true, false),
            request(6, "tasks/get", json!({"taskId":id}), true, false),
        ],
    );
    assert_eq!(replies[0]["result"]["resultType"], "complete");
    assert_eq!(replies[0]["result"]["status"], "working");
    assert_eq!(replies[1]["result"], json!({"resultType":"complete"}));
    assert_eq!(replies[2]["result"], replies[1]["result"]);
    assert_eq!(replies[3]["result"]["status"], "cancelled");
    assert_eq!(
        h.count("select count(*) from work_runs where root_id=?", root),
        0
    );
}
#[test]
fn oversized_requests_are_bounded_and_next_line_still_parses() {
    let h = Harness::new();
    let root = root(&h);
    let mut text = "x".repeat((1 << 20) + 1);
    text.push('\n');
    text.push_str(&request(1, "ping", json!({}), false, false).to_string());
    text.push('\n');
    let out = h.output(
        &[],
        &[
            "mcp",
            "--jobs",
            "--as",
            &root.to_string(),
            "--session",
            "0",
            "--binding",
            BINDING,
        ],
        text.as_bytes(),
    );
    assert!(out.status.success());
    let rows = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["error"]["code"], -32602);
    assert_eq!(rows[1]["result"]["resultType"], "complete");
}
#[test]
fn original_binding_can_restart_and_repeatedly_read_a_stale_generation_handle() {
    let h = Harness::new();
    let root = root(&h);
    let work = h.ok(
        &[],
        &[
            "jobs",
            "add",
            "--as",
            &root.to_string(),
            "--generation",
            "0",
            "--binding",
            BINDING,
            "--data",
            &json!({"key":"one","campaign":root,"title":"synthetic job"}).to_string(),
        ],
    )["work_id"]
        .as_i64()
        .unwrap();
    let created = server(
        &h,
        root,
        &[request(
            1,
            "tools/call",
            json!({"name":"work_await","arguments":{"work":work,"revision":1}}),
            true,
            false,
        )],
    );
    let id = created[0]["result"]["taskId"].as_str().unwrap();
    h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [root],
        )
        .unwrap();
    let replies = server(
        &h,
        root,
        &[
            request(2, "tasks/get", json!({"taskId":id}), true, false),
            request(3, "tasks/get", json!({"taskId":id}), true, false),
            request(
                4,
                "tools/call",
                json!({"name":"work_await","arguments":{"work":work,"revision":1}}),
                true,
                false,
            ),
        ],
    );
    assert_eq!(replies[0]["result"], replies[1]["result"]);
    assert_eq!(
        replies[0]["result"]["result"]["structuredContent"]["outcome"],
        "released_to_channel"
    );
    assert_eq!(replies[0]["result"]["routeState"], "released");
    assert!(replies[2]["error"].is_object());
    assert_eq!(
        h.count("select count(*) from jobs_operations where root_id=?", root),
        1
    );
}
#[test]
fn live_adapter_reads_old_handles_after_generation_change_without_new_effects() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, Stdio},
        sync::mpsc,
        thread,
        time::Duration,
    };
    struct Live {
        child: Child,
        input: ChildStdin,
        replies: mpsc::Receiver<String>,
        reader: Option<thread::JoinHandle<()>>,
    }
    impl Live {
        fn call(&mut self, value: Value) -> Value {
            writeln!(self.input, "{value}").unwrap();
            self.input.flush().unwrap();
            let line = self
                .replies
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|e| {
                    panic!(
                        "MCP response deadline: {e}; child={:?}",
                        self.child.try_wait()
                    )
                });
            serde_json::from_str(&line).unwrap()
        }
    }
    impl Drop for Live {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
    let h = Harness::new();
    let root = root(&h);
    let work = h.ok(
        &[],
        &[
            "jobs",
            "add",
            "--as",
            &root.to_string(),
            "--generation",
            "0",
            "--binding",
            BINDING,
            "--data",
            &json!({"key":"one","campaign":root,"title":"synthetic job"}).to_string(),
        ],
    )["work_id"]
        .as_i64()
        .unwrap();
    let ask=h.ok(&[],&["ask","--owner","--as",&root.to_string(),"--question",&json!({"question":"Synthetic choice?","options":[{"label":"Proceed"},{"label":"Hold"}]}).to_string()])["ask_id"].as_i64().unwrap();
    h.ok(
        &[],
        &[
            "jobs",
            "allow-ask",
            "--as",
            &root.to_string(),
            "--generation",
            "0",
            "--binding",
            BINDING,
            "--data",
            &json!({"ask":ask}).to_string(),
        ],
    );
    let mut child = h
        .command(&[])
        .args([
            "mcp",
            "--jobs",
            "--as",
            &root.to_string(),
            "--session",
            "0",
            "--binding",
            BINDING,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let (tx, replies) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let mut live = Live {
        child,
        input,
        replies,
        reader: Some(reader),
    };
    let created = live.call(request(
        1,
        "tools/call",
        json!({"name":"work_await","arguments":{"work":work,"revision":1}}),
        true,
        false,
    ));
    let id = created["result"]["taskId"].as_str().unwrap().to_string();
    let owner = live.call(request(
        2,
        "tools/call",
        json!({"name":"owner_input","arguments":{"ask":ask}}),
        true,
        true,
    ));
    let owner = owner["result"]["taskId"].as_str().unwrap().to_string();
    let deadline: String = h
        .conn()
        .query_row(
            "select deadline from jobs_routes where operation_id=?",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    h.conn()
        .execute(
            "insert into events(task_id,kind,created_at) values(?,'adopt','t')",
            [root],
        )
        .unwrap();
    assert!(live.child.try_wait().unwrap().is_none());
    let first = live.call(request(3, "tasks/get", json!({"taskId":id}), true, false));
    assert_eq!(
        first["result"]["result"]["structuredContent"]["outcome"],
        "released_to_channel"
    );
    assert_eq!(first["result"]["routeState"], "released");
    for seq in 4..7 {
        assert_eq!(
            live.call(request(seq, "tasks/get", json!({"taskId":id}), true, false))["result"],
            first["result"]
        );
    }
    assert_eq!(
        h.conn()
            .query_row(
                "select deadline from jobs_routes where operation_id=?",
                [&id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        deadline
    );
    for (seq, name, args) in [
        (
            7,
            "work_add",
            json!({"key":"refused","campaign":root,"title":"refused"}),
        ),
        (8, "work_await", json!({"work":work,"revision":1})),
        (9, "result_receipt", json!({"handle":id,"result":1})),
    ] {
        assert!(
            live.call(request(
                seq,
                "tools/call",
                json!({"name":name,"arguments":args}),
                true,
                false
            ))["error"]
                .is_object()
        );
    }
    assert!(live.call(request(10,"tasks/update",json!({"taskId":owner,"inputResponses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}}),true,true))["error"].is_object());
    assert!(live.child.try_wait().unwrap().is_none());
    assert_eq!(
        h.count("select count(*) from jobs_operations where root_id=?", root),
        2
    );
    assert_eq!(
        h.count("select count(*) from work_items where root_id=?", root),
        1
    );
    assert_eq!(
        h.count(
            "select count(*) from events where id=? and answered_by is null",
            ask
        ),
        1
    );
}
#[test]
fn opted_in_owner_task_roundtrips_typed_reply_and_records_native_answer_once() {
    let h = Harness::new();
    let root = root(&h);
    let ask=h.ok(&[],&["ask","--owner","--as",&root.to_string(),"--question",&json!({"question":"Synthetic choice?","options":[{"label":"Proceed"},{"label":"Hold"}]}).to_string()])["ask_id"].as_i64().unwrap();
    h.ok(
        &[],
        &[
            "jobs",
            "allow-ask",
            "--as",
            &root.to_string(),
            "--generation",
            "0",
            "--binding",
            BINDING,
            "--data",
            &json!({"ask":ask}).to_string(),
        ],
    );
    let created = server(
        &h,
        root,
        &[request(
            1,
            "tools/call",
            json!({"name":"owner_input","arguments":{"ask":ask}}),
            true,
            true,
        )],
    );
    let id = created[0]["result"]["taskId"].as_str().unwrap();
    assert_eq!(created[0]["result"]["status"], "input_required");
    let update = json!({"taskId":id,"inputResponses":{"owner_choice":{"action":"accept","content":{"choice":"Proceed"}}}});
    let replies = server(
        &h,
        root,
        &[
            request(2, "tasks/get", json!({"taskId":id}), true, false),
            request(3, "tasks/get", json!({"taskId":id}), true, true),
            request(4, "tasks/update", update.clone(), true, true),
            request(5, "tasks/update", update, true, true),
            request(6, "tasks/get", json!({"taskId":id}), true, true),
        ],
    );
    assert!(replies[0]["error"].is_object());
    assert_eq!(
        replies[1]["result"]["inputRequests"]["owner_choice"]["method"],
        "elicitation/create"
    );
    assert_eq!(replies[2]["result"], json!({"resultType":"complete"}));
    assert_eq!(replies[2]["result"], replies[3]["result"]);
    assert_eq!(replies[4]["result"]["status"], "completed");
    assert_eq!(
        replies[4]["result"]["result"]["structuredContent"]["provenance"],
        format!("native:{BINDING}")
    );
    assert_eq!(
        h.count(
            "select count(*) from events where kind='answer' and related_event_id=?",
            ask
        ),
        1
    );
}
