# Inbox fencing: bounded safety package (execution passed, review pending)

This checks a small TLA+ model with TLC and replays selected paths through the
real taskr CLI and SQLite. It is not a proof of the Rust implementation. The
source/action map is `manifest.json`; its source pin is
`a21f83afc3fe2082f3da980f1d93e6992f804f64`. Parent execution of package `9af0ea5`
in frozen checkout `9af0ea5` passed all 25 exact cases and six driver self-checks.
The retained command/evidence inventories and tool hashes were checked;
**independent execution review remains pending**. The nineteen covered blobs
also match `0e99765`; this does not extend execution proof to its uncovered code.

[Source correspondence](SYNC.md) explains every covered change from `71d4146`
to `a21f83a`, why model/driver logic stays unchanged, and the repaired map anchors.
Portable receipts and tool hashes are in `last_execution` and `tool_identity`.
Historical source `71d4146`, package `d4c08a6`, frozen checkout `69f3ff9` and its
READY execution review remain separately preserved in `historical_executions`.
Each result covers its executed package bytes, not subsequent metadata edits.
Neither result proves v5 work, controller or jobs state machines.

## Run

Prerequisites: Python 3.11+ with SQLite, a working Java runtime, an existing
`tla2tools.jar`, a taskr binary built from the mapped source **with the `contract`
feature**, and Cargo/Rust plus cached locked dependencies for offline mutation
builds. The driver installs and downloads nothing. The supplied binary's source
provenance is a caller assertion; its SHA-256 is recorded, not inferred from its
version string. Java and the jar are identified by SHA-256 and Java version in
each receipt. TLC must implement the standard `tlc2.TLC` CLI, exhaustive BFS,
exit 12 for an invariant violation, and the English result protocol documented
in [TLC's output implementation](https://github.com/tlaplus/tlaplus/blob/master/tlatools/org.lamport.tlatools/src/tlc2/output/MP.java)
and [exit status implementation](https://github.com/tlaplus/tlaplus/blob/master/tlatools/org.lamport.tlatools/src/tlc2/output/EC.java).
No jar version or unexecuted source pin is represented as a previously tested tool.
Executable invocation keeps the requested path and name, including a `cargo`
symlink to a multicall shim; validation and SHA-256 hashing follow its target.

Use an independent disposable source copy outside the editing checkout. Its
covered files must match the manifest. Put the source commit (above), followed
by a newline, in `.inbox-proof-disposable` at that copy's root. Do not use a
shared worktree, symlinks, or hardlinked covered files. That marker explicitly
allows three temporary mutations and offline builds in the copy. Do not edit
the copy concurrently. For example, after provisioning tools and the source copy:

```sh
python3 -B products/taskr/verification/inbox/verify.py \
  --taskr /tmp/inbox-tools/taskr-contract \
  --tlc-jar /tmp/inbox-tools/tla2tools.jar \
  --java /tmp/inbox-tools/java \
  --cargo /tmp/inbox-tools/cargo \
  --mutation-source /tmp/inbox-source \
  --out /tmp/inbox-evidence
```

All example paths and identities are synthetic. Paths must exist except `--out`,
which must be fresh and outside the editing checkout. Add `--doctor` for read-only
source/tool/source-copy checks and `java -version`; proof cases remain unrun in
that mode. Doctor names missing prerequisites in `result.json`, including a
missing disposable copy. It cannot establish that offline build dependencies
are cached until a build is attempted. A default taskr build blocks the expiry
case because it deliberately ignores `TASKR_FROZEN_NOW`.

Full verification returns 0 only if **every** case passes, 1 for a failed check,
and 2 for blocked prerequisites, source drift, timeout, or incomplete coverage.
There is no model-only success mode. Missing `--mutation-source` leaves real
mutation cases blocked even if model and baseline CLI cases pass. Source drift
prevents model and CLI execution; verify mode never refreshes hashes. Do not
use `python -O` or `PYTHONOPTIMIZE`; the driver rejects disabled assertions.

## Policy, bounds, and atomicity

The named policy is `ASK_ONLY`: one open local root consumes the fixed inbox
`got`, `ready`, `ask` with `--for ask`, no `--from`. The ask is required work;
got is store-coalescible and ready is deliberately excluded by this policy.
A compliant consumer handles an ask before calling ack. The implementation
cannot establish that external handling happened; `Handle` is an explicit
environment assumption. Never apply this policy's promise to an arbitrary
filter or to a consumer that acks work without handling it.

The bounds are two consumers (each initially plain and able to take a fenced
lease), three pre-existing ordered events, three acquisitions/fences, two
identity generations (0 and 1), and three clock values (0, 1, 2). A lease lasts
two abstract time units; deadline equality is expired. Renewal extends the
deadline from the current time. All bounds are in the model, not a search-depth
cutoff. TLC completes the reachable graph; no fairness or liveness is claimed.

Lease authority is evaluated at the transactional guard's linearization point.
Time advances between modeled transactions. This does not prove that a real
transaction's final wall-clock commit timestamp precedes lease expiry: the
clock can advance during SQL execution. Callers retain their own pinned claims;
this is not isolation from a malicious caller that obtains a current capability.
The holder label itself is not checked by `holding`; fence, binding and host are.

`Take`, `Renew`, `Release`, `Offer`, and `Ack` each correspond to a single store
transaction (`BEGIN IMMEDIATE` through commit). Offer and the CLI filter ack
are separate actions and can interleave with takeover, expiry, and adopt.
`Handle` and `Crash` are outside SQLite; crash discards only the caller's local
offered-event observation, leaving the persisted pending event intact. Replay
can offer a pending event again, including to a successor. `Tick` abstracts
the wall clock; `Adopt` abstracts the atomic generation change. Ghost `takes`
tracks acquisition identity independently of the mutable fence; otherwise NC2
could reuse a fence and fool the property that is supposed to detect reuse.

Properties:

- `AckAuthority`: a committed ack used a live, unexpired, current-generation
  capability from the current acquisition, or a plain caller with no live lease.
- `RequiredHandled`: advancing the cursor never crosses an unhandled ask under
  `ASK_ONLY`. It does not forbid intentional skips of non-required events.
- `CursorShape` and `TypeOK`: a pending event is the next event and all modeled
  values remain within their bounds.

Forced takeover of an unexpired lease is legal and normally increments the
fence. Physical delivery may repeat. The model does not assert exactly-once
handling, exactly-once external effects, or eventual progress.

## Exact cases and negative controls

| Case ID | Required result |
| --- | --- |
| `SYNC` | Covered source and verification hashes match the maintained manifest |
| `DOCTOR` | Existing Java, jar containing `tlc2.TLC`, and taskr executable identified |
| `MODEL-BASELINE` | Complete BFS, nontrivial counts, all four invariants hold |
| `MODEL-NC1` | Removing fence equality yields `AckAuthority` counterexample |
| `MODEL-NC2` | Removing repeat forced-take fence increment yields `AckAuthority` counterexample |
| `MODEL-NC3` | Coalescing a required ask yields `RequiredHandled` counterexample |
| `REACH-*` | A real multi-state trace violates deliberately false `WitnessNever` |
| `CLI-STALE` | Live takeover increments fence; old ack rejected without cursor/lease writes; successor replays and commits |
| `CLI-REQUIRED` | Excluded ready skipped; required ask remains pending and replays until explicitly handled and acked |
| `CLI-EXPIRY` | Ack at the exact lease deadline rejected without cursor/lease writes |
| `CLI-GENERATION` | Ack with old adopt generation rejected without cursor/lease writes |
| `RUST-NC1`, `RUST-NC2` | Real stale consumer ack commits after live takeover in independently mutated code |
| `RUST-NC3` | Real filtered wait loses the unhandled required ask in independently mutated code |

The twelve `REACH-*` suffixes are `forcedTakeover`, `fencedAck`, `plainAck`,
`replay`, `expiryReject`, `generationReject`, `staleReject`, `filterAck`,
`coalesce`, `crash`, `renew`, and `release`. Witness runs retain the real safety
invariants: another violation does not satisfy a reachability check. They prevent
an impossible action or a disabled path from silently passing as coverage.

Shortest NC1 trace: consumer A takes fence 1; got coalesces; the excluded ready
(event 2) is offered; B force-takes the live lease with fence 2; A's stale
filter acknowledgement advances the cursor. NC2 has the same shortest trace,
but takeover reuses fence 1, and the ghost acquisition identity detects reuse.
Both expose the same cursor authority defect for filter acknowledgements.
NC3's shortest trace skips ready by filter ack, then incorrectly coalesces the
required ask and advances the watermark without handling it. These
BFS traces are retained in the receipt's `command-004.log`, `command-005.log`,
and `command-006.log`.

The CLI reproductions are fixed supported transaction schedules, not an automatic
translation of arbitrary TLC traces. NC1/NC2 CLI replay skips ready, offers and
handles the ask, then attempts a stale ack after takeover, exercising the same
cursor authority defect. The CLI replay does not manufacture database rows: it
creates a root, worker, launch record, ready and ask through taskr. A synthetic
`handled.json` records the external handling boundary; SQLite is read-only to
the harness. `taskr launch` records metadata and starts no process.

Each Rust mutation is applied alone to its named source anchor, built using
`cargo build --offline --locked -p taskr --features contract` into the evidence
directory, and restored in `finally`. NC2 keeps fence 1 on initial acquisition
and removes increment on later forced takes; initial fence 0 would fail CLI
validation before reaching the target bug. No failed build, parse error, wrong
invariant, missing trace, or timeout counts as a negative control.

## Evidence, cleanup, and limits

`result.json` persists pass/fail/blocked for every exact case. `commands.json`
records argv, working directory, exit, elapsed time and log hash. Each model
case retains its copied model/config and stdout trace. `manifest.json`,
`package-hashes.json` and `evidence-hashes.json` bind inputs and retained evidence;
the inventory excludes rebuildable Cargo output and TLC state caches. Java,
jar, supplied taskr and built mutants have binary hashes in receipts; doctor
also hashes the Cargo shim target when mutation prerequisites are present. Tool
output and non-vacuous state counts are required, not merely exit 0.

The current successful receipt identifies all 83 command logs and 176 retained
evidence entries. Its result, commands, package inventory, evidence inventory and
original manifest are pinned by SHA-256 in `last_execution.receipt_sha256`.
`self-checks-and-proof.log` is the portable label for the combined parent build,
six-self-check and proof receipt. Independent execution review is pending; no
current READY review or review hash is asserted. The earlier successful execution
and its READY review hash remain under `historical_executions`. The artifacts
remain externally retained; metadata contains no machine paths, host identities
or task IDs. Receipt hashes bind each original run and its exact package, not
subsequent documentation/metadata edits or later uncovered source changes.

Every CLI fixture gets a fresh HOME, TASKR_DB, temporary directory and an allowlisted
environment. A failing synthetic `herdr` executable and absent scratch socket
prevent reaching the real Herdr daemon. There is no network service or live
ledger dependency. Child commands have bounded timeouts; timeout/interrupt kills
their owned process group, including compiler children. After draining child
output, `KeyboardInterrupt` propagates through source restoration and evidence
finalization; Ctrl-C stops the run rather than starting later cases. Evidence, fixture
ledgers and build output remain for inspection after commands exit. Remove only
the chosen evidence directory and disposable source copy when finished. A hard
kill or host crash can interrupt source restoration: discard that disposable
copy, never copy its mutated files back to the editing checkout.

Unmodeled/unimplemented cases: dynamic append schedules, arbitrary event kinds,
PR/hint/receipt coalescing beyond `got`, `--from`, plain wait advisory markers,
task closure, worker relaunch (root generation is modeled instead), host changes,
integer overflow, ack-already idempotency, delivery-attempt receipts, dispatch,
filesystem writers, cross-host transport, native MCP, and future workflow/result
controllers. Baseline CLI has deterministic expiry and root-generation replay;
model crash/replay and renew/release do not have separate CLI replay cases.
This package certifies none of these omissions. No automatic full-system
acceptance, release, publication, or repository gate integration is provided.

## Maintenance

On any source-sync failure, stop using old proof results. Read the changed code,
all writers of cursor/lease/generation state, and the action map. Decide whether
the verification drifted or the code has a bug; do not weaken a property to bless
an unexplained implementation failure. Update model transitions when atomicity
or policy changes, update CLI fixtures and mutation anchors, then manually update
covered hashes and source commit in the manifest. Do not mark execution confirmed
or move `last_successful_source_commit` until the complete suite and all negative
controls pass on the new source. `last_successful_source_commit` now records
`a21f83a`; the historical `71d4146` result stays separately retained. Keep
execution outcome separate from independent review status.
A candidate covered source pin may advance after that correspondence review,
with execution explicitly pending. Rerun before advancing the successful source
pin or claiming proof for a later source revision.

After editing verifier/model/config, maintain `verification_files` hashes too.
Run driver self-checks with `python3 -B -m unittest discover -s
products/taskr/verification/inbox -p 'test_*.py'`, then follow the full command
above. Tests of the receipt parser are not formal or implementation proof.
Read every TLC result as a shortest story in code terms; compare its atomic
steps to the real transaction schedule. The pinned pstack `/statechart` guidance
can help human review, but is not installed, copied, or invoked by this package.
The model, driver and documentation here are original project files.
