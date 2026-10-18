# Usage

This file explains the slotr commands, their options, and what a workload sees when it runs.

## Commands

```sh
slotr run --pool default --campaign docs --purpose "local preview" -- npm run dev
slotr status
slotr status --json
slotr stop slotr-default-1 --reason "owned cancellation"
slotr priority set team-a 1
slotr priority clear team-a
slotr priority list --json
slotr touch slotr-default-1
slotr config show
slotr config check examples/devbox.toml
slotr --version
```

- `run` waits in weight order, then FIFO within each weight in a pool. It
  accounts for reserved memory and runs the admitted command in a transient
  systemd user service. Each service
  contains its own supervisor; there is no daemon or remote service.
- `status` shows holders and the queue. `--json` prints JSON. It observes
  without creating, changing, or deleting files and without signaling any
  process.
- `stop RUN [--reason TEXT]` audits the registered target and caller before
  asking systemd to stop the complete unit. Failed audit append refuses the
  signal. Reason text is optional, at most 1024 bytes without controls.
- `priority set CAMPAIGN WEIGHT` sets one exact campaign override (u32, including
  zero). `clear CAMPAIGN` restores legacy fallback; it does not force zero.
  `list [--json]` shows the managed store, path and generation without writes.
  Names must be nonempty without controls or wildcards. Weight changes apply
  at admission and never interrupt admitted work.
- `touch RUN` resets a running holder's inactivity and CPU-idle clocks,
  defending against idle reclaim only. It never renews the lease or clears
  a pending warning or stop claim; eligibility is checked at grace end.
  Overdue leases and pressure still apply. It cannot revive
  a run already stopping. A `touch` event is appended to `events.jsonl` when
  event logging is available.
- `config show` and `config check` are described in
  [configuration.md](configuration.md).

`status` and `config` are portable. `run`, `stop`, `touch`, and the internal
`_supervise` command need Linux and a systemd user bus; see
[install.md](install.md#requirements).

## Options for `run`

- `--campaign` and `--purpose` are required, nonempty free text. Campaign
  names are supplied by the caller; use one consistent name for each campaign.
- `--pool NAME` selects the pool. The default is `default`.
- `--kind NAME` selects a configured cost, or `--cost MIB` supplies it
  directly. Otherwise the pool's `default_cost_mib` applies. You cannot pass
  both `--kind` and `--cost`. Kinds are per-pool; an unknown name exits 2
  before admission and lists the selected pool's valid keys (or `none`).
- `--lease 30m` requests a shorter lease; the pool's `max_lease` caps it.
  Duration suffixes are `s`, `m`, `h`, and `d`; a bare number is seconds and
  `"0"` means unlimited.
- `--task ID` is an opaque string used in hook substitutions and exact
  priority-file matches. `--pane PANE`
  also identifies the holder for the optional liveness probe. Pass the pane
  ID (`--pane "$HERDR_PANE_ID"`) on the same Herdr server, not an agent name.
  Moving a pane changes its ID; the original ID then reads as gone.
- The command follows `--`. Commands and their arguments keep their literal
  argv boundaries; slotr itself never invokes a shell.

## What the workload sees

The workload receives these environment variables:

- `SLOTR_RUN`
- `SLOTR_POOL`
- `SLOTR_SLOT`
- `SLOTR_PORT_BASE`, when a port block is configured

On admission stderr prints exactly `slotr: admitted RUN`.
Every automatic stop prints `slotr: stopped RUN: REASON` on stderr and
returns exit code 75. Use the reason when deciding when to rerun the command.

Status text shows `weight` and `weight_source` (managed, legacy, default). JSON retains
`level == weight`, adds policy path/generation/error, pool caps, quiet expiry, age,
`priority_blocker`, `fair_turn`, `fair_deferred` and actual scheduler order.
`priority.state` and `age_seconds` still describe the legacy file. Status resolves
current policy in memory without writing. Policy errors report cached weights and block
admission with `priority_policy_error`.

## Example: a development stack

To wrap a development stack, copy `examples/devbox.toml` into your config
location, then have the lead start:

```sh
slotr run --pool runtime --campaign my-feature --purpose "UI smoke" \
  --task 123 --pane "$HERDR_PANE_ID" -- sh -c \
  'PORT=$((SLOTR_PORT_BASE+0)) ./start-dev.sh'
```

Port probes are not reservations: applications must use strict port binding.

## Heavy one-shot commands

Type checks, lint, tests, builds, and installs can run through a heavy pool,
such as the `heavy` pool in `examples/devbox.toml`. They then queue by memory
and priority. Weight changes never revoke admission; pressure or configured
lifecycle rules can stop admitted commands.

```sh
slotr run --pool heavy --kind tsc --campaign team-a --purpose "type check" \
  -- pnpm tsc --noEmit
```

The example heavy kinds are `tsc`, `oxlint`, `vitest`, `build`, and `install`.
For Cargo checks on a host with that configuration, use `--kind build`.
`tests` and `lint` are not aliases and are rejected unless your pool
explicitly defines them.

For checks needing a scratch home or taskr database, keep slotr's outer
`HOME` and `XDG_RUNTIME_DIR` set to the real host values. Pin its config with
an absolute path and put the scratch variables after `--`:

```sh
SLOTR_CONFIG=/absolute/slotr/config.toml slotr run --pool heavy --kind build \
  --campaign team-a --purpose "isolated checks" \
  --task 123 --pane "$HERDR_PANE_ID" \
  -- env HOME=/scratch/home TASKR_DB=/scratch/taskr.db cargo test --locked
```

This keeps admission and supervision on the host config and queue while
the payload uses the scratch variables. Changing the outer `XDG_RUNTIME_DIR`
would select a different queue. An absolute config path does not pin a
`~/` priority file: it still follows slotr's `HOME`; use an absolute priority
path when needed. Keep task and pane identity for hooks and task priority.

- Output and the exit code pass through, and the working directory is kept.
  A script reads the result as it would without slotr.
- Exit 75 with `slotr: stopped RUN: REASON` means an automatic pressure or
  configured lifecycle stop; it is not evidence that the check failed. A
  workload's own exit 75 needs separate classification. Manual cancellation
  is recorded with reason `manual` and must not trigger an automatic retry.
- Keep queued work queued. Never stop another campaign's run to enforce
  priority. Ask the host operator to change weights with `slotr priority set`.
- Retry only under the owner's cleanup/recovery instructions, with a finite
  attempt budget. Inspect pressure or lease reasons and partial build/install
  outputs before retrying; slotr does not automatically replay commands.

For selectively refusing bare legacy flock calls, see the opt-in
[cooperative flock guard](flock.md). Its checker uses a fixed absolute config; the
optional native C launcher delegates unrelated native calls unchanged; it is not a host
security policy.

## Exit codes

- Normal workload exit codes are relayed.
- A signal returns 128 + the signal number.
- 2 means a CLI, config, platform, or manager failure.
  The flock guard also uses 2 for a protected target or uncertain policy/syntax.
- 75 means slotr stopped the workload automatically (pressure, lease,
  or idle release) when stderr also has the `slotr: stopped RUN: REASON`
  line. A workload's own exit 75 is relayed like any other code.
