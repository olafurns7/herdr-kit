#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use serde_json::{Value, json};
use support::*;
use taskr_core::store::documents;

const HEAD: &str = "1111111111111111111111111111111111111111";
const FIELDS: &[&str] = &[
    "--kv",
    "repo=example/project",
    "--kv",
    "pr=42",
    "--kv",
    "head=1111111111111111111111111111111111111111",
];
const VERDICT: &[&str] = &[
    "--kv",
    "verdict=PASS",
    "--kv",
    "p0=0",
    "--kv",
    "p1=0",
    "--kv",
    "p2=2",
    "--kv",
    "p3=0",
];

fn gh(h: &Harness) {
    h.script("gh", &format!("#!/bin/sh\n[ \"$*\" = 'pr view 42 --repo example/project --json number,commits' ] || exit 1\nprintf '%s\\n' '{{\"number\":42,\"commits\":[{{\"oid\":\"{HEAD}\"}}]}}'\n"));
}
fn refused(h: &Harness, env: &Env, args: &[&str], expected: &str, code: i32) {
    let before: i64 = h
        .conn()
        .query_row("select count(*) from events", [], |r| r.get(0))
        .unwrap();
    let out = h.output(env, &[&["--json"][..], args].concat(), b"");
    assert_eq!(out.status.code(), Some(code), "{out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["error"].as_str().unwrap().contains(expected), "{v}");
    if code == 2 {
        let error = v["error"].as_str().unwrap();
        assert!(!error.contains('\n') && error.contains("--kv"), "{v}");
    }
    if code == 5 {
        assert!(v["error"].as_str().unwrap().contains("retry"), "{v}");
    }
    assert_eq!(
        before,
        h.conn()
            .query_row("select count(*) from events", [], |r| r.get::<_, i64>(0))
            .unwrap()
    );
}
fn seat_brief(h: &Harness, task: i64, pr: &str) {
    documents::capture_brief(
        &h.conn(),
        task,
        Some(&documents::body(
            format!("HERDR-BRIEF role=code-reviewer\nPR: {pr}\n").as_bytes(),
            "seat.md",
        )),
    )
    .unwrap();
}

#[test]
fn r21_writer_requires_pr_reason_and_membership() {
    let h = Harness::new();
    let (_, _, env) = h.lane("codex", "implementer");
    for kind in ["ready", "done"] {
        refused(
            &h,
            &env,
            &[kind, "slice", "--kv", "repo=example/project"],
            "pr=NUMBER",
            2,
        );
        refused(
            &h,
            &env,
            &[
                kind,
                "slice",
                "--kv",
                "repo=example/project",
                "--kv",
                "pr=none",
            ],
            "reason",
            2,
        );
    }
    gh(&h);
    refused(
        &h,
        &env,
        &[&["ready", "slice"][..], FIELDS, &["--kv", "head=bad"]].concat(),
        "--kv head=FULL_40_HEX_SHA",
        2,
    );
    let fields = [
        FIELDS,
        &["--kv", "head=2222222222222222222222222222222222222222"],
    ]
    .concat();
    refused(
        &h,
        &env,
        &[&["ready", "slice"][..], &fields].concat(),
        "not a commit",
        6,
    );
    h.script("gh", "#!/bin/sh\nexit 1\n");
    refused(
        &h,
        &env,
        &[&["ready", "slice"][..], FIELDS].concat(),
        "gh failed",
        5,
    );
    h.script("gh", "#!/bin/sh\necho '{}'\n");
    refused(
        &h,
        &env,
        &[&["ready", "slice"][..], FIELDS].concat(),
        "missing gh PR number",
        5,
    );
    std::fs::remove_file(h.home.join("bin/gh")).unwrap();
    let mut no_gh = env.clone();
    no_gh.push(("PATH".into(), h.home.join("bin").to_str().unwrap().into()));
    refused(
        &h,
        &no_gh,
        &[&["ready", "slice"][..], FIELDS].concat(),
        "cannot verify PR with gh",
        5,
    );
}

#[test]
fn r21_writer_refs_done_inheritance_and_none() {
    let h = Harness::new();
    let (task, _, env) = h.lane("codex", "implementer");
    gh(&h);
    h.ok(&env, &[&["ready", "slice"][..], FIELDS].concat());
    let done = h.ok(&env, &["done"])["event_id"].as_i64().unwrap();
    let data: String = h
        .conn()
        .query_row("select data from events where id=?", [done], |r| r.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&data).unwrap()["kv"]["head"],
        HEAD
    );
    let campaign = h.ok(&[], &["campaign", "1"]);
    assert!(
        campaign["prs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["task_id"] == task && p["value"] == "example/project#42")
    );
    let fields = &[
        "--kv",
        "repo=example/project",
        "--kv",
        "pr=none",
        "--kv",
        "reason=unpushed",
    ];
    h.ok(&env, &[&["ready", "slice"][..], fields].concat());
    h.ok(&env, &["done"]);
    assert!(
        h.ok(&[], &["campaign", "1"])["prs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let next = h.ok(
        &[],
        &[
            "launch",
            &task.to_string(),
            "--provider",
            "codex",
            "--model",
            "fixture",
            "--effort",
            "high",
        ],
    )["launch_id"]
        .to_string();
    let mut fresh = env;
    fresh
        .iter_mut()
        .find(|(k, _)| k == "TASKR_LAUNCH")
        .unwrap()
        .1 = next;
    refused(&h, &fresh, &["done"], "repo=", 2);
}

#[test]
fn r21_seat_verdict_prints_stores_and_binds_to_brief() {
    let h = Harness::new();
    let (task, _, env) = h.lane("claude", "reviewer");
    gh(&h);
    seat_brief(&h, task, "example/project#41");
    refused(
        &h,
        &env,
        &[&["ready", "seat"][..], FIELDS, VERDICT].concat(),
        "seat PR",
        6,
    );
    seat_brief(&h, task, "bad target");
    refused(
        &h,
        &env,
        &[&["ready", "seat"][..], FIELDS, VERDICT].concat(),
        "seat brief needs",
        2,
    );
    seat_brief(&h, task, "example/project#42");
    // A captured task brief can use the task's role without a HERDR-BRIEF header.
    documents::capture_brief(
        &h.conn(),
        task,
        Some(&documents::body(b"PR: example/project#42\n", "seat.md")),
    )
    .unwrap();
    refused(
        &h,
        &env,
        &[&["ready", "seat"][..], FIELDS, VERDICT, &["--kv", "p0=-1"]].concat(),
        "--kv p0=NON_NEGATIVE_INTEGER",
        2,
    );
    refused(
        &h,
        &env,
        &[&["ready", "seat"][..], FIELDS].concat(),
        "verdict=",
        2,
    );
    let expected = format!("VERDICT example/project#42 {HEAD} PASS p0=0 p1=0 p2=2 p3=0");
    let args = [
        &["ready", "seat", "--key", "seat-record"][..],
        FIELDS,
        VERDICT,
    ]
    .concat();
    let out = h.output(&env, &args, b"");
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with(&format!("{expected}\nr1 "))
    );
    // A replay prints the saved verdict without another GitHub read.
    h.script("gh", "#!/bin/sh\nexit 1\n");
    assert_eq!(h.ok(&env, &args)["verdict_line"], expected);
    let log = h.rows(&[], &["log", &task.to_string()]);
    let row = log.iter().find(|v| v["kind"] == "ready").unwrap();
    assert_eq!(row["data"]["verdict_line"], expected);
    assert_eq!(row["data"]["kv"]["p2"], "2");
    let campaign = h.ok(&[], &["campaign", "1"]);
    assert!(
        campaign["log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["data"]["verdict_line"] == expected)
    );
    gh(&h);
    let done = h.output(&env, &["done"], b"");
    assert!(done.status.success(), "{done:?}");
    assert!(
        String::from_utf8(done.stdout)
            .unwrap()
            .starts_with(&format!("{expected}\nd1 "))
    );
}

#[test]
fn r21_role_header_prefix_and_researcher() {
    for (role, name, header, required) in [
        ("researcher", "lane", "", false),
        ("researcher", "impl-slice", "", true),
        ("researcher", "fix-slice", "", true),
        ("researcher", "rev-slice", "", true),
        (
            "researcher",
            "lane",
            "HERDR-BRIEF role=mechanical-worker\n",
            true,
        ),
    ] {
        let h = Harness::new();
        let (task, _, env) = h.lane("codex", role);
        h.conn()
            .execute(
                "update tasks set name=? where id=?",
                taskr_core::db::params![name, task],
            )
            .unwrap();
        if !header.is_empty() {
            documents::capture_brief(
                &h.conn(),
                task,
                Some(&documents::body(header.as_bytes(), "brief.md")),
            )
            .unwrap();
        }
        if required {
            refused(&h, &env, &["ready", "slice"], "repo=", 2);
        } else {
            h.ok(&env, &["ready", "slice"]);
            h.ok(&env, &["done"]);
            assert_eq!(h.ok(&[], &["campaign", "1"])["prs"], json!([]));
        }
    }
}
