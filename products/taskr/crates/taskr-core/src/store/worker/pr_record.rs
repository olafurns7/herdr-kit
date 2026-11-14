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

pub(super) fn prepare(
    db: &Connection,
    t: &Task,
    kind: &str,
    data: &mut Option<Value>,
) -> Result<()> {
    let brief = brief(db, t)?;
    let (required, seat) = roles(t, &brief);
    if !required {
        return Ok(());
    }
    let d = data.get_or_insert_with(|| json!({}));
    // A bare done carries the latest ready from this launch, then checks it again.
    if kind == "done" && d.get("kv").is_none() {
        let raw: Option<String> = db.query_row(
            "select json_extract(data,'$.kv') from events where task_id=? and launch_id=? and kind='ready' order by id desc limit 1",
            params![t.id, t.launch], |r| r.get(0),
        ).optional()?.flatten();
        if let Some(raw) = raw {
            d["kv"] = serde_json::from_str(&raw)?;
        }
    }
    let kv = &d["kv"];
    let (repo, pr, head) = (text(kv, "repo"), text(kv, "pr"), text(kv, "head"));
    if !repo_ok(repo) {
        return Err(reject("writer/seat report needs --kv repo=OWNER/NAME"));
    }
    if pr != "none"
        && (pr.is_empty()
            || !pr.bytes().all(|c| c.is_ascii_digit())
            || pr.parse::<u64>().ok().filter(|n| *n > 0).is_none())
    {
        return Err(reject(
            "writer/seat report needs --kv pr=NUMBER or --kv pr=none --kv reason=TEXT",
        ));
    }
    if pr == "none" && text(kv, "reason").trim().is_empty() {
        return Err(reject("pr=none needs a non-empty --kv reason=TEXT"));
    }
    if (pr != "none" || seat || !head.is_empty())
        && (head.len() != 40 || !head.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(reject("report needs --kv head=FULL_40_HEX_SHA"));
    }
    let mut verdict_line = None;
    if seat {
        if pr != "none" {
            let targets: Vec<_> = brief
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("PR:")
                        .or_else(|| line.strip_prefix("Review:"))
                        .map(str::trim)
                })
                .collect();
            if targets.len() != 1 || targets[0] != format!("{repo}#{pr}") {
                return Err(reject(
                    "seat PR must equal its brief's single PR: OWNER/NAME#NUMBER or Review: OWNER/NAME#NUMBER line",
                ));
            }
        }
        let verdict = text(kv, "verdict");
        if !matches!(verdict, "PASS" | "FAIL") {
            return Err(reject("seat report needs --kv verdict=PASS|FAIL"));
        }
        let mut counts = Vec::new();
        for key in ["p0", "p1", "p2", "p3"] {
            let value = text(kv, key);
            if value.is_empty()
                || !value.bytes().all(|c| c.is_ascii_digit())
                || value.parse::<u64>().is_err()
            {
                return Err(reject(format!(
                    "seat report needs --kv {key}=NON_NEGATIVE_INTEGER"
                )));
            }
            counts.push(format!("{key}={value}"));
        }
        verdict_line = Some(format!(
            "VERDICT {repo}#{pr} {head} {verdict} {}",
            counts.join(" ")
        ));
    }
    if pr != "none" {
        // PATH can point to a fake gh in isolated tests. There is no production bypass.
        let output = std::process::Command::new("gh")
            .args(["pr", "view", pr, "--repo", repo, "--json", "number,commits"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| reject(format!("cannot verify PR with gh (required): {e}")))?;
        if !output.status.success() {
            return Err(reject(
                "cannot verify PR: gh failed; check GitHub access and the PR number",
            ));
        }
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| reject("cannot verify PR: invalid gh response"))?;
        if value["number"].as_u64() != pr.parse::<u64>().ok()
            || !value["commits"].as_array().is_some_and(|cs| {
                cs.iter().any(|c| {
                    c["oid"]
                        .as_str()
                        .is_some_and(|oid| oid.eq_ignore_ascii_case(head))
                })
            })
        {
            return Err(reject(
                "head is not a commit of the specified PR (gh commit list)",
            ));
        }
    }
    if let Some(line) = verdict_line {
        d["verdict_line"] = json!(line);
    }
    Ok(())
}

pub(super) fn refs(db: &Connection, t: &Task, data: Option<&Value>) -> Result<()> {
    if !roles(t, &brief(db, t)?).0 {
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
