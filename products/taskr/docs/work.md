# Durable local work

`taskr work` keeps backlog, contract revisions, runs, launch-bound assignments,
results and acceptance receipts in the existing ledger. It never launches a
process and acceptance grants no merge, deployment or publication authority.
Runtime schema v5 adds work tables; the frozen Go `schema.sql` is unchanged.
Do not install an intermediate binary against a live ledger.

Commands return a complete JSON object (`j1 JSON`, or untagged with `--json`).
Unknown fields, duplicate/unknown flags, malformed JSON and invalid transitions
are refused. JSON is bounded to 256 KiB. `--file` captures bounded UTF-8 JSON on
the invoking client before RPC; a server invocation with `--file` is refused.
No server-side path is used as result evidence.

## Authority and configuration

Every controller mutation requires `--as ROOT --generation GEN`, where GEN is
`store::delivery::generation(db, ROOT)` (the latest adopt event ID, initially 0).
The store checks root identity, authenticated caller host and generation in the
mutation transaction. A stale generation exits 6; stop and refresh authority
before another tick. Only the campaign or existing designated hub can add,
revise, cancel or configure its work. Other controller mutations require the
campaign itself. Workers submit only as current `TASKR_TASK`/`TASKR_LAUNCH`.

Campaigns start disabled. `list` returns configuration, parked state and current
root generation along with work for detail consumers. Daemon scans use the thin
`campaigns` and `dispatch` reads below.
Configure before claiming:

```sh
taskr work configure 42 --as 42 --generation 0 \
  --enabled on --workspace w1 --max-active 2 --max-review-pending 1
# Equivalent JSON: {"enabled":true,"max_active":2,"max_review_pending":1,"workspace":"w1","no_slotr":false}
taskr work add --as 42 --generation 0 \
  --data '{"campaign":42,"title":"A backlog item"}'
taskr work list 42
taskr work show 1
```

Limits are 1..64. Lower limits prevent new claims without cancelling existing
assignments. Disabled and parked campaigns cannot claim. Creating a backlog
item does not assign a lane or reserve a checkout.

## Contract

Queue requires a complete typed contract. `revise` accepts `expected_revision`
and optional `title`, `priority`, `contract`. Title/priority edits preserve the
semantic revision; a changed contract creates a new immutable revision.

An example contract (replace IDs, digests and client checkout identity):

```json
{
  "intent": "Implement the scoped change",
  "acceptance": "Independent review is clear and required checks pass",
  "scope": ["src"],
  "authority": "Local edits only",
  "checkout": {"host": "host-a", "cwd": "/workspace/project"},
  "base": {
    "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "tree": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "input_digest": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "write_scope": ["src"],
  "dependencies": [],
  "executor": {"task": null, "name": "executor-lane", "provider": "example", "model": "executor", "effort": "medium"},
  "reviewer": {"task": null, "name": "review-lane", "provider": "example", "model": "reviewer", "effort": "medium"},
  "adjudicator": null,
  "max_rework": 2,
  "require_adjudicator": false,
  "profile": {
    "id": "unit", "version": 1,
    "digest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "checks": [{"id": "unit", "kind": "cases", "argv": ["checker", "--json"], "timeout_ms": 60000, "cases": ["required-case"], "admission": "heavy", "receipt_file": "cases.json"}]
  }
}
```

Compute the canonical checkout path on its client with `pwd -P`. The server
validates canonical absolute lexical shape and admits the named host through
existing host freshness rules; it cannot resolve paths on another host.
Source commit/tree are lowercase 40- or 64-digit object IDs; input/profile and
artifact digests are lowercase SHA-256. Compute profile/input hashes with the
canonical helpers below; arbitrary digest labels are rejected. Scope paths are relative without `..`.
Dependencies pin work IDs and semantic revisions in the same campaign.

