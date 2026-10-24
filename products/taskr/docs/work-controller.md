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
Cancelled items whose current run still needs stop/checkout cleanup are included too.
Those items take a reconciliation-only path: stop an exactly bound process and use the
core reconciliation receipt API. They never claim, reserve, start, prompt or advance
new work. Unknown native identities and unproved gate cleanup remain uncertain with
the checkout retained.
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
Explicit nonempty `TASKR_DB` also namespaces all controller locks and artifacts by
SHA-256 of its lexically normalized absolute path. The directory contains only that
digest, so separate scratch ledgers with identical root/launch IDs cannot share locks
or stale receipts. With no override, the existing shared-ledger state path is retained.
Workspace validation always selects the configured workspace explicitly, independent
of the daemon's inherited pane context.

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
controller waits for the exact native agent to be idle or done, rechecks identity and
that ready state before exit keys, and observes agent absence plus interactive shell
foreground. It does not confirm dialogs with Enter. Gate shells also require a
successful actual shell exit receipt.
Only the core's stopped/advance APIs permit handoff or final acceptance; a changed
native identity or unknown writer remains reserved for explicit recovery. Local
acceptance grants no merge, release or publication authority.

Stopped receipts also require `process_scan` with method
`local-user-session-cwd-v1`, UID, shell PID, true POSIX session ID, scanned count,
empty `session_remaining` and `checkout_remaining`, and evidence-bearing tagged
`exempt` entries. The shell PID must match the foreground observation. The local scanner
uses `/bin/ps -ww -U UID` for PID/PPID/PGID/tty/state/command and safe rustix `getsid` for
session IDs; macOS `ps sess` is not a session-ID source. It checks the shell's
descendants, session and tty, including background process groups. Linux cwd
reads use `/proc/PID/cwd`; macOS uses `/usr/sbin/lsof -a -u UID -d cwd -F0pcfn`
with a strict NUL-field parser. Canonical cwd paths match the checkout itself or
descendants using path components, so similarly named siblings do not match.
Configured and observed cwd aliases are resolved to real paths at scan time;
failure to resolve a required cwd is incomplete evidence.

Only the observer PID and its exact owned probe PIDs, freshly attested campaign
root foreground PIDs, and observed interactive foreground shells of the selected
workspace with no other live process in their session can be exempt. Names alone
and arbitrary observer/root descendants never grant exemptions. Every other
same-user process with a checkout cwd blocks handoff, including manual workers.
A single login-shell leading dash is normalized only for supported shell
basenames (for example, ps `-zsh` versus Herdr `zsh`); exact PID, foreground group,
session and pane evidence still controls the exemption.
The scan rechecks process inventory stability, root/shell observations and target
native absence. The stopped assignment's own shell is a valid exemption. Each
shell entry records its pane, true session ID, foreground PGID and singleton
live-session PID set. Observer entries record the exact PPID chain ending at the
observer (the observer itself has a one-element chain). Root entries record the
registered root pane, foreground PGID/PID set and fresh observation time; that
root pane must differ from the stopped assignment's pane. Disappeared PIDs are
checked with signal zero; zombies are excluded from remaining writers and need
no cwd. A live non-zombie PID without cwd coverage or a validated exact exemption
makes the inventory incomplete. lsof exit1 or warnings alone are tolerated only
when that coverage requirement is met; malformed, truncated or failed probes
still fail closed.

Each OS scan has its own fifteen-second total budget, independent of the
four-second between-items dispatch budget, at most 4096 processes, one MiB of captured
output per probe, at most 1024 entries in each evidence array, and 1024-byte
command/cwd fields. Unknown, partial, oversized, changed, unavailable or timed-out
observations produce the named `process-stop-incomplete` blocked condition and
retain the checkout fence. The scanner never kills an unexplained process.
Probe execution/cleanup is confined to its own process group. The budget is checked
between OS reads; this local profile assumes responsive local process/filesystem
interfaces and does not bound a kernel call stalled on a remote filesystem.
The surrounding root/shell observations must complete within thirty seconds.
Scanning starts only after native absence. Native absence and shell foreground
are rechecked after the scan before stamping the stopped observation time.

`taskr work inspect-stop WORK --launch LAUNCH` is a read-only local diagnostic.
It selects that launch's immutable revision checkout and recorded workspace/pane,
then uses the same scanner and fresh root/shell checks. JSON includes `complete`,
`remaining`, `error` and `process_scan` alongside work/launch IDs. A complete scan
may have remaining processes; an incomplete scan returns `remaining:null` and
an error. It sends no exit keys, writes no stopped receipt and never accepts a
handoff. The command must run on the checkout host, outside ledger-host RPC.
The legacy campaign root projection uses an empty host for a ledger-local NULL
machine. It is normalized with core host semantics only in local ledger mode
(including explicit scratch TASKR_DB). A client cannot reinterpret that empty
ledger-host value as its own host. Explicit foreign hosts and missing host fields
refuse the scan; explicit matching hosts retain the usual client-local path.
Inspection remains available for disabled campaigns and cancelled work without
enabling dispatch or changing ledger state.

This observes cooperative same-user local processes. It is not a security sandbox
or proof about another UID, containers/remotes, any detached process with cwd
outside the checkout (including absolute-path writers with no retained descriptor),
or a hostile same-user writer.
Automatic workers must not detach background jobs or create external runtimes.
Tasks needing these require a named managed runtime and stronger explicit cleanup
proof; this local profile cannot automatically hand them off. Generated worker
briefs include these prohibitions and limits. Model agreement cannot certify
containment. Parent-owned controlled-child proof is required on both macOS and
Linux; parser fixtures alone do not establish OS/runtime behavior.

Reserved launches pin the actual Herdr native identity after native start and
before sending the brief, using root-authorized `work observe LAUNCH --as ROOT
--generation GEN --data {pane,native_session,observed_at}`. The launcher checks
the exact native name/pane and a single nonempty bounded session ID, then requires
a JSON acknowledgment matching launch/task/pane/native identity before prompting.
Missing/conflicting identity, rejection or unconfirmed acknowledgment withholds
the brief and retains the reservation. Observe does not fabricate a got/start
receipt or move a starting process to running. Legacy/manual launches are unchanged.
Model processes without identity need explicit reconciliation; they cannot use
an anonymous stopped receipt. Gate shells retain their separate shell-exit proof.

Worker validation is limited to source/parser and repository leaf checks. Builds,
behavioral fixtures, local loop proof, Linux admission proof, compatible installation
and independent exact-source review are rollout gates owned by the campaign lead.
