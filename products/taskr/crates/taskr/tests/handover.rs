//! A successor's handover stays readable: note first, newest rows only, clipped rows.
#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use support::*;

fn id(h: &Harness, args: &[&str]) -> String {
    h.ok(&[], args)["task_id"].as_i64().unwrap().to_string()
}
fn handover(h: &Harness, args: &[&str]) -> String {
    let out = h.output(&[], args, b"");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn section<'a>(md: &'a str, heading: &str) -> &'a str {
    let start = md
        .find(heading)
        .unwrap_or_else(|| panic!("{heading}: {md}"));
    let rest = &md[start + heading.len()..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

#[test]
fn handover_keeps_newest_rows_and_puts_the_note_first() {
    let h = Harness::new();
    let (lane, _, env) = h.lane("claude", "implementer");
    let root = "1";
    for i in 0..30 {
        h.ok(
            &[],
            &["decide", "--as", root, &format!("rule number {i:02}")],
        );
    }
    let long = format!("long rule {}", "x".repeat(2000));
    h.ok(&[], &["decide", "--as", root, &long]);
    let ask = h.ok(&env, &["ask", "Ship it?", "--owner", "--blocking"])["event_id"]
        .as_i64()
        .unwrap()
        .to_string();
    h.ok(&[], &["answer", &ask, "yes, ship", "--as", root]);
    for i in 0..25 {
        let role = if i < 3 { "gate" } else { "implementer" };
        let child = id(
            &h,
            &[
                "new",
                &format!("lane-{i:02}"),
                "--role",
                role,
                "--parent",
                root,
            ],
        );
        h.ok(&[], &["close", &child]);
    }
    h.ok(&[], &["close", &lane.to_string()]);

    let md = handover(&h, &["handover", "--as", root, "--note", "checkpoint text"]);
    let (identity, note, decisions) = (
        md.find("## Identity").unwrap(),
        md.find("## Note").unwrap(),
        md.find("## Decisions in force").unwrap(),
    );
    assert!(identity < note && note < decisions, "{md}");
    assert!(section(&md, "## Note").contains("checkpoint text"));

    // 31 decisions and one owner answer: the newest 20 in full, the other 12 by id.
    let d = section(&md, "## Decisions in force");
    assert!(
        d.contains("12 older entries are in force but not shown here (newest 20 below)."),
        "{d}"
    );
    assert!(d.contains("`taskr search TERMS --root 1`"), "{d}");
    assert_eq!(d.matches("\n- ").count(), 20, "{d}");
    assert!(!d.contains("rule number 11"), "{d}");
    assert!(
        d.contains("rule number 12") && d.contains("rule number 29"),
        "{d}"
    );
    assert!(d.contains(&format!("Owner answer to ask {ask} ")), "{d}");
    let clipped = d.lines().find(|l| l.contains("long rule")).unwrap();
    assert!(clipped.ends_with('…') && clipped.len() < 700, "{clipped}");

    let c = section(&md, "## Closed lanes (no earlier handover or adopt)");
    assert!(
        c.contains("6 earlier closed lanes are not listed (3 of them gates; newest 20 below). `taskr campaign 1 --all` lists every lane."),
        "{c}"
    );
    assert_eq!(c.matches("\n- ").count(), 20, "{c}");
    assert!(!c.contains("lane\\-05 ") && c.contains("lane\\-06 "), "{c}");
    assert!(md.len() < 25_000, "{} bytes", md.len());
}

#[test]
fn first_adopted_handover_lists_lanes_closed_since_the_previous_adopt() {
    let h = Harness::new();
    let root = id(&h, &["new", "hub", "--role", "orchestrator"]);
    let before = id(
        &h,
        &["new", "before", "--role", "implementer", "--parent", &root],
    );
    h.ok(&[], &["close", &before]);
    let out = h.output(&[], &["adopt", &root, "--pane", "w1:p1"], b"");
    assert!(out.status.success());
    let during = id(
        &h,
        &["new", "during", "--role", "implementer", "--parent", &root],
    );
    h.ok(&[], &["close", &during]);
    handover(&h, &["handover", "--as", &root]);

    let md = handover(&h, &["adopt", &root, "--pane", "w1:p2"]);
    let c = section(&md, "## Closed since the adopt (event ");
    assert!(c.contains("during"), "{md}");
    assert!(!c.contains("before"), "{md}");
}

#[test]
fn glance_state_is_outside_the_reference_budget() {
    let h = Harness::new();
    let root = id(&h, &["new", "full", "--role", "orchestrator"]);
    for i in 0..20 {
        h.ok(&[], &["set", &root, &format!("k{i:02}=v")]);
    }
    h.ok(&[], &["set", &root, "glance.state=parked"]);
    let out = h.output(&[], &["set", &root, "k20=v"], b"");
    assert_eq!(out.status.code(), Some(6));
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        err.contains("would have 21 references; at most 20 (delete one with KEY=): k00, k01,"),
        "{err}"
    );
    assert!(!err.contains("glance.state"), "{err}");
}

#[test]
fn late_and_superseding_owner_answers_count_as_newest_decisions() {
    let h = Harness::new();
    let (_, _, env) = h.lane("claude", "implementer");
    let root = "1";
    let ask = |q: &str| {
        h.ok(&env, &["ask", q, "--owner"])["event_id"]
            .as_i64()
            .unwrap()
            .to_string()
    };
    let (late, superseded) = (ask("Late?"), ask("Superseded?"));
    h.ok(
        &[],
        &["answer", &superseded, "old instruction", "--as", root],
    );
    for i in 0..25 {
        h.ok(
            &[],
            &["decide", "--as", root, &format!("rule number {i:02}")],
        );
    }
    h.ok(
        &[],
        &["answer", &late, "NEW OWNER INSTRUCTION", "--as", root],
    );
    h.ok(
        &[],
        &[
            "answer",
            &superseded,
            "newer instruction",
            "--as",
            root,
            "--supersede",
        ],
    );

    let md = handover(&h, &["handover", "--as", root]);
    let d = section(&md, "## Decisions in force");
    assert!(d.contains("NEW OWNER INSTRUCTION"), "{d}");
    assert!(
        d.contains("newer instruction") && !d.contains("old instruction"),
        "{d}"
    );
    // The 7 oldest decisions drop out; neither ask is among them.
    assert!(d.contains("Decisions: "), "{d}");
    assert!(!d.contains("Owner answers to asks:"), "{d}");
    assert!(
        !d.contains("rule number 06") && d.contains("rule number 07"),
        "{d}"
    );
}
