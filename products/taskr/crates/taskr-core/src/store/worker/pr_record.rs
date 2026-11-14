//! PR report fields use existing lane refs; no separate PR table or snapshot.
use super::*;

fn brief(db: &Connection, t: &Task) -> Result<String> {
    Ok(db.query_row(
        "select b.body from documents d join doc_blobs b on b.sha256=d.sha256 where d.task_id=? and (d.kind='brief' or (d.kind='prompt' and d.event_id in (select id from events where launch_id=?))) and d.captured=1 and (d.kind='brief' or b.body like 'HERDR-BRIEF%') order by d.id desc limit 1",
        params![t.id, t.launch], |r| r.get(0),
    ).optional()?.unwrap_or_default())
}

fn roles(t: &Task, brief: &str) -> (bool, bool) {
    let role = brief
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .find_map(|s| s.strip_prefix("role="))
        .unwrap_or(&t.role);
    let seat = matches!(role, "code-reviewer" | "reviewer" | "code-validator")
        || t.name.starts_with("rev-");
    (
        seat || matches!(role, "implementer" | "mechanical-worker")
            || t.name.starts_with("impl-")
            || t.name.starts_with("fix-"),
        seat,
    )
}

fn text<'a>(kv: &'a Value, key: &str) -> &'a str {
    kv[key].as_str().unwrap_or("")
}

fn repo_ok(repo: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    repo.split_once('/')
        .is_some_and(|(a, b)| part(a) && part(b))
}

fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}
fn target(line: &str) -> Option<&str> {
    let value = line
        .strip_prefix("PR: ")
        .or_else(|| line.strip_prefix("Review: "))?;
    let (repo, pr) = value.split_once('#')?;
    (repo_ok(repo) && number(pr).is_some_and(|n| n > 0)).then_some(value)
}
fn assigned(db: &Connection, t: &Task) -> Result<bool> {
    Ok(db.query_row(
        "select exists(select 1 from work_assignments where launch_id=? and lane_id=?)",
        params![t.launch, t.id],
        |r| r.get(0),
    )?)
}

