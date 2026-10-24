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
Claim idempotency also includes the maximum historical run ID from the selected full
work snapshot (zero initially). Concurrent/retry snapshots of one queued execution
share the key; a proved stopped/abandoned run followed by explicit same-revision
requeue gets a new key instead of replaying the abandoned run.
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
Failed reserved launcher processes retain exact bounded stdout/stderr bytes in private
`launch.stdout` and `launch.stderr` files under that root/launch attempt directory.
`launch-output.json` records exit, signal, timeout/cancellation and each stream's full
byte count/hash and truncation flag. Files are exclusive mode0600 in an owned mode0700
directory. Failure messages point to the metadata without printing captured output or
environment. Existing evidence refuses overwrite; this adds no retry or launch effect.

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

Gate dispatch records its exact shell command in a private mode0600 immutable
`gate-command.sh` under the already-owned attempt directory outside the checkout.
Only `/bin/sh 'ABSOLUTE_FILE'` is sent through the pane, bounded to512 bytes; a long
inherited PATH stays in the file and cannot be truncated by canonical terminal input.
The script retains the selected CLI/ledger environment, actual status capture and
shell-exit receipt. Existing files refuse rather than being overwritten. Starting
gate recovery observes receipts and never resends the command.

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
After both guarded exit requests, a row temporarily lacking native metadata may
remain in the existing five-second read-only absence wait. No more keys are sent;
an explicit different native identity still refuses, and timeout remains uncertain.
Pre-exit identity checks and final absence/shell/scan requirements are unchanged.
Only the core's stopped/advance APIs permit handoff or final acceptance; a changed
native identity or unknown writer remains reserved for explicit recovery. Local
acceptance grants no merge, release or publication authority.

The Herdr transport accepts exact empty stdout/stderr success only for the
effect-only `pane send-keys`, `pane run` and `tab rename` calls. Exit, deadline,
signal/cancellation, output bounds and structured errors are checked first.
Reads and ID-producing commands such as `tab create` still require JSON; malformed
or whitespace-only output does not become a successful empty response. An effect's
empty acknowledgment supplies no native identity, shell-exit or process-stop proof.

Stopped receipts also require `process_scan` with method
`local-user-session-cwd-v1`, effective UID, shell PID, true POSIX session ID, scanned count,
required integer `deleted_cwd` count (zero when none),
required integer `privileged_cwd` count (macOS zero; at most16 and at most scanned),
empty `session_remaining` and `checkout_remaining`, and evidence-bearing tagged
`exempt` entries. The shell PID must match the foreground observation. The local scanner
uses bounded `/bin/ps -ww -A -o uid=,ruid=,pid=,ppid=,pgid=,tty=,stat=,comm=`
and explicitly filters the numeric effective `uid` column against safe rustix
`geteuid`. Real UID never grants scope: setuid processes with another effective
UID are outside this profile even if their real UID matches. Conversely, a matching
effective UID still requires cwd coverage even if its real UID differs. This uses
no process-name exception. Bounded UID/ruid tokens accept unsigned u32 or Darwin's
signed i32 bit representation (`-2` becomes `4294967294`) before filtering;
malformed or out-of-range values fail closed. Safe rustix `getsid` supplies
session IDs; macOS `ps sess` is not a session-ID source. It checks the shell's
descendants, session and tty, including background process groups. Linux cwd
reads use `/proc/PID/cwd` for that filtered set; macOS uses
`/usr/sbin/lsof -a -u EUID -d cwd -F0pcfn` over the same effective UID
with a strict NUL-field parser. Canonical cwd paths match the checkout itself or
descendants using path components, so similarly named siblings do not match.
Configured and observed cwd aliases are resolved to real paths at scan time;
the checkout root itself must resolve. A kernel-reported cwd that fails with exactly
ENOENT is conservatively classified as deleted: strip one Linux ` (deleted)` suffix,
canonicalize its longest existing ancestor, then append the remaining raw components.
Empty, dot and parent components are refused. Deleted cwd inside the checkout still
blocks; deleted cwd outside is outside. It never grants a shell, root, observer or
probe exemption. `deleted_cwd` counts those classifications for surviving live rows.
An absent kernel record cannot establish deletion. Permission errors, ENOTDIR,
ELOOP, parse failures and bounds remain incomplete evidence.

