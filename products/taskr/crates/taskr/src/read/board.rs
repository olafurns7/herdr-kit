//! `taskr board`: the hub board's rows, read live from the ledger and the PR poller's
//! state. The daemon serves this JSON at /api/board; nothing is stored.
use super::campaign::lane_state;
use super::glance::{clip, n, s};
use super::queries::rows;
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;

/// Tickets per root, as the campaign snapshot caps its own at 100.
const PER_ROOT: i64 = 500;

pub fn run(f: &FlagSet) -> Result<()> {
    let db = open()?;
    let now = taskr_core::frozen_now().map_err(|e| usage(e.to_string()))?;
    db.execute_batch("begin")?;
    let value = snapshot(&db, now, &crate::daemon::github::default_repo())?;
    db.execute_batch("rollback")?;
    cli::emit(f.json(), &value, false);
    Ok(())
}

fn hours(now: OffsetDateTime, at: &str) -> f64 {
    taskr_core::store::parse_time(at)
        .map_or(0., |t| ((now - t).as_seconds_f64() / 360.).round() / 10.)
}

/// `blocked:`, `parked:` and `waits:` segments of an intent: a segment starts at the
/// text's start, after a newline, or after a period and spaces, with `word:`.
fn segments(intent: &str) -> BTreeMap<&'static str, String> {
    let b = intent.as_bytes();
    let key_at = |j: usize| {
        if !b.get(j).is_some_and(u8::is_ascii_lowercase) {
            return false;
        }
        let mut k = j + 1;
        while k < b.len() && k - j <= 20 && (b[k].is_ascii_lowercase() || b[k] == b' ') {
            k += 1;
        }
        b.get(k) == Some(&b':')
    };
    let mut pieces = vec![];
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let cut = if b[i] == b'\n' && key_at(i + 1) {
            Some((i, i + 1))
        } else if b[i] == b'.' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            (j > i + 1 && key_at(j)).then_some((i + 1, j))
        } else {
            None
        };
        if let Some((end, next)) = cut {
            pieces.push(&intent[start..end]);
            start = next;
            i = next;
        } else {
            i += 1;
        }
    }
    pieces.push(&intent[start..]);
    let mut out = BTreeMap::new();
    for piece in pieces {
        let piece = piece.trim();
        for key in ["blocked", "parked", "waits"] {
            if let Some(rest) = piece.strip_prefix(key).and_then(|r| r.strip_prefix(':')) {
                out.insert(key, rest.trim().trim_end_matches('.').to_string());
            }
        }
    }
    out
}

/// `A12 title` -> ("A12", "title"); the tag letters build.py used.
fn tag(title: &str) -> (&str, &str) {
    let b = title.as_bytes();
    let digits = b.iter().skip(1).take_while(|c| c.is_ascii_digit()).count();
    if b.first().is_some_and(|c| b"AUPSF".contains(c))
        && digits > 0
        && b.get(1 + digits).is_some_and(u8::is_ascii_whitespace)
    {
        (&title[..1 + digits], title[1 + digits..].trim())
    } else {
        ("", title)
    }
}