pub(super) fn prepare(
    db: &Connection,
    t: &Task,
    kind: &str,
    data: &mut Option<Value>,
) -> Result<()> {
    let brief = brief(db, t)?;
    let (required, seat) = roles(t, &brief);
    if !required || assigned(db, t)? {
        return Ok(());
    }
    let d = data.get_or_insert_with(|| json!({}));
    if kind == "done" && d.get("kv").is_none() {
        let raw: Option<String> = db.query_row(
            "select data from events where task_id=? and launch_id=? and kind='ready' order by id desc limit 1",
            params![t.id,t.launch], |r| r.get(0),
        ).optional()?;
        if let Some(raw) = raw {
            let saved: Value = serde_json::from_str(&raw)?;
            for key in ["kv", "verdict_line", "pr_verified"] {
                if let Some(value) = saved.get(key) {
                    d[key] = value.clone();
                }
            }
            if saved["pr_verified"] == true {
                return Ok(());
            }
        }
    }
    let kv = &d["kv"];
    let (repo, pr, head) = (text(kv, "repo"), text(kv, "pr"), text(kv, "head"));
    let mut errors = Vec::new();
    if !repo_ok(repo) {
        errors.push("--kv repo=OWNER/NAME".to_string());
    }
    if pr != "none" && !number(pr).is_some_and(|n| n > 0) {
        errors.push("--kv pr=NUMBER (or --kv pr=none --kv reason=TEXT)".into());
    }
    if pr == "none" && text(kv, "reason").trim().is_empty() {
        errors.push("--kv reason=TEXT".into());
    }
    if (pr != "none" || seat || !head.is_empty())
        && (head.len() != 40 || !head.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        errors.push("--kv head=FULL_40_HEX_SHA".into());
    }
    let targets: Vec<_> = brief.lines().filter_map(target).collect();
    if seat {
        if pr != "none" && targets.len() != 1 {
            errors.push(
                "taskr ask 'Need one exact PR: OWNER/NAME#NUMBER line from the lead' --blocking"
                    .into(),
            );
        }
        if !matches!(text(kv, "verdict"), "PASS" | "FAIL") {
            errors.push("--kv verdict=PASS|FAIL".into());
        }
        for key in ["p0", "p1", "p2", "p3"] {
            if number(text(kv, key)).is_none() {
                errors.push(format!("--kv {key}=NON_NEGATIVE_INTEGER"));
            }
        }
        if text(kv, "verdict") == "PASS"
            && ["p0", "p1"]
                .iter()
                .any(|k| number(text(kv, k)).is_some_and(|n| n > 0))
        {
            errors.push("--kv verdict=FAIL (PASS requires --kv p0=0 --kv p1=0)".into());
        }
    }
    if !errors.is_empty() {
        return Err(usage(format!("report needs {}", errors.join("; "))));
    }
    if seat && pr != "none" && targets[0] != format!("{repo}#{pr}") {
        return Err(reject(
            "seat PR must equal its brief's single PR: OWNER/NAME#NUMBER or Review: OWNER/NAME#NUMBER line",
        ));
    }
    let verdict_line = seat.then(|| {
        format!(
            "VERDICT {repo}#{pr} {head} {} {}",
            text(kv, "verdict"),
            ["p0", "p1", "p2", "p3"]
                .iter()
                .map(|key| format!("{key}={}", text(kv, key)))
                .collect::<Vec<_>>()
                .join(" ")
        )
    });
    if pr != "none" {
        // PATH can point to a fake gh in isolated tests. There is no production bypass.
        let mut command = std::process::Command::new("gh");
        command.args(["pr", "view", pr, "--repo", repo, "--json", "number,commits"]);
        let stdout = gh_output(command, std::time::Duration::from_secs(20))?;
        let value: Value = serde_json::from_slice(&stdout)
            .map_err(|_| transport("cannot verify PR: invalid gh response; retry"))?;
        let number = value["number"]
            .as_u64()
            .ok_or_else(|| transport("cannot verify PR: missing gh PR number; retry"))?;
        let commits = value["commits"]
            .as_array()
            .ok_or_else(|| transport("cannot verify PR: missing gh commit list; retry"))?;
        if commits.iter().any(|c| {
            c["oid"]
                .as_str()
                .is_none_or(|s| s.len() != 40 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
        }) {
            return Err(transport("cannot verify PR: invalid gh commit list; retry"));
        }
        if Some(number) != pr.parse::<u64>().ok()
            || !commits.iter().any(|c| {
                c["oid"]
                    .as_str()
                    .is_some_and(|oid| oid.eq_ignore_ascii_case(head))
            })
        {
            return Err(reject(
                "head is not a commit of the specified PR (gh commit list)",
            ));
        }
    }
    if pr != "none" {
        d["pr_verified"] = json!(true);
    }
    if let Some(line) = verdict_line {
        d["verdict_line"] = json!(line);
    }
    Ok(())
}

fn gh_output(mut command: std::process::Command, timeout: std::time::Duration) -> Result<Vec<u8>> {
    use std::{
        io::Read,
        os::unix::process::CommandExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = command
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| {
            transport(format!(
                "cannot verify PR with gh (required): {e}; retry after gh is available"
            ))
        })?;
    let mut pipe = child.stdout.take().expect("pipe");
    let (send, recv) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
        let _ = send.send(result);
    });
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| transport(format!("cannot verify PR: {e}; retry")))?
        {
            let bytes = recv.recv_timeout(timeout.saturating_sub(start.elapsed()));
            if let Ok(Ok(bytes)) = bytes {
                if status.success() {
                    return Ok(bytes);
                }
                return Err(transport(
                    "cannot verify PR: gh failed; check GitHub access and the PR number, then retry",
                ));
            }
            break;
        }
        if start.elapsed() >= timeout {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    terminate_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
    Err(transport(
        "cannot verify PR: gh timed out or output failed; retry",
    ))
}
#[test]
fn r21_gh_deadline() {
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "sleep 2"]);
    let error = gh_output(command, std::time::Duration::from_millis(20)).unwrap_err();
    assert_eq!(error.code, ExitCode::Transport);
    assert!(error.message.contains("retry"));
}

fn transport(message: impl Into<String>) -> Error {
    Error {
        code: ExitCode::Transport,
        message: message.into(),
    }
}

pub(super) fn refs(db: &Connection, t: &Task, data: Option<&Value>) -> Result<()> {
    if !roles(t, &brief(db, t)?).0 || assigned(db, t)? {
        return Ok(());
    }
    let Some(kv) = data.and_then(|d| d.get("kv")) else {
        return Ok(());
    };
    if text(kv, "pr").is_empty() {
        return Ok(());
    }
    let repo = text(kv, "repo");
    let pr = if text(kv, "pr") == "none" {
        String::new()
    } else {
        format!("{repo}#{}", text(kv, "pr"))
    };
    for (key, value) in [("repo", repo), ("pr", pr.as_str())] {
        if key == "repo" && text(kv, "pr") == "none" {
            continue;
        }
        let old: Option<String> = db.query_row("select json_extract(data,'$.value') from events where task_id=? and kind='ref' and json_extract(data,'$.key')=? order by id desc limit 1", params![t.id,key], |r| r.get(0)).optional()?;
        if old.as_deref().unwrap_or("") != value {
            event(
                db,
                Event {
                    task: t.id,
                    launch: t.launch,
                    kind: "ref",
                    summary: &format!("{key}={value}"),
                    data: Some(json!({"key":key,"value":value})),
                    ..Event::default()
                },
            )?;
        }
    }
    Ok(())
}
