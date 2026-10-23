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
root generation along with work, so the controller can refresh its authority.
Configure before claiming:

```sh
taskr work configure 42 --as 42 --generation 0 \
  --enabled on --max-active 2 --max-review-pending 1
# Equivalent JSON: {"enabled":true,"max_active":2,"max_review_pending":1}
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
    "checks": [{"id": "unit", "kind": "cases", "argv": ["checker", "--json"], "timeout_ms": 60000, "cases": ["required-case"]}]
  }
}
```

Compute the canonical checkout path on its client with `pwd -P`. The server
validates canonical absolute lexical shape and admits the named host through
existing host freshness rules; it cannot resolve paths on another host.
Source commit/tree are lowercase 40- or 64-digit object IDs; input/profile and
artifact digests are lowercase SHA-256. Scope paths are relative without `..`.
Dependencies pin work IDs and semantic revisions in the same campaign.

Lane task IDs are optional. With null/omitted task IDs, claim/reserve creates a
fresh campaign child lane and an existing launch record transactionally. Optional
reused tasks must belong to the campaign and have no live assignment elsewhere.
`name` pins the approved lane choice. Provider/model/effort are immutable
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
| list / show | campaign / work | read; show includes revision, runs, assignments, immutable results |
| revise | work | expected_revision, optional title/priority/contract |
| queue | work | expected_revision |
| claim | work | expected_revision, key; returns run_id and executor assignment/launch_id |
| reserve | run | stage: executor/reviewer/adjudicator; returns assignment |
| bind | launch | workspace, tab, pane, dispatch_key |
| started | launch | derives start proof from hook session or got/start receipt |
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
taskr work started 100 --as 42 --generation 0
taskr work advance 5 --as 42 --generation 0
```

Claim is one immediate transaction: enabled/parked/limit checks, exact accepted
dependencies, checkout reservation, run, executor lane and existing launch row.
The same campaign key and input replay the same identities. Another input or
competing claim cannot create another run/writer. A failed claim rolls back all
its rows. Claims reserve only; bind durably records dispatch intent and mirrors
pane/tab/workspace onto the lane and launch before any external start.

Assignments move reserved → starting → running → settled, or uncertain.
A starting/uncertain assignment is never restarted automatically. Without hooks,
`taskr start` or a matching `got` receipt proves the reserved launch started.
`recover` derives missing start transitions. `reconcile` refuses an unproven stop.
Stop evidence must be a launch-bound `work_process_stopped` ledger event with
`data.origin="controller"` and `data.run=RUN`. No CLI verb here produces this
receipt; the later controller must record it after observing actual process exit.
Until that producer exists, uncertain dispatches require escalation and keep the
writer reservation. Hook Stop/idle hints do not prove process exit.

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

**Runner boundary:** required checks always return `action:"check"`, with the
pinned profile and exact execution source. There is no proof-submit or mark-pass
CLI. `validate_proof(profile, source, run, revision, checker_launch, proof)` checks
exact binding/source/profile/argv/exit/case sets, but is only
structural validation; it does not attest execution. The next runner slice must
supply gate-origin attestation before proof-bearing work can ever be accepted.
An empty profile may accept after independent review/required adjudication.

Work audit events have no inbox recipient. Actionable notices use compatible
self-addressed `message` events with typed `notice` data and the current push
generation (or null). Parked roots get audit only. Self-notice exhaustion never
alarms a fictitious sender. After typed submission, routine work-bound ready/done
stay in history without waking an enabled campaign; asks/failures retain routes.
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
