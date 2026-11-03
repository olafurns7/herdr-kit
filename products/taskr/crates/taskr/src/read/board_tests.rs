//! Column rules, groups, asks and alarms over a synthetic ledger.
use super::*;

fn now() -> OffsetDateTime {
    taskr_core::store::parse_time("2026-11-03T19:00:00.000Z").unwrap()
}
fn ago(minutes: i64) -> String {
    taskr_core::store::stamp(now() - time::Duration::minutes(minutes))
}

fn ledger() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(taskr_core::schema::SCHEMA).unwrap();
    db.execute_batch(taskr_core::store::work::DDL).unwrap();
    let task = |id: i64, parent: Option<i64>, status: &str, launch: Option<i64>, at: &str| {
        db.execute("insert into tasks(id,parent_id,name,role,status,current_launch_id,created_at,updated_at) values(?,?,?,'implementer',?,?,?,?)", params![id, parent, format!("t{id}"), status, launch, at, at]).unwrap();
    };
    task(1, None, "open", None, &ago(600));
    task(9, None, "closed", None, &ago(600));
    task(10, Some(1), "open", Some(100), &ago(30));
    task(11, Some(1), "closed", None, &ago(120));
    task(12, Some(1), "closed", None, &ago(240));
    task(13, Some(1), "open", None, &ago(5));
    db.execute("update tasks set name='camp' where id=1", [])
        .unwrap();
    db.execute("insert into launches(id,task_id,provider,model,effort,observed_status,present,recorded_at) values(100,10,'claude','claude-opus-5-5','high','working',1,'t')", []).unwrap();
    let item = |id: i64, title: &str, state: &str, reason: Option<&str>, contract: Value| {
        db.execute("insert into work_items(id,root_id,title,priority,revision,state,reason,creator,created_at) values(?,1,?,0,1,?,?,'{}',?)", params![id, title, state, reason, ago(900)]).unwrap();
        db.execute("insert into work_revisions(work_id,revision,contract,creator,created_at) values(?,1,?,'{}',?)", params![id, contract.to_string(), ago(900)]).unwrap();
    };
    item(
        1,
        "A1 alpha",
        "backlog",
        None,
        json!({"intent":"Do it. parked: 311906 far off."}),
    );
    item(2, "A2 beta", "active", None, json!({"intent":"Build."}));
    item(3, "U1 gamma", "backlog", None, json!({}));
    item(4, "U2 delta", "backlog", None, json!({}));
    item(5, "S1 eps", "done", Some("shipped in #30"), json!({}));
    item(
        6,
        "S2 zeta",
        "backlog",
        None,
        json!({"intent":"blocked: vendor. parked: 311906 wait.","dependencies":[{"work":5,"revision":1},{"work":1,"revision":1}]}),
    );
    item(7, "S3 gone", "cancelled", None, json!({}));
    item(
        8,
        "S4 eta",
        "backlog",
        None,
        json!({"intent":"parked: 311906 wait"}),
    );
    item(14, "plain", "backlog", None, json!({}));
    let event = |task: i64, kind: &str, summary: &str, data: Value, at: &str| {
        db.execute(
            "insert into events(task_id,kind,summary,data,created_at) values(?,?,?,?,?)",
            params![task, kind, summary, data.to_string(), at],
        )
        .unwrap();
    };
    let r = |task: i64, key: &str, value: &str| {
        event(
            task,
            "ref",
            &format!("{key}={value}"),
            json!({"key":key,"value":value}),
            &ago(60),
        )
    };
    r(10, "work", "2");
    r(11, "work", "3");
    r(11, "pr", "31");
    r(12, "work", "4");
    r(12, "pr", "32");
    r(13, "work", "14");
    r(13, "pr", "33");
    r(13, "pr", "");
    db.execute("insert into meta(key,value) values('pr_state:demo/x#31',?)", [json!({"head":"abc","state":"OPEN","merge":"BLOCKED","draft":true,"named":{"Kami review":"success"},"queue":2}).to_string()]).unwrap();
    event(
        13,
        "ask",
        "approve W8 now",
        json!({"owner":1,"blocking":1}),
        &ago(90),
    );
    event(13, "ask", "about W80", json!({"owner":1}), &ago(30));
    event(13, "ask", "lane question W8", json!({}), &ago(30));
    event(
        1,
        "note",
        "ALARM 18:20Z STUCK #31 in queue 40 min",
        json!({}),
        &ago(40),
    );
    event(1, "note", "ALARM LAG merge-watch late", json!({}), &ago(10));
    event(1, "note", "ALARM 14:00Z FAILED old", json!({}), &ago(300));
    event(1, "note", "HEALTH ok", json!({}), &ago(5));
    event(
        9,
        "note",
        "ALARM 18:50Z HUB-RED closed root",
        json!({}),
        &ago(10),
    );
    db
}