On Linux only, exact EACCES for a snapshot PID with the same effective UID can use
the existing noninteractive permission through fixed absolute argv:
`/usr/bin/sudo -n -- /usr/bin/readlink -z -- /proc/NUMERIC_PID/cwd`.
Both executables must be root-owned regular executables without group/world write;
sudo must already be setuid root. No permission or sudoers changes are made.
The child has an empty inherited environment and no shell. Only exit0, empty stderr
and exactly one absolute NUL-terminated record of at most4096 bytes are accepted;
usual canonical/deleted-cwd and field bounds then apply. Effective UID and proc stat
starttime are checked before and after, in addition to paired inventory snapshots.
A known session writer with EACCES blocks without an unnecessary privileged read.
There are at most16 helper launches across all snapshot retries under the SAME15s
scan budget. `privileged_cwd` counts successful classifications for surviving live
rows; it can overlap `deleted_cwd`, so there is no sum constraint.

This uses an exclusive, fsynced per-launch `protected-cwd/pending.json` intent under
the existing private controller artifact root outside checkout. Private mode0700
directories and mode0600 files are checked for owner, type and mode. Evidence retains
the target PID/start/effective UID, fixed argv, and owned helper PID/start/argv before
any result is trusted. Known completion/reap is recorded before clearing the latch;
completed evidence remains available. A pending, partial, malformed or unknown latch
withholds another helper from both drive and inspect-stop, even if no PID now needs
the fallback. Unknown cleanup produces `process-stop-privileged-uncertain`, retains
the checkout and marks the assignment uncertain, rather than retrying every tick.
Cancellation also stops automatic reconciliation while any assignment is uncertain;
a bound native ID alone does not resolve that uncertainty or authorize another stop.
Manual reconciliation must establish exact helper identity/absence before resolving
the latch; there is no automatic root kill or latch reset.

The dedicated helper supervisor uses only nonblocking output and `try_wait`. On
deadline/read uncertainty it attempts an unprivileged SIGTERM to its own unreaped
child PID, then only a nonblocking status check; it never waits for a privileged
child to terminate and never signals an inspected process or a broad process group.
It does not use the ordinary check process runner's kill-and-wait cleanup. Sudo's
policy phase itself cannot be bounded by a wrapper, so unproved termination keeps
the latch. Ordinary denial/unavailable executables/malformed completed output are
incomplete evidence. Hosts with protected systemd user/session or similar processes
need this existing noninteractive sudo permission for full coverage; requiretty or
missing permission fails closed, with no process-name exemption.

Only the observer PID and its exact owned probe PIDs, freshly attested campaign
root foreground PIDs, and observed interactive foreground shells of the selected
workspace with no other live process in their session can be exempt. Names alone
and arbitrary observer/root descendants never grant exemptions. Every other
process with the same effective UID and a checkout cwd blocks handoff, including manual workers.
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
four-second between-items dispatch budget, at most 4096 raw ps rows across all users,
including foreign-effective-UID rows before filtering. Exceeding that raw bound makes
the scan incomplete, even if few rows belong to the observer. There is one MiB of captured
output per probe, at most 1024 entries in each evidence array, and 1024-byte
command/cwd fields. Changed snapshot pairs retry the complete inventory/cwd pair
within that one budget; attempts do not reset the deadline or reuse partial cwd
coverage. Unknown, partial, oversized, changed, unavailable or timed-out
observations produce the named `process-stop-incomplete` blocked condition and
retain the running assignment and checkout fence. A remaining cooperative writer
uses the same stable, idempotent block reason, so future ticks retry automatically.
No stopped receipt is issued until the scan is complete and remaining sets empty.
Native identity/absence or target foreground ambiguity still marks the assignment
uncertain for explicit reconciliation. The scanner never kills an unexplained process.
Transport outages and incomplete reads during stop's pane/agent/wait, foreground and
trust observations also use the retryable block. Explicit observation/exit rejection,
native drift, missing bound native identity and exit-dialog anomalies remain uncertain.
Probe execution/cleanup is confined to its own process group. The budget is checked
between OS reads; this local profile assumes responsive local process/filesystem
interfaces and does not bound a kernel call stalled on a remote filesystem.
The surrounding root/shell observations must complete within thirty seconds.
Scanning starts only after native absence. Native absence and shell foreground
are rechecked after the scan before stamping the stopped observation time.
The two inventories do not make cwd observation atomic: a process can chdir into the
checkout after its cwd read while retaining an identical ps row. This snapshot race
is a limit of the cooperative profile.

