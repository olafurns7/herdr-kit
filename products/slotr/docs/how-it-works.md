# How it works

This file explains how slotr decides who runs, when it stops a run, and what it stores.

## Admission

A request is admitted when its pool has a free slot and, for memory-gated
pools, both of these hold:

```text
MemAvailable - sum(max(0, holder.reservation_mib - holder.anon_mib)) - request.reservation_mib >= reserve_mib
PSI full avg60 <= psi_full_avg60_max
```

- In a memory-gated pool, when MemTotal can be read, reservations above MemTotal
  minus the reserve are rejected before queueing.
- Unknown anonymous memory gets zero resident credit; file cache gets none.
- Missing memory or PSI data fails closed for admission.
- The optional load gate compares load1 per logical core with
  `load1_per_core_max`. A zero load limit turns that gate off.
- Port probes check both 127.0.0.1 and ::1 when IPv6 is available. Port
  blocks across pools cannot overlap live holders. Probes are not
  reservations.
- Compatibility locks are held by the in-unit supervisor for the workload's
  life. Queued probes release the lock before sleeping, and status reads lock
  owners and probes existing files without creating them.
- A dead ticket cannot block FIFO; an inactive unit is reclaimed by the next
  active waiter or supervisor.
- Unit names are persisted before startup; unit-name collisions retry with a
  fresh admission sequence.

## Queue order and campaign caps

Within each pool, higher unsigned weights go first, then original enqueue
sequence. `level` remains an equal-valued JSON alias of `weight`. Managed exact
campaign overrides beat all legacy campaign/prefix/task matches. Every
admission refreshes all queued and running weights from one managed generation
and one legacy snapshot under the state lock. Changing weight never resets age
or stops admitted work.

Positive-weight campaigns have a hard admission ceiling of
`min(slots, campaign_cap)` (or `slots` when the cap is zero). At cap, they
hold no extra slots or memory. Weight-zero campaigns may borrow spare slots
unless another queued campaign is below its cap. Cap reductions drain naturally.

Fresh queued demand below a higher campaign's cap protects its own pool and
other memory-gated pools sharing the host budget, even when slots or memory
are short. When its queue empties, `priority.quiet_seconds` retains a handoff tail
(default 30 s). Running holders never refresh that tail; a policy entry alone
creates no hold. A ticket needs a held lock and a heartbeat within five seconds
plus three polls. Cancellation or staleness starts the quiet transition;
resuming preserves original sequence and age. Lowering weight recalculates holds.
Port or legacy-lock blockers cannot create a hold in unrelated pools or start
a quiet tail. Unrelated non-memory pools remain independent.

At `priority.fair_wait_seconds` (default 300 s), the oldest fresh eligible ticket
earns the next safe admission before weight or quiet preference. Slot shortage
reserves only its pool's next vacancy; memory shortage reserves the shared
memory domain. When memory alone blocks that fair turn, up to **eight smaller
tickets from the same pool** may backfill it. Each must fit the current budget
including the reserve and all holders' outstanding reservations. Among fitting
tickets, existing age/weight/enqueue ordering and campaign caps still apply.
The counter belongs to the head ticket and persists across polls, stale/resumed
heartbeats and changes of fair turn; completed backfill runs do not reset it.
After eight admissions, backfill stops and competing starts wait while capacity
accumulates. A head that fits always goes first. Other pools cannot backfill its
reservation. Recovery, PSI/load, slots, ports, locks and priority caps still apply.

This bounds bypass to eight admissions per queued head, rather than promising a
wall-clock start deadline: existing runs must release enough memory and a slot
before the head can start. A steady stream of small arrivals cannot extend that
bound. Status names a fitting backfill candidate `fair_turn_backfill`, includes
its head's sequence in `backfill_for`, and shows each ticket's
`backfill_admissions` and `backfill_limit`. After the limit, otherwise fitting
contenders report `fair_turn_backfill_limit`; the head keeps `memory_budget`
until it fits. No configuration knob is needed.

