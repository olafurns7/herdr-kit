//! Synchronous owner dialogs; ledger work uses the same CLI/RPC path as other writes.
use super::*;
use db::{OptionalExtension, params};
use std::{io::Read, process::Stdio, time::Duration};

const WORKER_DENIAL: &str = "workers do not question the owner; use `taskr ask TEXT --blocking`";

pub(crate) fn hook(args: &[String]) -> Option<ExitCode> {
    let args = if args.first().is_some_and(|s| s == "--json") {
        &args[1..]
    } else {
        args
    };
    if args.len() != 3
        || args[0] != "hook"
        || args[1] != "claude"
        || !["PreToolUse", "PostToolUse"].contains(&args[2].as_str())
    {
        return None;
    }
    if store::env("HERDR_ENV") != "1" {
        return Some(ExitCode::Ok);
    }
    let post = args[2] == "PostToolUse";
    let (send, recv) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = std::io::stdin()
            .take((128 << 10) + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.send(result);
    });
    let input = match recv.recv_timeout(Duration::from_millis(500)) {
        Ok(Ok(bytes)) if bytes.len() <= 128 << 10 => {
            serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
        }
        _ => Err("dialog input unreadable, too large or timed out".into()),
    };
    let unrecorded = input
        .as_ref()
        .ok()
        .filter(|v| post && v["tool_name"] == "AskUserQuestion")
        .map(|v| v["tool_response"].clone());
    let mut unavailable = false;
    let result = input.and_then(|v| {
        if v["tool_name"] != "AskUserQuestion" {
            return Ok(());
        }
        if !store::env("TASKR_TASK").is_empty() {
            return Err(WORKER_DENIAL.into());
        }
        // Keep only the question/answer data; never send the rest of the hook envelope.
        let payload = json!({"questions":v["tool_input"]["questions"],
            "answers":v["tool_response"]["answers"],
            "response":v["tool_response"]["response"],
            "annotations":v["tool_response"]["annotations"],
            "afkTimeoutMs":v["tool_response"]["afkTimeoutMs"]});
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime.block_on(async {
            let mut cmd =
                tokio::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
            cmd.args([
                "--json",
                "_dialog",
                if post { "post" } else { "pre" },
                &payload.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
            match tokio::time::timeout(Duration::from_secs(3), cmd.output()).await {
                Ok(Ok(out)) if out.status.success() => {
                    eprint!("{}", String::from_utf8_lossy(&out.stderr));
                    Ok(())
                }
                Ok(Ok(out)) => {
                    unavailable = out.status.code() == Some(5);
                    Err(format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stderr),
                        String::from_utf8_lossy(&out.stdout)
                    ))
                }
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => {
                    unavailable = true;
                    Err("dialog ledger operation timed out".into())
                }
            }
        })
    });
    Some(match result {
        Ok(()) => ExitCode::Ok,
        Err(e) => {
            eprintln!("taskr owner dialog: {e}");
            if let Some(response) = unrecorded {
                eprintln!("unrecorded dialog answers: {response}");
            }
            if !post {
                eprintln!(
                    "Ask in plain text and record it with the taskr CLI{}.",
                    if unavailable {
                        " when the server is back"
                    } else {
                        ""
                    }
                );
            }
            ExitCode::Usage // Claude blocks a PreToolUse command that exits 2.
        }
    })
}

