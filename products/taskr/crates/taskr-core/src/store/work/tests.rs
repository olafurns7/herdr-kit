use super::*;

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
        }],
    };
    let good = json!({"run":1,"revision":1,"source":src(),"profile_digest":profile.digest,"check":"cases","checker_launch":2,"argv":["checker"],"exit":0,"cases":[{"id":"one","status":"passed"},{"id":"two","status":"passed"}],"artifacts":[]});
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