Ports or legacy locks defer a fair ticket without changing its age or reserving other
pools; status shows `fair_deferred`. Unknown port ownership cannot justify a
shared reservation: busy sockets in registered ranges retain pool-local slot
ordering but defer cross-pool holds until a port block is observed free. Status
reports `cross_pool_deferred`. Recovery, PSI/load, caps, ports and locks still apply.

This is a fairness floor, not a start deadline: twenty aged 15-minute gates in
a two-slot pool can take roughly ten waves (150 minutes) ahead of a later high
arrival. A 30-second tail is a tunable grace, not a cadence guarantee: a lower
job admitted at second 30 continues if higher demand arrives at second 31.

Campaign names are cooperative same-user labels, not authentication.

## Leases and idle release

Priority and campaign contention never evict an admitted run. Existing
configured lease and idle lifecycle rules remain:

- An overdue lease continues when nobody waits.
- Under contention, only an eligible holder whose release helps a fitting
  head waiter earns a warning and stop claim. Conditions are rechecked at
  grace expiry; a departing or replaced waiter cancels the claim.
- `lease.on_expiry = "warn"` never stops a holder; `off` disables this path.
- CPU-idle release requires a fitting waiter. Optional holder probes sample
  pane activity; errors clear the idle clock. `slotr touch RUN` resets idle
  clocks without extending a lease or clearing a stop claim.
- Pools with `evictable = false` disable all automatic stops, including pressure.

For finite gates intended to finish, use unlimited leases and disable idle and
holder-probe cleanup, or select the appropriate warn policy. Weight changes
cannot make a healthy, in-lease, active gate eligible for lifecycle cleanup.

## Memory pressure

Admission uses each ticket's resolved `reservation_mib`, defaulting to its
`cost_mib`. Outstanding memory is the positive remainder of that reservation
after observed anonymous memory, so growth already reflected in available
memory is not reserved twice. Lower per-kind reservations allow more concurrent
runs without lowering their unit caps. This is deliberate overcommit: the sum
of actual peaks may exceed the slice or host budget even when each run respects
its cap. Remove the optional kind reservation to restore full-cost reservations
for new tickets; queued and running tickets keep their original values.

On pressure the lowest-weight evictable started holder stops itself. Within
that weight, the newest admission goes first. A stop records live
stats, holder costs and anonymous memory, and optional lock observations.
After a pressure stop, memory-gated admission waits for the recovery interval
(`recovery_healthy_seconds`). Pools without the memory gate ignore pressure
recovery.

Pressure handling uses cached last-known weights and does not load managed
policy before selecting or signaling its victim. A malformed policy closes
new admission while existing workloads retain emergency protection. Events
include caller uid/pid and actor `watchdog` or `lease`. Automatic stops send
TERM first, then append the audit event in the hook thread while the supervisor
drives KILL escalation. Audit failures are reported there and cannot suppress
host protection.
Manual or lifecycle stops do not start memory-pressure recovery.

An automatic stop sends TERM, waits `term_grace_seconds`, then KILL; systemd
`TimeoutStopSec` adds 5 seconds. The run exits with code 75.

## A heavy pool

A pool for heavy one-shot commands (type checks, lint, tests, builds,
installs) works like any other pool. The memory budget is shared across
pools: a heavy check and a runtime count against the same available memory.
Priority and fairness act at admission. An admitted lower-weight build
finishes while a higher-weight ticket waits for a natural vacancy. Real
pressure and explicitly configured lifecycle limits still apply.

A shared legacy lock lets old `flock LOCK CMD` callers keep working during a
transition. An old exclusive caller waits for the slotr holders to leave,
then excludes every slotr waiter, which reports `legacy_lock`. This has two
costs. An old caller ignores priority, so a non-priority `flock` command
blocks a priority slotr run for its whole length. And `flock` has no
fairness, so steady overlapping shared holders can keep an exclusive caller
waiting a long time. Move callers to `slotr run` together, and read the
`legacy_lock` wait reasons in status during the transition.

