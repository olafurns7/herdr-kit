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
            && text(&search).contains("in a maintenance window, run `taskr search --repair`"),
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
    // A goflag true value is the flag; over RPC both are refused (they take the write lock).
    let check = h.output(&[], &["--json", "search", "--check=1"], b"");
    assert_eq!(check.status.code(), Some(4), "{}", text(&check));
    for flag in ["--check", "--repair"] {
        let rpc = h.output(
            &[("TASKR_RPC_CALLER".into(), "host-b".into())],
            &["--hub-child", "--json", "search", flag],
            b"{}",
        );
        assert_eq!(rpc.status.code(), Some(2), "{flag}: {}", text(&rpc));
        assert!(
            text(&rpc).contains("run it on the ledger host, in a maintenance window"),
            "{flag}: {}",
            text(&rpc)
        );
    }
    for flag in ["--check", "--repair"] {
        let lane = h.output(
            &[("TASKR_TASK".into(), "1".into())],
            &["--json", "search", flag],
            b"",
        );
        assert_eq!(lane.status.code(), Some(2), "{flag}: {}", text(&lane));
        assert!(
            text(&lane).contains("operator step"),
            "{flag}: {}",
            text(&lane)
        );
    }
    let check = h.output(&[], &["--json", "search", "--check"], b"");
    assert_eq!(check.status.code(), Some(4), "{}", text(&check));
    let repair = h.ok(&[], &["search", "--repair"]);
    assert_eq!(repair["search_index"], "rebuilt");
    assert_eq!(repair["rows"], 2);
    let check = h.output(&[], &["--json", "search", "--check"], b"");
    assert!(check.status.success(), "{}", text(&check));
    let hits = h.output(&[], &["--json", "search", "orphantoken"], b"");
    assert!(hits.status.success(), "{}", text(&hits));
    assert_eq!(String::from_utf8_lossy(&hits.stdout).lines().count(), 2);
    // The last occurrence of a flag wins: ending false is a plain search, which needs a QUERY.
    for args in [["--check", "--check=false"], ["--repair", "--repair=false"]] {
        let bare = h.output(&[], &[&["--json", "search"][..], &args].concat(), b"");
        assert_eq!(bare.status.code(), Some(2), "{args:?}: {}", text(&bare));
        let query = h.output(
            &[],
            &[&["--json", "search"][..], &args, &["orphantoken"]].concat(),
            b"",
        );
        assert!(query.status.success(), "{args:?}: {}", text(&query));
        assert_eq!(String::from_utf8_lossy(&query.stdout).lines().count(), 2);
    }
}