Lane task IDs are optional. With null/omitted task IDs, claim/reserve creates a
fresh campaign child lane and an existing launch record transactionally. Optional
reused tasks must belong to the campaign and have no live assignment elsewhere.
`name` pins the approved launcher lane choice, independently of ledger roles.
Executor tasks use the ledger role `implementer`; reviewer/adjudicator tasks use
`reviewer`. Optional reused tasks must match that role. Provider/model/effort are immutable
snapshots. Reviewer and adjudicator need independent tasks and distinct
models. The later launcher must validate these choices against
its actual lane policy before dispatch. This API is not a model-policy bypass.
`max_rework` defaults to 2, bounded 0..8. Check IDs and case IDs are unique;
checks have approved argv (1..128 arguments) and timeout (1..3600000 ms).

## Lifecycle and IDs

| Verb | ID | JSON input / result |
| --- | --- | --- |
| configure | campaign root | enabled, max_active, max_review_pending (or flags above) |
| add | none | campaign, title, optional priority/contract; returns work_id |
| campaigns | none | thin enabled root/configuration/generation snapshots for authenticated host |
| dispatch | campaign | thin nonterminal work summaries; --limit defaults 32, bounded 1..64 |
| list / show | campaign / work | compatibility/detail reads; show includes revision, runs, assignments, immutable results |
| revise | work | expected_revision, optional title/priority/contract |
| queue | work | expected_revision |
| claim | work | expected_revision, key; returns run_id and executor assignment/launch_id |
| reserve | run | stage: executor/reviewer/adjudicator; returns assignment |
| bind | launch | workspace, tab, pane, dispatch_key |
| observe | launch | {pane,native_session,observed_at}; pins actual launcher-observed identity before prompt, without start evidence |
| started | launch | derives launch receipt; optional {pane,native_session,observed_at} explicitly binds native identity |
| stopped | launch | {pane,native_session,foreground:{pid,command},agent_absent,process_scan,observed_at}; required scan below |
| block | work | {reason}; identical named condition is a no-op |
| inputs | work | canonical profile/dependency/input/checkout snapshot |
| hash | none | {profile,dependencies:[{work,revision,input_digest}]} -> canonical digests |
| uncertain | launch | reason |
| reconcile | launch | optional stopped_event; empty object derives recorded start |
| submit | launch | launch-bound typed submission below; returns immutable result_id/digest |
| advance | run | computes reserve/review/check/rework/adjudicate/accepted/escalation |
| cancel | work | reason; active dispatches retain checkout reservation |
| recover | campaign | derives missing starts and returns durable work for reconciliation |

```sh
taskr work revise 1 --as 42 --generation 0 --file revision.json
taskr work queue 1 --as 42 --generation 0 --data '{"expected_revision":2}'
taskr work claim 1 --as 42 --generation 0 \
  --data '{"expected_revision":2,"key":"claim-unique-1"}'
# Use returned launch_id; never infer it from work_id or run_id.
taskr work bind 100 --as 42 --generation 0 \
  --data '{"workspace":"w1","tab":"w1:t2","pane":"w1:p2","dispatch_key":"dispatch-100"}'
# External controller starts only after bind commits, exporting the reserved task/launch.
# Read actual native identity from the successful agent start before sending its brief.
taskr work observe 100 --as 42 --generation 0 \
  --data '{"pane":"w1:p2","native_session":"<actual native identity>","observed_at":"<fresh RFC3339>"}'
# Only send the reserved prompt after observe succeeds; do not guess an identity.
# A got/start or hook receipt is still required to establish running.
taskr work started 100 --as 42 --generation 0
taskr work advance 5 --as 42 --generation 0
```

Claim is one immediate transaction: enabled/parked/limit checks, exact accepted
dependencies, checkout reservation, run, executor lane and existing launch row.
The same campaign key and input replay the same identities. Another input or
competing claim cannot create another run/writer. A failed claim rolls back all
its rows. Claims reserve only; bind durably records dispatch intent and mirrors
pane/tab/workspace onto the lane and launch before any external start.

Result settlement and process state are separate. Assignments retain `state`
(reserved/starting/running/settled), with independent `process_state`
(reserved/starting/running/uncertain/stopped), `native_session`, `stop_event`,
`identity_observation` and `stopped` receipt data. A result never proves process
exit. A starting/uncertain process is never automatically restarted.

