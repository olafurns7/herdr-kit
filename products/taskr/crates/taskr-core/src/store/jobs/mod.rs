//! Durable presentation handles. Business acceptance remains owned by `work`.
use super::*;
use serde::{Deserialize, Serialize};
mod owner;
mod routes;
pub use owner::*;
pub use routes::{accepted, reconcile};
#[cfg(test)]
mod tests;

pub const DDL: &str = include_str!("schema.sql");
pub const LEASE_SECONDS: i64 = 90;
pub const POLL_MS: i64 = 5000;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub root: i64,
    pub generation: i64,
    pub binding: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Await {
    pub work: i64,
    pub revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Add {
    pub key: String,
    pub campaign: i64,
    pub title: String,
    #[serde(default)]
    pub priority: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub handle: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub handle: String,
    pub result: i64,
}

fn auth_root(db: &Connection, s: &Session) -> Result<()> {
    if s.root <= 0
        || s.generation < 0
        || !(16..=128).contains(&s.binding.len())
        || !s
            .binding
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(usage(
            "jobs requires root, generation and a 16..128 character binding identity",
        ));
    }
    let root = worker::resolve(db, s.root)?;
    if root.role != "orchestrator" {
        return Err(reject(
            "jobs requires an orchestrator root, not a worker or sub-orchestrator",
        ));
    }
    Ok(())
}
fn auth(db: &Connection, s: &Session) -> Result<()> {
    auth_root(db, s)?;
    if delivery::generation(db, s.root)? != s.generation {
        return Err(reject("stale jobs generation"));
    }
    Ok(())
}
fn auth_read(db: &Connection, s: &Session) -> Result<()> {
    auth_root(db, s)?;
    if delivery::generation(db, s.root)? != s.generation {
        let known:bool=db.query_row("select exists(select 1 from jobs_operations where root_id=? and generation=? and binding=? and host=?)",params![s.root,s.generation,s.binding,host()],|r|r.get(0))?;
        if !known {
            return Err(reject(
                "stale jobs session has no previously authorized operations",
            ));
        }
    }
    Ok(())
}
pub fn validate(db: &mut Connection, s: &Session) -> Result<Value> {
    transaction(db, |tx| {
        auth_read(tx, s)?;
        routes::reconcile_tx(tx)?;
        Ok(
            json!({"ok":true,"root":s.root,"generation":s.generation,"current":delivery::generation(tx,s.root)?==s.generation}),
        )
    })
}
fn host() -> String {
    machine_name(caller_machine().as_deref())
}
fn deadline() -> String {
    stamp(parse_time(&now()).expect("server clock") + time::Duration::seconds(LEASE_SECONDS))
}
struct Operation {
    id: String,
    root: i64,
    generation: i64,
    binding: String,
    host: String,
    kind: String,
    reference: i64,
    revision: i64,
    status: String,
    snapshot: Option<Value>,
    reply: Option<String>,
    created: String,
    updated: String,
}
fn operation(db: &Connection, id: &str) -> Result<Operation> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(usage("invalid jobs handle"));
    }
    let row = db.query_row("select id,root_id,generation,binding,host,kind,reference_id,revision,status,snapshot,reply,created_at,updated_at from jobs_operations where id=?",[id], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?,r.get::<_,String>(8)?,r.get::<_,Option<String>>(9)?,r.get::<_,Option<String>>(10)?,r.get::<_,String>(11)?,r.get::<_,String>(12)?))
    }).optional()?.ok_or_else(||reject("jobs handle does not exist; use jobs recover"))?;
    Ok(Operation {
        id: row.0,
        root: row.1,
        generation: row.2,
        binding: row.3,
        host: row.4,
        kind: row.5,
        reference: row.6,
        revision: row.7,
        status: row.8,
        snapshot: row.9.map(|s| serde_json::from_str(&s)).transpose()?,
        reply: row.10,
        created: row.11,
        updated: row.12,
    })
}
fn own(s: &Session, o: &Operation) -> Result<()> {
    if o.root != s.root {
        return Err(reject("jobs handle belongs to another campaign"));
    }
    Ok(())
}
fn bound(s: &Session, o: &Operation) -> bool {
    o.generation == s.generation && o.binding == s.binding && o.host == host()
}
fn require_bound(s: &Session, o: &Operation) -> Result<()> {
    own(s, o)?;
    if !bound(s, o) {
        return Err(reject("jobs operation belongs to another session binding"));
    }
    Ok(())
}
fn insert(
    db: &Connection,
    s: &Session,
    kind: &str,
    reference: i64,
    revision: i64,
    status: &str,
    snapshot: Option<Value>,
) -> Result<String> {
    let id: String = db.query_row("select lower(hex(randomblob(16)))", [], |r| r.get(0))?;
    db.execute("insert into jobs_operations(id,root_id,generation,binding,host,kind,reference_id,revision,status,snapshot,created_at,updated_at) values(?,?,?,?,?,?,?,?,?,?,?,?)",params![id,s.root,s.generation,s.binding,host(),kind,reference,revision,status,snapshot.map(|v|compact_json(&v)).transpose()?,now(),now()])?;
    Ok(id)
}
fn finish(db: &Connection, id: &str, status: &str, snapshot: Option<Value>) -> Result<()> {
    db.execute("update jobs_operations set status=?,snapshot=?,updated_at=? where id=? and status in ('working','input_required')",params![status,snapshot.map(|v|compact_json(&v)).transpose()?,now(),id])?;
    Ok(())
}
fn tool(value: Value) -> Value {
    json!({"content":[{"type":"text","text":compact_json(&value).expect("JSON")}],"structuredContent":value,"isError":false})
}
fn view(db: &Connection, o: &Operation) -> Result<Value> {
    let mut v = json!({"resultType":"complete","taskId":o.id,"status":o.status,"createdAt":o.created,"lastUpdatedAt":o.updated,"ttlMs":null,"pollIntervalMs":POLL_MS});
    match o.status.as_str() {
        "completed" => {
            v["result"] = o
                .snapshot
                .clone()
                .ok_or_else(|| reject("completed jobs result missing"))?
        }
        "failed" => {
            v["error"] = o
                .snapshot
                .clone()
                .ok_or_else(|| reject("failed jobs error missing"))?
        }
        "input_required" => v["inputRequests"] = owner::requests(o)?,
        _ => {}
    }
    if o.kind == "await" {
        let state: String = db.query_row(
            "select state from jobs_routes where operation_id=?",
            [&o.id],
            |r| r.get(0),
        )?;
        v["routeState"] = json!(state);
    }
    Ok(v)
}
pub fn get_work(db: &Connection, s: &Session, work: i64) -> Result<Value> {
    auth(db, s)?;
    let value = work::show(db, work)?;
    if value["campaign"] != s.root {
        return Err(reject("work belongs to another campaign"));
    }
    Ok(value)
}
pub fn add(db: &mut Connection, s: &Session, input: Add) -> Result<Value> {
    // work::add rechecks root/generation/host and designated-hub authority in its transaction.
    auth(db, s)?;
    if input.key.is_empty() {
        return Err(usage("work_add requires an idempotency key"));
    }
    work::add(
        db,
        work::Authority {
            root: s.root,
            generation: s.generation,
        },
        work::Add {
            key: Some(input.key),
            campaign: input.campaign,
            title: input.title,
            priority: input.priority,
            contract: None,
        },
    )
}
pub fn await_work(db: &mut Connection, s: &Session, input: Await) -> Result<Value> {
    transaction(db, |tx| {
        auth(tx, s)?;
        routes::reconcile_tx(tx)?;
        get_work(tx, s, input.work)?;
        let exists: bool = tx.query_row(
            "select exists(select 1 from work_revisions where work_id=? and revision=?)",
            params![input.work, input.revision],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(reject("work revision does not exist"));
        }
        let prior:Option<String>=tx.query_row("select id from jobs_operations where root_id=? and kind='await' and reference_id=? and revision=?",params![s.root,input.work,input.revision],|r|r.get(0)).optional()?;
        let id = if let Some(id) = prior {
            let o = operation(tx, &id)?;
            // Returning a handle cannot reclaim or change another binding's route.
            require_bound(s, &o)?;
            id
        } else {
            let id = insert(tx, s, "await", input.work, input.revision, "working", None)?;
            tx.execute(
                "insert into jobs_routes(operation_id,state,deadline) values(?,'active',?)",
                params![id, deadline()],
            )?;
            id
        };
        routes::refresh(tx, &id)?;
        view(tx, &operation(tx, &id)?)
    })
}
pub fn recover(db: &mut Connection, s: &Session) -> Result<Value> {
    transaction(db, |tx| {
        auth_read(tx, s)?;
        routes::reconcile_tx(tx)?;
        let mut stmt =
            tx.prepare("select id from jobs_operations where root_id=? and generation=? and binding=? and host=? order by created_at,id")?;
        let ids = stmt
            .query_map(params![s.root, s.generation, s.binding, host()], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let operations = ids
            .iter()
            .map(|id| {
                let o = operation(tx, id)?;
                view(tx, &o)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"operations":operations}))
    })
}
pub fn poll(db: &mut Connection, s: &Session, id: &str) -> Result<Value> {
    transaction(db, |tx| {
        auth_root(tx, s)?;
        require_bound(s, &operation(tx, id)?)?;
        routes::reconcile_tx(tx)?;
        let o = operation(tx, id)?;
        if delivery::generation(tx, s.root)? == s.generation && o.kind == "await" {
            tx.execute(
                "update jobs_routes set deadline=? where operation_id=? and state='active'",
                params![deadline(), id],
            )?;
        }
        if o.kind == "owner" {
            if o.status == "input_required" {
                require_bound(s, &o)?;
            }
            owner::refresh(tx, &o)?;
        }
        view(tx, &operation(tx, id)?)
    })
}
pub fn release(db: &mut Connection, s: &Session, id: &str, cancel: bool) -> Result<Value> {
    transaction(db, |tx| {
        auth(tx, s)?;
        routes::reconcile_tx(tx)?;
        let o = operation(tx, id)?;
        require_bound(s, &o)?;
        if o.kind == "await" {
            routes::release_tx(tx, id)?;
        }
        finish(
            tx,
            id,
            if cancel { "cancelled" } else { "completed" },
            if cancel {
                None
            } else {
                Some(tool(
                    json!({"outcome":if o.kind=="await" {"released_to_channel"} else {"no_decision"}}),
                ))
            },
        )?;
        view(tx, &operation(tx, id)?)
    })
}
pub fn receipt(db: &mut Connection, s: &Session, input: Receipt) -> Result<Value> {
    transaction(db, |tx| {
        auth(tx, s)?;
        routes::reconcile_tx(tx)?;
        let o = operation(tx, &input.handle)?;
        require_bound(s, &o)?;
        let result: Option<i64> = tx
            .query_row(
                "select result_id from jobs_routes where operation_id=?",
                [&input.handle],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if result != Some(input.result) {
            return Err(reject(
                "receipt must name this operation's stable work result",
            ));
        }
        tx.execute(
            "update jobs_routes set state='receipted' where operation_id=?",
            [&input.handle],
        )?;
        Ok(json!({"ok":true,"handle":input.handle,"result":input.result}))
    })
}
