//! Modern-only endpoint. All authority and durable state live behind `taskr jobs` RPC.
use super::*;
mod protocol;
use protocol::{PROTOCOL, complete, metadata, parameters, tool, tools};

pub(super) fn dispatch(json_mode: bool, keyed: bool, args: &[String]) -> ExitCode {
    let execute = || -> Result<(), String> {
        if keyed {
            return Err("mcp --jobs does not accept --request-key".into());
        }
        if !std::env::var("TASKR_LAUNCH").unwrap_or_default().is_empty() {
            return Err(
                "mcp --jobs requires root authority; worker launches cannot become roots".into(),
            );
        }
        if let Some(k) = crate::write::repeated(args, &["jobs", "as", "session", "binding", "help"])
        {
            return Err(format!("repeated jobs flag --{k}"));
        }
        let mut f = FlagSet::new("mcp --jobs", json_mode);
        f.bool("jobs", false, "modern jobs endpoint")
            .int("as", 0, "campaign root")
            .int("session", -1, "current root generation")
            .string("binding", "", "fixed session binding identity");
        f.parse(args, 0, 0)?;
        if f.help() {
            print!(
                "{}",
                f.usage("taskr mcp --jobs --as ROOT --session GEN --binding TOKEN")
            );
            return Ok(());
        }
        if !f.get_bool("jobs") || !f.was_set("session") {
            return Err("jobs requires --as ROOT --session GEN --binding TOKEN".into());
        }
        let server = Jobs {
            root: f.get_int("as"),
            generation: f.get_int("session"),
            binding: f.get_string("binding").into(),
            cwd: std::env::current_dir().map_err(|e| e.to_string())?,
        };
        server
            .call("validate", json!({}))
            .map_err(|e| format!("jobs startup: {}", e.message))?;
        server.serve();
        Ok(())
    };
    match execute() {
        Ok(()) => ExitCode::Ok,
        Err(e) => crate::cli::error(json_mode, "mcp --jobs", &e, ExitCode::Rejected),
    }
}
struct Jobs {
    root: i64,
    generation: i64,
    binding: String,
    cwd: PathBuf,
}
impl Jobs {
    fn call(&self, verb: &str, data: Value) -> Result<Value, RpcError> {
        let args = vec![
            "jobs".into(),
            verb.into(),
            "--as".into(),
            self.root.to_string(),
            "--generation".into(),
            self.generation.to_string(),
            "--binding".into(),
            self.binding.clone(),
            "--data".into(),
            compact_json(&data).expect("JSON"),
        ];
        run(&self.cwd, args, None)
            .value()
            .map_err(|v| rpc_error(-32000, compact_json(&v).expect("JSON")))
    }
    fn serve(&self) {
        // Bounded read-ahead allows maintenance while an idle client leaves stdin open.
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Some(line) = next_line(&mut input) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut maintenance = std::time::Instant::now();
        loop {
            if BROKEN.load(Ordering::SeqCst) {
                break;
            }
            if maintenance.elapsed() >= std::time::Duration::from_secs(5) {
                if let Err(e) = self.call("validate", json!({})) {
                    eprintln!("taskr mcp --jobs maintenance: {}", e.message);
                    break;
                }
                maintenance = std::time::Instant::now();
            }
            let timeout = std::time::Duration::from_secs(5).saturating_sub(maintenance.elapsed());
            let line = match rx.recv_timeout(timeout) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let line = match line {
                Ok(line) => line,
                Err(()) => {
                    reply(&Value::Null, Err(invalid_params("request exceeds 1 MiB")));
                    continue;
                }
            };
            if line.is_empty() {
                continue;
            }
            if line.len() > taskr_core::store::work::MAX_JSON {
                reply(
                    &Value::Null,
                    Err(invalid_params("jobs request exceeds 256 KiB")),
                );
                continue;
            }
            let value = match taskr_core::store::work::parse::<Value>(
                std::str::from_utf8(&line).unwrap_or(""),
            ) {
                Ok(v) => v,
                Err(e) => {
                    reply(&Value::Null, Err(rpc_error(-32700, e.message)));
                    continue;
                }
            };
            if !value.is_object() || value["jsonrpc"] != "2.0" || !value["method"].is_string() {
                reply(
                    &Value::Null,
                    Err(rpc_error(-32600, "invalid JSON-RPC request")),
                );
                continue;
            }
            // Notifications never invoke tools or mutate the ledger.
            if value.get("id").is_none() {
                continue;
            }
            let id = value["id"].clone();
            if !(id.is_string() || id.as_i64().is_some()) {
                reply(
                    &Value::Null,
                    Err(rpc_error(-32600, "invalid JSON-RPC request")),
                );
                continue;
            }
            reply(&id, self.request(&value));
        }
    }
    fn request(&self, r: &Value) -> Result<Value, RpcError> {
        if r["method"] == "initialize" {
            return Err(rpc_error(
                -32601,
                "This jobs endpoint supports modern MCP 2026-07-28 only; use server/discover or the legacy channel endpoint.",
            ));
        }
        let caps = metadata(&r["params"])?;
        let tasks = caps["extensions"]["io.modelcontextprotocol/tasks"]
            .as_object()
            .is_some_and(|m| m.is_empty());
        let method = r["method"]
            .as_str()
            .ok_or_else(|| invalid_params("method must be a string"))?;
        match method {
            "server/discover" => {
                parameters(&r["params"], &[])?;
                Ok(
                    json!({"resultType":"complete","supportedVersions":[PROTOCOL],"capabilities":{"tools":{},"extensions":{"io.modelcontextprotocol/tasks":{}}},"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"taskr-jobs","version":env!("CARGO_PKG_VERSION")}},"instructions":"Await an existing work revision. Native results require result_receipt after processing. Cancelling releases tracking; it does not stop worker execution. Owner replies can be supplied by hooks and are not proof of human consent."}),
                )
            }
            "ping" => {
                parameters(&r["params"], &[])?;
                Ok(json!({"resultType":"complete"}))
            }
            "tools/list" => {
                parameters(&r["params"], &["cursor"])?;
                if r["params"].get("cursor").is_some_and(|v| !v.is_string()) {
                    return Err(invalid_params("cursor must be a string"));
                }
                Ok(json!({"resultType":"complete","tools":tools()}))
            }
            "tools/call" => {
                parameters(&r["params"], &["name", "arguments"])?;
                let name = r["params"]["name"]
                    .as_str()
                    .ok_or_else(|| invalid_params("tool name required"))?;
                let args = &r["params"]["arguments"];
                protocol::arguments(name, args)?;
                let verb = match name {
                    "work_get" => "get",
                    "work_add" => "add",
                    "work_await" => "await",
                    "result_receipt" => "receipt",
                    "owner_input" => "owner",
                    _ => return Err(invalid_params("unknown jobs tool")),
                };
                let async_tool = matches!(name, "work_await" | "owner_input");
                let elicitation = caps["elicitation"]["form"].is_object();
                if async_tool && (!tasks || name == "owner_input" && !elicitation) {
                    return Ok(complete(tool(
                        json!({"outcome":"use_ledger","reason":"Required native capability unavailable; ordinary taskr ledger handling remains active."}),
                    )));
                }
                let mut value = self.call(verb, args.clone())?;
                if async_tool {
                    value.as_object_mut().expect("task").remove("result");
                    value.as_object_mut().expect("task").remove("error");
                    value.as_object_mut().expect("task").remove("inputRequests");
                    value["resultType"] = json!("task");
                    Ok(value)
                } else {
                    Ok(complete(tool(value)))
                }
            }
            "tasks/get" | "tasks/cancel" | "tasks/update" => {
                if !tasks {
                    return Err(invalid_params(
                        "tasks capability must be advertised on this request",
                    ));
                }
                parameters(
                    &r["params"],
                    if method == "tasks/update" {
                        &["taskId", "inputResponses"]
                    } else {
                        &["taskId"]
                    },
                )?;
                let id = r["params"]["taskId"]
                    .as_str()
                    .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                    .ok_or_else(|| invalid_params("valid taskId required"))?;
                let verb = match method {
                    "tasks/get" => "poll",
                    "tasks/cancel" => "cancel",
                    _ => "update",
                };
                let mut data = json!({"handle":id});
                if verb == "update" {
                    if !caps["elicitation"]["form"].is_object() {
                        return Err(invalid_params("elicitation form capability required"));
                    }
                    data["responses"] = r["params"]["inputResponses"].clone();
                }
                let value = self.call(verb, data)?;
                if method == "tasks/get" {
                    if value["status"] == "input_required"
                        && !caps["elicitation"]["form"].is_object()
                    {
                        return Err(invalid_params(
                            "owner input requires elicitation form capability on this request; continue through ledger UI",
                        ));
                    }
                    Ok(value)
                } else {
                    Ok(json!({"resultType":"complete"}))
                }
            }
            _ => Err(rpc_error(-32601, "unknown modern jobs method")),
        }
    }
}