`observe` is root/host/generation authorized and pins actual Herdr native identity
on the bound starting dispatch before the reserved launcher sends its brief. It
stores the root identity observation and initializes the launch session reference
when absent; it emits no got/start receipt and does not mark the process running.
Matching persisted/bound identity is idempotent without changing process/result
state; conflicting identity rejects. Current task launch, immutable pane, host
and fresh time guards apply. Missing identity withholds the prompt and retains
the uncertain reservation. Gate shells do not use this operation. Legacy/manual
launches are unchanged. An old identity-less model run needs explicit
reconciliation; no anonymous model stop is allowed.

`started` can derive a launch-bound got/start or hook receipt. A receipt alone
never invents a native identity. Model stages must have either a hook session
or an explicit root `started --data {pane,native_session,observed_at}` observation
before stop can be recorded. Pane must equal immutable dispatch; any existing
native identity must match. For running/uncertain assignments, late explicit
binding requires the exact persisted launch session reference, current task launch,
immutable dispatch/launch pane and launch host. Missing or conflicting stored
identity is rejected; no identity is guessed from a receipt. Binding preserves
uncertainty and settled results; only a complete stop observation ends process
uncertainty. Stopped assignments cannot bind again. Gate shell stages have null
native_session.

`stopped` is root/host/generation authorized. Its JSON includes exact bound pane
and native session, `agent_absent:true`, foreground `{pid,command}` identifying a
positive PID and a known shell (sh/bash/zsh/fish/dash/ksh/nu), a required
`process_scan`, plus a fresh RFC3339 observed_at (within 60 seconds, not before
launch or in the future). Changed native identity, mismatched pane and worker
authority are refused. Identical stop replay is idempotent; conflicting
observations cannot replace the immutable receipt. The controller must obtain
these observations from real teardown; the store does not inspect OS processes.

A native agent disappearing and an interactive foreground shell do not establish
that background writers stopped. The required scan has this exact shape:

```json
{"method":"local-user-session-cwd-v1","uid":501,"shell_pid":123,"session_id":123,"scanned":40,"deleted_cwd":0,"privileged_cwd":0,"session_remaining":[],"checkout_remaining":[],"exempt":[{"pid":123,"reason":"workspace-shell","pane":"w1:p2","session_id":123,"foreground_pgid":123,"session_pids":[123]},{"pid":456,"reason":"observer","observer_pid":456,"parent_chain":[456]}]}
```

`session_remaining` entries are `{pid,command}`; `checkout_remaining` entries are
`{pid,cwd}`. Both arrays must be empty to record stop. All PID/session IDs are
positive u32; shell_pid equals foreground.pid; uid is u32 (including root UID0).
`scanned` is 1..65536. Required `deleted_cwd` is an integer count in
0..scanned (bounded by65536), with no default. It records how many process cwds
were classified from kernel paths after canonicalization returned exactly ENOENT;
it does not exempt those processes. A deleted cwd classified inside the checkout
still blocks, while one outside remains outside. The scanner strips Linux's
trailing ` (deleted)` once, canonicalizes the longest existing ancestor to resolve
aliases, and appends only suffix components without empty, dot or parent entries.
The checkout root must still canonicalize. Other errors or malformed paths remain
incomplete scans; absolute-path writers remain outside this cooperative profile.
Missing, negative, noninteger or greater-than-scanned counts are rejected.

Required `privileged_cwd` is an integer count in0..min(scanned,16), with no
default. It audits Linux protected-cwd reads; macOS always produces0. It may
overlap `deleted_cwd`, so their sum is not constrained. Missing, negative, null,
noninteger, overflow or out-of-bound counts are rejected.

The Linux scanner may read same-effective-UID EACCES cwds only with the existing
noninteractive privilege, using fixed argv
`/usr/bin/sudo -n -- /usr/bin/readlink -z -- /proc/<u32>/cwd`, never a shell or
user-supplied command. The adapter grants no sudo permission, sudoers entry or
credential; requiretty or missing NOPASSWD fails closed. Hosts with protected
systemd user-session processes require that existing permission to complete a
scan. Before execution, the scanner verifies both binaries root-owned and not
group/world writable, and sudo setuid root. It requires exit0, empty stderr and
one NUL-terminated path under4096 bytes, with the stable inventory pair and PID
starttime unchanged. Already-blocking session processes need no privileged read.
There are at most16 reads within the existing15-second scan deadline; timeout,
unavailable permission or malformed output keeps the scan incomplete. This is a
cwd read, never a process-name exemption or permission grant.

