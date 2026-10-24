//! Lazy, bounded evidence text for the existing document pager.
use serde_json::Value;

fn text(v: &Value, key: &str) -> String {
    let s = v[key].as_str().unwrap_or("unavailable");
    s.chars().take(1024).collect()
}
fn rows(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

pub(crate) fn document(id: i64, v: &Value) -> Result<String, String> {
    if v["work_id"].as_i64() != Some(id) {
        return Err("the ledger sent another work item".into());
    }
    let revision = &v["revision"];
    let mut out = vec![
        format!("# W{id}: {}", text(v, "title")),
        format!(
            "Revision {revision} · {} · priority {}",
            text(v, "state"),
            v["priority"]
        ),
        format!("Blocked/recheck reason: {}", text(v, "reason")),
    ];
    if let Some(contract) = rows(&v["revisions"])
        .iter()
        .find(|r| r["revision"] == *revision)
        .map(|r| &r["contract"])
        .filter(|c| !c.is_null())
    {
        out.extend([
            format!("Intent: {}", text(contract, "intent")),
            format!("Acceptance: {}", text(contract, "acceptance")),
            format!(
                "Required profile: {} · digest {} · {} checks",
                text(&contract["profile"], "id"),
                text(&contract["profile"], "digest"),
                rows(&contract["profile"]["checks"]).len()
            ),
        ]);
    } else {
        out.push("No contract for current revision".into());
    }
    let run = rows(&v["runs"])
        .iter()
        .find(|r| !v["current_run"].is_null() && r["run_id"] == v["current_run"]);
    let Some(run) = run else {
        out.push("Unassigned · no current run; proof/review/acceptance pending".into());
        return Ok(out.join("\n\n"));
    };
    out.extend([
        format!("Run {} · revision {} · round {} · {} · action {}", run["run_id"], run["revision"], run["round"], text(run, "state"), text(run, "action")),
        run["acceptance_id"].as_i64().map_or("Acceptance pending (no receipt)".into(), |id| format!("Acceptance receipt {id} (local workflow only)")),
        "Typed results, process stops, deterministic checks and source review are separate evidence. Missing evidence is unavailable, never PASS.".into(),
    ]);
    if run["revision"] != *revision {
        out.push("RECHECK: current semantic revision differs from this run; historical evidence does not verify the current task.".into());
    }
    for a in rows(&run["assignments"])
        .iter()
        .filter(|a| a["round"] == run["round"])
        .take(4)
    {
        out.push(format!(
            "## {} · lane {} · launch {}\n\nAssignment state: {}\n\nProcess: {} · stop receipt {}",
            text(a, "stage"),
            a["task_id"],
            a["launch_id"],
            text(a, "state"),
            text(a, "process_state"),
            a["stop_event"]
                .as_i64()
                .map_or("pending".into(), |id| id.to_string())
        ));
        let Some(result) = rows(&run["results"])
            .iter()
            .find(|r| r["launch_id"] == a["launch_id"])
        else {
            out.push("Result pending".into());
            continue;
        };
        let p = &result["payload"]["result"];
        out.push(format!(
            "Result {} · digest {}\n\nSource commit {}\n\nTree {}\n\nInput {}",
            result["result_id"],
            text(result, "digest"),
            text(&p["source"], "commit"),
            text(&p["source"], "tree"),
            text(&p["source"], "input_digest")
        ));
        match result["payload"]["kind"].as_str() {
            Some("execution") => out.push(format!("Execution report: {}", text(p, "report"))),
            Some("proof") => {
                out.push(format!(
                    "Gate profile {} · runner {} · {} check records",
                    text(p, "profile_digest"),
                    text(p, "runner_version"),
                    rows(&p["checks"]).len()
                ));
                if rows(&p["checks"]).is_empty() {
                    out.push("No check receipts; no proof of passing checks".into());
                }
                for check in rows(&p["checks"]).iter().take(64) {
                    out.push(format!(
                        "Check {} · exit {} · signal {} · timeout {} · error {}",
                        text(check, "check"),
                        check["exit"],
                        check["signal"],
                        check["timed_out"],
                        text(check, "error")
                    ));
                    let cases = rows(&check["cases"]);
                    if cases.is_empty() {
                        out.push("No case receipts (command-only evidence unless the profile requires cases)".into());
                    }
                    for case in cases.iter().take(32) {
                        out.push(format!(
                            "Case {}: {}",
                            text(case, "id"),
                            text(case, "status")
                        ));
                    }
                    if cases.len() > 32 {
                        out.push(format!(
                            "{} more cases; taskr work show {id} reads the stored result",
                            cases.len() - 32
                        ));
                    }
                    for artifact in rows(&check["artifacts"]).iter().take(16) {
                        out.push(format!(
                            "Artifact {} · sha256 {}",
                            text(artifact, "name"),
                            text(artifact, "sha256")
                        ));
                    }
                }
            }
            Some("review") => {
                out.push(format!(
                    "Source review: {} · execution result {} (not runtime proof)",
                    text(p, "verdict"),
                    p["execution_result"]
                ));
                for finding in rows(&p["findings"]).iter().take(32) {
                    out.push(format!(
                        "Finding {}: {}\n\nEvidence: {}",
                        text(finding, "id"),
                        text(finding, "reason"),
                        text(finding, "evidence")
                    ));
                }
                if rows(&p["findings"]).len() > 32 {
                    out.push("More findings retained in work show".into());
                }
            }
            Some("adjudication") => out.push(format!(
                "Adjudication: {} · {}",
                text(p, "verdict"),
                text(p, "reason")
            )),
            _ => out.push("Result kind unavailable".into()),
        }
    }
    Ok(out.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pending_and_settled_results_do_not_invent_acceptance_or_process_stops() {
        let mut v = json!({"work_id":7,"revision":2,"state":"backlog","runs":[]});
        assert!(document(7, &v).unwrap().contains("Unassigned"));
        assert!(document(8, &v).is_err());
        v["current_run"] = json!(4);
        v["runs"] = json!([{"run_id":4,"revision":1,"round":0,"assignments":[{"round":0,"stage":"gate","launch_id":9,"state":"settled","process_state":"running"}],"results":[{"launch_id":9,"payload":{"kind":"proof","result":{"checks":[]}}}]}]);
        let doc = document(7, &v).unwrap();
        for expected in [
            "RECHECK",
            "Process: running",
            "stop receipt pending",
            "Acceptance pending",
            "No check receipts",
        ] {
            assert!(doc.contains(expected), "{doc}");
        }
    }

    #[test]
    fn review_findings_and_receipts_remain_distinct() {
        let v = json!({"work_id":7,"revision":1,"current_run":4,"runs":[{"run_id":4,"revision":1,"round":1,"acceptance_id":12,"assignments":[{"round":1,"stage":"reviewer","launch_id":9,"stop_event":11}],"results":[{"result_id":8,"launch_id":9,"payload":{"kind":"review","result":{"verdict":"rework","findings":[{"id":"F1","reason":"incorrect exit","evidence":"exit 1"}]}}}]}]});
        let doc = document(7, &v).unwrap();
        for expected in [
            "Acceptance receipt 12",
            "Source review: rework",
            "not runtime proof",
            "Finding F1",
            "Evidence: exit 1",
        ] {
            assert!(doc.contains(expected), "{doc}");
        }
    }
}
