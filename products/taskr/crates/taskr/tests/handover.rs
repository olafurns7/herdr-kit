//! A successor's handover stays readable: note first, newest rows only, clipped rows.
#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use std::time::{Duration, Instant};
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

#[test]
fn handover_and_adopt_preserve_decision_and_owner_answer_scope() {
    let h = Harness::new();
    let root = id(&h, &["new", "hub", "--role", "orchestrator"]);
    assert_eq!(root, "1");
    h.conn()
        .execute_batch(
            "insert into tasks(id,parent_id,name,role,created_at,updated_at) values
             (2,1,'child','implementer','2026-01-01','2026-01-01'),
             (3,2,'nested','implementer','2026-01-01','2026-01-01'),
             (4,null,'other','orchestrator','2026-01-01','2026-01-01');
             insert into events(id,task_id,kind,summary,data,answered_by,created_at) values
             (100,1,'decision','normal rule',null,null,'2026-01-01'),
             (101,1,'decision','revoked rule',null,null,'2026-01-01'),
             (102,1,'decision','foreign revoke rule',null,null,'2026-01-01'),
             (103,2,'decision','child rule',null,null,'2026-01-01'),
             (104,4,'decision','outside rule',null,null,'2026-01-01'),
             (110,1,'ask','root question','{\"owner\":true}',111,'2026-01-01'),
             (111,1,'answer','root answer',null,null,'2026-01-01'),
             (112,3,'ask','nested question','{\"owner\":true}',113,'2026-01-01'),
             (113,1,'answer','nested answer',null,null,'2026-01-01'),
             (114,3,'ask','withdrawn question','{\"owner\":true}',115,'2026-01-01'),
             (115,1,'answer','withdrawn answer','{\"withdrawn\":true}',null,'2026-01-01'),
             (116,4,'ask','outside question','{\"owner\":true}',117,'2026-01-01'),
             (117,4,'answer','outside answer',null,null,'2026-01-01'),
             (118,2,'ask','peer question','{\"owner\":false}',119,'2026-01-01'),
             (119,1,'answer','peer answer',null,null,'2026-01-01'),
             (120,2,'ask','unanswered question','{\"owner\":true}',null,'2026-01-01'),
             (121,2,'ask','revoked question','{\"owner\":true}',122,'2026-01-01'),
             (122,1,'answer','revoked answer',null,null,'2026-01-01'),
             (123,2,'ask','foreign revoke question','{\"owner\":true}',124,'2026-01-01'),
             (124,1,'answer','foreign revoke answer',null,null,'2026-01-01');
             insert into events(task_id,kind,related_event_id,created_at) values
             (1,'revoke',101,'2026-01-01'),(1,'revoke',101,'2026-01-01'),
             (4,'revoke',102,'2026-01-01'),(1,'revoke',null,'2026-01-01'),
             (1,'revoke',121,'2026-01-01'),(4,'revoke',123,'2026-01-01');",
        )
        .unwrap();

    for args in [
        vec!["handover", "--as", &root],
        vec!["adopt", &root, "--pane", "w1:p2"],
    ] {
        let md = handover(&h, &args);
        let d = section(&md, "## Decisions in force");
        let kept = [
            "normal rule",
            "foreign revoke rule",
            "root question",
            "nested question",
            "foreign revoke question",
        ];
        let positions: Vec<_> = kept.iter().map(|s| d.find(s).expect(d)).collect();
        assert!(positions.windows(2).all(|p| p[0] < p[1]), "{d}");
        assert_eq!(d.matches("\n- ").count(), kept.len(), "{d}");
        for answer in ["root answer", "nested answer", "foreign revoke answer"] {
            assert!(d.contains(answer), "{d}");
        }
        for excluded in [
            "revoked rule",
            "child rule",
            "outside rule",
            "withdrawn question",
            "outside question",
            "peer question",
            "unanswered question",
            "revoked question",
        ] {
            assert!(!d.contains(excluded), "{d}");
        }
    }
}

