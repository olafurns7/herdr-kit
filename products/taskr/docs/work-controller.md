# Local work controller and deterministic checks

Work control is off by default. Only explicitly enabled campaigns on the authenticated
local host are scanned by the existing resident daemon. Historical campaigns do not
dispatch. The controller uses the ordinary taskr CLI/RPC boundary on client hosts;
it does not open a second authority database. Agent startup runs off the observer loop.
An off-thread `taskr --json jobs reconcile` is due immediately on startup and on a
five-second scheduling interval, independently of campaign reads and agent starts.
Its first confirmed result permits campaign scanning; failed or unconfirmed results
withhold new drive dispatches. Only one reconciliation call runs at a time. The timer
does not prove a native delivery bound: RPC delays, downtime and actual route expiry
and fallback behavior require measured runtime evidence.

Periodic discovery reads only thin `work campaigns` root/configuration/generation
metadata and `work dispatch ROOT` nonterminal metadata (default 32 rows, maximum 64).
The dispatch order is retained: active/verifying before queued, then priority and ID.
Only selected IDs receive `work show` before effects or brief generation; their
revision, state and current run must still match the metadata snapshot. Claim keeps
its expected-revision CAS. Generation and runner configuration reads use thin campaigns.
The reserved launcher retains its existing full-list identity check at launch time.
Explicit `limit`, `returned` and `truncated` fields are validated. A truncated root scan
withholds dispatch and reports the bound; a partial work pass returns `complete_scan:false`
and `bounded_conditions` when metadata is truncated or its local pass budget is reached.
The daemon records those conditions in rate-limited diagnostics. A returned count is
not represented as the total number of pending items.

```sh
taskr work drive ROOT --once
taskr --json work source WORK
taskr work run-checks WORK --launch LAUNCH
```

`drive` requires root context (no worker launch), rechecks campaign generation,
workspace/checkout ownership and installed lane policy, then uses transactional claims.
One local file lock also prevents overlapping passes for the same campaign. Every
agent start uses an existing reserved task and launch. A durable dispatch binding
precedes startup. Unknown startup or prompt delivery is reconciled from native identity,
launch receipts and the prompt ledger; it never triggers a blind restart or resend.

The controller writes a UTF-8 brief and result schema for executor, reviewer or
adjudicator, then uses `herdr-launch --reserved TASK LAUNCH LANE NAME BRIEF REPORT ROOT PANE`.
This mode requires the normal explicit pane, validates the bound reservation twice,
and skips only task creation and launch creation. Ordinary launcher calls retain their
existing behavior. A reserved start requires Python 3 for a bounded socket connection
before each Herdr command. A missing or incompatible running server blocks; it is never
installed, started or restarted.

Reserved children inherit a narrowly selected CLI/ledger context. The controller
executable must be named `taskr`; its absolute executable directory is prepended to
the controller's PATH in owned model and gate shells. `herdr-launch` itself is selected
from that controller PATH. The controller passes its executable as
`TASKR_WORK_EXECUTABLE` to the reserved adapter; a direct reserved invocation can set
that variable to an absolute executable path ending in `/taskr`, otherwise the adapter
resolves `taskr` from its PATH. An explicit `TASKR_DB` is made absolute and exported;
when absent, the reserved child explicitly unsets any stale override. A private bin
directory containing the candidate `taskr` and `herdr-launch`, prepended to PATH, plus
an absolute scratch `TASKR_DB`, is the supported staged-proof injection. No other
environment is copied from the controller into a reused pane, and no global config is
changed. Default client routing still uses the user's existing server configuration.

`source` obtains the approved typed inputs through fresh RPC, then observes the actual
local clean Git HEAD and tree. Ignored build artifacts are excluded; tracked and
untracked changes refuse source identity. It uses core's canonical profile/input hashes.
An executor must commit clean owned source and submit that identity before checks.

`run-checks` runs only under the current assigned gate task/launch environment. It reads
the approved profile and execution source, verifies actual source before and after
checks, and submits an immutable `Submission::Proof` through the normal CLI. Each
approved argv is executed directly, preserving its executable spelling. Output retention
is bounded to 1 MiB per stream, while SHA-256 covers all drained bytes. A separate
tool-context artifact records the resolved executable and its content hash. Timeouts,
signals, nonzero exits, failed cases and parse failures cannot pass. Actual failed checks
settle as failed proof so the core can schedule bounded rework before any reviewer.

Case checks read only their declared relative receipt under the fresh runner-owned
`TASKR_ATTEMPT_DIR`. The file is bounded JSON of this shape:

```json
{"cases":[{"id":"approved-case-id","status":"passed"}]}
```

The IDs must match the approved set exactly. Missing, duplicate, extra, skipped, failed
or error cases fail. Symlinks, parent traversal and absolute receipt paths are refused.
Command-only checks make no case-coverage claim. No output `PASS` text is interpreted.

Heavy Linux checks use slotr's heavy/build admission with campaign, work and task
attribution. A private `_bounded-check` payload inside the unit obtains the assigned
argv again, records `SLOTR_RUN`, `SLOTR_POOL`, `SLOTR_SLOT` and `SLOTR_INVOCATION`
before executing it, and writes a machine completion receipt. This requires the
invocation-export change; installed slotr 0.3.1 is not assumed compatible. Cleanup
requests `slotr stop` and observes systemd unit inactivity/absence, MainPID zero and
empty ControlGroup. Killing the wrapper alone is never cleanup proof. Missing/late
admission or completion receipts, changed invocation, or unproved unit absence block
the assignment and retain its checkout. macOS can bypass slotr only with the explicit
campaign `no_slotr` configuration; proof records that host exception.

Result settlement does not prove agent exit. Before handing off or accepting, the
controller waits for the exact native agent to be idle, rechecks identity before exit
keys, and observes agent absence plus interactive shell foreground. It does not confirm
dialogs with Enter. Gate shells also require a successful actual shell exit receipt.
Only the core's stopped/advance APIs permit handoff or final acceptance; a changed
native identity or unknown writer remains reserved for explicit recovery. Local
acceptance grants no merge, release or publication authority.

Worker validation is limited to source/parser and repository leaf checks. Builds,
behavioral fixtures, local loop proof, Linux admission proof, compatible installation
and independent exact-source review are rollout gates owned by the campaign lead.
