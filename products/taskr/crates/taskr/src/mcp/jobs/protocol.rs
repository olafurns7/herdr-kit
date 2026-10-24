use super::*;
pub(super) const PROTOCOL: &str = "2026-07-28";
pub(super) fn metadata(params: &Value) -> Result<&Value, RpcError> {
    if !params.is_object() {
        return Err(invalid_params(
            "params must include modern per-request metadata",
        ));
    }
    let meta = params["_meta"]
        .as_object()
        .ok_or_else(|| invalid_params("modern _meta required on every request"))?;
    let version = meta
        .get("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("protocolVersion metadata required"))?;
    if version != PROTOCOL {
        return Err(RpcError {
            code: -32022,
            message: "Unsupported protocol version".into(),
            data: Some(json!({"supported":[PROTOCOL],"requested":version})),
        });
    }
    let info = meta
        .get("io.modelcontextprotocol/clientInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_params("clientInfo metadata required"))?;
    for key in ["name", "version"] {
        if !info
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && s.len() <= 256 && !s.contains('\0'))
        {
            return Err(invalid_params(
                "clientInfo name and version must be bounded nonempty strings",
            ));
        }
    }
    let caps = meta
        .get("io.modelcontextprotocol/clientCapabilities")
        .filter(|v| v.is_object())
        .ok_or_else(|| invalid_params("clientCapabilities object required"))?;
    if let Some(ext) = caps.get("extensions") {
        let ext = ext
            .as_object()
            .ok_or_else(|| invalid_params("capability extensions must be an object"))?;
        if ext.values().any(|v| !v.is_object()) {
            return Err(invalid_params("extension settings must be objects"));
        }
        if ext
            .get("io.modelcontextprotocol/tasks")
            .and_then(Value::as_object)
            .is_some_and(|m| !m.is_empty())
        {
            return Err(invalid_params("tasks extension settings must be empty"));
        }
    }
    if caps.get("elicitation").is_some_and(|v| !v.is_object()) {
        return Err(invalid_params("elicitation capability must be an object"));
    }
    Ok(caps)
}
pub(super) fn parameters(params: &Value, allowed: &[&str]) -> Result<(), RpcError> {
    let obj = params
        .as_object()
        .ok_or_else(|| invalid_params("params must be an object"))?;
    if let Some(k) = obj
        .keys()
        .find(|k| k.as_str() != "_meta" && !allowed.contains(&k.as_str()))
    {
        return Err(invalid_params(format!("unknown parameter {k}")));
    }
    Ok(())
}
pub(super) fn complete(mut value: Value) -> Value {
    value["resultType"] = json!("complete");
    value
}
pub(super) fn tool(value: Value) -> Value {
    json!({"content":[{"type":"text","text":compact_json(&value).expect("JSON")}],"structuredContent":value,"isError":false})
}
pub(super) fn tools() -> Value {
    let object = |props: Value, required: Value| json!({"type":"object","properties":props,"required":required,"additionalProperties":false});
    let int = json!({"type":"integer","minimum":1});
    let handle = json!({"type":"string","pattern":"^[0-9a-f]{32}$"});
    json!([
        {"name":"work_get","description":"Read work in this campaign.","annotations":{"readOnlyHint":true},"inputSchema":object(json!({"work":int}),json!(["work"]))},
        {"name":"work_add","description":"Add unassigned work through canonical work authority. Reuse the same key for an identical retry.","annotations":{"idempotentHint":true},"inputSchema":object(json!({"key":{"type":"string","minLength":1,"maxLength":128},"campaign":int,"title":{"type":"string","minLength":1,"maxLength":512},"priority":{"type":"integer","minimum":-2147483648i64,"maximum":2147483647i64}}),json!(["key","campaign","title"]))},
        {"name":"work_await","description":"Track an existing work revision without executing it. Recover lost handles with taskr jobs recover. Process the stable result and call result_receipt.","annotations":{"idempotentHint":true},"inputSchema":object(json!({"work":int,"revision":int}),json!(["work","revision"]))},
        {"name":"result_receipt","description":"Record semantic handling of a stable result after processing it. Idempotent; polling alone is not a receipt.","annotations":{"idempotentHint":true},"inputSchema":object(json!({"handle":handle,"result":int}),json!(["handle","result"]))},
        {"name":"owner_input","description":"Present an existing eligible owner ask. Requires explicit per-ask CLI allow-ask policy; native answers may come from hooks.","annotations":{"idempotentHint":true},"inputSchema":object(json!({"ask":int}),json!(["ask"]))}
    ])
}
pub(super) fn arguments(name: &str, args: &Value) -> Result<(), RpcError> {
    if !args.is_object() {
        return Err(invalid_params("arguments must be an object"));
    }
    let schema = tools()
        .as_array()
        .expect("tools")
        .iter()
        .find(|t| t["name"] == name)
        .cloned()
        .ok_or_else(|| invalid_params("unknown jobs tool"))?;
    let props = schema["inputSchema"]["properties"]
        .as_object()
        .expect("properties");
    for (k, v) in args.as_object().expect("object") {
        let prop = props
            .get(k)
            .ok_or_else(|| invalid_params(format!("unknown argument {k}")))?;
        let valid = match prop["type"].as_str() {
            Some("integer") => v.as_i64().is_some_and(|n| {
                prop["minimum"].as_i64().is_none_or(|m| n >= m)
                    && prop["maximum"].as_i64().is_none_or(|m| n <= m)
            }),
            _ => v.as_str().is_some_and(|s| {
                !s.contains('\0')
                    && prop["minLength"]
                        .as_u64()
                        .is_none_or(|n| s.len() >= n as usize)
                    && prop["maxLength"]
                        .as_u64()
                        .is_none_or(|n| s.len() <= n as usize)
                    && (prop.get("pattern").is_none()
                        || s.len() == 32
                            && s.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            }),
        };
        if !valid {
            return Err(invalid_params(format!("invalid argument {k}")));
        }
    }
    for key in schema["inputSchema"]["required"]
        .as_array()
        .expect("required")
    {
        if args.get(key.as_str().expect("key")).is_none() {
            return Err(invalid_params(format!("missing argument {key}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta(caps: Value) -> Value {
        json!({"_meta":{"io.modelcontextprotocol/protocolVersion":PROTOCOL,"io.modelcontextprotocol/clientInfo":{"name":"synthetic","version":"1"},"io.modelcontextprotocol/clientCapabilities":caps}})
    }
    #[test]
    fn modern_metadata_is_per_request_and_version_error_is_actionable() {
        assert!(metadata(&json!({})).is_err());
        let mut p = meta(json!({"extensions":{"io.modelcontextprotocol/tasks":{}}}));
        assert!(metadata(&p).is_ok());
        p["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2025-06-18");
        let e = metadata(&p).err().unwrap();
        assert_eq!(e.code, -32022);
        assert_eq!(
            e.data.unwrap(),
            json!({"supported":[PROTOCOL],"requested":"2025-06-18"})
        );
        assert!(
            metadata(&meta(
                json!({"extensions":{"io.modelcontextprotocol/tasks":{"unknown":true}}})
            ))
            .is_err()
        );
        assert!(metadata(&meta(json!({}))).is_ok());
    }
    #[test]
    fn bounded_typed_tool_arguments_and_no_policy_tool() {
        assert!(arguments("work_await", &json!({"work":1,"revision":1})).is_ok());
        for args in [
            json!({"work":1,"revision":1,"root":2}),
            json!({"work":1}),
            json!({"work":"1","revision":1}),
            json!({"work":-1,"revision":1}),
        ] {
            assert!(arguments("work_await", &args).is_err());
        }
        assert!(arguments("allow-ask", &json!({"ask":1})).is_err());
        assert!(arguments("work_add", &json!({"campaign":1,"title":"x"})).is_err());
        assert!(arguments("result_receipt", &json!({"handle":"x","result":1})).is_err());
    }
    #[test]
    fn capability_fallback_has_no_native_side_effect() {
        let s = Jobs {
            root: 1,
            generation: 0,
            binding: "synthetic-session".into(),
            cwd: PathBuf::from("/nonexistent"),
        };
        for (name, args, caps) in [
            ("work_await", json!({"work":1,"revision":1}), json!({})),
            (
                "owner_input",
                json!({"ask":1}),
                json!({"extensions":{"io.modelcontextprotocol/tasks":{}}}),
            ),
        ] {
            let mut p = meta(caps);
            p["name"] = json!(name);
            p["arguments"] = args;
            let r = s
                .request(&json!({"method":"tools/call","params":p}))
                .unwrap_or_else(|e| panic!("{}", e.message));
            assert_eq!(r["resultType"], "complete");
            assert_eq!(r["structuredContent"]["outcome"], "use_ledger");
        }
        let r = s
            .request(&json!({"method":"server/discover","params":meta(json!({}))}))
            .unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(r["supportedVersions"], json!([PROTOCOL]));
    }
}