#[test]
fn handover_and_adopt_keep_goal_capture_preference_and_latest_plans() {
    let h = Harness::new();
    let root = id(&h, &["new", "hub", "--role", "orchestrator"]);
    id(&h, &["new", "other", "--role", "orchestrator"]);
    h.conn()
        .execute_batch(
            "insert into doc_blobs(sha256,bytes,body) values
             ('old',8,'OLD GOAL'),('selected',13,'SELECTED GOAL'),('wrong',10,'WRONG GOAL');
             insert into documents(id,root_id,task_id,kind,name,version,captured,sha256,created_at) values
             (10,1,1,'goal','alpha',1,1,'old','2026-01-01'),
             (11,1,1,'goal','alpha',2,1,'selected','2026-01-01'),
             (12,1,1,'goal','alpha',3,0,null,'2026-01-01'),
             (13,1,1,'goal','zeta',1,1,'wrong','2026-01-01'),
             (20,1,1,'plan','',1,1,'old','2026-01-01'),
             (21,1,1,'plan','',2,0,null,'2026-01-01'),
             (22,1,1,'plan','alpha',1,1,'old','2026-01-01'),
             (23,1,1,'plan','alpha',2,0,null,'2026-01-01'),
             (24,1,1,'plan','zeta',1,1,'old','2026-01-01'),
             (25,2,2,'goal','',9,1,'wrong','2026-01-01'),
             (26,2,2,'plan','alpha',9,1,'wrong','2026-01-01'),
             (27,1,1,'handover','',9,1,'wrong','2026-01-01');",
        )
        .unwrap();

    for args in [
        vec!["handover", "--as", &root],
        vec!["adopt", &root, "--pane", "w1:p2"],
    ] {
        let md = handover(&h, &args);
        assert!(md.contains("Goal (doc 11, v2):\nSELECTED GOAL"), "{md}");
        assert!(
            !md.contains("OLD GOAL") && !md.contains("WRONG GOAL"),
            "{md}"
        );
        assert!(md.contains("Plan (doc 21, v2, event 0):"), "{md}");
        assert!(md.contains("alpha (doc 23, v2), zeta (doc 24, v1)"), "{md}");
    }
}

#[test]
fn handover_and_adopt_scale_with_root_event_history() {
    let h = Harness::new();
    let root = id(&h, &["new", "hub", "--role", "orchestrator"]);
    h.conn()
        .execute_batch(
            "with recursive history(n) as(values(1) union all select n+1 from history where n<100000)
             insert into events(task_id,kind,summary,created_at)
             select 1,'note','background','2026-01-01' from history;
             with recursive decisions(n) as(values(1) union all select n+1 from decisions where n<1000)
             insert into events(task_id,kind,summary,created_at)
             select 1,'decision','scale rule '||n,'2026-01-01' from decisions;",
        )
        .unwrap();

    for args in [
        vec!["handover", "--as", &root],
        vec!["adopt", &root, "--pane", "w1:p2"],
    ] {
        let start = Instant::now();
        let md = handover(&h, &args);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        let d = section(&md, "## Decisions in force");
        assert!(d.contains("980 older entries are in force"), "{d}");
        assert_eq!(d.matches("\n- ").count(), 20, "{d}");
        assert!(
            d.contains("scale rule 981") && d.contains("scale rule 1000"),
            "{d}"
        );
    }
}

#[test]
fn handover_and_adopt_scale_with_document_versions() {
    let h = Harness::new();
    let root = id(&h, &["new", "hub", "--role", "orchestrator"]);
    h.conn()
        .execute_batch(
            "with recursive versions(v) as(values(1) union all select v+1 from versions where v<5000)
             insert into documents(root_id,task_id,kind,name,version,captured,source_path,reason,created_at)
             select 1,1,kind,'',v,0,'fixture','not captured','2026-01-01'
             from versions cross join (select 'goal' as kind union all select 'plan');",
        )
        .unwrap();
    let plan = h.count(
        "select id from documents where kind='plan' and version=?",
        5000,
    );

    for args in [
        vec!["handover", "--as", &root],
        vec!["adopt", &root, "--pane", "w1:p2"],
    ] {
        let start = Instant::now();
        let md = handover(&h, &args);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        assert!(md.contains("Goal: recorded at fixture"), "{md}");
        assert!(
            md.contains(&format!("Plan (doc {plan}, v5000, event 0):")),
            "{md}"
        );
    }
}
