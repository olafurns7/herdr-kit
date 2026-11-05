use super::*;

#[test]
fn done_on_merge_only_finishes_never_run_backlog_once() {
    let mut db = Connection::open_in_memory().unwrap();
    crate::schema::migrate(&mut db).unwrap();
    db.execute_batch("insert into tasks(id,name,role,created_at,updated_at) values(1,'root','orchestrator','t','t');").unwrap();
    for (id, state) in [
        (1, "backlog"),
        (2, "done"),
        (3, "cancelled"),
        (4, "backlog"),
    ] {
        db.execute("insert into work_items(id,root_id,title,priority,revision,state,creator,created_at) values(?,1,'ticket',0,1,?,'{}','t')", params![id,state]).unwrap();
        db.execute(
            "insert into work_revisions(work_id,revision,creator,created_at) values(?,1,'{}','t')",
            [id],
        )
        .unwrap();
    }
    db.execute_batch("insert into work_runs(id,work_id,revision,root_id,generation,claim_key,claim_input,dependencies,action,created_at) values(1,4,1,1,0,'claim','{}','[]','execute','t');").unwrap();
    let note = "merged demo-org/demo#31 abcdef123";
    assert!(done_on_merge(&db, 1, note).unwrap());
    for id in 1..=4 {
        assert!(!done_on_merge(&db, id, "must not replace note").unwrap());
    }
    assert_eq!(
        db.query_row("select reason from work_items where id=1", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        note
    );
    assert_eq!(
        db.query_row("select count(*) from events where kind='work'", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("select count(*) from work_acceptances", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(item(&db, 3).unwrap().state, "cancelled");
    assert_eq!(item(&db, 4).unwrap().state, "backlog");
}

fn src() -> Source {
    Source {
        commit: "a".repeat(40),
        tree: "b".repeat(40),
        input_digest: "c".repeat(64),
    }
}
#[test]
fn proof_requires_exact_successful_case_set_source_profile_argv_and_exit() {
    let profile = Profile {
        id: "unit".into(),
        version: 1,
        digest: "d".repeat(64),
        checks: vec![Check {
            id: "cases".into(),
            kind: "cases".into(),
            argv: vec!["checker".into()],
            timeout_ms: 1000,
            cases: vec!["one".into(), "two".into()],
            admission: Admission::Direct,
            receipt_file: Some("cases.json".into()),
        }],
    };
    let good = json!({"run":1,"revision":1,"source":src(),"profile_digest":profile.digest,"check":"cases","checker_launch":2,"argv":["checker"],"exit":0,"signal":null,"error":null,"timed_out":false,"admission":{"method":"direct","ticket":null,"reason":null},"stdout":{"sha256":"e".repeat(64),"bytes":0,"truncated":false},"stderr":{"sha256":"e".repeat(64),"bytes":0,"truncated":false},"cases":[{"id":"one","status":"passed"},{"id":"two","status":"passed"}],"artifacts":[{"name":"receipt","sha256":"e".repeat(64)}]});
    validate_proof(
        &profile,
        &src(),
        1,
        1,
        2,
        &serde_json::from_value(good.clone()).unwrap(),
    )
    .unwrap();
    for patch in [
        json!({"cases":[]}),
        json!({"cases":[{"id":"one","status":"passed"}]}),
        json!({"cases":[{"id":"one","status":"passed"},{"id":"one","status":"passed"}]}),
        json!({"cases":[{"id":"one","status":"passed"},{"id":"two","status":"skipped"}]}),
        json!({"cases":[{"id":"one","status":"failed"},{"id":"two","status":"passed"}]}),
        json!({"cases":[{"id":"one","status":"passed"},{"id":"wrong","status":"passed"}]}),
        json!({"exit":1}),
        json!({"run":2}),
        json!({"revision":2}),
        json!({"checker_launch":3}),
        json!({"profile_digest":"e".repeat(64)}),
        json!({"argv":["other"]}),
        json!({"source":Source{tree:"e".repeat(40),..src()}}),
        json!({"check":"unknown"}),
    ] {
        let mut bad = good.clone();
        for (k, v) in patch.as_object().unwrap() {
            bad[k] = v.clone();
        }
        assert!(
            validate_proof(
                &profile,
                &src(),
                1,
                1,
                2,
                &serde_json::from_value(bad).unwrap()
            )
            .is_err(),
            "{patch}"
        );
    }
}
#[test]
fn bounded_typed_json_rejects_unknown_fields_and_malformed_data() {
    for raw in [
        "{",
        r#"{"campaign":1,"title":"x","oops":true}"#,
        r#"{"campaign":"1","title":"x"}"#,
    ] {
        assert!(parse::<Add>(raw).is_err());
    }
    assert!(parse::<Add>(&" ".repeat(MAX_JSON + 1)).is_err());
    assert!(parse::<Submission>(r#"{"kind":"proof","result":{"exit":0}}"#).is_err());
    assert!(
        parse::<Add>(r#"{"campaign":1,"title":"x"}"#)
            .unwrap()
            .contract
            .is_none()
    );
}
#[test]
fn v4_migration_preserves_legacy_rows_cursors_launches_and_documents() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(crate::schema::SCHEMA).unwrap();
    db.execute_batch(crate::schema::SUBSCRIPTIONS).unwrap();
    db.execute_batch(crate::schema::DELIVERY_ATTEMPTS).unwrap();
    db.execute_batch(crate::schema::CONSUMER_LEASES).unwrap();
    db.execute_batch("pragma user_version=4; insert into tasks(id,name,role,acked_event_id,pending_event_id,created_at,updated_at) values(1,'kept','orchestrator',3,4,'t','t'); insert into launches(id,task_id,session_ref,recorded_at) values(1,1,'kept-session','t'); insert into documents(task_id,root_id,kind,version,captured,created_at) values(1,1,'goal',1,0,'t');").unwrap();
    crate::schema::migrate(&mut db).unwrap();
    crate::schema::migrate(&mut db).unwrap();
    let shape:(i64,i64,i64,String)=db.query_row("select (select user_version from pragma_user_version),(select acked_event_id from tasks),(select pending_event_id from tasks),(select session_ref from launches)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(shape, (5, 3, 4, "kept-session".into()));
    assert_eq!(
        db.query_row("select count(*) from documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("select count(*) from work_items", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn canonical_hashes_ignore_digest_label_and_input_commit_but_pin_profile_and_dependencies() {
    let mut profile: Profile = parse(r#"{"id":"p","version":1,"digest":"","checks":[]}"#).unwrap();
    let hash = profile_digest(&profile).unwrap();
    profile.digest = "label-does-not-enter-the-hash".into();
    assert_eq!(hash, profile_digest(&profile).unwrap());
    assert_eq!(
        canonical_json(&json!({"z":1,"a":{"b":2,"a":"þ"}})).unwrap(),
        "{\"a\":{\"a\":\"þ\",\"b\":2},\"z\":1}"
    );
    let d = DependencyInput {
        work: 1,
        revision: 1,
        input_digest: "a".repeat(64),
    };
    let empty = input_digest(&profile, &[]).unwrap();
    let pinned = input_digest(&profile, std::slice::from_ref(&d)).unwrap();
    assert_ne!(empty, pinned);
    profile.version = 2;
    assert_ne!(pinned, input_digest(&profile, &[d]).unwrap());
    assert_ne!(hash, profile_digest(&profile).unwrap());
}