Each array, including exempt, has at most1024 entries;
commands/cwd strings are nonempty and at most4096 bytes. Exempt PIDs are positive
and unique. Exemptions are a typed enum tagged by `reason`, with required bases:

- `workspace-shell`: pid, pane, session_id, foreground_pgid, session_pids. The
  pane belongs to the immutable dispatch workspace and session_pids is exactly `[pid]`.
  The stopped pane's own idle shell is valid: its PID, pane and session must
  match the top-level foreground/scan. Other idle workspace shells may also
  qualify; the controller must actually observe the singleton session.
- `observer`: pid, observer_pid, parent_chain. The chain contains positive unique
  PIDs, starting with pid and ending with observer_pid; the observer itself has
  the one-element chain `[observer_pid]`. The controller attests each PPID link.
- `campaign-root`: pid, pane, foreground_pgid, foreground_pids, observed_at. Pane
  equals the recorded campaign root pane and differs from the stopped assignment
  pane. The positive unique foreground list includes pid, and observed_at is
  fresh under the launch/time checks. For example:
  `{"pid":789,"reason":"campaign-root","pane":"w1:p1","foreground_pgid":789,"foreground_pids":[789],"observed_at":"<fresh RFC3339>"}`.

All process-group IDs are positive; chains/foreground lists have1..1024 entries.
Only the root exemption requires a different pane. Enum fields are required and
unknown fields rejected; reason-only exemptions are invalid.
Unknown fields, missing scan/fields and invalid bounds are rejected. There is no
compatibility default for this unreleased API. An invalid scan writes no receipt
and retains the checkout reservation.

The controller must perform a complete bounded same-UID inventory, including
shell descendants and every process in its true POSIX session/tty, including
background process groups. It must scan same-user cwd values and match the
canonical checkout or a path under it by component boundaries. Resolve checkout
and observed aliases at scan time; unresolved canonical paths make the scan
incomplete. Zombies are non-writers and need no cwd. The scan has its own
15-second total deadline, independent of the between-item dispatch budget; record
its measured duration. lsof exit1 or stderr warnings alone do not invalidate a
complete inventory, but every live non-zombie process still needs a cwd record or
a validated exact exemption. This detects
same-session background jobs and detached children that retain checkout cwd.
macOS `ps sess` is not a usable POSIX session ID; use getsid. Linux cwd reads use
/proc; macOS uses a bounded, strictly parsed lsof cwd inventory. Recheck vanished
PIDs; a still-live non-zombie process with unavailable cwd, permission errors,
parse errors, truncation or deadline failures makes the scan incomplete.

Exempt only exact observed idle workspace-shell PIDs, the current observer and
its exact short-lived probes, and freshly verified foreground PIDs of the
registered campaign root. Never exempt by process name or arbitrary ancestry.
Recheck native absence and foreground shell after scanning. Unknown processes
block handoff; do not kill them merely because their cwd matches. An incomplete
or unavailable scan produces no stop receipt and one named blocked condition.
Closing the final tab/workspace is not cleanup evidence.

This is trusted root-attested observation of cooperative same-user local
processes. The store checks receipt shape/identity, not the truth of the OS scan.
It is not a security sandbox or proof about other UIDs, containers/remotes,
hostile same-user writers, or any detached process with cwd outside the checkout,
including absolute-path writers without retained file descriptors. Automatic task workers must not detach background
jobs or create external runtimes. Such work needs a named managed runtime and
stronger explicit cleanup proof; no automatic handoff under this local profile.
Generated worker briefs must state these limits; model agreement cannot certify
containment.

