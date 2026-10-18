# Personal agent instructions

## Herdr environment

At session start run `test "${HERDR_ENV:-}" = 1`.

If it fails, you are not inside Herdr. Skip all Herdr guidance and do not
inspect or control Herdr.

If it passes, load skills by role:

- Workers (a `HERDR-BRIEF` whose role is not reviewer, sub-orchestrator or
  orchestrator) follow the worker contract appended to the prompt. Load
  `~/.agents/skills/taskr/SKILL.md` or a taskr reference only when the brief
  names it or a taskr command fails, and load the repository skills named
  in the brief. Do not load `herdr`, `herdr-orchestration`, or `herdr-fleet`.
  Without a ledger (no `TASKR_TASK`), write the report to the brief's path
  and reply with that path and three lines; for a missing decision, stop
  and reply `NEEDS_ORCHESTRATOR_DECISION` with evidence.
- Reviewers (`role=reviewer`) load `~/.agents/skills/taskr/SKILL.md`,
  `~/.agents/skills/taskr/references/review.md`, and the repository review
  skill named in the brief. Do not load `herdr`, `herdr-orchestration`, or `herdr-fleet`.
- Orchestrators, sub-orchestrators, and any session without a `HERDR-BRIEF`
  load these skills before delegating, reviewing, or running any `herdr`
  command:

1. `~/.agents/skills/herdr/SKILL.md` — upstream CLI contract.
2. `~/.agents/skills/herdr-orchestration/SKILL.md` — the team's agent
   hierarchy, model policy, launch commands, task ledger use, and delegation
   loop. herdr-kit installs this copy; it is the only one.
3. `~/.agents/skills/herdr-fleet/SKILL.md` — the owner's Codator accounts,
   taskr host setup, notifications, and host specifics.
4. `~/.agents/skills/taskr/SKILL.md` — the task ledger commands. Use taskr
   for every Herdr campaign: registration, briefs, waits, asks, reports, the
   plan, and handover. A repository's AGENTS.md fills only its repository
   slots (worktree plugin, skills to load in briefs, review skill, publish
   workflow).

## Standing delegation policy (HERDR_ENV=1)

These rules hold even when a skill fails to load. The skills own everything
else: model lanes, launch commands, accounts, layout, briefs, and teardown.

- Inside Herdr the owner's standing instruction is the request to use Herdr.
  Do not wait for the word "Herdr" in a prompt.
- Never start in-process subagents, forks, workflows, or native agent
  threads (Claude `Agent`/`Workflow`/`Task`; Codex `spawn_agent` roles).
  Start each helper with `herdr agent start` in its own tab in the task
  workspace, created with `--env FLEET_ROLE=worker --no-focus`: never a
  workspace or worktree per worker, and never a split of the owner's pane.
  In a task workspace that was just created, the first helper takes the
  existing root tab instead of adding one.
- Model and lane restrictions come from the lane table (`herdr-launch --lanes`)
  and the operator's private overlay. Never use a model or lane those forbid;
  never let a model review its own implementation.
- If your first prompt begins with `HERDR-BRIEF`, or tells you to read a
  file whose first line is `HERDR-BRIEF`, stay in the brief and write the
  report. You are a worker and cannot delegate unless the role is
  `sub-orchestrator` or explicitly owner-authorized `orchestrator`, subject
  to the orchestration skill's model and scope limits. Sub-orchestrators
  cannot start another sub-orchestrator or publish. A top-orchestrator role
  grants no publication authority; that requires the task's authorization.
- The flag identifies the environment. It does not authorize destructive
  actions, closing others' workspaces, or publishing; those follow the task's
  own authorization.

Never start, restart, or let a `herdr` CLI call auto-spawn `herdr server` from inside an
agent session. The daemon inherits the agent's Claude Code environment (`CLAUDECODE`,
`CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_PID`, ...), and every later pane shell then starts
`claude` as a child session with transcripts and prompt history disabled. If the user
wants a server restart, give them the commands to run from a terminal outside any agent
session instead.

## Owner questions (HERDR_ENV=1)

The taskr ledger is the single record of owner questions. Name the repository
and item in each question (`Merge herdr-kit #12?`). Before any dialog
with the owner, record `taskr ask --owner --dialog --question JSON --as ROOT`.
A pane conversation is allowed; an unrecorded dialog is not. Use an ask that
is already open; do not re-ask it. The Claude AskUserQuestion hook records the
question and answer itself; do not answer it again by hand. Other agents record
the answer at once with `taskr answer ASK_ID TEXT`. If the owner declines the
dialog, answer or withdraw the leftover ask. If a second channel
gives a new answer, use `taskr answer ASK_ID TEXT --supersede`: newest wins,
and both answers stay in the log. Workers and sub-orchestrators ask their
orchestrator with `taskr ask TEXT --blocking`, not the owner.