The optional [flock guard](flock.md) refuses file/FD aliases of existing
`queue_only` legacy files by device/inode in the Rust checker. After silent
checker success, the native C launcher execs `/usr/bin/flock` with original argv, environment
and FDs. This cooperative PATH launcher does not alter admission, supervision,
or slotr's own legacy locks, and requires a stable policy/lock namespace.
Direct native calls and deliberate namespace substitution can bypass it.

## Reservation replay

`python3 products/slotr/tools/replay.py EVENTS --config CONFIG --since ISO_TIME
--until ISO_TIME --slice-limit-mib MIB` is a read-only analysis tool using Python
3.11 or later and its standard library. Give it the heavy slice's actual
`MemoryMax` in MiB; it does not change configuration, units or event logs.
It prints per-kind measured counts and nearest-rank p50/p95/max cgroup peaks,
then proposes p95 rounded up to 256 MiB, clamped to the configured cost. Kinds
with no measured runs stay unset. Untyped mixed commands keep their recorded
reservations.

The replay compares recorded overlap with static-budget baseline and proposed
reservations, using the configured slots, reserve, campaign caps, recorded
weights, fair-wait age and eight-admission backfill bound. Held runs charge the
larger of reservation and peak, treating the whole peak as anonymous memory
already used; the log cannot reconstruct the anonymous/cache split. It reports peak-sum
overrun admissions, intervals and seconds, plus maximum/mean concurrency and
queue time. Each run is charged its whole-run peak for its entire lifetime.
Missing end/peak records and reservations above the model budget are reported
as exclusions. Recorded arrivals and durations stay fixed; PSI/load, quiet tails,
cross-pool demand, cancellations and dependency feedback are not reconstructed.
An overlap overrun is a conservative model result, not a predicted kernel OOM.

The tool separately queries `journalctl --user` for explicit slotr OOM evidence
in the same window and deduplicates unit/admission episodes. SIGKILL alone does
not count. An inaccessible journal is reported as unavailable, not zero; use
`--no-journal` only for offline fixtures. Missing journal history and censored
OOM peaks limit conclusions. Keep output containing host evidence private.

## Units

Each admitted command runs in a transient systemd user service with its own
supervisor. Units use `OOMScoreAdjust=500`, `OOMPolicy=continue`,
`KillMode=control-group`, memory accounting, no restart, and a bounded stop.
`MemoryMax` is the pool's `memory_max_mib` or the run's admission cost (falling
back to `default_cost_mib` for zero cost; a zero ceiling is rejected), and
`MemorySwapMax` is the pool's `memory_swap_max_mib`, so a runaway run is
OOM-killed alone inside its own unit. The supervisor survives that kill and
reports the payload's signal exit (137).

Before the supervisor exits, it reads systemd's unit `Result` and the unit's
`memory.events` `oom_kill` counter alongside peak accounting. OOM evidence travels
in the current invocation's exit receipt, allowing the client to append one
`oom_kill` event and print `slotr: OOM-killed RUN`. The event includes the unit,
pool, kind, campaign, purpose, cost, reservation and memory peak (null if
unavailable). The client also attempts the unit query when no receipt survives.
An arbitrary SIGKILL or exit 137 alone is not evidence of a kernel OOM. If both
the receipt and unit accounting are unavailable, OOM attribution is unknown;
an event append failure is reported as an OOM audit gap. Payload exit-code
handling remains unchanged.

The client passes a random invocation ID to the supervisor, which writes the
payload's exit code to `exit-RUN-INVOCATION.json` beside the runtime state.
The client reads only that invocation's file and exits with its valid code
(0–255), so a stale or late-written file from an older run cannot satisfy it.
`systemd-run --wait` can
report success for a failed unit that systemd has already collected, so its
own status is only a fallback. A run that recorded no valid current code never
passes: the client exits with systemd-run's failure code or 1. The last stderr line is
`slotr: PASS RUN` or `slotr: FAIL RUN: exit N`.
systemd-run receives only valid environment variable names via
`--setenv=NAME`, keeping values off its command line.

