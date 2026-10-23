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