fn ticket(v: &Value, w: i64) -> &Value {
    v["tickets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["w"] == w)
        .unwrap_or_else(|| panic!("W{w} missing"))
}

#[test]
fn columns_prs_groups_asks_and_alarms() {
    let db = ledger();
    let v = snapshot(&db, now(), "demo/x").unwrap();
    let col = |w| ticket(&v, w)["column"].as_str().unwrap().to_string();
    // Not done, parked; a working lane; an open draft PR on a closed lane; a PR with
    // no poller state yet is unknown, not in progress; done with its note.
    assert_eq!(col(1), "not_done");
    assert_eq!(ticket(&v, 1)["parked"], "311906 far off");
    assert_eq!(col(2), "in_progress");
    assert_eq!(ticket(&v, 2)["lanes"], "10 working (opus-5-5 high)");
    assert_eq!(col(3), "in_progress");
    let t3 = ticket(&v, 3);
    assert_eq!(
        (
            t3["pr"].clone(),
            t3["pr_repo"].clone(),
            t3["pr_state"].clone()
        ),
        (json!(31), json!("demo/x"), json!("draft"))
    );
    assert_eq!(
        (t3["queue"].clone(), t3["checks"].clone()),
        (json!(2), json!({"Kami review":"success"}))
    );
    assert_eq!(t3["lanes"], "", "a closed lane is not listed as working");
    assert_eq!(col(4), "not_done");
    assert_eq!(
        (
            ticket(&v, 4)["pr"].clone(),
            ticket(&v, 4)["pr_state"].clone()
        ),
        (json!(32), json!(""))
    );
    assert_eq!(col(5), "done");
    assert_eq!(ticket(&v, 5)["note"], "shipped in #30");
    // A cleared pr ref shows no PR; cancelled tickets are not shown.
    assert_eq!(ticket(&v, 14)["pr"], "");
    assert!(v["tickets"].as_array().unwrap().iter().all(|t| t["w"] != 7));
    // Blocked by segment, dependencies, and a blocking owner ask naming the ticket.
    let t6 = ticket(&v, 6);
    assert_eq!(
        (
            t6["blocked"].as_str(),
            t6["waits"].as_str(),
            t6["waits_open"].as_str()
        ),
        (Some("vendor"), Some("W5 W1"), Some("W1"))
    );
    let asks = v["asks"].as_array().unwrap();
    assert_eq!(asks.len(), 2, "the non-owner ask is not an owner ask");
    let blocking = asks[0]["ask"].to_string();
    assert_eq!(
        (
            asks[0]["blocking"].clone(),
            asks[0]["campaign"].clone(),
            asks[0]["age_h"].clone()
        ),
        (json!(true), json!("camp"), json!(1.5))
    );
    assert_eq!(
        ticket(&v, 8)["blocked"],
        format!("blocking owner ask {blocking}")
    );
    assert_eq!(ticket(&v, 8)["asks"], blocking, "W80 is not W8");
    // Idle hours from the newest linked lane, else the ticket's creation.
    assert_eq!(ticket(&v, 2)["idle_h"], 0.5);
    assert_eq!(ticket(&v, 1)["idle_h"], 15.0);
    // Groups: campaign and tag letter; the wait line is the parked text all open
    // tickets share.
    let groups: Vec<(String, String)> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["key"].as_str().unwrap().into(),
                g["parked"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            ("1A".into(), "".into()),
            ("1S".into(), "311906 wait".into()),
            ("1U".into(), "".into()),
            ("1".into(), "".into())
        ]
    );
    // Alarms: open roots' ALARM notes of the last 3 h, newest first; the note's time
    // stands in when the text has none.
    let alarms: Vec<(String, String, String)> = v["alarms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["time"].as_str().unwrap().into(),
                a["event"].as_str().unwrap().into(),
                a["detail"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        alarms,
        [
            ("18:50Z".into(), "LAG".into(), "merge-watch late".into()),
            (
                "18:20Z".into(),
                "STUCK".into(),
                "#31 in queue 40 min".into()
            )
        ]
    );
}

#[test]
fn without_work_tables_the_board_is_empty() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(taskr_core::schema::SCHEMA).unwrap();
    let v = snapshot(&db, now(), "").unwrap();
    assert_eq!(v["tickets"], json!([]));
}

#[test]
fn intent_segments_tags_and_mentions() {
    let seg = segments("Spec x. parked: a b. spec: c\nwaits: W1 and W2..\nblocked:  vendor");
    assert_eq!(seg["parked"], "a b");
    assert_eq!(seg["waits"], "W1 and W2");
    assert_eq!(seg["blocked"], "vendor");
    assert!(segments("parked:x").contains_key("parked"));
    assert!(
        segments("not parked: here").is_empty(),
        "a segment starts the text or follows a period"
    );
    assert_eq!(tag("A12 title"), ("A12", "title"));
    assert_eq!(tag("S1\tswitch"), ("S1", "switch"));
    for plain in ["Alpha", "A1", "X1 other", "A title"] {
        assert_eq!(tag(plain), ("", plain));
    }
    assert!(names("approve W8, then", 8));
    assert!(!names("W80 and XW8 and W8x", 8));
    assert_eq!(pr_label(&json!({"state":"OPEN","merge":"DIRTY"})), "dirty");
    assert_eq!(pr_label(&json!({"state":"OPEN"})), "open");
    assert_eq!(pr_label(&json!({"state":"MERGED","draft":true})), "merged");
}
