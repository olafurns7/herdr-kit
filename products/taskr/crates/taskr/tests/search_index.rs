#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;
use support::Harness;

fn text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn search_names_a_damaged_index_and_repair_rebuilds_it() {
    let h = Harness::new();
    let root = h.ok(&[], &["new", "search-root", "--role", "orchestrator"])["task_id"]
        .as_i64()
        .unwrap()
        .to_string();
    h.ok(&[], &["note", "orphantoken first note", "--as", &root]);
    h.ok(&[], &["note", "orphantoken second note", "--as", &root]);
    let check = h.output(&[], &["--json", "search", "--check"], b"");
    assert!(check.status.success(), "{}", text(&check));
    assert!(
        text(&check).contains(r#""search_index":"ok""#),
        "{}",
        text(&check)
    );
    // An orphan index entry: the first note's content and docsize rows are gone.
    h.conn()
        .execute_batch(
            "delete from search_fts_content where c0='orphantoken first note';
             delete from search_fts_docsize where id not in (select id from search_fts_content);",
        )
        .unwrap();
    let search = h.output(&[], &["--json", "search", "orphantoken"], b"");
    assert_eq!(search.status.code(), Some(4), "{}", text(&search));
    assert!(
        text(&search).contains("search index is damaged")
            && text(&search).contains("taskr search --repair"),
        "{}",
        text(&search)
    );
    // Other reads still work.
    let status = h.output(&[], &["--json", "status"], b"");
    assert!(status.status.success(), "{}", text(&status));
    let check = h.output(&[], &["--json", "search", "--check"], b"");
    assert_eq!(check.status.code(), Some(4), "{}", text(&check));
    assert!(
        text(&check).contains("taskr search --repair"),
        "{}",
        text(&check)
    );
    let usage = h.output(&[], &["--json", "search", "--repair", "orphantoken"], b"");
    assert_eq!(usage.status.code(), Some(2), "{}", text(&usage));
    let repair = h.ok(&[], &["search", "--repair"]);
    assert_eq!(repair["search_index"], "rebuilt");
    assert_eq!(repair["rows"], 2);
    let check = h.output(&[], &["--json", "search", "--check"], b"");
    assert!(check.status.success(), "{}", text(&check));
    let hits = h.output(&[], &["--json", "search", "orphantoken"], b"");
    assert!(hits.status.success(), "{}", text(&hits));
    assert_eq!(String::from_utf8_lossy(&hits.stdout).lines().count(), 2);
}
