use super::*;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ask {
    pub ask: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub handle: String,
    pub responses: std::collections::BTreeMap<String, Reply>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub action: Action,
    pub content: Option<Choice>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Accept,
    Decline,
    Cancel,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub choice: String,
}

struct Question {
    identity: String,
    question: String,
    choices: Vec<String>,
    answered: Option<i64>,
}
fn question(db: &Connection, root: i64, ask: i64) -> Result<Question> {
    let row:Option<(i64,String,String,Option<i64>)>=db.query_row("select task_id,coalesce(summary,''),coalesce(data,'{}'),answered_by from events where id=? and kind='ask'",[ask],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (task_id, summary, raw, answered) =
        row.ok_or_else(|| reject("owner ask does not exist"))?;
    let data: Value = serde_json::from_str(&raw)?;
    let t = open_task(db, task_id)?;
    if task_id != root
        || t.parent.is_some()
        || t.launch.is_some()
        || t.role == "sub-orchestrator"
        || data["owner"] != true
    {
        return Err(reject("only this root's existing owner asks are eligible"));
    }
    let q = &data["question"];
    if q["multiSelect"] == true {
        return Err(reject("native owner input requires single-choice question"));
    }
    let text = q["question"]
        .as_str()
        .ok_or_else(|| reject("native owner input requires a structured question"))?;
    if text.is_empty() || text.len() > 4096 {
        return Err(usage("invalid owner question length"));
    }
    let options = q["options"]
        .as_array()
        .filter(|a| (2..=4).contains(&a.len()))
        .ok_or_else(|| reject("native owner input requires 2..4 typed choices"))?;
    let mut choices = Vec::new();
    for opt in options {
        let label = opt["label"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 240 && !s.contains('\0'))
            .ok_or_else(|| usage("invalid owner choice"))?;
        if choices.iter().any(|s| s == label) {
            return Err(usage("duplicate owner choice"));
        }
        choices.push(label.to_string());
    }
    let identity = format!(
        "{:x}",
        Sha256::digest(
            compact_json(&json!({"ask":ask,"root":root,"summary":summary,"data":data}))?.as_bytes()
        )
    );
    Ok(Question {
        identity,
        question: text.to_string(),
        choices,
        answered,
    })
}
fn permitted(db: &Connection, s: &Session, ask: i64, q: &Question) -> Result<()> {
    let allowed:bool=db.query_row("select exists(select 1 from jobs_ask_policy where ask_id=? and root_id=? and generation=? and host=? and identity=?)",params![ask,s.root,s.generation,host(),q.identity],|r|r.get(0))?;
    if !allowed {
        return Err(reject(
            "owner ask is reserved for ledger UI; explicit jobs allow-ask is required for this exact question and generation",
        ));
    }
    Ok(())
}
/// Deliberate CLI-only per-ask opt-in; never exported as an MCP tool.
pub fn allow_ask(db: &mut Connection, s: &Session, ask: i64) -> Result<Value> {
    transaction(db, |tx| {
        auth(tx, s)?;
        let q = question(tx, s.root, ask)?;
        if q.answered.is_some() {
            return Err(reject("owner ask is already answered"));
        }
        tx.execute("insert into jobs_ask_policy(ask_id,root_id,generation,host,identity) values(?,?,?,?,?) on conflict(ask_id) do update set root_id=excluded.root_id,generation=excluded.generation,host=excluded.host,identity=excluded.identity",params![ask,s.root,s.generation,host(),q.identity])?;
        Ok(
            json!({"ok":true,"ask":ask,"version":q.identity,"provenance":"native replies may come from a hook; this policy grants no human-consent claim"}),
        )
    })
}
pub fn owner_input(db: &mut Connection, s: &Session, ask: i64) -> Result<Value> {
    transaction(db, |tx| {
        auth(tx, s)?;
        let q = question(tx, s.root, ask)?;
        permitted(tx, s, ask, &q)?;
        let old:Option<String>=tx.query_row("select id from jobs_operations where root_id=? and kind='owner' and reference_id=?",params![s.root,ask],|r|r.get(0)).optional()?;
        let id = if let Some(id) = old {
            require_bound(s, &operation(tx, &id)?)?;
            id
        } else {
            if q.answered.is_some() {
                return Err(reject(
                    "owner ask was already answered through another channel",
                ));
            }
            insert(
                tx,
                s,
                "owner",
                ask,
                ask,
                "input_required",
                Some(json!({"identity":q.identity,"question":q.question,"choices":q.choices})),
            )?
        };
        refresh(tx, &operation(tx, &id)?)?;
        view(tx, &operation(tx, &id)?)
    })
}
pub(super) fn requests(o: &Operation) -> Result<Value> {
    let snap = o
        .snapshot
        .as_ref()
        .ok_or_else(|| reject("owner question snapshot missing"))?;
    Ok(
        json!({"owner_choice":{"method":"elicitation/create","params":{"mode":"form","message":snap["question"],"requestedSchema":{"type":"object","properties":{"choice":{"type":"string","enum":snap["choices"]}},"required":["choice"],"additionalProperties":false}}}}),
    )
}
pub(super) fn refresh(db: &Connection, o: &Operation) -> Result<()> {
    if o.status != "input_required" {
        return Ok(());
    }
    let q = question(db, o.root, o.reference)?;
    if q.answered.is_some() {
        finish(
            db,
            &o.id,
            "completed",
            Some(tool(
                json!({"outcome":"answered_elsewhere","ask":o.reference,"answer":q.answered}),
            )),
        )?;
    } else if o.snapshot.as_ref().and_then(|v| v["identity"].as_str()) != Some(q.identity.as_str())
    {
        finish(
            db,
            &o.id,
            "failed",
            Some(json!({"code":-32000,"message":"Owner question changed; use the ledger UI."})),
        )?;
    }
    Ok(())
}
pub fn update(db: &mut Connection, s: &Session, input: Update) -> Result<Value> {
    let raw = serde_json::to_string(&input.responses)?;
    if raw.len() > work::MAX_JSON {
        return Err(usage("jobs update exceeds 256 KiB"));
    }
    transaction(db, |tx| {
        auth(tx, s)?;
        let o = operation(tx, &input.handle)?;
        require_bound(s, &o)?;
        if o.kind != "owner" {
            return Err(reject("tasks/update applies only to owner input"));
        }
        if let Some(prior) = &o.reply {
            if *prior != raw {
                return Err(reject("conflicting immutable owner reply"));
            }
            return view(tx, &o);
        }
        if o.status != "input_required" {
            return Err(reject("owner input is terminal"));
        }
        if input.responses.len() != 1 {
            return Err(usage(
                "exactly the stable owner_choice response is required",
            ));
        }
        let reply = input
            .responses
            .get("owner_choice")
            .ok_or_else(|| usage("unknown input response key"))?;
        let q = question(tx, s.root, o.reference)?;
        permitted(tx, s, o.reference, &q)?;
        if o.snapshot.as_ref().and_then(|v| v["identity"].as_str()) != Some(q.identity.as_str()) {
            return Err(reject("stale owner question identity"));
        }
        if q.answered.is_some() {
            return Err(reject(
                "owner ask was answered elsewhere; native replies never supersede",
            ));
        }
        let value = match reply.action {
            Action::Accept => {
                let c = reply
                    .content
                    .as_ref()
                    .ok_or_else(|| usage("accept requires an exact choice"))?;
                // The immutable task handle pins the question version, checked above.
                if !q.choices.contains(&c.choice) {
                    return Err(reject("invalid owner choice"));
                }
                let text = c.choice.clone();
                let provenance = format!("native:{}", s.binding);
                let answer = event(
                    tx,
                    Event {
                        task: s.root,
                        to: Some(s.root),
                        kind: "answer",
                        summary: &text,
                        data: Some(
                            json!({"owner":true,"provenance":provenance,"jobs_operation":o.id,"question_identity":q.identity}),
                        ),
                        related: Some(o.reference),
                        ..Event::default()
                    },
                )?;
                if tx.execute(
                    "update events set answered_by=? where id=? and answered_by is null",
                    params![answer, o.reference],
                )? != 1
                {
                    return Err(reject("owner ask answered concurrently"));
                }
                orch::relay_owner(tx, o.reference, Some(answer))?;
                json!({"outcome":"answered","ask":o.reference,"answer":answer,"choice":c.choice,"provenance":provenance})
            }
            Action::Decline | Action::Cancel => {
                if reply.content.is_some() {
                    return Err(usage("decline/cancel must omit content"));
                }
                json!({"outcome":"no_decision","ask":o.reference,"provenance":format!("native:{}",s.binding)})
            }
        };
        tx.execute(
            "update jobs_operations set reply=? where id=?",
            params![raw, o.id],
        )?;
        finish(tx, &o.id, "completed", Some(tool(value)))?;
        view(tx, &operation(tx, &o.id)?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_question_identity_pins_exact_options_and_refuses_worker_or_plain_asks() {
        let db = super::super::tests::fixture();
        let data = json!({"owner":true,"question":{"question":"Synthetic choice?","options":[{"label":"Proceed"},{"label":"Hold"}]}});
        let ask = event(
            &db,
            Event {
                task: 1,
                kind: "ask",
                summary: "Synthetic choice?",
                data: Some(data.clone()),
                ..Event::default()
            },
        )
        .unwrap();
        let q = question(&db, 1, ask).unwrap();
        db.execute("update events set data=? where id=?",params![json!({"owner":true,"question":{"question":"Synthetic choice?","options":[{"label":"Stop"},{"label":"Hold"}]}}).to_string(),ask]).unwrap();
        assert_ne!(q.identity, question(&db, 1, ask).unwrap().identity);
        assert!(question(&db, 2, ask).is_err());
        db.execute("update events set data='{}' where id=?", [ask])
            .unwrap();
        assert!(question(&db, 1, ask).is_err());
    }
}
