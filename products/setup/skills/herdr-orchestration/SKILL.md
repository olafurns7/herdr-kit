---
name: herdr-orchestration
description: "Delegate work inside Herdr (HERDR_ENV=1): the agent hierarchy, which model runs which lane, launch commands for Claude, Codex, and OpenCode workers, task layout, briefs, the taskr task ledger, waits, teardown, and recovery. Load this skill when orchestrating or sub-orchestrating, before delegating, reviewing, or starting an agent inside Herdr. The upstream `herdr` skill owns CLI syntax."
---

# Herdr orchestration policy

This skill is the team policy for delegating work inside Herdr: **when** to delegate, **where** agents live, and **which** model runs each lane. The upstream `herdr` skill owns CLI syntax. The **owner** is the developer who runs the session. The installed `herdr` is the command authority: use only commands it accepts, never a web-only or remembered one. The upstream skill comes from `herdr --skill`; regenerate it after each Herdr upgrade.

herdr-kit (github.com/olafurns7/herdr-kit) is the canonical home. Its installer puts this skill at `~/.agents/skills/herdr-orchestration` and links it into Claude Code, Codex, and OpenCode; repositories carry no copy. The taskr hub, orchestrators, and sub-orchestrators load it with the upstream `herdr`, `herdr-fleet`, and `taskr` skills. Workers and reviewers do not load it: the global instructions, the taskr skill, and their brief carry the role rules they need. A repository's AGENTS.md fills its slots: worktree setup plugin, skills to load in briefs, issue tracker, review skill, publish workflow, leaf checks (the exact fast commands and their real scope; no build, suite, type check, type-aware lint, or heavy-command lock; any commit hook that runs heavy checks and, only when the head gate runs those checks, the repository's approved way to commit; a leaf never bypasses a hook that the slot does not name), and head gate (the parent's gate command and its cost; `workflow.md` "Head gate"). A repository without Leaf checks and Head gate keeps the commands that the lead writes into Validation. A personal host skill may add account selection, notification wiring, a heavy-command lock, and machine specifics; its private table and overlay define operator-specific lanes and rules.

A host may carry a private `lanes.json` and fleet `local.md`. Review against source text; use the installed text for what a host does today.

Two upstream defaults are overridden. Inside Herdr (`HERDR_ENV=1`), working in Herdr is the request: delegate without waiting for the word. Each helper gets its own tab in the task workspace, not a sibling pane of the caller; in a task workspace that was just created, the first helper takes its root tab.

## Owner questions

The taskr ledger is the single record of owner questions. Before any dialog with the owner, record `taskr ask --owner --dialog --question JSON --as ROOT`; a pane conversation is allowed, an unrecorded dialog is not. Name the repository and item in the question (`Merge herdr-kit #12?`).
Use an ask that is already open; do not re-ask it. The Claude AskUserQuestion hook records the question and answer itself; do not answer it again by hand. Other agents record the answer at once with `taskr answer ASK_ID TEXT`.
If the owner declines the dialog, answer or withdraw the leftover ask. If a second channel gives a new answer, use `taskr answer ASK_ID TEXT --supersede`: newest wins, and both answers stay in the log.
Workers and sub-orchestrators ask their orchestrator with `taskr ask TEXT --blocking`, not the owner.

## Reference files

Load each file at the step that names it, not up front. Each says at its top when to load it.

| File | Load before |
| --- | --- |
| `lanes.md` | choosing a configured lane or effort, a quota fallback, or a resume; splitting work into small verified tasks (loop verification) |
| `launch.md` | a recurring watch or poll (`herdr-loop`); opening the hub workspace; creating a campaign or task worktree or workspace; starting a campaign lead; reusing or rearranging an existing tab or pane; a manual launch, or recovery after `herdr-launch` is missing or failed |
| `workflow.md` | registering a campaign root or changing its plan; durable task-local loops; writing an orchestrator, sub-orchestrator, or code-reviewer brief; settled waits on a reviewer; a head gate; work without a ledger; a stalled, blocked, or odd-looking agent |
| `owner.md` | (hub or campaign lead) asking the owner anything or handing the owner an action, each milestone, parking a campaign, going idle on unattended work, answering "what happened" |
| `teardown.md` | stopping an owned worker, closing a tab or workspace, after a reboot or crash |
| `recovery.md` | any resume or prompt to a restored pane; your own session restarted; the taskr server unreachable |
| `model-selection.md` | choosing a model; operator-specific policy lives in the fleet overlay |

## Roles and briefs

The operator authorizes scope and publication. Leads own plans, typed work
contracts, worker briefs, integration and acceptance. Sub-orchestrators own only
their assigned part and cannot publish or start another sub-orchestrator. Workers
stay in their briefs; deterministic gates need no model; reviewers inspect independently.
Model choices and restrictions come from `lanes.md` and the fleet's private overlay.

Never start in-process subagents. Each helper uses an owned tab in the task
workspace. Never implement outside the brief's owned paths or infer publication
authority from a role. A worker acknowledges the taskr delivery, reports progress
through taskr, and writes the named report. Missing decisions use `taskr ask
--blocking`; follow the taskr skill's plain or push-lane answer protocol.

## Preflight

```bash
test "${HERDR_ENV:-}" = 1
herdr status                    # client and server versions match
herdr agent list                # who is live; reuse or tear down before adding
taskr version                   # the task ledger CLI, pinned by the herdr-kit installer
taskr daemon --status           # daemon fresh or stale; tailnet_url is the hub URL for clients
free -m | awk 'NR==2{print "available_mb="$7}'   # Linux: column 7, never 4
```

Check available memory before adding a heavy agent; keep at most three heavy agents live while dev servers run, and stop dev servers between eval runs. If `taskr` is missing or not the pinned version, tell the owner: herdr-kit's installer (`products/taskr/install.sh`) installs and upgrades it (binary `~/.local/bin/taskr`, skill `~/.agents/skills/taskr/SKILL.md`); until then use the plain Herdr loop in `workflow.md`. `stale: true` from `daemon --status` means an older daemon still runs; the owner runs `taskr daemon --restart`. The daemon is an optimization: `taskr wait` works without it.

Load `~/.agents/skills/taskr/SKILL.md` and its `references/orchestrator.md` for the ledger loop, plan, and handover; `references/recovery.md` when a command fails, an alarm arrives, or a lane must be resumed; and `references/format.md` before decoding output. A reviewer lane needs only the core and `references/review.md`. taskr owns command semantics; Herdr owns transport. Use the ledger for every campaign; record its goal at registration (`workflow.md`). One consumer per inbox: stop a predecessor's wait before adoption (`recovery.md`).

## Launch

Start an ordinary worker in three steps. Stop if any command fails or an id is missing, and never focus a helper.

**1. Get its pane**, in one of two ways.

**(a) A new helper tab in an existing task workspace.** From the calling task pane, resolve live ownership with `herdr pane current --current` (launch-time `HERDR_WORKSPACE_ID` and `HERDR_TAB_ID` can be stale), then create the tab with the worker marker, labelled with the agent name, and parse its ids:

```bash
task_workspace_id=$(herdr pane current --current | jq -er '.result.pane.workspace_id')
task_cwd=$(pwd -P)
helper_tab=$(herdr tab create --workspace "$task_workspace_id" --cwd "$task_cwd" \
  --label <agent-name> --env FLEET_ROLE=worker --env DISABLE_AUTO_UPDATE=true --no-focus)
helper_tab_id=$(printf '%s\n' "$helper_tab" | jq -er '.result.tab.tab_id')
helper_pane_id=$(printf '%s\n' "$helper_tab" | jq -er '.result.root_pane.pane_id')
```

**(b) The root pane of a task workspace you just created** (creating one: `launch.md`). Give its one idle shell pane to the first helper and rename its tab to the agent name; `herdr-launch` exports `FLEET_ROLE=worker` in it and verifies the export:

```bash
helper_pane_id=$(herdr pane list --workspace "$task_workspace_id" \
  | jq -er '.result.panes | if length == 1 then .[0].pane_id else error("expected one pane") end')
helper_tab_id=$(herdr pane get "$helper_pane_id" | jq -er '.result.pane.tab_id')
herdr tab rename "$helper_tab_id" <agent-name>
```

The agent name keeps its role prefix and the task slug (`impl-cart-price`), matches `[a-z][a-z0-9_-]{0,31}`, and is unique among live agents; `herdr-launch` checks the prefix and the pattern.

**2. Write the brief** (below) with that workspace, tab, and pane, in the campaign folder on the host that runs `taskr new`.

**3. Launch:** `herdr-launch [--push] LANE NAME BRIEF REPORT PARENT [PANE]` (without PANE it creates the tab itself in the caller's workspace or `--workspace W`). It refuses a lane that does not launch as NAME's prefix, checks that a given PANE (in any workspace) is at its shell prompt, registers the task and launch, exports `FLEET_ROLE=worker`, `TASKR_TASK`, and `TASKR_LAUNCH` (for Claude also `CLAUDE_CODE_SANDBOXED=1`, the folder-trust bypass; with Codator installed, its forwarding functions), verifies the export before `herdr agent start` with the lane's flags: full permissions, no sandbox, folder and hook trust (`launch.md` "Unattended flags and trust"), then sends the brief with `taskr prompt --file`. It prints `task= launch= tab= pane=`.

On a failure it prints the failed step and the ids obtained so far. Inspect those (`taskr status`, the pane) before any manual step in `launch.md`; never repeat a completed registration, start, or prompt.

```sh
herdr-launch LANE impl-SLUG BRIEF REPORT PARENT [PANE]
```

The lane's allowed prefixes and settings come from the resolved table. Optional
tools and account policy belong to the host overlay.

## Brief and wait

Every brief starts with the header and ends with the report contract:

```text
HERDR-BRIEF role=<orchestrator|sub-orchestrator|implementer|mechanical-worker|researcher|code-reviewer|reviewer|architect|code-validator>
orchestrator=<your agent name or pane id>   report=<absolute path>.md
Task topology: workspace=<task-workspace-id> tab=<owned-helper-tab-id> pane=<owned-pane-id> cwd=<task-worktree-path>

Objective: <one bounded outcome>
Intent: <optional: the orchestrator's own words for what done looks like for the owner>
Non-goals: <explicitly excluded outcomes>
Ownership: <files or module you may edit>   Forbidden: <files you must not touch>
Context: <current behavior, evidence, decisions already made>
Skills to load: <repository skills mapped to this work; code-reviewers also name taskr references/review.md>
Acceptance: <observable conditions>
Validation: <exact commands, one per acceptance item>: leaf checks only (the Leaf checks slot); no build, suite, or gate
  Gate: parent at head (implementer or fixer)
Progress: taskr SKILL.md worker rules

Other agents share this repository. Preserve their edits. Do not commit, push,
open a PR, update the issue tracker, or start agents. If a product, ownership,
or acceptance decision is missing, ask it through taskr as Progress says
(without a ledger, reply NEEDS_ORCHESTRATOR_DECISION with evidence). Write
changed files, summary, validation evidence, risks, and blockers to the report
path, then reply with the path and three lines.
```

An implementer or fixer brief replaces the sentence that starts "Do not commit" with: "You MAY commit your owned paths only, with exactly `git add -- PATHS && git commit -m MSG -- PATHS` and only the hook switch that the Leaf checks slot names. On `index.lock`, retry up to 3 times, a few seconds apart through normal commands, not a sleep loop, then `taskr ask --blocking`; never delete a lock. Do not push, amend, rebase, open a PR, update the issue tracker, or start agents. When committed: `taskr ready SLICE --report REPORT --kv head=<full OID>`, then `taskr done`. Stay in your pane: the lane stays done but open until a green gated head that contains your head (`git merge-base --is-ancestor`) is reviewed READY." Ownership must be disjoint: a path-limited commit does not separate two lanes' edits to one file. Reviewer and researcher briefs stay read-only.

A brief that hands a slotr runtime to a worker (`herdr-fleet`) says the worker stops its run with `slotr stop <run>` before `taskr done`, unless the brief hands the runtime to the lead.

Never run `slotr stop` on another campaign's run to enforce priority. Keep your request queued and ask the hub through taskr with the campaign, pool and observed wait reason. The hub sets campaign weights with `slotr priority set`. slotr applies caps, quiet holds and the fairness floor at admission. Weight changes never revoke an admission. Stop only an owned run under its cleanup or cancellation authority, with `--reason`.

The brief and launch record carry the task topology and the task-qualified agent name; a writing brief lists the helper's disjoint ownership, so that a recovery can confirm the task workspace. An orchestrator, sub-orchestrator, or code-reviewer brief adds the rules in `workflow.md`; write a code-reviewer brief only after reading its journal rules there.

Require each Codex and Claude worker's initial report or checkpoint to record the provider, the effective native-home path, and the native session reference. Inside Codex it queries only `CODEX_HOME` and `CODEX_THREAD_ID` (`printf '%s\n' "$CODEX_HOME" "$CODEX_THREAD_ID"`; empty home means `~/.codex`); inside Claude only `CLAUDE_CONFIG_DIR` (empty means `~/.claude`). No environment dumps, invented Claude variables, or credential or profile stores. `CODEX_THREAD_ID` is the Codex session ID; a Herdr `agent_session` from `agent get` is an optional fallback while the name is live. A Claude session reference counts only if its CLI or Herdr reports it. Validate a saved ID as nonempty and not literal `null`; if none is verified, record that limitation, and stop and report when a resume needs it. Record provider, model, and effort from the actual launch argv, not `agent get`.

When the host skill defines a heavy-command lock, at most one heavy command runs host-wide under it, and your head gates use it too. A leaf gets a wrapped command only for the one focused repro that a failure prompt names (`workflow.md` "Head gate"). Keep a campaign's briefs, reports, and plan doc in `~/.local/state/herdr-orch/<repo>-<campaign>/` on the host where its lanes run, never in a worktree (removed at teardown) or `/tmp` (lost at reboot). Keep database dumps, toolchains, `node_modules`, and repository clones out of it: bulk evidence stays in the task worktree.

Collect with one long wait (default 540000 ms timeout, Codex orchestrators included); pass `--ack` only for a newly handled pending event, and after a timeout or an error that reports `acked_event_id`, do not repeat that ack:

```bash
taskr wait --as <me> --for ready,ask,answer,owner_answer,done,fail,prompt_outcome,herdr,message [--ack <handled>]
```

Never loop short waits, shell `sleep` for a lane, or put this wait in an `&` background job: it dies with the tool call. On a timeout, `w1` with `"owed":0` means no lane owes a result. After `ready`, read the report and the diff, not the pane, and record each decision, sha, and next step as it lands (`workflow.md`). Without a ledger, stalls, gates, and blocked agents: `workflow.md`. After accepting a result: `teardown.md`. Process lane results as they land; never leave a `ready` or `done` unread. When idle with nothing to do, leave a root note that says why, or park after your final note (`owner.md`).

## Gates before publishing

- Never branch, switch, stash, or commit in the shared main checkout. Other agents run there. The orchestrator's own edits to tracked files go to a task worktree first, and every commit, push, and PR runs from that task worktree. A task worktree has one task workspace; it is not a worker-only workspace.
- Implementer report read and the worktree diff inspected by the orchestrator.
- Independent review of a gated head under the brief and operator policy, then the repository's review skill on the final diff.
- A green head gate at the final head that ran the repository's full gate command after `git clean -ffdx` is the publish gate. Else run the full gate once at the final head, under the host's heavy-command lock if it has one. Then publish with the repository's PR workflow from the task worktree. Publication stays with the orchestrator.
- Merge only when verified, evaled where a runtime lane applies, CI green where the repository has CI, and every review comment resolved.

Only the hub and the campaign leads write to the owner (`owner.md`).
