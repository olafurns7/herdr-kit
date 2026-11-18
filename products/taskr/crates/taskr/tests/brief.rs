//! `log --brief` and `inbox --brief`: decision kinds print whole, routine kinds are cut.
#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use support::*;
use taskr_core::store::documents;

const HEAD: &str = "1111111111111111111111111111111111111111";

fn text(h: &Harness, env: &[(String, String)], args: &[&str]) -> String {
    let out = h.output(env, args, b"");
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}
fn id(v: &serde_json::Value) -> i64 {
    v["event_id"].as_i64().unwrap()
}
fn parent(h: &Harness, task: i64) -> i64 {
    h.count("select parent_id from tasks where id=?", task)
}

/// A worker under its root asks a long question, notes at length and fails; the
/// owner's answer reaches the root. Returns (root, worker, ask, note, fail, owner_answer).
fn ledger(h: &Harness) -> (i64, i64, String, i64, String, String) {
    let (worker, _, env) = h.lane("claude", "implementer");
    let root = parent(h, worker);
    let ask = format!("Which base should S2 use? {}end", "context ".repeat(40));
    let ask_id = id(&h.ok(&env, &["ask", &ask]));
    let note_id = id(&h.ok(&env, &["note", &"n".repeat(300)]));
    let fail = format!("cargo test failed: {}tail", "reason ".repeat(40));
    let fail_id = id(&h.ok(&env, &["fail", &fail]));
    let owner = format!("A: Merge after CI — {}done", "because ".repeat(30));
    let owner_id: i64 = h
        .conn()
        .query_row(
            "insert into events(task_id,recipient_task_id,kind,summary,created_at) values(?,?,'owner_answer',?,'2026-11-18T00:00:00Z') returning id",
            taskr_core::db::params![root, root, owner],
            |r| r.get(0),
        )
        .unwrap();
    (
        root,
        worker,
        format!("{ask_id} ask {worker}:impl {ask}"),
        note_id,
        format!("{fail_id} fail {worker}:impl {fail}"),
        format!("{owner_id} owner_answer {root}:top {owner}"),
    )
}

#[test]
fn brief_log_prints_decisions_whole_and_cuts_notes() {
    let h = Harness::new();
    let (root, worker, ask, note, fail, owner) = ledger(&h);
    let out = text(&h, &[], &["log", &root.to_string(), "--tree", "--brief"]);
    let lines: Vec<_> = out.lines().collect();
    // A long ask, a fail and an owner answer print whole on one line each.
    for whole in [&ask, &fail, &owner] {
        assert!(whole.chars().count() > 200, "{whole}");
        assert!(lines.contains(&whole.as_str()), "{whole}\n{out}");
    }
    // A long note keeps 160 chars and ends with its id and the dropped count.
    let cut = format!(
        "{note} note {worker}:impl {} [{note} +140 chars]",
        "n".repeat(160)
    );
    assert!(lines.contains(&cut.as_str()), "{out}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with('L') && l.contains(" launch ")),
        "{out}"
    );
    // --limit keeps its meaning: the newest events and the usual m1 trailer.
    let out = text(
        &h,
        &[],
        &[
            "log",
            &root.to_string(),
            "--tree",
            "--brief",
            "--limit",
            "1",
        ],
    );
    assert!(out.contains(&owner) && !out.contains(&fail), "{out}");
    assert!(
        out.lines().last().unwrap().starts_with("m1 {\"older\":"),
        "{out}"
    );
}

#[test]
fn brief_inbox_prints_decisions_whole() {
    let h = Harness::new();
    let (root, _, ask, _, fail, owner) = ledger(&h);
    let out = text(&h, &[], &["inbox", "--as", &root.to_string(), "--brief"]);
    let lines: Vec<_> = out.lines().collect();
    assert!(
        lines[0].starts_with("inbox ack=") && lines[0].contains(" pend=- events="),
        "{out}"
    );
    for whole in [&ask, &fail, &owner] {
        assert!(lines.contains(&whole.as_str()), "{whole}\n{out}");
    }
    let out = text(
        &h,
        &[],
        &[
            "inbox",
            "--as",
            &root.to_string(),
            "--brief",
            "--limit",
            "1",
        ],
    );
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(
        out.ends_with("more: raise --limit (max 200) or ack\n"),
        "{out}"
    );
}

#[test]
fn brief_seat_ready_keeps_its_verdict_line_whole() {
    let h = Harness::new();
    h.script("gh", &format!("#!/bin/sh\n[ \"$*\" = 'pr view 42 --repo example/project --json number,commits' ] || exit 1\nprintf '%s\\n' '{{\"number\":42,\"commits\":[{{\"oid\":\"{HEAD}\"}}]}}'\n"));
    let (seat, _, env) = h.lane("codex", "reviewer");
    documents::capture_brief(
        &h.conn(),
        seat,
        Some(&documents::body(
            b"HERDR-BRIEF role=code-reviewer\nPR: example/project#42\n",
            "seat.md",
        )),
    )
    .unwrap();
    let summary = format!("review {}end", "finding ".repeat(30));
    let mut args = vec!["ready", summary.as_str()];
    let head = format!("head={HEAD}");
    for kv in [
        "repo=example/project",
        "pr=42",
        &head,
        "verdict=PASS",
        "p0=0",
        "p1=0",
        "p2=2",
        "p3=0",
    ] {
        args.extend(["--kv", kv]);
    }
    let ready = id(&h.ok(&env, &args));
    let verdict = format!("  VERDICT example/project#42 {HEAD} PASS p0=0 p1=0 p2=2 p3=0");
    let root = parent(&h, seat).to_string();
    for out in [
        text(&h, &[], &["log", &seat.to_string(), "--brief"]),
        text(&h, &[], &["inbox", "--as", &root, "--brief"]),
    ] {
        let lines: Vec<_> = out.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with(&format!("{ready} ready {seat}:impl review ")))
            .unwrap_or_else(|| panic!("{out}"));
        assert!(
            lines[at].ends_with(&format!(" [{ready} +{} chars]", summary.len() - 160)),
            "{out}"
        );
        assert_eq!(lines[at + 1], verdict, "{out}");
    }
}

#[test]
fn without_brief_output_is_unchanged() {
    let h = Harness::new();
    let (root, _, _, _, _, _) = ledger(&h);
    let root = root.to_string();
    for args in [vec!["log", &root, "--tree"], vec!["inbox", "--as", &root]] {
        let plain = text(&h, &[], &args);
        assert!(
            plain
                .lines()
                .all(|l| l.starts_with("j1 ") || l.starts_with("m1 ")),
            "{plain}"
        );
        assert!(plain.contains(&"reason ".repeat(40)), "{plain}");
        let off = text(&h, &[], &[&args[..], &["--brief=false"]].concat());
        assert_eq!(off, plain, "{args:?}");
        let json = h.output(&[], &[&["--json"][..], &args, &["--brief"]].concat(), b"");
        assert_eq!(json.status.code(), Some(2), "{args:?}: {json:?}");
        let env = [("TASKR_FORMAT".to_string(), "json".to_string())];
        let json = h.output(&env, &[&args[..], &["--brief"]].concat(), b"");
        assert_eq!(json.status.code(), Some(2), "{args:?}: {json:?}");
    }
}