## State and events

- State is locked and atomically replaced in
  `${XDG_RUNTIME_DIR:-/run/user/UID}/slotr`.
- Events append to `${XDG_STATE_HOME:-~/.local/state}/slotr/events.jsonl`.
  Each `admit` event carries `waited_ms`, the whole milliseconds from entering
  the queue to admission, clamped at 0; a request cancelled while queued writes
  no admit event, so it is not counted.
- `admit` also records `kind`, the requested `--kind` name, or null for
  explicit `--cost` and default-cost requests. `cost_mib` remains the resolved
  cost used to choose the unit cap; `reservation_mib` is the admission reservation.
  `backfill_for` is the fair-turn head's enqueue sequence for a backfill admission,
  or null for an ordinary admission. `purpose` is also recorded. The requested
  kind and reservation are retained in queue and holder state.
  `cmd` is the payload argv, so a run can be attributed to a package or command;
  the events file is readable by its owner only.
- An `end` event repeats `run`, `pool`, and `kind`. `exit_code` and
  `exit_signal` describe the payload's actual wait status (one is null);
  `slotr_exit_code` is the supervisor's result, including 75 for automatic
  stops. `stopped` and `stop_reason` distinguish slotr stops from payload
  failures: reasons are the automatic stop reason, `manual` for `slotr stop`,
  or `cancelled` for other supervisor cancellation signals. `wall_ms` measures
  elapsed whole milliseconds from payload startup through supervision and
  payload reaping, excluding the accounting query and terminal audit append.
  `memory_peak_bytes` and `cpu_usage_nsec` come from the unit's `MemoryPeak`
  and `CPUUsageNSec` properties. Missing, unsupported, invalid, or unset values
  are null with an explanation in `accounting_error`; zero remains zero.
  `memory_peak_bytes` is the cgroup high-water mark and includes page cache
  charged to the unit, so a peak near the cap alone does not show a memory kill;
  those show as `exit_signal` 9.
  `error` describes a supervisor error, otherwise null; unavailable payload
  status and supervisor result are also null. Both events have UTC `at`.
  All fields are additive; consumers should ignore unknown fields and events.
- Terminal accounting is sampled after payload reaping, while the supervisor
  keeps the unit alive, before `systemd-run --collect` discards it. These are
  unit totals, including supervisor overhead, before final supervisor and
  remaining-descendant teardown. A supervisor killed by SIGKILL cannot write
  `end`; an `admit` without a matching `end` means unknown outcome. Audit I/O
  failures can also leave gaps and do not replace the workload's exit status.
- Managed weights live in `${XDG_STATE_HOME:-~/.local/state}/slotr/priority.json`.
  Scheduling clocks and manual stop intent use `scheduling.json` and `stops.json`
  beside runtime state, under the existing lock. Runtime schema is 1.
- `slotr stop RUN --reason TEXT` audits a requested intent before systemd stop
  submission, including caller uid/pid, optional task, campaign and reason.
  Failed audit append refuses the signal. Outcomes are submitted/failed;
  submitted means accepted by systemd, not completed. Reconciliation retries
  pending intent or records completion when a later observation sees the unit gone.
  Duplicates share the stop ID; a failed submission can be retried. A crash can
  leave an orphan requested event, so no exactly-once claim is made.
- Neither persists workload argv or environment contents.
- State timing uses CLOCK_BOOTTIME (including suspend); displayed and logged
  timestamps use UTC RFC 3339.
- `status` observes without creating, changing, or deleting files and
  without signaling any process.
- Failed admission ticks retain their tickets and report errors. Emergency
  pressure protection continues when policy loading or audit logging fails.

## Limits

- A 2-second poll cannot beat a multi-GB burst; the kernel OOM killer remains
  the final protection.
- Port probes are not reservations: applications must use strict port
  binding.
- Detached containers and workloads escaping the user unit are unsupported.
- There is no hosted queue, campaign authentication, auto-restart, or Mac run
  backend.