pub(super) fn apply(phase: &str, raw: &str) -> Result<Value> {
    if !["pre", "post"].contains(&phase) {
        return Err(store::usage("_dialog needs pre or post"));
    }
    if !store::env("TASKR_TASK").is_empty() {
        return Err(store::reject(WORKER_DENIAL));
    }
    let pane = store::env("HERDR_PANE_ID");
    if pane.is_empty() {
        return Ok(json!({"ok":true}));
    }
    if !db::path().map_err(store::usage)?.exists() {
        return Ok(json!({"ok":true}));
    }
    let mut db = open()?;
    let root: Option<i64> = db.query_row(
        "select id from tasks where parent_id is null and role='orchestrator' and status!='closed' and pane_id=? and machine is ? order by id desc limit 1",
        params![pane, store::caller_machine()], |r| r.get(0)).optional()?;
    let Some(root) = root else {
        return Ok(json!({"ok":true}));
    };
    worker::resolve(&db, root)?;
    let payload: Value = serde_json::from_str(raw).map_err(|e| store::usage(e.to_string()))?;
    let questions = payload["questions"]
        .as_array()
        .filter(|q| (1..=4).contains(&q.len()))
        .ok_or_else(|| store::usage("dialog needs 1-4 questions"))?;
    let questions = questions
        .iter()
        .map(|q| {
            let mut q = q.clone();
            if let Some(options) = q.get_mut("options").and_then(Value::as_array_mut) {
                for option in options {
                    if let Some(option) = option.as_object_mut() {
                        option.remove("preview");
                    }
                }
            }
            question::parse(&q.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    if phase == "pre" {
        return store::transaction(&mut db, |tx| {
            worker::resolve(tx, root)?;
            for q in &questions {
                if matching(tx, root, q, true)?.is_some() {
                    continue;
                }
                let summary = question::summary("", q);
                let ask = store::event(
                    tx,
                    store::Event {
                        task: root,
                        kind: "ask",
                        summary: &summary,
                        data: Some(
                            json!({"owner":true,"dialog":true,"blocking":false,"question":q}),
                        ),
                        ..store::Event::default()
                    },
                )?;
                orch::relay_owner(tx, ask, None)?;
            }
            Ok(json!({"ok":true}))
        });
    }
    if !payload["afkTimeoutMs"].is_null() {
        eprintln!("taskr owner dialog: AFK timeout; ask left open");
        return Ok(json!({"ok":true}));
    }
    if !payload["answers"].is_object() && payload["response"].as_str().unwrap_or("").is_empty() {
        return Err(store::usage(
            "dialog response has no answers map or free text",
        ));
    }
    for q in &questions {
        let Some(text) = answer_text(q, &payload)? else {
            continue;
        };
        let ask = matching(&db, root, q, false)?.ok_or_else(|| {
            store::reject(format!(
                "no ledger ask for {:?}; answer not recorded: {text}",
                q["question"]
            ))
        })?;
        // No --as: same owner-answer authority as the CLI and taskr-tui.
        orch::answer(&mut db, ask, &text, None, false, true)?;
    }
    Ok(json!({"ok":true}))
}

fn matching(db: &db::Connection, root: i64, q: &Value, open: bool) -> Result<Option<i64>> {
    let mut stmt = db.prepare(
        "with recursive live(id) as (
           select t.id from tasks t where t.parent_id is null and t.status!='closed'
           and (t.id=?1 or (select json_extract(e.data,'$.value') from events e
             where e.task_id=?1 and e.kind='ref' and json_extract(e.data,'$.key')='hub'
             order by e.id desc limit 1)='on')
           union all select t.id from tasks t join live on t.parent_id=live.id)
         select e.id,e.data from events e join tasks t on t.id=e.task_id
         where e.kind='ask' and json_extract(e.data,'$.owner')=1
         and t.status!='closed' and t.id in (select id from live)
         and json_extract(e.data,'$.question.question')=?2
         and (?3=0 or e.answered_by is null)
         order by (e.answered_by is null) desc,e.id desc",
    )?;
    let rows = stmt.query_map(params![root, q["question"].as_str().unwrap(), open], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, raw) = row?;
        let stored: Value = serde_json::from_str(&raw).map_err(|e| store::usage(e.to_string()))?;
        if let Some(options) = stored["question"]["options"].as_array()
            && options.iter().map(|o| &o["label"]).eq(q["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| &o["label"]))
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn answer_text(q: &Value, payload: &Value) -> Result<Option<String>> {
    let key = q["question"].as_str().unwrap();
    let response = payload["response"].as_str().unwrap_or("").trim();
    let annotation = payload["annotations"][key]["notes"]
        .as_str()
        .unwrap_or("")
        .trim();
    let answer = match payload["answers"].get(key) {
        Some(Value::String(s)) if s == "(notes only)" => "",
        Some(Value::String(s)) => s.trim(),
        Some(_) => return Err(store::usage("dialog answer must be a string")),
        None if response.is_empty() && annotation.is_empty() => return Ok(None),
        None => "",
    };
    let labels = q["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    let mut selected = vec![];
    let mut notes = vec![];
    let mut rest = answer;
    while !rest.is_empty() {
        // shortcut: comma-joined labels can be ambiguous; use exact picks when Claude exposes them.
        let label = labels
            .iter()
            .copied()
            .filter(|label| {
                rest.strip_prefix(label)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(','))
            })
            .max_by_key(|label| label.len());
        if let Some(label) = label {
            selected.push(labels.iter().position(|l| *l == label).unwrap());
            rest = rest[label.len()..].trim_start_matches(',').trim_start();
        } else {
            let (note, tail) = rest.split_once(',').unwrap_or((rest, ""));
            if !note.trim().is_empty() {
                notes.push(note.trim());
            }
            rest = tail.trim_start();
        }
    }
    if !response.is_empty() {
        notes.push(response);
    }
    if !annotation.is_empty() {
        notes.push(annotation);
    }
    if selected.is_empty() && notes.is_empty() {
        return Ok(None);
    }
    selected.sort_unstable();
    selected.dedup();
    let mut text = selected
        .iter()
        .map(|&i| format!("{}: {}", char::from(b'A' + i as u8), labels[i]))
        .collect::<Vec<_>>()
        .join("; ");
    if !notes.is_empty() {
        text.push_str(" — ");
        text.push_str(&notes.join(", "));
    }
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_with_commas_and_free_text() {
        let q = json!({"question":"Which?","options":[{"label":"North"},{"label":"North, South"},{"label":"West"}]});
        for (answer, response, want) in [
            ("North, South, West", "", "B: North, South; C: West"),
            (
                "North, South, custom text, West",
                "later",
                "B: North, South; C: West — custom text, later",
            ),
            ("custom, text", "", " — custom, text"),
            ("West", "", "C: West"),
            ("West, North", "", "A: North; C: West"),
        ] {
            assert_eq!(
                answer_text(
                    &q,
                    &json!({"answers":{"Which?":answer},"response":response})
                )
                .unwrap(),
                Some(want.into())
            );
        }
    }
}