/// Mentions `W<id>` as a word.
fn names(text: &str, w: i64) -> bool {
    let needle = format!("W{w}");
    text.match_indices(&needle).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + needle.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The card's PR state from the poller's `pr_state` row; "" before its first poll.
fn pr_label(state: &Value) -> String {
    match s(state, "state") {
        "MERGED" => "merged".into(),
        "CLOSED" => "closed".into(),
        "OPEN" if state["draft"] == true => "draft".into(),
        "OPEN" if !s(state, "merge").is_empty() => s(state, "merge").to_lowercase(),
        "OPEN" => "open".into(),
        _ => String::new(),
    }
}

pub(super) fn snapshot(db: &Connection, now: OffsetDateTime, default_repo: &str) -> Result<Value> {
    let at = taskr_core::store::stamp(now);
    let has_work: bool = db.query_row(
        "select exists(select 1 from sqlite_master where type='table' and name='work_items')",
        [],
        |r| r.get(0),
    )?;
    let items = if has_work {
        rows(
            db,
            "select w.id,w.root_id,coalesce(r.name,'') as campaign,w.title,w.priority,w.state,coalesce(w.reason,'') as reason,w.created_at,coalesce(v.contract,'{}') as contract from (select *,row_number() over (partition by root_id order by priority desc,id) as n from work_items) w left join tasks r on r.id=w.root_id left join work_revisions v on v.work_id=w.id and v.revision=w.revision where w.n<=? order by w.root_id,w.id",
            vec![PER_ROOT.into()],
        )?
    } else {
        vec![]
    };
    let states: BTreeMap<i64, String> = items
        .iter()
        .map(|i| (n(i, "id"), s(i, "state").to_string()))
        .collect();
    // Latest non-empty `work` and `pr` refs of every task under a root with tickets.
    let refs = if has_work {
        rows(
            db,
            "with recursive tree(id) as (select distinct root_id from work_items union select t.id from tasks t join tree on t.parent_id=tree.id) select e.task_id,json_extract(e.data,'$.key') as key,json_extract(e.data,'$.value') as value from events e where e.task_id in (select id from tree) and e.kind='ref' and json_extract(e.data,'$.key') in ('work','pr') and e.id=(select max(x.id) from events x where x.task_id=e.task_id and x.kind='ref' and json_extract(x.data,'$.key')=json_extract(e.data,'$.key')) and coalesce(json_extract(e.data,'$.value'),'')!=''",
            vec![],
        )?
    } else {
        vec![]
    };
    let mut lane_work: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let mut lane_pr: BTreeMap<i64, String> = BTreeMap::new();
    for r in &refs {
        let task = n(r, "task_id");
        match s(r, "key") {
            "work" => {
                if let Ok(w) = s(r, "value").trim().trim_start_matches('W').parse::<i64>() {
                    lane_work.entry(w).or_default().insert(task);
                }
            }
            _ => {
                lane_pr.insert(task, s(r, "value").to_string());
            }
        }
    }
    let linked: Vec<i64> = lane_work.values().flatten().copied().collect();
    let mut lanes: BTreeMap<i64, Value> = BTreeMap::new();
    for mut lane in rows(
        db,
        "select t.id,t.status,t.updated_at,coalesce(case when t.current_launch_id is null then t.machine else l.machine end,'') as host,coalesce(l.model,'') as model,coalesce(l.effort,'') as effort,coalesce(l.observed_status,'') as observed_status,l.present from tasks t left join launches l on l.id=t.current_launch_id where t.id in (select value from json_each(?))",
        vec![serde_json::to_string(&linked).expect("ids").into()],
    )? {
        lane["state"] = json!(lane_state(db, &lane)?);
        lanes.insert(n(&lane, "id"), lane);
    }
    let mut prs: BTreeMap<String, Option<(String, u64, Option<Value>)>> = BTreeMap::new();
    for value in lane_pr.values() {
        if !prs.contains_key(value) {
            let found =
                crate::daemon::github::lookup(db, value, default_repo).map_err(|e| Error {
                    code: e.code,
                    text: e.message,
                })?;
            prs.insert(value.clone(), found);
        }
    }
    let asks = rows(
        db,
        "with recursive tree(root,id) as (select id,id from tasks where parent_id is null and status!='closed' union all select tree.root,t.id from tasks t join tree on t.parent_id=tree.id) select e.id,r.name as campaign,coalesce(e.summary,'') as text,e.created_at,coalesce(json_extract(e.data,'$.blocking'),0) as blocking from tree join tasks t on t.id=tree.id join tasks r on r.id=tree.root join events e on e.task_id=t.id where t.status!='closed' and e.kind='ask' and e.answered_by is null and json_extract(e.data,'$.owner')=1 order by e.id",
        vec![],
    )?;
    let mut tickets = vec![];
    for i in &items {
        let w = n(i, "id");
        let state = s(i, "state");
        if state == "cancelled" {
            continue;
        }
        let contract: Value = serde_json::from_str(s(i, "contract")).unwrap_or_default();
        let seg = segments(contract["intent"].as_str().unwrap_or(""));
        let (reference, title) = tag(s(i, "title"));
        let mine: Vec<&Value> = lane_work
            .get(&w)
            .into_iter()
            .flatten()
            .filter_map(|l| lanes.get(l))
            .collect();
        let working: Vec<&&Value> = mine
            .iter()
            .filter(|l| matches!(s(l, "state"), "working" | "ready"))
            .collect();
        // (repo, number) -> poller state, over every PR of the ticket's lanes.
        let mut linked = BTreeMap::new();
        for l in &mine {
            if let Some(Some((repo, number, state))) = lane_pr.get(&n(l, "id")).map(|v| &prs[v]) {
                linked.insert((*number, repo.clone()), state.clone());
            }
        }
        let open = |state: &Option<Value>| state.as_ref().is_some_and(|v| v["state"] == "OPEN");
        let shown = linked
            .iter()
            .rfind(|(_, st)| open(st))
            .or_else(|| linked.iter().next_back());
        let named: Vec<&Value> = asks.iter().filter(|a| names(s(a, "text"), w)).collect();
        let mut blocked = seg.get("blocked").cloned().unwrap_or_default();
        let blocking: Vec<String> = named
            .iter()
            .filter(|a| a["blocking"] == true)
            .map(|a| n(a, "id").to_string())
            .collect();
        if blocked.is_empty() && !blocking.is_empty() {
            blocked = format!("blocking owner ask {}", blocking.join(", "));
        }
        let column = if state == "done" {
            "done"
        } else if !working.is_empty() || linked.values().any(open) {
            "in_progress"
        } else {
            "not_done"
        };
        let mut card = json!({"pr":"","pr_repo":"","pr_state":"","queue":"","checks":{}});
        if let Some(((number, repo), st)) = shown {
            card["pr"] = json!(number);
            card["pr_repo"] = json!(repo);
            if let Some(st) = st {
                card["pr_state"] = json!(pr_label(st));
                if st["state"] == "OPEN" {
                    if let Some(q) = st["queue"].as_u64() {
                        card["queue"] = json!(q);
                    }
                    card["checks"] = st["named"].clone();
                }
            }
        }
        let deps: Vec<i64> = contract["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["work"].as_i64())
            .collect();
        let last = mine
            .iter()
            .map(|l| s(l, "updated_at"))
            .max()
            .unwrap_or_else(|| s(i, "created_at"));
        let model = |l: &Value| {
            [s(l, "model").trim_start_matches("claude-"), s(l, "effort")]
                .into_iter()
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut row = json!({
            "w": w, "key": format!("W{w}"), "ref": reference, "phase": &reference[..reference.len().min(1)],
            "group": format!("{}{}", n(i, "root_id"), &reference[..reference.len().min(1)]),
            "ord": reference.get(1..).and_then(|d| d.parse::<i64>().ok()).unwrap_or(0) * 1000 + w,
            "root": n(i, "root_id"), "campaign": s(i, "campaign"), "title": title, "state": state,
            "column": column, "priority": n(i, "priority"), "blocked": blocked,
            "parked": seg.get("parked").cloned().unwrap_or_default(),
            "lanes": working.iter().map(|l| format!("{} {} ({})", n(l, "id"), s(l, "state"), model(l))).collect::<Vec<_>>().join("; "),
            "waits": deps.iter().map(|d| format!("W{d}")).collect::<Vec<_>>().join(" "),
            "waits_open": deps.iter().filter(|d| states.get(d).is_none_or(|s| s != "done")).map(|d| format!("W{d}")).collect::<Vec<_>>().join(" "),
            "waits_text": seg.get("waits").cloned().unwrap_or_default(),
            "asks": named.iter().map(|a| n(a, "id").to_string()).collect::<Vec<_>>().join(" "),
            "note": if state == "done" { clip(s(i, "reason"), 200) } else { String::new() },
            "idle_h": hours(now, last),
        });
        row.as_object_mut()
            .unwrap()
            .extend(card.as_object().unwrap().clone());
        tickets.push(row);
    }
    tickets.sort_by_key(|t| (-n(t, "priority"), n(t, "ord")));
    // One group per campaign and tag letter; its wait line is the parked text every
    // ticket not done shares.
    let mut groups: BTreeMap<(i64, String), Value> = BTreeMap::new();
    for t in &tickets {
        let phase = s(t, "phase").to_string();
        let g = groups
            .entry((n(t, "root"), if phase.is_empty() { "~".into() } else { phase.clone() }))
            .or_insert_with(|| json!({"key":t["group"],"root":t["root"],"campaign":t["campaign"],"phase":phase,"parked":null}));
        if t["column"] != "done" {
            g["parked"] = match (&g["parked"], s(t, "parked")) {
                (Value::Null, p) => json!(p),
                (Value::String(have), p) if have == p => json!(p),
                _ => json!(""),
            };
        }
    }
    let groups: Vec<Value> = groups
        .into_values()
        .map(|mut g| {
            if g["parked"].is_null() {
                g["parked"] = json!("");
            }
            g
        })
        .collect();
    let mut owner_asks: Vec<Value> = asks
        .iter()
        .map(|a| json!({"ask":a["id"],"blocking":a["blocking"] == true,"campaign":a["campaign"],"age_h":hours(now,s(a,"created_at")),"text":clip(s(a,"text"),240)}))
        .collect();
    owner_asks.sort_by_key(|a| (a["blocking"] != true, n(a, "ask")));
    let since = taskr_core::store::stamp(now - time::Duration::hours(3));
    let mut alarms = vec![];
    for a in rows(
        db,
        "select e.id,e.created_at,e.summary from tasks r join events e on e.task_id=r.id where r.parent_id is null and r.status!='closed' and e.kind='note' and substr(e.summary,1,6)='ALARM ' and e.created_at>=? order by e.id desc limit 100",
        vec![since.into()],
    )? {
        let text = s(&a, "summary")["ALARM ".len()..].trim();
        let (first, rest) = text.split_once(' ').unwrap_or((text, ""));
        let clock = |t: &str| t.len() == 6 && t.ends_with('Z') && t.as_bytes()[2] == b':';
        let (time, text) = if clock(first) {
            (first.to_string(), rest.trim())
        } else {
            (
                format!("{}Z", s(&a, "created_at").get(11..16).unwrap_or("")),
                text,
            )
        };
        let (event, detail) = text.split_once(' ').unwrap_or((text, ""));
        alarms.push(
            json!({"seq":a["id"],"time":time,"event":event,"detail":clip(detail.trim(),200)}),
        );
    }
    Ok(json!({"at":at,"tickets":tickets,"groups":groups,"asks":owner_asks,"alarms":alarms}))
}

#[cfg(test)]
#[path = "board_tests.rs"]
mod tests;
