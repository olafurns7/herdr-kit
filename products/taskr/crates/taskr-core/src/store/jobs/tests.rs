use super::*;
pub(super) fn fixture() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(crate::schema::SCHEMA).unwrap();
    db.execute_batch(crate::schema::SUBSCRIPTIONS).unwrap();
    db.execute_batch(work::DDL).unwrap();
    db.execute_batch(DDL).unwrap();
    db.execute_batch("insert into tasks(id,name,role,created_at,updated_at) values(1,'synthetic','orchestrator','t','t'); insert into work_items(id,root_id,title,priority,revision,state,creator,created_at) values(1,1,'work',0,1,'active','{}','t'); insert into work_revisions(work_id,revision,creator,created_at) values(1,1,'{}','t'); insert into work_results(id,launch_id,digest,payload,created_at) values(1,1,'digest','{\"kind\":\"execution\"}','t');").unwrap();
    db
}
fn pending(db: &Connection) -> String {
    let s = Session {
        root: 1,
        generation: 0,
        binding: "synthetic-session".into(),
    };
    let id = insert(db, &s, "await", 1, 1, "working", None).unwrap();
    db.execute(
        "insert into jobs_routes(operation_id,state,deadline) values(?,'active',?)",
        params![id, deadline()],
    )
    .unwrap();
    id
}
fn notices(db: &Connection) -> i64 {
    db.query_row(
        "select count(*) from events where kind='message'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}
#[test]
fn accepted_native_result_needs_semantic_receipt_and_expiry_falls_back_once() {
    let mut db = fixture();
    let id = pending(&db);
    transaction(&mut db, |tx| {
        assert!(accepted(tx, 1, 1, 1, 1, 1)?);
        Ok(())
    })
    .unwrap();
    assert_eq!(notices(&db), 0);
    let before = view(&db, &operation(&db, &id).unwrap()).unwrap();
    assert_eq!(before["status"], "completed");
    assert_eq!(before["result"]["structuredContent"]["result_id"], 1);
    assert!(
        before["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("result_receipt")
    );
    for _ in 0..3 {
        routes::reconcile_tx(&db).unwrap();
        assert_eq!(notices(&db), 0);
    }
    db.execute(
        "update jobs_routes set deadline='2000-01-01T00:00:00.000Z'",
        [],
    )
    .unwrap();
    for _ in 0..3 {
        routes::reconcile_tx(&db).unwrap();
    }
    assert_eq!(notices(&db), 1);
    let after = view(&db, &operation(&db, &id).unwrap()).unwrap();
    assert_eq!(after["result"], before["result"]);
    assert_eq!(after["lastUpdatedAt"], before["lastUpdatedAt"]);
    assert_eq!(after["routeState"], "released");
    assert_eq!(
        db.query_row("select acked_event_id from tasks where id=1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn release_before_result_retains_fallback_and_does_not_rewrite_cancelled_task() {
    let db = fixture();
    let id = pending(&db);
    routes::release_tx(&db, &id).unwrap();
    finish(&db, &id, "cancelled", None).unwrap();
    assert_eq!(notices(&db), 0);
    assert!(accepted(&db, 1, 1, 1, 1, 1).unwrap());
    assert!(accepted(&db, 1, 1, 1, 1, 1).unwrap());
    assert_eq!(notices(&db), 1);
    assert_eq!(
        view(&db, &operation(&db, &id).unwrap()).unwrap()["status"],
        "cancelled"
    );
}
#[test]
fn receipt_prevents_fallback_and_channel_first_is_never_suppressed_or_advanced() {
    let db = fixture();
    let _id = pending(&db);
    accepted(&db, 1, 1, 1, 1, 1).unwrap();
    db.execute(
        "update jobs_routes set state='receipted',deadline='2000-01-01T00:00:00.000Z'",
        [],
    )
    .unwrap();
    routes::reconcile_tx(&db).unwrap();
    assert_eq!(notices(&db), 0);
    // A different fixture models channel presentation before native tracking starts.
    let db = fixture();
    event(
        &db,
        Event {
            task: 1,
            to: Some(1),
            kind: "message",
            data: Some(json!({"notice":{"reason":"accepted","work":1,"result":1}})),
            ..Event::default()
        },
    )
    .unwrap();
    let event_id = db.last_insert_rowid();
    db.execute("update tasks set pending_event_id=? where id=1", [event_id])
        .unwrap();
    let id = pending(&db);
    accepted(&db, 1, 1, 1, 1, 1).unwrap();
    routes::reconcile_tx(&db).unwrap();
    assert_eq!(notices(&db), 1);
    assert_eq!(
        view(&db, &operation(&db, &id).unwrap()).unwrap()["routeState"],
        "released"
    );
    assert_eq!(
        db.query_row("select pending_event_id from tasks where id=1", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        event_id
    );
    assert_eq!(id.len(), 32);
}
#[test]
fn generation_change_and_superseded_revision_release_without_reclaim() {
    let db = fixture();
    let id = pending(&db);
    event(
        &db,
        Event {
            task: 1,
            kind: "adopt",
            ..Event::default()
        },
    )
    .unwrap();
    routes::reconcile_tx(&db).unwrap();
    assert_eq!(
        view(&db, &operation(&db, &id).unwrap()).unwrap()["result"]["structuredContent"]["outcome"],
        "released_to_channel"
    );
    accepted(&db, 1, 1, 1, 1, 1).unwrap();
    routes::reconcile_tx(&db).unwrap();
    assert_eq!(notices(&db), 1);
    let db = fixture();
    let id = pending(&db);
    db.execute("update work_items set revision=2", []).unwrap();
    routes::refresh(&db, &id).unwrap();
    let v = view(&db, &operation(&db, &id).unwrap()).unwrap();
    assert_eq!(v["status"], "failed");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("inspect work_get")
    );
}
#[test]
fn immutable_result_identity_and_unique_operation_are_enforced() {
    let db = fixture();
    let id = pending(&db);
    accepted(&db, 1, 1, 1, 1, 1).unwrap();
    assert!(accepted(&db, 1, 1, 1, 1, 2).is_err());
    let s = Session {
        root: 1,
        generation: 0,
        binding: "another-session-id".into(),
    };
    assert!(insert(&db, &s, "await", 1, 1, "working", None).is_err());
    assert!(!bound(&s, &operation(&db, &id).unwrap()));
}
#[test]
fn typed_inputs_reject_unknown_invalid_and_oversized_values() {
    for raw in [
        "{",
        r#"{"work":1,"revision":1,"root":2}"#,
        r#"{"work":"1","revision":1}"#,
    ] {
        assert!(work::parse::<Await>(raw).is_err());
    }
    assert!(work::parse::<Await>(&" ".repeat(work::MAX_JSON + 1)).is_err());
    assert!(work::parse::<Update>(r#"{"handle":"x","responses":{"owner_choice":{"action":"accept","content":{"choice":"x","version":"v","extra":true}}}}"#).is_err());
}
#[test]
fn schema_migration_includes_jobs_and_preserves_existing_inbox_state() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(crate::schema::SCHEMA).unwrap();
    db.execute_batch("insert into tasks(id,name,role,created_at,updated_at,acked_event_id,pending_event_id) values(1,'kept','orchestrator','t','t',7,8)").unwrap();
    db.pragma_update(None, "user_version", 4).unwrap();
    crate::schema::migrate(&mut db).unwrap();
    crate::schema::migrate(&mut db).unwrap();
    assert!(db.table_exists(None, "jobs_operations").unwrap());
    assert_eq!(
        db.query_row(
            "select acked_event_id,pending_event_id from tasks",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        )
        .unwrap(),
        (7, 8)
    );
}
