# Launch: task layout and manual launch

Load when: before opening the taskr hub workspace; before creating a campaign or task worktree or workspace; before starting a campaign lead; before reusing or rearranging an existing tab or pane; for a manual launch, or recovery after `herdr-launch` is missing or failed (first inspect what the script completed, below). The core's Launch section covers an ordinary launch in a new helper tab or a fresh root pane. Part of herdr-orchestration; `SKILL.md` is the core, and its Launch section is the normal path.

## herdr-core backend

`HERDR_KIT_BACKEND=hc` selects herdr-core (`hc`). Unset or any other value means Herdr 0.9 and nothing in this section applies. Under the switch:

- `herdr` on PATH is `herdr0`, which serves the `herdr` commands this skill uses over hc, with the same id shapes (`wN`, `wN:tN`, `wN:pN`) and `--machine`. Use those commands; `plugin`, `server` and `worktree` subcommands, `status` and `agent explain` exit 2 with `herdr0: unsupported: ...`. So the work controller's worktree step (`herdr worktree list`) does not work under hc. `herdr-launch --reserved` refuses a Codex lane before it starts; a fresh reserved Claude lane starts but fails closed at step 6 (no native identity, prompt withheld).
- `herdr-hub move` does not work under hc. To restart or repair the hub, record its handover, quit the hub Claude, and at its hc shell terminal's prompt type the start form `herdr-hub <root> --lane <lane> --account <account> -- --resume <session id>` (without `-- --resume` for a new session). A session that started with Code Mode resumes only with Code Mode on: `env HERDR_HUB_CODE_MODE=1 herdr-hub <root> --lane <lane> --account <account> -- --resume <session id>`.
- Start one root per Codator account at a time. A second start on an account whose first start has not finished fails with `account is busy`; start it after the first root is up, or on another account.
- The Herdr worktree plugin does not run under hc: `herdr worktree` exits 2 and no repository setup plugin runs. Create a task worktree with git, then open its workspace (`.result.workspace.workspace_id` and `.result.root_pane.pane_id` in the output):

  ```bash
  git -C <source checkout> worktree add -b <type>/<slug> <worktree> origin/main
  herdr workspace create --cwd <worktree> --label "task: <campaign>-<slice>" --no-focus
  ```

  If the repository's AGENTS.md names a worktree setup plugin, run its setup script by hand from the new worktree before a worker needs dependencies. For a plugin that reads `HERDR_PLUGIN_CONTEXT_JSON`: `env HERDR_ENV=1 HERDR_PLUGIN_CONTEXT_JSON='{"worktree":{"checkout_path":"<worktree>"}}' sh <source checkout>/.herdr/setup.sh`, with the setup script path the repository uses.
- An install leaves a changed installed file in place and stages the new text beside it as `<file>.herdr-setup`; the installer's last block lists every such sidecar. Until the owner resolves one (compare it with the file, then move the sidecar into place: `mv <file>.herdr-setup <file>`), agents still read the old text. A settings sidecar (`~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.codex/config.toml`) is the kit's template: the owner merges it by hand as QUICKSTART.md says and never moves it into place. A stale sidecar (listed as such) blocks the new text until it is removed and the install runs again. When this section's rules are missing from an installed skill file, look for its sidecar and tell the owner.
- No agent ever starts an hc core: never run `hc server`, `herdr-core`, the hc installer or any bare `hc` command, because the first `hc` call starts a core that keeps the caller's environment. With no core running, `herdr` exits 3 with `herdr0: no hc core running; the owner starts it`: stop and tell the owner (`owner.md`); the commands are in herdr-kit's [herdr-core runbook](https://github.com/olafurns7/herdr-kit/blob/main/products/setup/docs/herdr-core.md).
- The owner starts each root (hub or campaign lead) in its own terminal with one line, `hc run --bg --name NAME --keep --no-parent --cwd DIR -- herdr0 enter --new-workspace LABEL -- CMD`. Every terminal `herdr0` creates runs through `herdr0 enter`, which sets `HERDR_ENV=1`, the pane, workspace and tab ids, and the switch, and passes `--no-parent`, so lanes outlive their starter and never type notes into its input.
- The Herdr desktop app shows the fleet. Every terminal `herdr0` creates carries the app's `herdr.space` and `herdr.tab` labels: one herdr0 workspace is one app space and one tab one app tab. The app names a space after its terminal's folder and numbers its tabs; herdr0's workspace and tab labels (and their renames) do not set those names. Under hc `herdr-hub` labels the hub's terminal `agent=claude` and `agent_name=hub-<root>`, so `herdr pane get` shows the hub. Closing a space, tab or pane in the app ends its terminals, the hub's included. `herdr app-labels [--workspace W | --pane P] [--apply]` labels terminals made before this release: a dry run without `--apply`, and it touches only terminals with a herdr0 `ws` label (the app's own terminals have none).
- `herdr-launch` starts Codex with `--no-daemon`; a Codex lane without it runs its commands outside hc. Add it to any manual Codex start.
- Plugins and radar are off: no `[[startup]]` hook starts the taskr daemon (its systemd unit does) and no radar plugin runs.
- Status, `notification show`, reboot and teardown differ too: `workflow.md` "Blocked or odd status" (status), `owner.md` (notifications), `recovery.md` (reboot), `teardown.md` (closing).

## Another host

For every Herdr read, send, close or launch on another host, use
`herdr --machine <label>` (upstream Herdr skill: "To control a saved SSH machine").
Use ssh only for non-Herdr work, such as git, files and slotr.
Run `herdr-launch --machine` on the ledger host; from a client host, `taskr prompt` reaches only lanes on that host and the server host, so step 7 fails for a third host.

