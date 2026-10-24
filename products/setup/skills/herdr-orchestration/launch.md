# Launch: task layout and manual launch

Load when: before opening the taskr hub workspace; before creating a campaign or task worktree or workspace; before starting a campaign lead; before reusing or rearranging an existing tab or pane; for a manual launch, or recovery after `herdr-launch` is missing or failed (first inspect what the script completed, below). The core's Launch section covers an ordinary launch in a new helper tab or a fresh root pane. Part of herdr-orchestration; `SKILL.md` is the core, and its Launch section is the normal path.

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

To restart a running hub onto a new session, the hub records its handover and then runs, in its own pane:

```bash
taskr handover --as <root> --note "<checkpoint>"
herdr-hub move <root> --lane opus-high    # --no-handover skips the handover check
```

`move` refuses unless the root is recorded at this pane and has a handover newer than its latest adopt. It needs `setsid` (util-linux; macOS lacks it by default) and fails before anything is stopped without it. It records the Claude session of the agent in this pane, starts the helper with `setsid` (log `lead-push/<root>/hub-move-<pid>.log`), and returns only once the helper reports that it leads its own process group; the hub then ends its turn. The helper waits for the hub to go idle and stops it with `herdr-lead stop <root> <pane> --session <sid>`, which refuses a pane now holding another agent. It waits for the lease to expire, types `herdr-hub <root> --lane <lane>` at the pane's shell, and follows `hub-check-<gen>.result` for up to 2520 s, the prover's full lease and backlog budget. A helper failure is noted on the root.

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
record `live` or `failed: REASON`; `check` reports lease/generation only (0/1/2:
live/stale/unknown). After a failure, see `recovery.md` for `start.lock`.
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
# Alternatively, replace an existing plain lead after stopping it:
herdr-lead start <root> <pane> --worktree <campaign worktree> --predecessor-pane <old pane> --prompt-file <lead brief>
# Or move a running lead to a new session in one command (same pane, or name another):
herdr-lead move <root> <pane> --worktree <campaign worktree> --prompt-file <lead brief>
herdr-lead check <root>
```

`move` first checks the destination (free name and pane, location; for another pane also its shell prompt and host) and that the agent in the root's recorded pane is the root's lead `orch-<slug>` on this host. It then asks that lead for `taskr handover` (`--no-handover` skips this) and fails before stopping anything if none is recorded. It then stops that lead (`herdr-lead stop <root> <pane>`: wait for idle, `esc ctrl+c ctrl+c`, at most 120 s, shell prompt, lease free within 120 s) and runs `start` with that pane as predecessor. `stop` sends keys only to the root's lead: it rechecks the agent's name and Claude session right before each key sequence and refuses any other occupant of the pane. Run `start` or `move` detached or with a tool timeout of at least 30 min. Each run logs its steps to `lead-push/<root>/lead-start-<pid>.log`.

Do not use `herdr-launch` for a lead: it registers a child. `herdr-lead` accepts any Claude lane that allows `orch-` (default `opus-high`), checks the target shell and predecessor lease, clears worker identity, adopts the root once, and keeps the worktree cwd. It writes immutable `0600` configs and handovers under `$HOME/.local/state/herdr-orch/lead-push/<root>/`, passes the config with `--mcp-config`, and answers the development-channel dialog. It never writes `.mcp.json`. Its acceptance check must still pass on every start.

After the lead is closed and its generation's work/recovery are settled, delete
its `$HOME/.local/state/herdr-orch/lead-push/<root>/taskr-mcp-<gen>.json` by literal path.

The lead brief says **"herdr-lead adopted you; skip adopt"** and carries the root and generation from the helper's prompt. A second adopt, even in the same pane, fences that channel. Handle pushed events with taskr MCP receipt/inbox/consumer/ack tools and the supplied fence; deduplicate against the inbox before effects. No plain `taskr wait` or `taskr ack` in a push lead. A live lease alone is insufficient: `start` creates a throwaway gate child, emits its ready event, waits up to 240 s for current-generation receipt and ack, and closes the child accepted. A successor first acks older pushed events in order; it prints that backlog, and each new ack extends the wait by up to 240 s (at most 30 min). Only its final `ROOT live at GEN (event E received and acked)` line accepts startup. `check` prints `live`, `stale`, or `unknown` (exit 0, 1, or 2); it checks lease/generation only, not end-to-end delivery.

Move leads one at a time, starting low-traffic; stop rollout on any failed live check. On failure at or after adoption the helper prints `ROOT= GEN= pane= mcp_config= agent= lease= check_task= log=`. Inspect that state and the ledger before recovery; never blindly rerun, re-adopt, or start a second agent. Recovery and account-pinned resume are in `recovery.md` "Your own session".

For a plain lead, use an authorized `orch-` lane and send the brief with `herdr agent prompt`. It registers its own root if needed (`workflow.md` "Brief variants"), and its first ledger act on the root is `taskr adopt <root> --pane <pane>`. The hub checks that `taskr campaign <root>` does not read `unregistered` before reporting startup. A root without a pane cannot be watched or focused from the owner's view. Every lead brief says owner decisions and actions are owner asks; owner notes only summarize them (`owner.md`). A repository's worktree setup plugin may skip `orch/*` branches. The lead needs no dependencies and runs gates in task worktrees.

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

Never create a worktree per agent or only to organize the layout. If the repository links a worktree setup plugin, wait for the ready line that its AGENTS.md names before a worker needs dependencies: `herdr plugin log list --plugin <plugin-id> --limit 5`.

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

`herdr-launch` stops at the first failed step and prints `step N failed; so far: workspace= task= launch= tab= pane=`. Steps: 1 tab or given pane (the pane must be at its shell prompt), 2 `taskr new`, 3 `taskr launch`, 4 pane exports, 5 export check, 6 `herdr agent start` (with `--push` for claude, first a private `--mcp-config` file and then the development-channel dialog; failures also print `mcp_config=` once its path is known), 7 `taskr prompt`, 8 (codex `--push` only) the push sidecar tab, whose failure also prints `push_tab=` once that tab exists; close it with the worker's tab. Before any manual step, inspect what exists: `taskr status <task>` for a printed task, and the pane (`herdr pane read`, `herdr agent list`) for a started agent. Continue from the first missing step below with the printed ids; never register a second task, start a second agent, or resend a prompt that may have landed (`workflow.md` "Without a ledger" explains stall evidence).

## Manual launch steps

Write the brief file first, on the host that runs `taskr new`: it refuses a `--brief` path that does not exist (exit 2). Register the helper in the ledger after you parse its tab and pane ids and before `agent start`: `taskr new <agent-name> --parent <me> --role <role>` with its workspace, tab, pane, cwd, brief, and report prints the task id, and `taskr launch <task>` with the lane's provider, model, and effort from the resolved lane table (`herdr-launch --lanes`) prints the launch id. Export both ids with the same `pane run` that exports `FLEET_ROLE` or does the host skill's pane setup: `export TASKR_TASK=<task> TASKR_LAUNCH=<launch>`. The `taskr` skill has the full commands.

`agent start` uses the pane that you give it and creates no layout. The pane must be at its interactive shell prompt. Always launch with `herdr agent start --kind codex|claude|opencode|pi` so that Herdr keeps detection and lifecycle tracking (`lanes.md` lists the public lanes); never start an agent with a raw pane command. `agent start` runs the CLI by name in the pane's shell and has no executable override, so a shell function with that name in the pane applies; a personal host skill may define one. The `--` in `herdr agent start ... -- <native args>` belongs to Herdr. Then send the brief with `taskr prompt <task> --file <brief>`.

## --push: the taskr push adapter

`herdr-launch --push ...` (also with PANE) starts the lane with its taskr push adapter, so that taskr events reach an idle worker without a `taskr wait` (taskr [push documentation](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/push.md#start-a-herdr-worker)). Enable the flag only when the brief authorizes a push lane. Inputs are checked before step 1 (exit 2):

- opencode: `--push` is refused before task registration; use normal pane prompts. Legacy private pi lanes can still use `HERDR_PUSH_PI_EXTENSION` with taskr's existing adapter.
- claude: no variable. Step 6 writes `$HOME/.local/state/herdr-orch/lane-push/taskr-mcp-<task>.json` at mode `0600` through a temporary file and no-clobber rename, refusing any existing path. The server args remain `mcp --channel`, using the pane's worker TASKR_TASK/TASKR_LAUNCH. The launcher never reads, creates or checks a cwd `.mcp.json`. The final line ends `mcp_config=<path>`; keep that private file while its lane runs. Step 6 adds `--mcp-config <path>` and `--dangerously-load-development-channels server:taskr`. The latter shows a dialog at every start, so `agent start` fails with agent_not_ready: the script reads the pane, presses Enter on "Loading development channels" (the first option, "I am using this for local development"), and waits up to 90 s for the agent to be idle. No dialog, or no idle, stops at step 6.
- codex: `HERDR_PUSH_CODEX_BIN` is the absolute path of the real Codex CLI (not the Codator wrapper; no quote characters), and the brief must contain `taskr start`: the sidecar needs the thread and home that only `taskr start` in the session records. Step 8 opens a `<name>-push` tab that waits up to 30 min for that registration, then runs `taskr push codex --codex-bin <bin>`; the final line ends `push_tab= push_pane=`. At teardown close the push tab too.

A push worker's inbox lease belongs to the adapter, so a plain `taskr wait` or `taskr ack` from the worker exits 6. Its brief tells it to end its turn after an ask instead of waiting.

After a Claude push lane is closed, delete its `$HOME/.local/state/herdr-orch/lane-push/taskr-mcp-<task>.json` by literal path.

## Unattended flags and trust

The launch helpers run coding agents with full permissions, no sandbox, and hooks always trusted. Workers run unattended in their own tab, where a prompt or a dialog stalls them. `herdr-launch` builds the flags: `start_agent` and `trust_paths` in `bin/herdr-launch` are the source of truth, and a manual launch or a resume copies them from there. Isolation between workers comes from the worktree and the brief's owned and forbidden file lists, not from a sandbox or a workspace per worker. Every launch uses `--timeout 90000`.

- Claude: `--dangerously-skip-permissions` and `--settings '<claude settings>'`, with no `sandbox` block. The pane exports `CLAUDE_CODE_SANDBOXED=1`, which is the folder-trust bypass, not a sandbox: it skips the folder-trust dialog in a new worktree and never turns on `sandbox.enabled`. That variable is not documented (found in Claude Code 2.1.292). The shared settings.json sets `permissions.defaultMode: "bypassPermissions"`, so bare `claude` and `herdr agent start --kind claude` without native arguments default to bypass mode. It also keeps `skipDangerousModePermissionPrompt: true`, which `--settings` cannot set.
- Codex: `--dangerously-bypass-approvals-and-sandbox <codex trust> --dangerously-bypass-hook-trust` (the installed Codex has no `--yolo` alias). `<codex trust>` is `-c 'projects={...}'` (the cwd and its main checkout trusted), so no folder-trust dialog appears. `-c` splits a key at every dot, so paths go only in inline-table values.
- OpenCode v2.1.1: `--standalone --auto`. The standalone server inherits this worker's config and hook environment; `--auto` approves permissions that are not explicitly denied. The root TUI has no model or effort flag. The launcher exports process-local `OPENCODE_CONFIG_CONTENT` with the lane's `model`, `default_agent: "build"`, and `agents.build.model` so an existing default-agent setting cannot select another model. No global config is written. Lane effort is recorded but ignored. The CLI resolves from PATH or `~/.opencode/bin/opencode`, runs directly without Codator, and opens without a first-start dialog in the v2.1.1 probe.

Codex trusts a hook by its file path and command hash. A worktree uses its main checkout's `.codex/hooks.json`, so a Codex profile that has not trusted the repository's current hooks shows "Hooks need review" at every launch. A worker that continues without trusting runs with the repository's hooks off, including any worktree guard. The bypass flag runs the enabled hooks for that launch only, and it gives no access that a full-access worker does not already have.

Every Claude worker launch's `--settings` also sets `"agentPushNotifEnabled":false,"enabledPlugins":{"posthog@claude-plugins-official":false}` so that it cannot push to the owner's phone and starts without the PostHog plugin's skill list. Allowed prefixes and efforts per lane come from the resolved lane table (`lanes.md`, `herdr-launch --lanes`).

## Launch matrix

Read the resolved lane table described in `lanes.md`. Use `herdr-launch` for
workers and `herdr-lead` for a Claude lane that allows the `orch` prefix.
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
