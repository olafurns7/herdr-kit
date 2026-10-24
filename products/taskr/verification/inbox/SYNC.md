# Integrated v5 source correspondence (execution passed, review READY)

Inspected source: `a21f83afc3fe2082f3da980f1d93e6992f804f64`.
Previous source and historical successful execution: `71d4146fdbe99a67c0c698877478bc7c796cc360`.
Parent execution of package/frozen checkout `9af0ea5` passed all 25 exact cases
and six self-checks against the inspected source map. Command/evidence inventories
and retained tool/binary hashes were checked. Independent correspondence/execution
review is READY for source `a21f83a`, package `9af0ea5` and run 3; metadata
`6da0a9a` was found consistent. Historical execution, tool hashes and READY review remain separately
preserved in `historical_executions`; last successful source now records `a21f83a`.

## Every covered change

All nineteen covered file blobs were compared between those exact commits.
Five changed; fourteen are byte-identical. This is a correspondence review,
not a checksum-only acceptance or migration/runtime test.

| Changed file | Pinned source evidence | Effect within the modeled scope |
| --- | --- | --- |
| `taskr-core/src/schema.rs` | version/fingerprint at 51–53; migration at 81–131; updated migration test expectations | v5 adds work/jobs DDL and compatibility columns on work tables. Existing consumer lease definition at 37–50 and base task/event definitions are unchanged. Migration remains one immediate transaction. Fresh initialized root state still matches `Init`; migration preservation is not a modeled action. |
| `taskr-core/src/store/mod.rs` | exports at 14,19; transaction at 96–101 | Adds `jobs` and `work` module exports. `transaction` is byte-equivalent apart from its two-line shift: immediate transaction, closure, commit. Repair `TransactionBoundary`/`RejectAck` anchors; no transition split or guard move. |
| `taskr-core/src/store/worker.rs` | identity resolution at 12–89; ready/done recipient choice at 243–252 | `resolve_inner` is unchanged. `write` now omits a ready/done recipient for an enabled live work assignment. Ask routing is unchanged. The replay's fresh legacy worker has no work assignment, so its ready still reaches the root. Fixed pre-existing `got,ready,ask` remain a valid `ASK_ONLY` input; active-work suppression is excluded. |
| `taskr-core/src/store/orch.rs` | launch guard at 188–196; close guard at 280–288 | Refuses replacing or closing a work-owned process without reconciliation. These guards do not write inbox positions or consumer leases. The replay's ordinary fresh lane has no assignment, so launch setup still passes. Root generation/adopt behavior is unchanged; closure and relaunch remain excluded. |
| Root `Cargo.lock` | slotr package version row; matching `products/slotr/Cargo.toml:3` | Only slotr 0.3.1 becomes 0.3.2. No dependency, checksum or toolchain change; no inbox/model transition changes. This is release preparation, not installation or publication. |

Crate paths above are under `products/taskr/crates/`. Source commit `9f25eaf` already
precedes the inspected target and pins the observed combined v5 schema fingerprint;
`schema.rs` is identical at those two commits. No production fingerprint is edited.

The unchanged covered files are root `Cargo.toml` and
`rust-toolchain.toml`; `products/taskr/schema.sql`; both taskr and taskr-core
`Cargo.toml` files; taskr-core `src/lib.rs` and `src/store/{consumer,inbox,delivery}.rs`;
taskr CLI `src/write/{mod,inbox,handover}.rs`; and
taskr `tests/consumer.rs` and `tests/hook/support.rs`.
Thus the three exact Rust mutation anchors, CLI filter/ack split, fence/generation
checks, exclusive expiry and replay/pending behavior did not change.

## Supporting paths and assumptions

Inspected the pinned work and jobs DDL reached from `schema::migrate`. They add
their own tables/indexes and foreign keys; they add no trigger or update of
existing inbox positions or consumer leases. Inspected `work/results.rs:299–301`:
`quiet` is a read-only existence query over assignments, active runs and enabled
campaigns. The replay creates none of these, so quiet is false. The new work/jobs
production modules contain no direct writes to `acked_event_id`,
`pending_event_id` or `consumer_leases`; test fixtures are separate.

These supporting paths were read for correspondence, not added to the nineteen
source-sync covered file hashes. No proof of their work/route state machines is
claimed. The model assumes an initialized open local root, same host, fixed
pre-existing events and no active work-owned producer suppression or native route.
External handling before required ack remains a caller assumption. Lease validity
is at the transactional guard, not a wall-clock guarantee about final commit time.

The final source pin includes the inspected slotr version bump after the initial
`8e4b1f2` correspondence candidate. All other eighteen covered blobs match that
candidate. Supporting work/jobs DDL and `quiet` are unchanged. Later supporting
module diffs concern jobs receipt/fallback behavior and work validation/row typing;
they add no protected cursor/lease writer and are outside the unassigned fixture.
Their state machines remain excluded; the slotr manifest is not added to coverage.

## Maintained correspondence and reviewed execution

Only the source pin/hashes, supporting action-map anchors/assumptions, documentation
and verification-file hashes are maintained. `Inbox.tla`, `Inbox.cfg`, `verify.py`
and `test_verify.py` remain byte-identical: all 25 cases, twelve witnesses, three
named model/Rust mutations, no-vacuity checks, executable symlink handling and
Ctrl-C cleanup are preserved. Their logic needs no change for the inspected
inbox/root/legacy-fixture subset.

The parent executed the complete case set at frozen checkout `9af0ea5`, with
package files exactly at that commit. Baseline completed 17,340 distinct states
and 98,941 generated states with zero queued; all model negative controls, twelve
witnesses, four CLI schedules and three real Rust mutants passed their exact
criteria. All 83 command log hashes and 176 evidence inventory hashes match.
The nineteen covered blobs also match `0e99765`, without repinning or claiming
execution of its uncovered changes. No model/build was rerun to maintain metadata.

The READY review is separately pinned by its source/package/metadata commits and
report hash. The copied self-check/proof log is a supplementary receipt; the
original 176-entry proof inventory is unchanged. Source drift still refuses a run;
hashes are not auto-refreshed. Uncovered modules may change independently, so each
run must name its exact complete checkout and binary provenance. Later metadata
edits are outside the executed package; hashes bind the retained original bytes.

This covers bounded inbox safety, not the controller, v5 acceptance/dispatch,
native delivery, filesystem writers, cross-host behavior, migration/release
correctness or deployment. All existing bounds and exclusions remain in force.