```sh
herdr-launch --machine devbox [--work W] LANE NAME BRIEF REPORT PARENT PANE
herdr-launch --machine devbox --workspace W --cwd DIR LANE NAME BRIEF REPORT PARENT
```

Use a target PANE, or both a target workspace and absolute cwd. There is no caller
workspace or cwd default. Only claude and codex are supported; `--push`,
`--reserved` and `--check` are refused. BRIEF is on the caller; REPORT is on
the target and must match the brief's report header. The launcher resolves
one enabled saved label or id and reads cwd, CLI, Codex trust and Codator
facts from one target-pane probe before task registration. It adds
`--machine HOST` to `taskr new` and `taskr launch`; HOST is the target's
lowercase short hostname. The final line adds `machine=LABEL`.
For manual recovery, add `--machine LABEL` to every Herdr command below.

`herdr-launch --account NAME` selects a Codator account for a Claude or Codex
lane and requires Codator on the target. Names must be 1–48 ASCII letters,
digits, underscores or hyphens, starting with a letter or digit. Without the
flag, every Claude lane selects from the target's
`~/.local/share/codator/claude/*/.claude-usage.json` snapshots. Usable, readable
snapshots modified within 30 minutes with a valid weekly reset qualify when
the overall week, Fable for Fable lanes, and the Opus or Sonnet weekly limit
when present for that model have at least 3% left, and the five-hour limit
has at least 15% left when present. A missing Fable row is unknown: Fable
lanes skip the account. Order is the weekly reset rounded to the nearest
minute (a reset already passed counts one week later), soonest first at every
horizon, then lowest name. A Fable lane takes the first
qualifying account. Other lanes count spare = week left − 0.55 × Fable left
(Fable counted only when its row has at least 3% left) and take the first
account with spare of 3 or more; when none has, they take the largest spare
and the final line adds `fable_spend=1`.
`~/.config/herdr-fleet/accounts.json`, `{"protect":["NAME",...],"reserve":15}`,
names accounts that run a live hub or lead; lane picks subtract the reserve
from their week and Fable left. Nothing writes the file yet; a later ticket
makes herdr-hub and herdr-lead write it. A missing or malformed file is no list,
and herdr-launch says so: an automatic pick prints
`herdr-launch: no protect list (missing|malformed): ~/.config/herdr-fleet/accounts.json`
on stderr (`missing` when the file is absent, `malformed` for any other read
or value failure) and picks as it would without a list; an explicit
`--account` prints nothing.
Invalid snapshots and account names are skipped. No eligible account leaves
selection to Codator. The final line reports `account=NAME drain=soon` (reset
within 24 hours) or `account=NAME drain=later` for an automatic pick,
`account=default` for fallback, and `account=NAME` for an explicit choice.
Selection reads no credentials, stores no state, and runs no Codator commands.

`herdr-launch --quota` reads local snapshots without a pane, agent or taskr
call; `--machine` is refused because reading the target would need a pane,
and unavailable Python exits 2.
It prints `account=NAME week_used=N fable_used=N|- reset=ISO role=ROLE` per
validly named snapshot account, ranked as an Opus lane pick. Roles are `soon`
or `later` for the chosen account (with `fable_spend=1` when it has no
spare), `next` for other eligible accounts, `out` for accounts below a margin,
and `stale` for skipped snapshots. Roles apply the protect reserve; numbers are
raw, and a protected account's line ends with `protected=1`. Unavailable values
print `-`. A missing or malformed protect list adds a last line
`protect=none reason=missing` or `protect=none reason=malformed`, so no
reserve applied; a valid list adds nothing. No account is
driven to zero for a free reset any more, so `--quota` prints no alert line.

## Workspaces and tabs

Use one workspace per independent task, project, or worktree, never one per worker. Give each helper its own tab in the task workspace, including helpers that a sub-orchestrator starts. A full-size tab keeps the agent's terminal wide enough for reliable status detection, and the sidebar still lists every agent and rolls its state up to the task workspace. Split a pane only when the owner asks to watch two agents side by side, and split the owner's conversation pane only on the owner's explicit request.

## Keep the owner's focus

Keep the owner's view where it is. Pass `--no-focus` to every `worktree create`, `workspace create`, `tab create`, and `pane split`. Never run `agent focus`, `pane focus`, or `tab focus` on a helper: they move the attached clients, and `worktree create` focuses its new workspace unless you pass `--no-focus`.

## Resolve live ownership

Resolve live ownership before you change a task layout. Launch-time `HERDR_WORKSPACE_ID` and `HERDR_TAB_ID` can be stale after a pane move. From the calling task pane, `herdr pane current --current` is the authority:

```bash
current=$(herdr pane current --current)
task_workspace_id=$(printf '%s\n' "$current" | jq -er '.result.pane.workspace_id')
task_cwd=$(pwd -P)
herdr tab list --workspace "$task_workspace_id"
herdr pane list --workspace "$task_workspace_id"
```

## Hub and campaign workspaces

Each repository main checkout has exactly one workspace: the hub's. Never open a second workspace on a main checkout. An ad-hoc session there is a tab in the hub workspace. Herdr binds a checkout to one workspace (`herdr worktree list` shows `open_workspace_id`), and every linked worktree nests under that workspace. Extra workspaces on the checkout made task workspaces nest under the wrong one.

When no workspace is bound to the main checkout, open the hub workspace. Check the binding with `herdr worktree list`:

```bash
herdr worktree open --path <main checkout> --label <repo> --no-focus
herdr worktree list --workspace <hub-workspace-id>
```