A normal stop preserves the active run and its checkout through gate/review.
`reconcile {stopped_event}` acknowledges that recorded receipt without abandoning
work, including after acceptance. Explicit `abandon:true` requires a current
active run and all dispatched processes stopped; it returns work to backlog and
releases the checkout. Cancel also releases only after every dispatched process
is stopped. Undispatched reservations, including uncertainty without dispatch,
can be cancelled without pretending an OS process existed. Accepted work cannot
be abandoned or reverted by late reconciliation. Live work lanes cannot be
closed or their launches replaced until process stop/cancellation reconciliation.

## Submission and acceptance

Submit requires a running assignment, current launch, current contract revision,
current workflow round and unchanged dependency acceptance receipts. Results
are self-contained immutable JSON, including report text/artifact hashes;
document purge cannot remove them. Identical replay returns the original result;
changed content cannot replace a settled result.

```json
{"kind":"execution","result":{"source":{"commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","tree":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","input_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"report":"Change and validation details","artifacts":[]}}
```

Execution input digest must match the pinned contract input. Review uses
`kind:"review"` with `source`, `execution_result`, `verdict` (clear/rework/disputed)
and `findings:[{id,reason,evidence}]`. Its source and execution result must match
exactly. Clear has no findings; rework/disputed require keyed findings.
Adjudication uses `kind:"adjudication"` with `source`, `review_result`, exact
`finding_ids`, `verdict` (clear/rework) and `reason`. Only its distinct assigned
launch can submit it. An executor cannot dismiss reviewer findings.

Advance computes the next action. Rework preserves old rounds/results and stops
at the contract budget. Disputes without an adjudicator escalate. Semantic or
dependency changes reject in-flight acceptance without stopping an active
process. Completed dependents keep their receipts and show recheck required.
Acceptance revalidates pinned dependencies in the transaction and emits one
immutable receipt for the work revision. Repeated advance cannot duplicate it.

Advance orders execution → executor stop → deterministic gate proof → gate stop
→ independent review → reviewer stop → optional adjudication → all needed stops
→ acceptance. `action:"stop"` identifies its `launch_id`. Empty profiles still
require process-stop safety and independent review. Gate failure uses the same
total max_rework budget as review failure; it never reserves a reviewer first.

A gate reservation uses `stage:"gate"` and creates a fresh lane/launch with ledger
role gate and fixed provider/model/effort shell/none/none. Only its current,
running, launch-bound worker identity can submit `{kind:"proof",result:...}`.
Root/executor/reviewer JSON cannot mark a check passed. Proof results contain
source, profile_digest, exact dependency acceptance receipts, host, runner_version,
environment name/value records and `checks`. Each check records run/revision,
source/profile/checker launch, approved check ID/argv, nullable exit/signal,
timed_out, nullable error, admission evidence, stdout/stderr hashes/byte counts/
truncation, exact cases and artifact hashes. Receipt-backed checks require an
artifact named `receipt` with the captured receipt SHA-256.

Nonzero/null exit, signal, timeout, parse/missing receipt error, missing/extra/
duplicate/skipped/failed cases or wrong admission cannot pass. A failed record
is stored immutably so the stopped gate can enter bounded rework; identity drift
(source/profile/dependencies/argv/launch) is rejected. The later runner owns real
execution/attestation, clean Git verification before/after, receipt capture,
admission and process cleanup; the store enforces its assigned writer boundary.

Per-check admission is `direct` (default) or `heavy`. Proof evidence is
`{method:"direct"|"slotr"|"host_exception",ticket:null|string,reason:null|string}`.
Heavy requires a slotr ticket, or an explicitly configured campaign `no_slotr`
host exception with a reason. That exception is off by default. A nonempty case
set requires `receipt_file`, a relative path up to 256 bytes without empty/dot/
parent components. The runner resolves it only inside its own attempt directory.

Work audit events have no inbox recipient. Actionable notices use compatible
self-addressed `message` events with typed `notice` data and the current push
generation (or null). Parked roots get audit only. Self-notice exhaustion never
alarms a fictitious sender. Routine work-bound ready/done stay in history without
waking an enabled live campaign from assignment reservation onward, including
before typed submission. Leads must use work state for routine progress; do not
wait for pushed ready/done reports from these lanes. Asks/failures retain routes. Submitted and
bounded rework are audit-only. Acceptance and actionable exceptions retain notices.
Root-initiated add/revise/queue/cancel do not send self-notices; hub-to-other-root
writes still notify. `block` emits at most one notice per unchanged reason.
Existing offered events/cursors are not suppressed or advanced by this feature.
Legacy lanes retain their launch and report behavior.

