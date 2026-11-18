//! `log --brief` and `inbox --brief`: one text line per event, `ID KIND TASK SUMMARY`.
//! Only routine kinds are cut; anything a lead decides on prints whole.
use serde_json::Value;

/// Summary chars kept on a cut line.
pub(super) const CUT: usize = 160;

/// Routine kinds whose summary may be cut. Every other kind, unknown ones
/// included, prints whole. A `herdr` row is routine only as a plain hint:
/// a reason (stall, push_exhausted, model_capacity, message_unreceived, ...)
/// or a quota field makes it an alarm.
fn routine(ev: &Value) -> bool {
    match ev["kind"].as_str().unwrap_or("") {
        "got" | "note" | "start" | "launch" | "ready" | "done" => true,
        "herdr" => {
            let d = &ev["data"];
            d.get("reason").is_none_or(Value::is_null) && d.get("quota").is_none()
        }
        _ => false,
    }
}

/// Newlines, carriage returns and tabs print escaped so one event stays one line.
fn flat(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn task(v: &Value) -> String {
    let id = &v["task_id"];
    match v["task_name"].as_str() {
        Some(name) if !name.is_empty() => format!("{id}:{name}"),
        _ => id.to_string(),
    }
}

/// The text for one log or inbox row, newline-terminated. A launch record from
/// `log` prints as `L<id> launch TASK provider model effort pane=P status`.
pub(super) fn render(v: &Value) -> String {
    if v["record"] == "launch" {
        let mut parts = vec![
            format!("L{}", v["id"]),
            "launch".into(),
            v["task_id"].to_string(),
        ];
        for key in ["provider", "model", "effort"] {
            if let Some(s) = v[key].as_str().filter(|s| !s.is_empty()) {
                parts.push(flat(s));
            }
        }
        if let Some(s) = v["pane_id"].as_str().filter(|s| !s.is_empty()) {
            parts.push(format!("pane={}", flat(s)));
        }
        if let Some(s) = v["observed_status"].as_str().filter(|s| !s.is_empty()) {
            parts.push(flat(s));
        }
        return parts.join(" ") + "\n";
    }
    let id = &v["id"];
    let kind = v["kind"].as_str().unwrap_or("-");
    let mut head = format!("{id} {} {}", flat(kind), task(v));
    let summary = flat(v["summary"].as_str().unwrap_or(""));
    if !summary.is_empty() {
        head.push(' ');
        let len = summary.chars().count();
        if routine(v) && len > CUT {
            head.extend(summary.chars().take(CUT));
            head.push_str(&format!(" [{id} +{} chars]", len - CUT));
        } else {
            head.push_str(&summary);
        }
    }
    head.push('\n');
    let d = &v["data"];
    // A seat's verdict and an ask's structured choices print whole on their own lines.
    if let Some(line) = d["verdict_line"].as_str() {
        head.push_str(&format!("  {}\n", flat(line)));
    }
    if matches!(kind, "ask" | "owner_ask")
        && let Some(q) = d.get("question").filter(|q| q.is_object())
    {
        head.push_str(&format!(
            "  question {}\n",
            taskr_core::compact_json(q).unwrap()
        ));
    }
    head
}

/// `inbox --brief`: a header line, one line per event, and a final line when more wait.
pub(super) fn inbox(v: &Value) -> String {
    let pend = v["pending_event_id"]
        .as_i64()
        .map_or("-".to_string(), |n| n.to_string());
    let events = v["events"].as_array().map_or(&[][..], Vec::as_slice);
    let mut out = format!(
        "inbox ack={} pend={pend} events={}\n",
        v["acked_event_id"],
        events.len()
    );
    for ev in events {
        out.push_str(&render(ev));
    }
    if v["more"] == true {
        out.push_str("more: raise --limit (max 200) or ack\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cuts_routine_kinds_only() {
        let long = "x".repeat(200);
        let note =
            render(&json!({"id":7,"task_id":3,"task_name":"impl","kind":"note","summary":long}));
        assert_eq!(
            note,
            format!("7 note 3:impl {} [7 +40 chars]\n", "x".repeat(160))
        );
        for kind in [
            "ask",
            "fail",
            "message",
            "owner_answer",
            "decision",
            "pr",
            "mystery",
        ] {
            let line = render(&json!({"id":8,"task_id":3,"kind":kind,"summary":long}));
            assert_eq!(line, format!("8 {kind} 3 {long}\n"));
        }
        let alarm = render(
            &json!({"id":9,"task_id":3,"kind":"herdr","summary":long,"data":{"reason":"stall"}}),
        );
        assert!(alarm.ends_with(&format!("{long}\n")), "{alarm}");
        let hint = render(&json!({"id":9,"task_id":3,"kind":"herdr","summary":long,"data":{}}));
        assert!(hint.ends_with(" [9 +40 chars]\n"), "{hint}");
    }

    #[test]
    fn counts_chars_and_flattens_newlines() {
        let s = "é".repeat(170);
        let line = render(&json!({"id":1,"task_id":2,"kind":"got","summary":s}));
        assert!(line.ends_with(" [1 +10 chars]\n"), "{line}");
        let line = render(&json!({"id":1,"task_id":2,"kind":"ask","summary":"a\nb\tc"}));
        assert_eq!(line, "1 ask 2 a\\nb\\tc\n");
        assert_eq!(
            render(&json!({"id":1,"task_id":2,"kind":"got","summary":""})),
            "1 got 2\n"
        );
    }
}
