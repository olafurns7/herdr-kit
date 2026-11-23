//! Root-to-root peer text uses the existing inbox and delivery receipts.
use super::*;

/// A root whose inbox lease names a push adapter, live or not. A release clears the holder,
/// so a released lease counts when an adapter has ever pushed to the root. The mark has no
/// end: a plain root that was a push lane keeps it.
fn push_lane(db: &Connection, root: i64) -> Result<bool> {
    Ok(db.query_row(
        "select exists(select 1 from consumer_leases l where l.task_id=?1 and (l.holder glob 'mcp-channel-*' or l.holder glob 'codex-queue-*' or l.holder is null and exists(select 1 from delivery_attempts d where d.task_id=?1 and d.session_gen is not null and d.adapter in ('claude-channel','codex-queue'))))",
        [root],
        |r| r.get(0),
    )?)
}

/// An open push-lane root that is not push-live: its pane may be starting up at a dialog.
/// Only an open root without a launch can become push-live; others keep the pane.
pub fn not_push_live(db: &Connection, t: &Task) -> Result<bool> {
    Ok(t.parent.is_none()
        && t.launch.is_none()
        && t.status == "open"
        && consumer::push_generation(db, t)?.is_none()
        && push_lane(db, t.id)?)
}

/// Choose once, on the ledger server, before any pane prerequisite. A push is terminal,
/// and so is the refusal of a push lane that is not push-live: its pane may be starting
/// up at a dialog that typed text would answer.
pub fn route(
    db: &mut Connection,
    target: i64,
    sender: i64,
    text: &str,
    file: &str,
    push: bool,
    pane: bool,
) -> Result<Option<Value>> {
    if push && pane {
        return Err(usage("--push and --pane are mutually exclusive"));
    }
    transaction(db, |tx| {
        let to = open_task(tx, target)?;
        if to.parent.is_some() {
            if sender != 0 {
                return Err(reject(
                    "--as needs a root target; worker prompts use the pane protocol",
                ));
            }
            if push {
                return Err(reject("--push needs a root target"));
            }
            return Ok(None);
        }
        if sender == target {
            return Err(reject("a root cannot message itself"));
        }
        let generation = consumer::push_generation(tx, &to)?;
        if pane || sender == 0 || generation.is_none() {
            if push {
                if sender == 0 {
                    return Err(reject(
                        "root messages need --as SENDER_ROOT (a worker cannot send them)",
                    ));
                }
                return Err(reject(format!("root {target} is not push-live")));
            }
            if !pane && not_push_live(tx, &to)? {
                return Ok(Some(json!({
                    "target": target,
                    "outcome": "rejected",
                    "reason": "not_push_live",
                    "error": format!(
                        "not_push_live: root {target} is a push lane and is not push-live (restarting?); not typed; retry when push is live; pass --pane only after you have read its pane and it shows the agent's input line, not a dialog"
                    ),
                    "_exit": ExitCode::Transport as u8,
                })));
            }
            return Ok(None);
        }
        let from = worker::resolve(tx, sender)?;
        if from.launch.is_some() {
            return Err(reject("the sender root must have no launch"));
        }
        if !file.is_empty()
            || text.is_empty()
            || text.len() > 4096
            || text.chars().any(|c| c.is_control() && c != '\n')
        {
            return Err(usage(
                "messages need --text: 1 to 4096 UTF-8 bytes, no control characters except newline; --file is not supported",
            ));
        }
        let generation = generation.expect("live generation");
        let e = event(
            tx,
            Event {
                task: from.id,
                to: Some(target),
                kind: "message",
                summary: text,
                data: Some(json!({"text":text,"gen":generation})),
                ..Event::default()
            },
        )?;
        Ok(Some(
            json!({"path":"push","event":e,"target":target,"gen":generation,"received":false}),
        ))
    })
}

/// Sender-scoped read; receipt and handling remain separate, including across adopts.
pub fn status(db: &Connection, event: i64, sender: i64) -> Result<Value> {
    if sender <= 0 {
        return Err(usage("message status needs --as SENDER_ROOT"));
    }
    let from = worker::resolve(db, sender)?;
    let target: Option<i64> = db
        .query_row(
            "select recipient_task_id from events where id=? and kind='message' and task_id=?",
            params![event, from.id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let target =
        target.ok_or_else(|| reject(format!("message {event} was not sent by root {sender}")))?;
    let acked = task(db, target)?.acked >= event;
    let attempts: i64 = db.query_row(
        "select count(*) from delivery_attempts where event_id=?",
        [event],
        |r| r.get(0),
    )?;
    let received: Option<(String, i64)> = db.query_row(
        "select received_at,session_gen from delivery_attempts where event_id=? and task_id=? and received_at is not null order by id desc limit 1",
        params![event, target], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional()?;
    let mut out = json!({"event":event,"target":target,"state":if acked {"acked"} else if received.is_some() {"received"} else {"pending"},"attempts":attempts});
    if let Some((at, generation)) = received {
        out["received_at"] = json!(at);
        out["gen"] = json!(generation);
    }
    Ok(out)
}

/// Called by the adapter at its redelivery cap; the server verifies and deduplicates.
pub fn unreceived(
    db: &mut Connection,
    as_id: i64,
    session: Option<i64>,
    event_id: i64,
    fence: Option<i64>,
) -> Result<Value> {
    transaction(db, |tx| {
        let (to, _) = delivery::identify(tx, as_id, session)?;
        let f = fence.map(|fence| consumer::Fence {
            fence,
            as_id,
            session,
        });
        consumer::guard(tx, to.id, f.as_ref())?;
        let sender: Option<i64> = tx
            .query_row(
                "select task_id from events where id=? and kind='message' and recipient_task_id=?",
                params![event_id, to.id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(sender) = sender else {
            return Ok(json!({"alarmed":false}));
        };
        if sender == to.id {
            return Ok(json!({"alarmed":false}));
        }
        // The adapter records each receipt itself: what is missing is the ack.
        let count: i64 = tx.query_row("select count(*) from delivery_attempts where event_id=? and task_id=? and session_gen is ?", params![event_id,to.id,session], |r|r.get(0))?;
        if count < 3 || to.acked >= event_id {
            return Ok(json!({"alarmed":false}));
        }
        if let Some(e) = tx
            .query_row("select id from events where kind='herdr' and related_event_id=? and task_id=? and recipient_task_id=? and json_extract(data,'$.reason')='message_unreceived'", params![event_id,sender,sender], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
        {
            return Ok(json!({"alarmed":true,"event":e}));
        }
        let e = event(
            tx,
            Event {
                task: sender,
                to: Some(sender),
                kind: "herdr",
                summary: &format!(
                    "message {event_id} to root {} has no ack after 3 pushes; inspect its pane and consumer lease before handoff or resume",
                    to.id
                ),
                data: Some(
                    json!({"reason":"message_unreceived","event":event_id,"target":to.id,"gen":session}),
                ),
                related: Some(event_id),
                ..Event::default()
            },
        )?;
        Ok(json!({"alarmed":true,"event":e}))
    })
}