The hub registers one long-lived root per repository once, with its location flags as in `workflow.md` "Brief variants": `taskr new hub-<repo> --role orchestrator --brief <goal> --workspace <id> --tab <id> --pane <id>`. After a restart the owner uses `herdr-hub` below in the hub pane; campaign leads use `herdr-lead` for Claude push or `taskr adopt <root>` for a plain lead. Campaign leads are separate roots; the hub push prover uses a temporary gate child.

### Owner hub in its current pane

`herdr-hub` and `herdr-lead` start and move accept a `hub` lane, such as `fable-high`; a lead uses an `orch` lane unless the operator names a `hub` lane with `--lane`.
The hub and leads start with Claude Code Mode off; the hub turns it on only with `HERDR_HUB_CODE_MODE=1`, a lead only with `HERDR_LEAD_CODE_MODE=1`. A Code Mode session resumes only with Code Mode on, and `herdr-hub move` keeps the moving hub's Code Mode by default (`--code-mode on|off` overrides it); a `--resume` of a hub session that started with Code Mode needs it on, for example `herdr-hub move ... --code-mode on -- --resume <session id>`; otherwise move without `--resume`. Neither `herdr-hub` nor `herdr-lead` answers the Code Mode consent dialog (Enter or Esc there means No): each records one ledger note on the root naming the hand step and waits up to 600 s for a person to select **Yes, turn on Code Mode**; then `herdr-lead` fails step 6 and the hub's push check fails, each with that reason, and the hand steps are in the README. Accept Code Mode once per account first: [setup README "Code Mode"](https://github.com/olafurns7/herdr-kit/blob/main/products/setup/README.md#code-mode).

To restart a running hub onto a new session, the hub records its handover and then runs, in its own pane (under hc: the start form in "herdr-core backend" instead):

```bash
taskr handover --as <root> --note "<checkpoint>"
herdr-hub move <root> --lane opus-high    # --no-handover skips the handover check
```

`move` refuses unless the root is recorded at this pane and has a handover newer than its latest adopt. It needs `setsid` (util-linux; macOS lacks it by default) and fails before anything is stopped without it. It records the Claude session of the agent in this pane, starts the helper with `setsid` (log `lead-push/<root>/hub-move-<pid>.log`), and returns only once the helper reports that it leads its own process group; the hub then ends its turn. The helper waits for the hub to go idle and stops it with `herdr-lead stop <root> <pane> --session <sid>`, which refuses a pane now holding another agent. It waits for the lease to expire, types `env HERDR_HUB_CODE_MODE=1|0 herdr-hub <root> --lane <lane> ...` (the move's Code Mode decision) at the pane's shell, and follows `hub-check-<gen>.result` for up to 2520 s, the prover's full lease and backlog budget. A helper failure is noted on the root.

After the old hub Claude exits, a person can also run at that pane's shell prompt:

```bash
herdr-hub <root> --lane opus-high
# Or create a root with this pane, workspace and tab:
herdr-hub --new <name> --lane opus-high -- <Claude args>
herdr-hub check <root>
```

`herdr-hub` (needs Codator and `--codator`) waits for the old lease, adopts once, and execs sibling `cdl`
with the lane and generation MCP config, passing the prompt after `--` (Claude's channel
and MCP config flags take several values). It preserves refs and only suggests
`taskr set <root> hub=on` when needed. The detached prover answers the development-channel
dialog once; the hub skips adopt and uses taskr MCP tools with pushed fences.
The prover requires the matching lease and a received-and-acked gate
event within 600 s; while the hub acks older pushed events, each new ack extends the wait (up to 30 min).
It closes the gate accepted or rejected and notes the result.
`$HOME/.local/state/herdr-orch/lead-push/<root>/hub-check-<gen>.log` and `.result`
record `live` or `failed: REASON`. `check` prints `live` (exit 0), `stale` or
`push-dropped` (exit 1), or `unknown` (exit 2). It checks the lease, generation,
and channel flag when the current process is found. It does not prove end-to-end
delivery. After a failure, see `recovery.md` for `start.lock`.
Keep the immutable generation config while recovering native handles; remove it
by literal path only after that generation's work and recovery are settled.

For each campaign the hub creates the campaign worktree workspace and starts the campaign lead in its root tab. The agent name is `orch-<campaign>`:

```bash
wt=$(herdr worktree create --workspace <hub-workspace-id> --branch orch/<campaign> --base origin/main \
  --label "orch: <campaign>" --no-focus)
task_workspace_id=$(printf '%s\n' "$wt" | jq -er '.result.workspace.workspace_id')
```

`task_workspace_id` is now the campaign workspace. Give its root pane to the lead as in "First helper takes the root tab" below. Register an open root orchestrator with `taskr new <campaign> --role orchestrator --brief <goal>` (no parent), then start the same-host Claude push lead:

```bash
# First start on a fresh root:
herdr-lead start <root> <pane> --worktree <campaign worktree> --lane opus-high --prompt-file <lead brief>
# Pin a fresh start or move to an account with quota for the lane (needs Codator; herdr-lead does not pick one by quota):
herdr-lead start <root> <pane> --worktree <campaign worktree> --lane <lane> --account <account> --prompt-file <lead brief>
# Alternatively, replace an existing plain lead after stopping it:
herdr-lead start <root> <pane> --worktree <campaign worktree> --predecessor-pane <old pane> --prompt-file <lead brief>
# Or move a running lead to a new session in one command (same pane, or name another):
herdr-lead move <root> <pane> --worktree <campaign worktree> --prompt-file <lead brief>
herdr-lead check <root>
```

`move` first checks the destination (free name and pane, location; for another pane also its shell prompt and host) and that the agent in the root's recorded pane is the root's lead `orch-<slug>` on this host. It then asks that lead for `taskr handover` (`--no-handover` skips this) and fails before stopping anything if none is recorded. It then stops that lead (`herdr-lead stop <root> <pane>`: wait for idle, `esc ctrl+c ctrl+c`, at most 120 s, shell prompt, lease free within 120 s) and runs `start` with that pane as predecessor. `stop` sends keys only to the root's lead: it rechecks the agent's name and Claude session right before each key sequence and refuses any other occupant of the pane. Run `start` or `move` detached or with a tool timeout of at least 30 min. Each run logs its steps to `lead-push/<root>/lead-start-<pid>.log`.

Do not use `herdr-launch` for a lead: it registers a child. `herdr-lead` accepts any Claude lane that allows `orch-` or `hub-` (default `opus-high`), checks the target shell and predecessor lease, clears worker identity, adopts the root once, and keeps the worktree cwd. It writes immutable `0600` configs and handovers under `$HOME/.local/state/herdr-orch/lead-push/<root>/`, passes the config with `--mcp-config`, and answers the development-channel dialog. `herdr-hub` and `herdr-lead` turn on digest pushes (hold 120 s; ask, owner_ask, answer, owner_answer and fail kinds still push at once); `herdr-launch` workers get one push per event. `herdr-lead` never writes `.mcp.json`. Its acceptance check must still pass on every start.

The lead helper denies only Agent, Workflow, ListAgents, SendFeedback, ReportFindings and ScheduleWakeup with `--disallowedTools`, leaving other built-in tool defaults unchanged. `--strict-mcp-config` loads only the explicit taskr config (also taskr-jobs with `--jobs`), retaining the `server:taskr` development channel; claude.ai connectors and the Slack and Stripe plugins are disabled. For one `start` or `move`, `--mcp` keeps user, project and enabled plugin MCP servers, and repeatable `--allow-tool NAME` omits the deny rule for that exact name (1–200 characters of `A-Za-z0-9_.:*()-`, not starting with `-`); other settings stay the same. These defaults reduce the context leads read on every turn without changing worker or hub launches.

After the lead is closed and its generation's work/recovery are settled, delete
its `$HOME/.local/state/herdr-orch/lead-push/<root>/taskr-mcp-<gen>.json` by literal path.

The lead brief says **"herdr-lead adopted you; skip adopt"** and carries the root and generation from the helper's prompt. A second adopt, even in the same pane, fences that channel. Handle pushed events with taskr MCP receipt/inbox/consumer/ack tools and the supplied fence; deduplicate against the inbox before effects. No plain `taskr wait` or `taskr ack` in a push lead. A live lease alone is insufficient: `start` creates a throwaway gate child, emits its ready event, waits up to 360 s (240 s plus the 120 s digest hold) for current-generation receipt and ack, and closes the child accepted. A successor first acks older pushed events in order; it prints that backlog, and each new ack extends the wait by up to 360 s (at most 30 min). Only its final `ROOT live at GEN (event E received and acked)` line accepts startup. `check` prints `live` (exit 0), `stale` or `push-dropped` (exit 1), or `unknown` (exit 2). It checks the lease, generation, and channel flag when the current process is found. It does not prove end-to-end delivery.

Move leads one at a time, starting low-traffic; stop rollout on any failed live check. On failure at or after adoption the helper prints `ROOT= GEN= pane= mcp_config= agent= lease= check_task= log=`. Inspect that state and the ledger before recovery; never blindly rerun, re-adopt, or start a second agent. Recovery and account-pinned resume are in `recovery.md` "Your own session".

For a plain lead, use an authorized `orch-` lane and send the brief with `herdr agent prompt`. It registers its own root if needed (`workflow.md` "Brief variants"), and its first ledger act on the root is `taskr adopt <root> --pane <pane>`. The hub checks that `taskr campaign <root>` does not read `unregistered` before reporting startup. A root without a pane cannot be watched or focused from the owner's view. Every lead brief says owner decisions and actions are owner asks; owner notes only summarize them (`owner.md`). A repository's worktree setup plugin may skip `orch/*` branches. The lead needs no dependencies and runs gates in task worktrees.

Do not type `/restart` in a Claude push lane. Do not accept an update restart
prompt. It drops the channel flag. Use the saved session ID and authorized lane (under hc the hub uses its start form, "herdr-core backend"):

```bash
herdr-hub move <root> --lane <lane> --account <account> -- --resume <session id>
herdr-lead move <root> <pane> --worktree <campaign worktree> --lane <lane> \
  --resume <session id> --account <account> --prompt-file <lead brief>
```

Both commands restore the flag and keep the session. `herdr-lead`
refuses `--resume` without `--account`. `check` reports
`push-dropped` and prints a repair command if the current process lost the flag.
If that hub session started with Code Mode, omit `-- --resume`; or, to keep
the session, run the hub move with `--code-mode on` (README "Code Mode").
See `recovery.md` "Your own session" before a move.

A session with Monitor or CronCreate tasks can show an exit dialog after the
stop keys. If it shows `Exit and stop tasks`, the helper sends Enter to select
that option, then waits for the process to exit. Keep the session ID from the
resume line. The successor must re-create the timers from the handover.

### Optional native jobs for future Claude launches

For an authorized new launch, add `--jobs` before the hub's `--` Claude-argument
separator. Supported paths are `herdr-hub <root>`, `herdr-hub --new <name>`,
`herdr-hub move <root>`, and `herdr-lead start|move <root> <pane>`:

```bash
herdr-hub <root> --jobs --lane opus-high
herdr-hub --new <name> --jobs --lane opus-high
herdr-lead start <root> <pane> --worktree <campaign worktree> --jobs --prompt-file <lead brief>
```

This is optional new-launch configuration, not permission to restart a current
hub. Omit the flag for the unchanged legacy config. With it, the same immutable
`0600` generation file keeps the `taskr` channel and adds `taskr-jobs` running
`taskr mcp --jobs --as ROOT --session GEN --binding TOKEN`. The binding is
`herdr-hub-root-ROOT-gen-GEN` or `herdr-lead-root-ROOT-gen-GEN`, saved in that file.
The hub carries the flag through its detached move helper; repeat it on each
future move that should opt in. `check`/`stop` do not accept it. With `--jobs`,
competing `--mcp-config` arguments after `--` are refused before any effects.

An MCP server restart in the same generation reuses the saved ROOT/GEN/binding
and recovers existing handles. These launch helpers adopt a new generation;
they are not a jobs-server-only restart. After a new adopt, old native routes
release to compatible channel fallback; do not change a binding to reclaim them.
Keep old configs for recovery and process/receipt stable results as described in
the [native jobs guide](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/jobs.md). The flag grants no owner
input permission: owner elicitation stays denied unless an existing exact owner
ask is separately opted in with CLI `taskr jobs allow-ask`.

## New task worktree

A new independent task or branch gets its own task worktree and workspace. The campaign lead creates it under the hub workspace, then uses the returned values, not the hub or campaign workspace or its cwd, for every later tab and pane command:

```bash
wt=$(herdr worktree create --workspace <hub-workspace-id> --branch <type>/<slug> --base origin/main \
  --label "task: <campaign>-<slice>" --no-focus)
task_workspace_id=$(printf '%s\n' "$wt" | jq -er '.result.workspace.workspace_id')
task_cwd=$(printf '%s\n' "$wt" | jq -er '.result.worktree.path')
```

Never create a worktree per agent or only to organize the layout. If the repository links a worktree setup plugin, wait for the ready line that its AGENTS.md names before a worker needs dependencies: `herdr plugin log list --plugin <plugin-id> --limit 5`. Under hc neither command works: see "herdr-core backend".

## First helper takes the root tab

A new task workspace already has one tab with one idle shell pane. Give that pane to the first helper instead of adding a tab and leaving the root tab empty. `worktree create` has no `--env`, so mark the shell as a worker before `agent start`, and label the tab with the agent name. When a personal host skill requires pane setup before `agent start`, such as shell functions that forward the CLI to an account manager, do it in this root pane too. `herdr-launch ... PANE` exports `FLEET_ROLE=worker` and the Codator forwarding functions itself; by hand:

```bash
helper_pane_id=$(herdr pane list --workspace "$task_workspace_id" \
  | jq -er '.result.panes | if length == 1 then .[0].pane_id else error("expected one pane") end')
helper_tab_id=$(herdr pane get "$helper_pane_id" | jq -er '.result.pane.tab_id')
herdr tab rename "$helper_tab_id" <agent-name>
herdr pane run "$helper_pane_id" 'export FLEET_ROLE=worker'
```

## Other helper tabs

Reuse an existing helper tab only after the task record and a live tab and pane inspection show that it belongs to this task. A label such as `agents` is not ownership proof. Do not rearrange an existing session unless the task authorizes it.

For every other helper, including each helper that an orchestrator starts in its own workspace, create its tab with the worker marker and use the root pane that the response returns. Label the tab with the agent name. Parse every ID from the response; stop if a command fails or an ID is missing:

```bash
helper_tab=$(herdr tab create --workspace "$task_workspace_id" --cwd "$task_cwd" \
  --label impl-<slug> --env FLEET_ROLE=worker --env DISABLE_AUTO_UPDATE=true --no-focus)
helper_tab_id=$(printf '%s\n' "$helper_tab" | jq -er '.result.tab.tab_id')
helper_pane_id=$(printf '%s\n' "$helper_tab" | jq -er '.result.root_pane.pane_id')
```

`FLEET_ROLE=worker` marks every process in the tab as a worker. Notification hooks can skip that role, so only the hub notifies the owner (`owner.md`).

## Shared worktrees and naming

Writers may share one task worktree only when their briefs give them disjoint file or module ownership. Serialize overlapping writes. Keep all tracked edits in the task worktree, never in the shared main checkout.

Label the campaign workspace `orch: <campaign>`, the task workspace `task: <campaign>-<slice>`, and each helper tab with its agent name. Agent names keep their role prefix (`impl-`, `mech-`, `rs-`, `rev-`, `arch-`, `glm-`, `orch-`, `gather-`), carry the task slug (`impl-cart-price`, `rev-cart-price`), match `[a-z][a-z0-9_-]{0,31}`, and are unique among live agents.

## After herdr-launch failed

`herdr-launch` stops at the first failed step and prints `step N failed; so far: workspace= task= launch= tab= pane=`. Steps: 1 tab or given pane (the pane must be at its shell prompt), 2 `taskr new` (with `--work`, then `taskr set TASK work=N`), 3 `taskr launch`, 4 pane exports, 5 export check, 6 `herdr agent start` (with `--push` for claude, first a private `--mcp-config` file and then the development-channel dialog; failures also print `mcp_config=` once its path is known), 7 `taskr prompt --confirm` (with no receipt in 60 s it logs the pane, presses Enter once and waits 60 s more for a `got`; still none fails step 7), 8 (codex `--push` only) the push sidecar tab, whose failure also prints `push_tab=` once that tab exists; close it with the worker's tab. Before any manual step, inspect what exists: `taskr status <task>` for a printed task, and the pane (`herdr pane read`, `herdr agent list`) for a started agent. Continue from the first missing step below with the printed ids; never register a second task, start a second agent, or resend a prompt that may have landed (`workflow.md` "Without a ledger" explains stall evidence). A brief pasted into the pane but never submitted is the usual cause of a missing receipt, so the one Enter submits it; a success after it adds `submit_retry=enter` to the final line.

## Manual launch steps

Write the brief file first, on the host that runs `taskr new`: it refuses a `--brief` path that does not exist (exit 2). Register the helper in the ledger after you parse its tab and pane ids and before `agent start`: `taskr new <agent-name> --parent <me> --role <role>` with its workspace, tab, pane, cwd, brief, and report prints the task id, and `taskr launch <task>` with the lane's provider, model, and effort from the resolved lane table (`herdr-launch --lanes`) prints the launch id. Export both ids with the same `pane run` that exports `FLEET_ROLE` or does the host skill's pane setup: `export TASKR_TASK=<task> TASKR_LAUNCH=<launch>`. The `taskr` skill has the full commands.

`agent start` uses the pane that you give it and creates no layout. The pane must be at its interactive shell prompt. Always launch with `herdr agent start --kind codex|claude|opencode|pi` so that Herdr keeps detection and lifecycle tracking (`lanes.md` lists the public lanes); never start an agent with a raw pane command. `agent start` runs the CLI by name in the pane's shell and has no executable override, so a shell function with that name in the pane applies; a personal host skill may define one. The `--` in `herdr agent start ... -- <native args>` belongs to Herdr. Then send the brief with `taskr prompt <task> --file <brief>`.

## --push: the taskr push adapter

`herdr-launch --push ...` (also with PANE) starts the lane with its taskr push adapter, so that taskr events reach an idle worker without a `taskr wait` (taskr [push documentation](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/push.md#start-a-herdr-worker)). Enable the flag only when the brief authorizes a push lane. Inputs are checked before step 1 (exit 2):

- opencode: `--push` is refused before task registration; use normal pane prompts. Legacy private pi lanes can still use `HERDR_PUSH_PI_EXTENSION` with taskr's existing adapter.
- claude: no variable. Step 6 writes `$HOME/.local/state/herdr-orch/lane-push/taskr-mcp-<task>.json` at mode `0600` through a temporary file and no-clobber rename, refusing any existing path. The server args remain `mcp --channel`, using the pane's worker TASKR_TASK/TASKR_LAUNCH. The launcher never reads, creates or checks a cwd `.mcp.json`. The final line includes `mcp_config=<path>`; keep that private file while its lane runs. Step 6 adds `--mcp-config <path>` and `--dangerously-load-development-channels server:taskr`. The latter shows a dialog at every start, so `agent start` fails with agent_not_ready: the script reads the pane, presses Enter on "Loading development channels" (the first option, "I am using this for local development"), and waits up to 90 s for the agent to be idle. No dialog, or no idle, stops at step 6.
- codex: `HERDR_PUSH_CODEX_BIN` is the absolute path of the real Codex CLI (not the Codator wrapper; no quote characters), and the brief must contain `taskr start`: the sidecar needs the thread and home that only `taskr start` in the session records. Step 8 opens a `<name>-push` tab that waits up to 30 min for that registration, then runs `taskr push codex --codex-bin <bin>`; the final line includes `push_tab= push_pane=`. At teardown close the push tab too.

A push worker's inbox lease belongs to the adapter, so a plain `taskr wait` or `taskr ack` from the worker exits 6. Its brief tells it to end its turn after an ask instead of waiting.

After a Claude push lane is closed, delete its `$HOME/.local/state/herdr-orch/lane-push/taskr-mcp-<task>.json` by literal path.

## --work: start a lane from a ticket

`herdr-launch --work W12 ...` (or `--work 12`; not with `--reserved`) starts the lane from PARENT's ticket, so the ticket's spec is not written twice. Before step 1 it reads `taskr --json work list PARENT` and refuses (exit 2, naming the ticket) a ticket that is not in that campaign, is done or cancelled, lacks intent, acceptance, scope or dependencies, or has a dependency that is not done. It then writes `<brief without .md>.W12.md` beside BRIEF, refusing an existing file: BRIEF with `Work: W12 <title>`, `Intent:`, `Acceptance:` (one `- ` line per item) and `Scope:` inserted after its three header lines. Steps 2 and 7 register and send that file, so the ledger holds what the lane saw; BRIEF is unchanged. Step 2 also runs `taskr set TASK work=12`. The final line adds `work_brief=<path>`, and step failures print it once written. A retry after a failed step needs that file removed or a new BRIEF name.

## Unattended flags and trust

The launch helpers run coding agents with full permissions, no sandbox, and hooks always trusted. Workers run unattended in their own tab, where a prompt or a dialog stalls them. `herdr-launch` builds the flags: `start_agent` and `trust_paths` in `bin/herdr-launch` are the source of truth, and a manual launch or a resume copies them from there. Isolation between workers comes from the worktree and the brief's owned and forbidden file lists, not from a sandbox or a workspace per worker. Every launch uses `--timeout 90000`.

- Claude: `--dangerously-skip-permissions`, the lean flags below, and `--settings '<claude settings>'`, with no `sandbox` block. The pane exports `CLAUDE_CODE_SANDBOXED=1`, which is the folder-trust bypass, not a sandbox: it skips the folder-trust dialog in a new worktree and never turns on `sandbox.enabled`. That variable is not documented (found in Claude Code 2.1.292). The shared settings.json sets `permissions.defaultMode: "bypassPermissions"`, so bare `claude` and `herdr agent start --kind claude` without native arguments default to bypass mode. It also keeps `skipDangerousModePermissionPrompt: true`, which `--settings` cannot set.
- Codex: `--dangerously-bypass-approvals-and-sandbox <codex trust> --dangerously-bypass-hook-trust` (the installed Codex has no `--yolo` alias). `<codex trust>` is `-c 'projects={...}'` (the cwd and its main checkout trusted), so no folder-trust dialog appears. `-c` splits a key at every dot, so paths go only in inline-table values.
- OpenCode v2.1.1: `--standalone --auto`. The standalone server inherits this worker's config and hook environment; `--auto` approves permissions that are not explicitly denied. The root TUI has no model or effort flag. The launcher exports process-local `OPENCODE_CONFIG_CONTENT` with the lane's `model`, `default_agent: "build"`, and `agents.build.model` so an existing default-agent setting cannot select another model. No global config is written. Lane effort is recorded but ignored. The CLI resolves from PATH or `~/.opencode/bin/opencode`, runs directly without Codator, and opens without a first-start dialog in the v2.1.1 probe. OpenCode serves another model without saying so when the configured one is not in its catalog, so step 4 requires `opencode models --standalone` in the pane to list the lane's exact model id; otherwise step 5 fails with `opencode models on HOST did not list MODEL (missing from its catalog, or the listing failed); the agent was not started`, and `herdr-launch --check LANE` fails the lane with `opencode models did not list MODEL (missing from its catalog, or the listing failed)`. Nothing is checked after the start: the footer shows a display name that does not map to the id (`opencode/deepseek-v4.1-flash` showed `DeepSeek V4.1 Flash 1017`).

Codex trusts a hook by its file path and command hash. A worktree uses its main checkout's `.codex/hooks.json`, so a Codex profile that has not trusted the repository's current hooks shows "Hooks need review" at every launch. A worker that continues without trusting runs with the repository's hooks off, including any worktree guard. The bypass flag runs the enabled hooks for that launch only, and it gives no access that a full-access worker does not already have.

Every Claude worker launch's `--settings` also sets `agentPushNotifEnabled:false`, `autoMemoryEnabled:false`, `promptSuggestionEnabled:false`, `awaySummaryEnabled:false`, `idleCompaction:false`, and `enabledPlugins` entries to `false` for `posthog@claude-plugins-official`, `cc-plugin-you-should-know@builtin`, `slack@claude-plugins-official`, and `stripe@claude-plugins-official`; the pane also exports `CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1`. These settings prevent phone pushes and auto memory use, and disable background model calls and plugin context that spend tokens outside the task. Claude review seats (brief role `code-reviewer`, `reviewer` or `code-validator`) and `gather-` lanes, including `--gather` helpers, also export `CLAUDE_CODE_PROMPT_CACHE_TTL=5m` (owner, W3438): these lanes are short, and a 5-minute cache write costs 1.25x input against 2x for a 1-hour write; other Claude lanes unset it, so a reused pane does not keep it, and every other lane keeps Claude Code's default. `herdr-lead` and `herdr-hub` omit `autoMemoryEnabled`, so orchestrator sessions keep auto memory. `herdr-lead` and `herdr-hub` both set `posthog@claude-plugins-official` to `false` in `--settings`. `herdr-lead` also sets `cc-plugin-you-should-know@builtin` to `false` (owner, 2026-11-14), so only the hub keeps that plugin; a running lead keeps it until its next start. `herdr-hub` also sets `slack@claude-plugins-official` to `false`. Slack reads and posts belong to the secretary and its Slack lanes. The hub settings take effect when the updated launcher starts a session. `herdr-lead` also sets `autoCompactWindow:250000` (owner ask 469609 B, 2026-11-14), so campaign roots compact at a 250k window. `HERDR_LEAD_COMPACT_WINDOW`, an integer from 100000 to 1000000 in the environment of the process that runs `herdr-lead start` or `move`, replaces that value; any other nonempty value refuses the start with one line before any call or state. `herdr-hub` sets `autoCompactWindow:400000` in `--settings` (owner, 2026-11-23). `HERDR_HUB_COMPACT_WINDOW`, an integer from 100000 to 1000000 in the environment of the process that runs `herdr-hub` (start or move), replaces that value; any other value, including an empty one, refuses the start before any call or state, and `herdr-hub move` passes a set value on to the new hub. Workers (`herdr-launch`), leads (`herdr-lead`) and the hub (`herdr-hub`) each pass `autoCompactEnabled:true` in `--settings`, because a `/config` toggle writes `false` into the user settings file every account on a host shares, and command-line settings outrank user settings, so kit sessions keep autocompact on. Allowed prefixes and efforts per lane come from the resolved lane table (`lanes.md`, `herdr-launch --lanes`).

### Lean Claude lanes

Every Claude worker launch (W2305) starts with only the tools, MCP servers and skill listings workers use. `start_agent` and `claude_settings` in `bin/herdr-launch` are the source of truth; each item names its documented flag or setting (Claude Code CLI reference, tools reference, settings reference, MCP page):

- `--tools Bash,Read,Edit,Write,Grep,Glob,ToolSearch,Skill,WebFetch,WebSearch` (`--tools`: "Restrict which built-in tools Claude can use"; it does not affect MCP tools, so a push lane keeps the taskr tools). Agent, Artifact, AskUserQuestion, ListAgents, ReportFindings, ScheduleWakeup, SendFeedback, SendMessage, Workflow, Monitor, Cron and the other built-ins are gone; without Agent the agent-type listing is gone too. Grep and Glob are listed (W2316) so every lane has the dedicated search tools. EndConversation cannot be removed while other tools remain.
- `--strict-mcp-config`: no `.mcp.json`, `~/.claude.json` or plugin MCP server. A `--push` lane adds only its private `--mcp-config` (server `taskr`); a plain lane has no MCP server. Exception (W2316): a lane whose agent name starts `rs-` or `arch-` starts without `--strict-mcp-config` and keeps the user and project MCP servers, because Hana production reads need Sentry, PlanetScale and Cloudflare MCP; its `--push` `--mcp-config` adds `taskr` to them. The name prefix decides, not the role, so `gather-` researchers stay strict. `herdr-launch --mcp ...` (W2317; claude lanes only, otherwise exit 2 before step 1) does the same for one lane of any name, such as an `impl-` or `rev-` lane that needs Sentry, PlanetScale or Cloudflare; every other flag is unchanged. `disableClaudeAiConnectors:true` also drops the claude.ai connectors Claude Code fetches itself (Claude Docs and others).
- `herdr-launch --deny-tool NAME` (W3332; repeatable, claude lanes only; NAME is 1-200 characters of `A-Za-z0-9_.:*()-`, not starting with `-`; a non-claude lane, `--check`, `--quota`, `--gather` or a bad NAME exits 2 before step 1) adds `--disallowedTools NAME` after `--tools` for each NAME in order, every other flag unchanged; with `--mcp` it blocks one MCP tool, such as `--mcp --deny-tool mcp__planetscale__planetscale_execute_write_query`.
- `herdr-launch --plugin NAME` (repeatable, claude lanes only; NAME is 1-200 characters of `A-Za-z0-9_.@:-`, not starting with `-`) sets `enabledPlugins` NAME true in `--settings` and drops `--strict-mcp-config`, under which a plugin's MCP server does not load. Like `--mcp`, the lane then keeps every user (`~/.claude.json`), project (`.mcp.json`) and enabled plugin MCP server, not only NAME's, so its `--deny-tool` list must cover every write tool of those servers too, or the lane starts from a cwd and account with no other MCP server; for example a read-only Slack lane with `--plugin slack@claude-plugins-official`.
- `herdr-launch --compact-window N` (once, claude lanes only, N 100000-1000000, the `HERDR_LEAD_COMPACT_WINDOW` range) adds `"autoCompactWindow":N` to `--settings`. Both flags exit 2 before step 1 on a non-claude lane, `--check`, `--quota`, `--gather` or a bad value.
- Skills: `disableBundledSkills:true` (bundled skills and workflows), `syncClaudeAiSkills:false` (the account's claude.ai skills; the settings reference moves synced copies to `.trash` only for user or managed settings, so `--settings` only stops loading them, and a later owner session syncs again), and `enabledPlugins` `false` for `cc-plugin-easel@builtin` and `cc-plugin-plugin-authoring@builtin`. Every role except `sub-orchestrator` sets `skillOverrides` `"off"` for `herdr`, `herdr-orchestration` and `herdr-fleet`, which the worker contract says not to load. User and repository skills stay listed, so the Skill tool still loads the skills a brief names. Claude Code has no setting that lists only the brief's skills, and `--disable-slash-commands` would disable the Skill tool, so it is not used.

A manual launch or resume copies these flags with the others.

## Launch matrix

Read the resolved lane table described in `lanes.md`. Use `herdr-launch` for
workers and `herdr-lead` for a Claude lane that allows the `orch` or `hub` prefix.
Both use the same validated kind, model, effort, and prefixes.
Run `herdr-launch --lanes` before choosing a worker lane to see the active table and its path.

## Recurring shell checks

Run `herdr-loop [--every MIN] NAME CHECK BRIEF ROOT` in its own idle task-workspace tab; default MIN is 7. NAME is at most 16 lowercase letters, digits, underscores or hyphens and starts with a letter. CHECK is trusted shell code; its stdout is untrusted items. BRIEF is a complete researcher brief for a source-grounded digest: each run rewrites its report header and adds an items-file path, then uses `herdr-launch sonnet-medium rs-NAME-N`. The handler reports with normal taskr ready/done; ROOT closes its task after accepting the report. Until it is closed (including while ready, done or failed), later distinct outputs queue under `~/.local/state/herdr-loop/NAME`. Failed checks back off to eight intervals and notify ROOT once after three consecutive failures. SIGTERM waits for a running CHECK or launch, saves its result, then stops; a hung command therefore delays shutdown. It interrupts interval sleeps and leaves an active handler alone. Status-read failures also notify ROOT once after three consecutive failures and reset on a good read. Status-read and notice transport failures back off and retry; exit 6 stops the watcher. A stale lock PID is recovered only after `kill -0` fails; an uncertain `starting` marker requires inspecting the recorded launch before manual removal. Keep NAME bound to the same ROOT and CHECK when restarting.

For a GitHub release watch, save this CHECK as an executable file and supply its path as CHECK (replace `OWNER/REPO`). The last-seen marker advances only after successful API/output writes; herdr-loop saves that stdout to its queue before checking again, including during a graceful TERM. These writes are not an atomic or fsynced transaction: a crash or power loss can still lose an in-flight result:

```sh
#!/bin/sh
set -eu
watch_dir=$HOME/.local/state/herdr-loop/releases
mkdir -p "$watch_dir"
tag=$(gh api repos/OWNER/REPO/releases/latest --jq .tag_name)
[ -n "$tag" ] && [ "$tag" != null ] || exit 1
[ "$tag" != "$(cat "$watch_dir/seen" 2>/dev/null || :)" ] || exit 0
printf '%s\n' "$tag"
printf '%s\n' "$tag" >"$watch_dir/seen.tmp"
mv "$watch_dir/seen.tmp" "$watch_dir/seen"
```

Start it in that shell tab with `herdr-loop --every 7 releases /path/to/release-check /path/to/researcher-brief.md ROOT`. This example watches tag changes; the helper also retains the last stdout digest to suppress repeats. Stop with SIGTERM and wait for exit before changing CHECK or cleaning its state; cleaning also resets this last-seen marker.
