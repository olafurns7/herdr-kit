use super::*;

/// Validate immutable identity separately from a check's pass/fail outcome.
/// Real failed/missing case receipts must settle so advance can schedule bounded rework.
pub(super) fn validate_gate(
    db: &Connection,
    r: &Run,
    c: &Contract,
    launch: i64,
    gate: &GateProof,
) -> Result<()> {
    let (_, exec) = execution(db, r)?;
    if gate.source != exec.source
        || gate.profile_digest != c.profile.digest
        || gate.host != c.checkout.host
        || serde_json::to_value(&gate.dependencies)? != serde_json::from_str::<Value>(&r.deps)?
    {
        return Err(reject("gate source/profile/dependency/host mismatch"));
    }
    token(&gate.runner_version, "runner version")?;
    if gate.environment.len() > 64 || gate.checks.len() > 64 {
        return Err(usage("too many gate records"));
    }
    unique(
        gate.environment.iter().map(|e| e.name.as_str()),
        "environment name",
    )?;
    for e in &gate.environment {
        text(&e.value, 4096, "environment value")?;
    }
    unique(gate.checks.iter().map(|p| p.check.as_str()), "check ID")?;
    for proof in &gate.checks {
        if proof.run != r.id
            || proof.revision != r.revision
            || proof.checker_launch != launch
            || proof.source != exec.source
            || proof.profile_digest != c.profile.digest
        {
            return Err(reject("check identity mismatch"));
        }
        let check = c
            .profile
            .checks
            .iter()
            .find(|c| c.id == proof.check)
            .ok_or_else(|| reject("unapproved check"))?;
        if proof.argv != check.argv {
            return Err(reject("unapproved check argv"));
        }
        if proof.cases.len() > 1024 {
            return Err(usage("too many cases"));
        }
        for case in &proof.cases {
            token(&case.id, "case ID")?;
            if !matches!(
                case.status.as_str(),
                "passed" | "failed" | "skipped" | "error"
            ) {
                return Err(usage("invalid case status"));
            }
        }
        for out in [&proof.stdout, &proof.stderr] {
            hash(&out.sha256, &[64])?;
        }
        artifacts(&proof.artifacts)?;
        if let Some(e) = &proof.error {
            text(e, 4096, "check error")?;
        }
        if let Some(ticket) = &proof.admission.ticket {
            token(ticket, "admission ticket")?;
        }
        if let Some(reason) = &proof.admission.reason {
            text(reason, 4096, "admission reason")?;
        }
    }
    Ok(())
}
pub(super) fn gate_passes(
    db: &Connection,
    r: &Run,
    c: &Contract,
    launch: i64,
    gate: &GateProof,
) -> Result<bool> {
    validate_gate(db, r, c, launch, gate)?;
    if gate.checks.len() != c.profile.checks.len() {
        return Ok(false);
    }
    let no_slotr = configuration(db, r.root)?["no_slotr"] == true;
    for proof in &gate.checks {
        if validate_proof(&c.profile, &gate.source, r.id, r.revision, launch, proof).is_err() {
            return Ok(false);
        }
        let check = c
            .profile
            .checks
            .iter()
            .find(|c| c.id == proof.check)
            .unwrap();
        let admitted = match (check.admission, &proof.admission.method) {
            (Admission::Direct, AdmissionMethod::Direct) => true,
            (Admission::Heavy, AdmissionMethod::Slotr) => proof.admission.ticket.is_some(),
            (Admission::Heavy, AdmissionMethod::HostException) => {
                no_slotr && proof.admission.reason.is_some()
            }
            _ => false,
        };
        if !admitted {
            return Ok(false);
        }
    }
    Ok(true)
}