`taskr work inspect-stop WORK --launch LAUNCH` is a read-only local diagnostic.
It selects that launch's immutable revision checkout and recorded workspace/pane,
then uses the same scanner and fresh root/shell checks. JSON includes `complete`,
`remaining`, `error` and `process_scan` alongside work/launch IDs. A complete scan
may have remaining processes; an incomplete scan returns `remaining:null` and
an error. It sends no exit keys, writes no stopped receipt and never accepts a
handoff. The command must run on the checkout host, outside ledger-host RPC.
Its narrow Linux protected-read exception may create the private per-launch safety
intent/helper/completion artifacts described above. It makes no ledger writes, sends
no pane input and does not mutate inspected processes. Drive and diagnostics share
the same latch and manual-reconciliation requirement.
The legacy campaign root projection uses an empty host for a ledger-local NULL
machine. It is normalized with core host semantics only in local ledger mode
(including explicit scratch TASKR_DB). A client cannot reinterpret that empty
ledger-host value as its own host. Explicit foreign hosts and missing host fields
refuse the scan; explicit matching hosts retain the usual client-local path.
Inspection remains available for disabled campaigns and cancelled work without
enabling dispatch or changing ledger state.

This observes cooperative local processes with the same effective UID. It is not a security sandbox
or proof about another effective UID, containers/remotes, any detached process with cwd
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
Crash recovery also observes and confirms the actual exact native identity before
its first recovery prompt, then rechecks that native agent and generation. Refused
or unconfirmed observation marks uncertainty and withholds the brief. A transport
failure withholds the brief and leaves the starting assignment blocked for a later
tick; it does not fabricate a start or mark identity uncertain. Existing
pending prompt attempts are observed rather than resent.
Model processes without identity need explicit reconciliation; they cannot use
an anonymous stopped receipt. Gate shells retain their separate shell-exit proof.

Recovery of a bound starting Codex before any work-prompt attempt invokes
`herdr-codex-identity TASK LAUNCH ROOT GENERATION NAME PANE CWD` before work delivery.
It requires the exact ready Codex name/pane, no pinned conflicting identity and no
prior work-prompt attempt. Idle and done both mean ready for input in Herdr; both
require `interactive_ready: true`. Their distinction reflects UI seen state, not
identity proof or prior work delivery. Fresh done may bootstrap just like idle.
Existing native metadata needs no shell send. If a durable sent marker exists,
either state must pass matching completed-receipt validation and reuse, even when
native metadata has appeared; an absent or invalid receipt withholds work and never
resends. A matching already-pinned ID may recover through this same receipt check.
The controller supplies its selected absolute `TASKR_WORK_EXECUTABLE`, prepends that
executable's directory to PATH and preserves an explicit absolute scratch TASKR_DB;
the default context has no DB override. The adapter owns the private nonce-bound
native shell receipt, completed-receipt reuse and separate provenance; its successful
JSON must contain `native_session` and an absolute `provenance_path`. The controller
independently rereads the exact native metadata, canonical pane location and campaign
generation, then retains the normal observe acknowledgment before the work prompt.
That reread and the fresh post-observe check require interactive idle or done with
the exact returned native ID/name/pane; working, blocked, replacement, unknown or
missing interactive readiness still refuse. The same readiness check protects exit
input, separately from ownership checks that can identify a working agent.
This is identity-only native shell evidence, not a SessionStart hook or model request.
The adapter invocation is bounded to thirty seconds and one MiB per output stream.
A failed, partial, rejected, timed-out or mismatched invocation/reread leaves uncertainty
and withholds work; it is never automatically repeated from uncertain state. A
pre-invocation observation transport failure remains retryable. Disabled campaigns,
cancelled cleanup and non-Codex providers never enter this initialization path.

Automatic work requires one canonical checkout root spelling. Before every new
claim, process start and recovery prompt, the configured cwd must literally equal
Rust `fs::canonicalize` (including OS realpath case normalization). Launch panes
must report that exact literal cwd. The reserved launcher also checks physical
`pwd -P` equality before step2; Python realpath is not substituted. Known aliases,
dot spellings and case aliases block with `checkout-not-canonical` before writer
effects; revise an old contract before starting new work. No new lock/schema is
used: canonical roots make the existing literal host/cwd fence collide for aliases.
This applies to automated launch/claim paths, not a global filesystem security fence.
Inspection, stopped observations and cancellation cleanup keep canonical comparison
so old alias contracts can still clean up their actual processes. Bind-mount aliases
and nested overlapping Git roots are outside this cooperative profile.

Worker validation is limited to source/parser and repository leaf checks. Builds,
behavioral fixtures, local loop proof, Linux admission proof, compatible installation
and independent exact-source review are rollout gates owned by the campaign lead.
