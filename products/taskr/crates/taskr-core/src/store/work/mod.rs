//! Durable, launch-bound work. No operation here starts a process or grants publication authority.
use super::*;
use sha2::{Digest, Sha256};
mod lifecycle;
mod results;
mod types;
pub use lifecycle::*;
pub use results::*;
pub use types::*;
#[cfg(test)]
mod tests;

pub const DDL: &str = include_str!("schema.sql");
fn digest(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
fn authority(db: &Connection, a: Authority, campaign: i64, hub_allowed: bool) -> Result<Task> {
    if a.root <= 0 {
        return Err(usage("work mutation requires --as ROOT --generation GEN"));
    }
    let t = worker::resolve(db, a.root)?;
    if delivery::generation(db, t.id)? != a.generation {
        return Err(reject("stale work controller generation"));
    }
    let hub: bool = db.query_row("select coalesce((select json_extract(data,'$.value') from events where task_id=? and kind='ref' and json_extract(data,'$.key')='hub' order by id desc limit 1)='on',0)", [t.id], |r| r.get(0))?;
    if t.id != campaign && !(hub_allowed && hub) {
        return Err(reject("work belongs to another campaign"));
    }
    let target = open_task(db, campaign)?;
    if target.parent.is_some() || target.launch.is_some() || target.status == "planned" {
        return Err(reject("campaign must be an open root"));
    }
    Ok(t)
}
fn parked(db: &Connection, root: i64) -> Result<bool> {
    Ok(db.query_row("select coalesce((select json_extract(data,'$.value') from events where task_id=? and kind='ref' and json_extract(data,'$.key')='glance.state' order by id desc limit 1)='parked',0)", [root], |r|r.get(0))?)
}
fn audit(
    db: &Connection,
    root: i64,
    reason: &str,
    work: Option<i64>,
    run: Option<i64>,
    result: Option<i64>,
    notify: bool,
) -> Result<()> {
    let revision = work.map(|w| item(db, w).map(|w| w.revision)).transpose()?;
    let notice = json!({"reason":reason,"work":work,"revision":revision,"run":run,"result":result});
    event(
        db,
        Event {
            task: root,
            kind: "work",
            summary: reason,
            data: Some(notice.clone()),
            ..Event::default()
        },
    )?;
    if notify && !parked(db, root)? {
        let generation = consumer::push_generation(db, &task(db, root)?)?;
        let text = format!(
            "work {}: {reason}",
            work.map(|w| w.to_string()).unwrap_or_default()
        );
        event(
            db,
            Event {
                task: root,
                to: Some(root),
                kind: "message",
                summary: &text,
                data: Some(json!({"text":text,"gen":generation,"notice":notice})),
                ..Event::default()
            },
        )?;
    }
    Ok(())
}
struct Item {
    id: i64,
    root: i64,
    revision: i64,
    state: String,
    run: Option<i64>,
}
fn item(db: &Connection, id: i64) -> Result<Item> {
    db.query_row(
        "select id,root_id,revision,state,current_run from work_items where id=?",
        [id],
        |r| {
            Ok(Item {
                id: r.get(0)?,
                root: r.get(1)?,
                revision: r.get(2)?,
                state: r.get(3)?,
                run: r.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| reject("work does not exist"))
}
fn contract(db: &Connection, w: &Item) -> Result<Contract> {
    contract_revision(db, w.id, w.revision)
}
fn contract_revision(db: &Connection, work: i64, revision: i64) -> Result<Contract> {
    let raw: Option<String> = db.query_row(
        "select contract from work_revisions where work_id=? and revision=?",
        params![work, revision],
        |r| r.get(0),
    )?;
    parse(&raw.ok_or_else(|| reject("work contract is incomplete"))?)
}
fn expected(w: &Item, revision: i64) -> Result<()> {
    if w.revision != revision {
        return Err(reject("stale semantic revision"));
    }
    Ok(())
}
fn validate_contract(
    db: &Connection,
    campaign: i64,
    work: Option<i64>,
    c: &Contract,
) -> Result<()> {
    encoded(c)?;
    text(&c.intent, 16384, "intent")?;
    text(&c.acceptance, 16384, "acceptance")?;
    text(&c.authority, 8192, "authority")?;
    paths(&c.scope)?;
    paths(&c.write_scope)?;
    source(&c.base)?;
    token(&c.checkout.host, "checkout host")?;
    text(&c.checkout.cwd, 4096, "checkout cwd")?;
    if !std::path::Path::new(&c.checkout.cwd).is_absolute()
        || orch::absolute("/", &c.checkout.cwd) != c.checkout.cwd
    {
        return Err(usage("checkout cwd must be canonical absolute client path"));
    }
    if c.max_rework > 8 || c.dependencies.len() > 64 {
        return Err(usage("rework/dependency limit exceeded"));
    }
    if c.require_adjudicator && c.adjudicator.is_none() {
        return Err(usage("adjudicator required"));
    }
    if c.executor.model == c.reviewer.model
        || c.executor.task.is_some() && c.executor.task == c.reviewer.task
    {
        return Err(usage("review requires an independent lane and model"));
    }
    if let Some(a) = &c.adjudicator {
        if a.task.is_some() && (a.task == c.executor.task || a.task == c.reviewer.task)
            || a.model == c.executor.model
            || a.model == c.reviewer.model
        {
            return Err(usage("adjudication requires a distinct lane and model"));
        }
    }
    for (stage, lane) in [
        (Stage::Executor, Some(&c.executor)),
        (Stage::Reviewer, Some(&c.reviewer)),
        (Stage::Adjudicator, c.adjudicator.as_ref()),
    ] {
        let Some(l) = lane else { continue };
        token(&l.provider, "provider")?;
        token(&l.model, "model")?;
        token(&l.effort, "effort")?;
        orch::valid_name(&l.name, "approved lane name")?;
        let Some(task_id) = l.task else { continue };
        let t = open_task(db, task_id)?;
        if root(db, t.id)? != campaign || t.parent.is_none() {
            return Err(reject("lane belongs to another campaign"));
        }
        let role_ok = t.role == stage.ledger_role();
        if !role_ok {
            return Err(reject("lane role does not match assignment stage"));
        }
    }
    token(&c.profile.id, "profile ID")?;
    hash(&c.profile.digest, &[64])?;
    if c.profile.version == 0 || c.profile.checks.len() > 64 {
        return Err(usage("invalid proof profile version/check count"));
    }
    unique(c.profile.checks.iter().map(|c| c.id.as_str()), "check ID")?;
    for check in &c.profile.checks {
        token(&check.kind, "proof kind")?;
        if check.argv.is_empty()
            || check.argv.len() > 128
            || check.timeout_ms == 0
            || check.timeout_ms > 3_600_000
            || check.cases.len() > 1024
            || check.kind == "cases" && check.cases.is_empty()
        {
            return Err(usage("invalid check argv/timeout/case count"));
        }
        for a in &check.argv {
            text(a, 8192, "argv")?;
        }
        unique(check.cases.iter().map(String::as_str), "case ID")?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in &c.dependencies {
        if d.work <= 0 || d.revision <= 0 || Some(d.work) == work || !seen.insert(d.work) {
            return Err(usage("invalid/self/duplicate dependency"));
        }
        let dep = item(db, d.work)?;
        if dep.root != campaign {
            return Err(reject("dependency belongs to another campaign"));
        }
        let exists: bool = db.query_row(
            "select exists(select 1 from work_revisions where work_id=? and revision=?)",
            params![d.work, d.revision],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(reject("dependency revision does not exist"));
        }
        if let Some(work) = work {
            let cycle:bool=db.query_row("with recursive edges(w) as (select ? union select cast(json_extract(j.value,'$.work') as integer) from edges e join work_items i on i.id=e.w join work_revisions v on v.work_id=i.id and v.revision=i.revision, json_each(v.contract,'$.dependencies') j) select exists(select 1 from edges where w=?)",params![d.work,work],|r|r.get(0))?;
            if cycle {
                return Err(reject("dependency cycle"));
            }
        }
    }
    Ok(())
}
fn dependencies(db: &Connection, c: &Contract) -> Result<String> {
    let mut accepted = Vec::new();
    for d in &c.dependencies {
        let w = item(db, d.work)?;
        if w.revision != d.revision || w.state != "done" {
            return Err(reject("dependency is not accepted at pinned revision"));
        }
        let (acceptance,reason):(Option<i64>,Option<String>)=db.query_row("select (select id from work_acceptances where work_id=w.id and revision=w.revision),reason from work_items w where id=?",[d.work],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let a = acceptance.ok_or_else(|| reject("dependency has no acceptance receipt"))?;
        if reason.is_some() {
            return Err(reject("dependency requires recheck"));
        }
        accepted.push(json!({"work":d.work,"revision":d.revision,"acceptance":a}));
    }
    encoded(&accepted)
}
fn invalidate_dependents(db: &Connection, work: i64) -> Result<()> {
    db.execute("with recursive affected(id) as (select i.id from work_items i join work_revisions v on v.work_id=i.id and v.revision=i.revision,json_each(v.contract,'$.dependencies') j where json_extract(j.value,'$.work')=? union select i.id from affected a join work_items i join work_revisions v on v.work_id=i.id and v.revision=i.revision,json_each(v.contract,'$.dependencies') j where json_extract(j.value,'$.work')=a.id) update work_items set reason='dependency changed; recheck required' where id in affected",[work])?;
    Ok(())
}
pub fn configure(
    db: &mut Connection,
    a: Authority,
    campaign: i64,
    input: Configure,
) -> Result<Value> {
    encoded(&input)?;
    if !(1..=64).contains(&input.max_active) || !(1..=64).contains(&input.max_review_pending) {
        return Err(usage("campaign limits must be 1..64"));
    }
    transaction(db, |tx| {
        authority(tx, a, campaign, true)?;
        tx.execute("insert into work_campaigns(root_id,enabled,max_active,max_review_pending) values(?,?,?,?) on conflict(root_id) do update set enabled=excluded.enabled,max_active=excluded.max_active,max_review_pending=excluded.max_review_pending",params![campaign,input.enabled,input.max_active,input.max_review_pending])?;
        audit(tx, campaign, "configured", None, None, None, false)?;
        Ok(json!({"ok":true,"campaign":campaign,"configuration":input}))
    })
}
pub fn add(db: &mut Connection, a: Authority, input: Add) -> Result<Value> {
    encoded(&input)?;
    text(&input.title, 512, "title")?;
    transaction(db, |tx| {
        authority(tx, a, input.campaign, true)?;
        if let Some(c) = &input.contract {
            validate_contract(tx, input.campaign, None, c)?;
        }
        let creator = json!({"root":a.root,"generation":a.generation,"host":machine_name(caller_machine().as_deref())});
        tx.execute("insert into work_items(root_id,title,priority,revision,state,creator,created_at) values(?,?,?,1,'backlog',?,?)",params![input.campaign,input.title,input.priority,encoded(&creator)?,now()])?;
        let id = tx.last_insert_rowid();
        tx.execute("insert into work_revisions(work_id,revision,contract,creator,created_at) values(?,1,?,?,?)",params![id,input.contract.as_ref().map(encoded).transpose()?,encoded(&creator)?,now()])?;
        audit(tx, input.campaign, "added", Some(id), None, None, true)?;
        show(tx, id)
    })
}
pub fn revise(db: &mut Connection, a: Authority, id: i64, input: Revise) -> Result<Value> {
    encoded(&input)?;
    if let Some(t) = &input.title {
        text(t, 512, "title")?;
    }
    transaction(db, |tx| {
        let w = item(tx, id)?;
        authority(tx, a, w.root, true)?;
        expected(&w, input.expected_revision)?;
        if w.state == "cancelled" {
            return Err(reject("cancelled work cannot be revised"));
        }
        if let Some(c) = &input.contract {
            validate_contract(tx, w.root, Some(id), c)?;
            let raw = encoded(c)?;
            let old: Option<String> = tx.query_row(
                "select contract from work_revisions where work_id=? and revision=?",
                params![id, w.revision],
                |r| r.get(0),
            )?;
            if old.as_deref() != Some(&raw) {
                let creator = encoded(
                    &json!({"root":a.root,"generation":a.generation,"host":machine_name(caller_machine().as_deref())}),
                )?;
                tx.execute("insert into work_revisions(work_id,revision,contract,creator,created_at) values(?,?,?,?,?)",params![id,w.revision+1,raw,creator,now()])?;
                tx.execute("update work_items set revision=revision+1,state=case when current_run is null or state='done' then 'backlog' else state end,reason='contract revised; recheck required' where id=?",[id])?;
                invalidate_dependents(tx, id)?;
            }
        }
        tx.execute("update work_items set title=coalesce(?,title),priority=coalesce(?,priority) where id=?",params![input.title,input.priority,id])?;
        audit(tx, w.root, "revised", Some(id), w.run, None, true)?;
        show(tx, id)
    })
}
pub fn queue(db: &mut Connection, a: Authority, id: i64, input: Revision) -> Result<Value> {
    transaction(db, |tx| {
        let w = item(tx, id)?;
        authority(tx, a, w.root, false)?;
        expected(&w, input.expected_revision)?;
        if w.state != "backlog" {
            return Err(reject("only backlog work can queue"));
        }
        if let Some(run) = w.run
            && run_row(tx, run)?.state == "active"
        {
            return Err(reject("prior run still owns checkout; reconcile first"));
        }
        let c = contract(tx, &w)?;
        validate_contract(tx, w.root, Some(id), &c)?;
        tx.execute(
            "update work_items set state='queued',current_run=null,reason=null where id=?",
            [id],
        )?;
        audit(tx, w.root, "queued", Some(id), None, None, true)?;
        show(tx, id)
    })
}
pub fn list(db: &Connection, campaign: i64) -> Result<Value> {
    let mut s = db.prepare(
        "select id from work_items where root_id=? order by priority desc,id limit 1000",
    )?;
    let ids = s
        .query_map([campaign], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rows = ids
        .into_iter()
        .map(|id| show(db, id))
        .collect::<Result<Vec<_>>>()?;
    let config: Option<(bool, u32, u32)> = db
        .query_row(
            "select enabled,max_active,max_review_pending from work_campaigns where root_id=?",
            [campaign],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (enabled, max_active, max_review_pending) = config.unwrap_or((false, 1, 1));
    Ok(
        json!({"campaign":campaign,"generation":delivery::generation(db,campaign)?,"parked":parked(db,campaign)?,"configuration":{"enabled":enabled,"max_active":max_active,"max_review_pending":max_review_pending},"work":rows}),
    )
}
pub fn show(db: &Connection, id: i64) -> Result<Value> {
    let w = item(db, id)?;
    let (title, priority, reason, creator): (String, i32, Option<String>, String) = db.query_row(
        "select title,priority,reason,creator from work_items where id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let mut s = db.prepare(
        "select revision,contract from work_revisions where work_id=? order by revision",
    )?;
    let revisions=s.query_map([id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Option<String>>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().map(|(r,c)|Ok(json!({"revision":r,"contract":c.map(|c|serde_json::from_str::<Value>(&c)).transpose()?}))).collect::<Result<Vec<_>>>()?;
    let mut s = db.prepare("select id from work_runs where work_id=? order by id")?;
    let runs = s
        .query_map([id], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| run_output(db, id))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"ok":true,"work_id":id,"campaign":w.root,"title":title,"priority":priority,"revision":w.revision,"state":w.state,"current_run":w.run,"reason":reason,"creator":serde_json::from_str::<Value>(&creator)?,"revisions":revisions,"runs":runs}),
    )
}
