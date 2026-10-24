use super::*;
use serde::{Deserialize, Serialize};

pub const MAX_JSON: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub struct Authority {
    pub root: i64,
    pub generation: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub commit: String,
    pub tree: String,
    pub input_digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checkout {
    pub host: String,
    pub cwd: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub work: i64,
    pub revision: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    pub task: Option<i64>,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub effort: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub kind: String,
    pub argv: Vec<String>,
    pub timeout_ms: u64,
    #[serde(default)]
    pub cases: Vec<String>,
    #[serde(default)]
    pub admission: Admission,
    pub receipt_file: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub version: u32,
    pub digest: String,
    pub checks: Vec<Check>,
}
fn rework_default() -> u32 {
    2
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub intent: String,
    pub acceptance: String,
    pub scope: Vec<String>,
    pub authority: String,
    pub checkout: Checkout,
    pub base: Source,
    pub write_scope: Vec<String>,
    pub dependencies: Vec<Dependency>,
    pub executor: Lane,
    pub reviewer: Lane,
    pub adjudicator: Option<Lane>,
    #[serde(default = "rework_default")]
    pub max_rework: u32,
    #[serde(default)]
    pub require_adjudicator: bool,
    pub profile: Profile,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configure {
    #[serde(default)]
    pub enabled: bool,
    pub max_active: u32,
    pub max_review_pending: u32,
    pub workspace: Option<String>,
    #[serde(default)]
    pub no_slotr: bool,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Add {
    pub key: Option<String>,
    pub campaign: i64,
    pub title: String,
    #[serde(default)]
    pub priority: i32,
    pub contract: Option<Contract>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Revise {
    pub expected_revision: i64,
    pub title: Option<String>,
    pub priority: Option<i32>,
    pub contract: Option<Contract>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub expected_revision: i64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub expected_revision: i64,
    pub key: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Executor,
    Gate,
    Reviewer,
    Adjudicator,
}
impl Stage {
    pub(super) fn ledger_role(self) -> &'static str {
        match self {
            Self::Executor => "implementer",
            Self::Gate => "gate",
            Self::Reviewer | Self::Adjudicator => "reviewer",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Executor => "executor",
            Self::Gate => "gate",
            Self::Reviewer => "reviewer",
            Self::Adjudicator => "adjudicator",
        }
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reserve {
    pub stage: Stage,
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bind {
    pub workspace: String,
    pub tab: String,
    pub pane: String,
    pub dispatch_key: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reason {
    pub reason: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub sha256: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub source: Source,
    pub report: String,
    pub artifacts: Vec<Artifact>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub reason: String,
    pub evidence: String,
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Clear,
    Rework,
    Disputed,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub source: Source,
    pub execution_result: i64,
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Adjudication {
    pub source: Source,
    pub review_result: i64,
    pub finding_ids: Vec<String>,
    pub verdict: Verdict,
    pub reason: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Submission {
    Execution(Execution),
    Review(Review),
    Adjudication(Adjudication),
    Proof(GateProof),
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub status: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub run: i64,
    pub revision: i64,
    pub source: Source,
    pub profile_digest: String,
    pub check: String,
    pub checker_launch: i64,
    pub argv: Vec<String>,
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    pub error: Option<String>,
    pub admission: AdmissionEvidence,
    pub stdout: OutputDigest,
    pub stderr: OutputDigest,
    pub cases: Vec<Case>,
    pub artifacts: Vec<Artifact>,
}

pub fn parse<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_JSON {
        return Err(usage("work JSON exceeds 256 KiB"));
    }
    serde_json::from_str(raw).map_err(|e| usage(format!("work JSON: {e}")))
}
pub(super) fn encoded<T: Serialize>(input: &T) -> Result<String> {
    let raw = serde_json::to_string(input)?;
    if raw.len() > MAX_JSON {
        return Err(usage("work JSON exceeds 256 KiB"));
    }
    Ok(raw)
}
pub(super) fn text(s: &str, max: usize, name: &str) -> Result<()> {
    if s.trim().is_empty() || s.len() > max || s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(usage(format!(
            "invalid {name}: nonempty text up to {max} bytes required"
        )));
    }
    Ok(())
}
pub(super) fn token(s: &str, name: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err(usage(format!("invalid {name}")));
    }
    Ok(())
}
pub(super) fn hash(s: &str, lengths: &[usize]) -> Result<()> {
    if !lengths.contains(&s.len())
        || !s
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(usage("expected lowercase hexadecimal digest"));
    }
    Ok(())
}
pub(super) fn source(s: &Source) -> Result<()> {
    hash(&s.commit, &[40, 64])?;
    hash(&s.tree, &[40, 64])?;
    hash(&s.input_digest, &[64])
}
pub(super) fn unique<'a>(items: impl IntoIterator<Item = &'a str>, name: &str) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for s in items {
        token(s, name)?;
        if !seen.insert(s) {
            return Err(usage(format!("duplicate {name}: {s}")));
        }
    }
    Ok(())
}
pub(super) fn paths(paths: &[String]) -> Result<()> {
    if paths.is_empty() || paths.len() > 128 {
        return Err(usage("scope requires 1..128 paths"));
    }
    for p in paths {
        text(p, 4096, "scope path")?;
        if std::path::Path::new(p).is_absolute() || p.split('/').any(|s| s == ".." || s.is_empty())
        {
            return Err(usage(
                "scope paths must be relative without parent traversal",
            ));
        }
    }
    Ok(())
}
pub(super) fn artifacts(a: &[Artifact]) -> Result<()> {
    if a.len() > 128 {
        return Err(usage("too many artifacts"));
    }
    unique(a.iter().map(|a| a.name.as_str()), "artifact name")?;
    for a in a {
        hash(&a.sha256, &[64])?;
    }
    Ok(())
}
/// Structural proof validation only. Acceptance additionally requires a runner attestation.
pub fn validate_proof(
    profile: &Profile,
    expected: &Source,
    run: i64,
    revision: i64,
    checker_launch: i64,
    proof: &Proof,
) -> Result<()> {
    if &proof.source != expected
        || proof.profile_digest != profile.digest
        || proof.run != run
        || run <= 0
        || proof.revision != revision
        || revision <= 0
        || proof.checker_launch != checker_launch
        || checker_launch <= 0
    {
        return Err(reject("proof source/profile/run/revision/checker mismatch"));
    }
    let c = profile
        .checks
        .iter()
        .find(|c| c.id == proof.check)
        .ok_or_else(|| reject("unknown check"))?;
    if proof.argv != c.argv
        || proof.exit != Some(0)
        || proof.signal.is_some()
        || proof.timed_out
        || proof.error.is_some()
    {
        return Err(reject("check argv or exit failed"));
    }
    unique(proof.cases.iter().map(|c| c.id.as_str()), "case ID")?;
    let actual: std::collections::BTreeSet<_> = proof.cases.iter().map(|c| c.id.as_str()).collect();
    let want: std::collections::BTreeSet<_> = c.cases.iter().map(String::as_str).collect();
    if actual != want || proof.cases.iter().any(|c| c.status != "passed") {
        return Err(reject("missing, extra, failed or skipped cases"));
    }
    artifacts(&proof.artifacts)?;
    if c.receipt_file.is_some() && !proof.artifacts.iter().any(|a| a.name == "receipt") {
        return Err(reject("missing receipt artifact hash"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    #[default]
    Direct,
    Heavy,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionEvidence {
    pub method: AdmissionMethod,
    pub ticket: Option<String>,
    pub reason: Option<String>,
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionMethod {
    Direct,
    Slotr,
    HostException,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputDigest {
    pub sha256: String,
    pub bytes: u64,
    pub truncated: bool,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateProof {
    pub source: Source,
    pub profile_digest: String,
    pub dependencies: Vec<DependencyReceipt>,
    pub host: String,
    pub runner_version: String,
    pub environment: Vec<EnvironmentValue>,
    pub checks: Vec<Proof>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DependencyReceipt {
    pub work: i64,
    pub revision: i64,
    pub acceptance: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DependencyInput {
    pub work: i64,
    pub revision: i64,
    pub input_digest: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentValue {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Started {
    pub pane: String,
    pub native_session: String,
    pub observed_at: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Foreground {
    pub pid: u32,
    pub command: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub enum ProcessScanMethod {
    #[serde(rename = "local-user-session-cwd-v1")]
    LocalUserSessionCwdV1,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutProcess {
    pub pid: u32,
    pub cwd: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProcessExemption {
    WorkspaceShell {
        pid: u32,
        pane: String,
        session_id: u32,
        foreground_pgid: u32,
        session_pids: Vec<u32>,
    },
    Observer {
        pid: u32,
        observer_pid: u32,
        parent_chain: Vec<u32>,
    },
    CampaignRoot {
        pid: u32,
        pane: String,
        foreground_pgid: u32,
        foreground_pids: Vec<u32>,
        observed_at: String,
    },
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessScan {
    pub method: ProcessScanMethod,
    pub uid: u32,
    pub shell_pid: u32,
    pub session_id: u32,
    pub scanned: u32,
    pub deleted_cwd: u32,
    pub session_remaining: Vec<Foreground>,
    pub checkout_remaining: Vec<CheckoutProcess>,
    pub exempt: Vec<ProcessExemption>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stopped {
    pub pane: String,
    pub native_session: Option<String>,
    pub foreground: Foreground,
    pub agent_absent: bool,
    pub process_scan: ProcessScan,
    pub observed_at: String,
}

pub(super) fn receipt_path(path: &str) -> Result<()> {
    if path.len() > 256
        || path.is_empty()
        || std::path::Path::new(path).is_absolute()
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || path.chars().any(char::is_control)
    {
        return Err(usage(
            "receipt_file must be a bounded relative path without traversal",
        ));
    }
    Ok(())
}
pub(super) fn location_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "null"
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b":_-".contains(&b))
    {
        return Err(usage("invalid workspace/pane ID"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HashInput {
    pub profile: Profile,
    pub dependencies: Vec<DependencyInput>,
}