## Rust API

All input structs/enums and operations are public in
`taskr_core::store::work`. For example:

```rust,ignore
use taskr_core::store::work::{self, Authority, Claim, Revision};
let auth = Authority { root: campaign, generation: snapshot_generation };
work::queue(&mut db, auth, work_id, Revision { expected_revision: 2 })?;
let run = work::claim(&mut db, auth, work_id, Claim {
    expected_revision: 2, key: "claim-unique-1".into(),
})?;
let next = work::advance(&mut db, auth, run["run_id"].as_i64().unwrap())?;
```

Mutations take `&mut Connection`, root operations take `Authority`; worker
`submit` resolves exported identity itself. Reads take `&Connection`. Typed
callers get the same bounded validation as the CLI. Errors use existing usage
(2), database (4), and rejected authority/transition (6) codes.

The checkout reservation covers upgraded work only. Symlink aliases and
independent legacy lanes are not detected; the launcher must reconcile these
before execution. This is a durable coordination fence, not process fencing
or filesystem isolation. Missed notices do not lose work: recover/list/advance
read the ledger directly.

## Canonical identity algorithms

Use `work hash --data '{"profile":...,"dependencies":[]}'`, or the public
`profile_digest`, `input_digest` and `canonical_json` Rust helpers. Profile inputs
are deserialized as typed Profile (all defaults and null optional fields become
explicit). Objects sort keys lexicographically; arrays retain declared order;
serialization is compact UTF-8 serde JSON, without whitespace or Go HTML escaping.
Digests are lowercase SHA-256 of these exact bytes:

- Profile: `{"algorithm":"taskr-work-profile-v1","profile":<typed Profile excluding digest>}`.
- Input: `{"algorithm":"taskr-work-input-v1","profile":<same content>,"dependencies":[{work,revision,input_digest},...]}`. Dependencies sort by positive work ID and cannot repeat. Their input digests come from the pinned immutable dependency contract revisions.

Commit/tree do not enter input_digest. They independently identify the actual
committed source; changing profile, pinned dependency revision or dependency input
changes the input digest. `work inputs WORK` returns all canonical inputs and
current dependency acceptance receipts. The controller-owned local `work source`
command uses this authoritative RPC snapshot and actual clean Git source on the
calling host; client-host daemons must never open a separate authority DB.
`work campaigns` returns only enabled root/configuration/generation metadata for
the current authenticated host, without work or report/history bodies. It is
bounded to 1000 roots and exposes `limit`, `returned` and `truncated`.

`work dispatch ROOT --limit N` returns only work ID, revision, state, priority,
current_run and reason for nonterminal dispatch/cleanup candidates. Default 32 and
maximum 64 rows keep even worst-case escaped reasons inside the RPC envelope. The
response includes `limit`, `returned`, `truncated` and `work`. Active/verifying
work and cancelled items whose current run still needs process-stop/checkout
cleanup precede queued work, then priority desc/ID asc. Cancelled state is
preserved; the read never reserves a launch. After cancellation cleanup makes
the run terminal, that item disappears from dispatch. The controller loads `show`
only for selected items. Existing `list`/`show` retain detail/history compatibility;
these reads do not impersonate workers or implement workflow transitions.

Add accepts optional `key`. The same authenticated root/key and canonical typed
Add input return the same work ID, including after process restart. A changed
input with the key is rejected transactionally. Omitting key retains ordinary
CLI creation behavior; the native adapter requires a key itself.

Recovery limits: revising an active contract rejects new bind/started operations
for its old semantic run; explicit stop/cancel/abandon still reconciles old owned
processes. A reused legacy launch without a work stop receipt has no safe takeover
path; use a fresh optional lane. Checkouts are compared by exact authenticated
host/path strings; filesystem/symlink aliases and independent legacy OS writers
still need launcher reconciliation. A ledger fence does not stop OS writes.
