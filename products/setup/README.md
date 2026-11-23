# Herdr setup assets

Fleet orchestration and the taskr plugin require [Herdr 0.9.0 or newer](https://herdr.dev/docs/install/).

taskr owns installation. From a herdr-kit checkout:

```sh
sh products/taskr/install.sh                         # taskr CLI/TUI + skill; stages the taskr Herdr plugin
sh products/taskr/install.sh --fleet                 # add fleet integration
sh products/taskr/install.sh --practices --verification
sh products/taskr/install.sh --fleet --files-only    # offline source staging
```

`products/setup/install.sh` is a compatibility wrapper that adds `--fleet`.
Set `TASKR_VERSION=taskr/v0.34.0` to select an exact product release. Binary
installs need no Cargo, Node, or npm. `gh` authentication or `GH_TOKEN` is
supported; anonymous downloads use curl. Unpinned curl installs also need jq.
Launch helpers require jq.

Fleet selection installs the public orchestration/fleet skills, `herdr-launch`,
`herdr-loop`, `herdr-worktrees`, `herdr-lead`, `herdr-hub`, `herdr-lanes.sh`,
`kit-pr` and `kit-prompt`, the shared lane table, and minimal taskr hooks.
`kit-pr N [--repo OWNER/REPO] [--checks] [--body]` prints one PR status line from `gh` and `jq`.
`kit-prompt ARGS` runs `taskr prompt ARGS`; a clean push prints `event N`, anything else taskr's output whole with its exit code.
If Herdr is available, its generated upstream skill is staged too. OpenCode/pi hooks
are selected only when the corresponding config directory already exists.
Existing agent config directories get skill loader links. No unrelated settings
are merged automatically. Services and the taskr plugin are staged; activation
is manual. The installer never starts or restarts Herdr or a service.

This repository supplies public install assets. Hosts may override them with
private `lanes.json` and fleet `local.md` files; use those local files to check
which policy a host runs.

`--practices` and `--verification` independently select the optional skills.
Their pinned pstack LICENSE and PROVENANCE.md install under
`~/.local/share/herdr-setup/third_party/pstack`. Attribution links are rendered
for the installed layout; source links continue to resolve in this repository.
slotr is a separate `--slotr` opt-in, supported by its product installer when
available in the checkout. `--codator` installs only the kit's `cdx`/`cdl` wrappers,
which forward to `codator codex`/`codator claude` with arguments unchanged;
install [Codator, the account selector, separately](../../README.md#optional-codator).
Codator's own optional `cdx`/`cdl` shortcuts (`CODATOR_SHORTCUTS=1`) bypass every
sandbox and approval prompt. Both sources use the same names and default paths,
`~/.local/bin/cdx` and `~/.local/bin/cdl`; the Codator installer replaces an
existing file only when it already calls Codator, so pick one source for these
commands (see [Codator's shortcuts](https://github.com/olafurns7/codator#optional-shortcuts-cdx-and-cdl)).
`--notifications` merges the public
Moshi (a mobile push-notification app) hook fragments through the usual
KEEP/UPDATE/MERGE rules, preserving other
settings keys. Notification credentials and destinations remain local.
`--radar` explicitly installs/configures the external
[herdr-radar](https://github.com/hhdebb/herdr-radar)
plugin (`hhdebb.herdr-radar`) at its existing pin;
it is never imported into this repository. Files-only mode suppresses radar
control actions. An absent optional tool never blocks the default core install.

The public source allowlist is in `products/taskr/install.sh`. It excludes
private overlays, backups, live state, and unselected settings. Standalone
installation resolves setup source from the same selected taskr tag.

`herdr-hub ROOT` (or `--new NAME`) adopts the taskr hub's root in its pane and
restarts its Claude lead with the taskr push channel through the sibling `cdl`
wrapper (needs `--codator` and Codator). Usage, checks and recovery:
[launch.md](skills/herdr-orchestration/launch.md). The hub starts with
[Code Mode](#code-mode) off unless `HERDR_HUB_CODE_MODE=1`. The hub starts with a
400000-token auto-compact window (`autoCompactWindow` in its `--settings`); set
`HERDR_HUB_COMPACT_WINDOW` to an integer from 100000 to 1000000 to change that;
any other value, including an empty one, refuses the start.
`herdr-hub move` passes a set value on to the new hub.

## Code Mode

[Code Mode](https://code.claude.com/docs/en/code-mode) is an experimental
Claude Code feature. Claude writes JavaScript that makes several tool calls in
one step, and Claude Code runs that code in the session.

`herdr-hub` starts the Claude hub with `CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=0`,
so the hub uses direct tools. This `0` also overrides a `1` in the shell or in
user settings. With `HERDR_HUB_CODE_MODE=1` in the environment of the shell
that runs `herdr-hub`, `herdr-hub` exports `1` instead; a `0` in user or
managed settings still wins. `herdr-lead` starts leads the same way with
`HERDR_LEAD_CODE_MODE`, and `herdr-launch` starts workers without the variable.

Requirements. The kit does not check the version, the account, the
organization or settings outside the kit. If Code Mode does not start, use the
upstream [Code Mode doesn't start](https://code.claude.com/docs/en/code-mode#code-mode-doesnt-start)
table.

- Claude Code v2.1.323 or later, native build.
- An interactive `claude` session in a terminal. Code Mode does not work with
  `-p`, the Agent SDK, the VS Code panel, the Desktop app or cloud sessions.
- A Claude subscription or Console account with the Anthropic API. Code Mode
  does not work through a gateway, a custom `ANTHROPIC_BASE_URL`, Amazon
  Bedrock, Claude Platform on AWS, Google Cloud's Agent Platform or Microsoft
  Foundry.
- No API key helper, `ANTHROPIC_AUTH_TOKEN`, or `Authorization` or `X-Api-Key`
  header in `ANTHROPIC_CUSTOM_HEADERS`.
- The organization does not use the HIPAA configuration or zero data
  retention, and its managed settings do not set the variable to `0`.
- Model Opus 5.5, Sonnet 5.5, Haiku 5.5 or Fable 5.2. The public Claude `hub`
  and `orch` lanes use these models.
- Claude Code can fetch feature flags: `DISABLE_TELEMETRY` and other variables
  that turn off feature-flag fetching are not set, and the network is up.
- No OpenTelemetry export: `CLAUDE_CODE_ENABLE_TELEMETRY` and
  `ENABLE_BETA_TRACING_DETAILED` are not set.
- No `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`, `--remote-control` or
  `--restricted`.
- `REPL` is available: no `--tools` list without `REPL`, and no `REPL` in
  `--disallowedTools` or a deny rule.
- No installed mod that checks tool calls.
- `~/.claude.json` is writable.
- Project settings cannot turn on Code Mode; only the shell and user settings
  can.

If Code Mode does not start, a line near the top of the session and `/status`
usually give the reason. An older version can show no line: then run
`claude --version`.

First start: Claude Code asks for confirmation before the first Code Mode
session. This "Code Mode (experimental)" dialog comes before the development-channel
dialog, and its default is **No, start without it**: Enter or Esc starts the
session without Code Mode. After **Yes, turn on Code Mode**, Claude Code saves
the answer, restarts, and can show the development-channel dialog again. A
config home that never accepted Code Mode shows the consent dialog; with
Codator each account has its own config home.

Do this once for each account, before its first Code Mode hub:

1. In a plain terminal pane, run
   `env CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=1 cdl --account ACCOUNT`.
2. Select **Yes, turn on Code Mode** and let Claude Code restart.
3. Quit it.

The `herdr-hub` prover never answers the consent dialog. If it sees it, it
records a ledger note once and waits: select **Yes, turn on Code Mode** in the
hub pane by hand. The prover then answers the development-channel dialog and
waits up to 600 s from your answer. If the consent dialog is still there after
600 s, the push check fails with that reason: select **Yes, turn on Code Mode**,
press Enter on the development-channel dialog by hand, then run
`herdr-hub check ROOT`, which prints `live` when the channel is up. If you
select No, the hub starts without Code Mode; its prompt tells it to record a
ledger note when Code Mode is off. A taskr prompt typed into the pane while the
dialog is open can answer it No (a separate taskr fix is pending), so do the
First start step before a Code Mode move. `herdr-hub move` with Code Mode on
prints a warning about this step and does not check it.

Resume: a Code Mode session resumes only with the variable at `1`. With the
variable at `0`, Claude refuses to resume a Code Mode session. A session that
started without Code Mode resumes without it. `herdr-hub` does not detect the
mode of the session it resumes. For a hub session that started with Code Mode,
for example one started before the default changed, start a new session from
the handover with `herdr-hub move ROOT` without `--resume`; or, to keep the
session, run `herdr-hub move ROOT --lane <lane> --account <account>
--code-mode on -- --resume <session-id>`. After a Herdr server restart, Herdr may resume panes with a plain
`claude --resume`. That command can be without the variable, and then a Code
Mode session refuses to resume. Quit it and resume with `herdr-hub` as `recovery.md` says.

To turn on Code Mode for the hub, export `HERDR_HUB_CODE_MODE=1` in the hub
pane shell before you run `herdr-hub`. Any other value, or no value, keeps the
hub at `0`. `herdr-hub move` keeps the moving hub's Code Mode by default: it is
on when the caller has `CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=1` or
`HERDR_HUB_CODE_MODE=1`, and off otherwise. `--code-mode on|off` on the move
overrides that. The move prints its decision and source, and it
types `env HERDR_HUB_CODE_MODE=1` or `env HERDR_HUB_CODE_MODE=0` before the
`herdr-hub` command, so a value exported in the hub pane shell does not change
the decision; the helper log's `move: run` line shows it. A `"0"` in the
`env` block of `~/.claude/settings.json`, or in managed settings for an
organization, keeps Code Mode off even with the switch. A running session keeps
its mode until it stops.

Leads: to turn on Code Mode for leads, set `HERDR_LEAD_CODE_MODE=1` in the
environment of the process that runs `herdr-lead`. For example, export it in
the hub pane shell before `herdr-hub`; the hub's shell commands then get it.
Without it, `herdr-lead` exports `CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=0` in the
lead pane shell, which also overrides a `1` in user settings. With it,
`herdr-lead` exports `1`, but a `0` in user or managed settings still wins.
Keep the same switch for `herdr-lead move --resume` of a lead that started
with Code Mode, or Claude refuses to resume it.

Limits for a fleet:

- On a tool call from Claude's code, a `PreToolUse` hook that cannot start, is
  killed, exits 126, 127 or above 128, returns an HTTP error or invalid output,
  or times out refuses the call. After code has run, a timeout refuses later
  calls too. `"onFailure": "continue"` on a `command` or `http` hook lets the
  call go ahead. The shipped synchronous hooks do not set `onFailure`. The
  shipped `AskUserQuestion` hook has a 5 s timeout: a slow ledger can then
  refuse the question.
- `UserPromptSubmit` command, `http` and `mcp_tool` hooks also run on
  background code output (input `source` is `poll_event`). The shipped hooks
  do not block. A local hook must not block when `source` is `poll_event`.
- Remote Control, Claude in Chrome, computer use and background sessions are
  not available. Code that does not run in the background stops after ten
  minutes. Background code needs auto or bypass permission mode. Leads start
  with `--dangerously-skip-permissions`; the hub gets bypass mode from the
  shipped `settings.json` default.
- A Herdr server started from a Code Mode session can pass the variable to new
  pane shells. Do not start the Herdr server from an agent session.

Not verified live: the taskr push channel under Code Mode, the confirmation
dialog and its restart through `cdl` and the prover, which installed plugins
count as mods that check tool calls, the Codator account settings, and leads
with `HERDR_LEAD_CODE_MODE=1`.

## Existing files

One shared placement library, `lib/placement.sh`, retains the setup contract:

- Missing files are installed with their declared mode.
- Equal content gets `SKIP`; an existing mode is preserved.
- A file matching its recorded hash gets atomic `UPDATE`, preserving its mode.
- An edited file gets `MERGE` and a `.herdr-setup` sidecar.
- Destination symlinks and conflicting sidecars are preserved. Directories are
  never overwritten as files. Existing sidecars are never replaced.
- `KEEP` entries are never written, even when the recorded content is unchanged.

Keep intentional differences in `~/.config/herdr-setup/keep`, one path relative
to HOME per line. Blank lines and comments are allowed. `--keep PATH` appends
atomically, without duplicates, and applies during that run. Absolute, empty,
multiline, `~`, `.`, `..`, and escaped paths are refused. Existing parent links
must stay inside physical HOME. A symlink or directory at the keep-list path
refuses append; unreadable lists stop installation.

`~/.local/state/herdr-setup/installed` remains the SHA-256 installation record.
Missing, unreadable, malformed, symlinked, or nonregular records cannot authorize
updates. Escaped metadata directories are ignored without reading or writing
outside HOME. An interrupted run can be rerun: equal files are recognized and
recorded, while differing unrecorded files still require a manual merge.

## Private policy and lanes

The existing `~/.agents/skills/herdr-fleet/local.md` stays private and untouched.
The public fleet skill includes it when present; the installer detects it
without opening, sourcing, copying, or printing its contents.

`--fleet` places global instructions in existing `~/.claude` and `~/.codex`
directories. Operator model policy and bans have one source, the optional
private `~/.config/herdr-fleet/instructions.local.md`. With it present, every
run regenerates each global file as the public base, a blank line, and the
overlay, even over hand edits; a keep-listed file or a symlink is left alone.
A file that differs from both the new content and the last recorded install is
first saved as `<file>.herdr-setup-prev` (mode 0600). A read-only (0444) file
is replaced through a temporary sibling and stays 0444. Without the overlay the
usual keep, record, and sidecar rules apply.

`herdr-launch`, `herdr-lead` and `herdr-hub` read `~/.config/herdr-fleet/lanes.json`
when present; this
private table replaces the whole public table. Otherwise they read the checkout's
`products/setup/lanes.json` when run from source, else the staged public
`~/.local/share/herdr-setup/lanes.json`. Each named lane has `kind`,
`model`, `effort`, and a `prefixes` array. Invalid fields and unknown lanes fail
before task registration. Values form quoted argv; no eval is used. The installer
never reads or modifies the private lane table.

`herdr-launch --check [LANE...]` checks CLI versions and Codator forwarding without
control-plane writes; add `--push` before `--check` to check push prerequisites too. An
optional `deny` object maps `model`, `provider`, and `kind` to arrays of jq regular
expressions; any match refuses resolution and is marked by `--lanes`. Optional
`deny.except` maps known prefixes to objects with the same regex fields. A denial is
exempt only when every prefix of the lane has a matching exception for that field.
Exceptions have no effect without a matching deny rule. Anchor patterns with `^` and `$`
for exact matches. An OpenCode lane uses a qualified `provider/model` model;
its `provider` is the model prefix, and an explicit value must match. Keep operator policy in
the private table; the public deny list is empty. An optional `pins` object maps a
task-name prefix to the only lane that may start it; `herdr-launch` refuses any other
lane for that name ([lanes.md](skills/herdr-orchestration/lanes.md) "Lane pins"). The
public `pins` is empty.

The lane table is the only list of models; `herdr-launch --lanes` prints the
resolved lanes, and the private instructions overlay holds per-model policy.

`herdr-launch` runs OpenCode directly, resolving PATH or `~/.opencode/bin/opencode`.
With v2.1.1 it uses `--standalone --auto` and process-local `OPENCODE_CONFIG_CONTENT`
to select the model and build agent without writing global config. Effort is
recorded but ignored; `--push` is refused for OpenCode.

Claude's synchronous PreToolUse hook denies `Agent`, `Task`, and `Workflow` when
`HERDR_ENV=1`, pointing to `herdr-launch`. Codex has no equivalent hook surface for
`spawn_agent`; that restriction remains instructions only.

Per-user incident intake config lives at `~/.config/herdr-fleet/intake.yaml`
(example in [intake.example.yaml](intake.example.yaml); see the herdr-practices
[incident-intake reference](skills/herdr-practices/references/incident-intake.md)).
`--fleet` stages only the placeholder
`~/.local/share/herdr-setup/intake.example.yaml` and never creates or overwrites
the private config.

## taskr roles

`--taskr-hub` stages `tailnet` in the dashboard address and a user-service
unit. `--taskr-client http://hub.example.ts.net:7788` stages the client
URL. Flags are mutually exclusive; invalid URLs are refused. Local ledgers,
hub roles, and legacy hub URLs require manual reconciliation before changing
roles. Apply the printed service/daemon steps yourself.

## Temp files

`herdr-launch` (every lane kind), `herdr-lead` and `herdr-hub` set `TMPDIR`
to `/var/tmp/agent-UID` for the agent they start, because `/tmp` is a
RAM-backed tmpfs on some hosts. Claude Code moves its `claude-UID` task and
scratchpad folder there, and its Bash tools and stdio MCP servers inherit
`TMPDIR` (measured). Codex reads `TMPDIR` (read from its binary, not yet
measured in a session). Fixed paths stay on `/tmp`: the `/tmp` writable root
of the Codex sandbox and Claude's sandbox default `/tmp/claude`. The launcher creates the folder with mode 0700 and refuses to start
when the path is a symlink, is not a directory, or belongs to another user.
`herdr-launch` runs this step in the pane, so a `--machine` lane gets the
folder on its own host. Codex binds `TMPDIR/codex-daemon-UID/<64 hex>` and
AF_UNIX allows 107 bytes, so the folder may have at most 24 bytes; a longer
one is refused. A `TMPDIR` that the caller set is replaced. Nothing sweeps the
folder by age; `systemd-tmpfiles` cleans `/var/tmp` on some hosts.

## Memory isolation

slotr caps each run unit with `MemoryMax` and `OOMPolicy=continue`, so a
runaway run is killed alone (slotr's [configuration](../slotr/docs/configuration.md),
`memory_max_mib`). The Herdr user service hosts every agent pane, so one
kernel OOM kill there must not stop the whole service. The installer does
not change that unit; add a drop-in yourself and restart Herdr from a
terminal outside any agent session:

```ini
# ~/.config/systemd/user/herdr.service.d/oom.conf
[Service]
OOMPolicy=continue
```

Leave `ManagedOOMMemoryPressure=` and `ManagedOOMSwap=` unset on this unit:
on a service without child cgroups, systemd-oomd would kill all of Herdr.
For swap selection, systemd-oomd honours `ManagedOOMPreference=` only for
root-owned cgroups. For memory-pressure selection, it also honours a
descendant's preference when its cgroup owner matches the monitored ancestor's
owner ([systemd v255 resource-control docs](https://github.com/systemd/systemd/blob/v255/man/systemd.resource-control.xml)).
These preferences do not protect a unit when an ancestor cgroup is killed;
the per-run `MemoryMax` limits remain necessary.

## Validation

From the repository root:

```sh
for t in products/setup/tests/*_test.sh; do sh "$t" || break; done
node products/setup/tests/taskr_hook_test.mjs
sh products/setup/tests/privacy_check.sh
```

The installer fixtures use scratch
HOME directories, synthetic private values, fake downloads, and fake control
commands. Run the full repository gate through the configured heavy pool.
