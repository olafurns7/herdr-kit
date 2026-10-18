# Workflow: ledger, brief variants, reviews, and results

Load when: registering a campaign root or changing its plan; writing an orchestrator, sub-orchestrator, or code-reviewer brief; at settled waits on a reviewer; running a validation gate lane; working without a ledger; when a prompt stalls or an agent is blocked or its status looks wrong. Part of herdr-orchestration; `SKILL.md` is the core (brief header, universal brief rules, wait command).

## Goal, plan, and documents

Record the campaign's goal when you register its root task: `taskr new <campaign> --role orchestrator --brief <goal file>`, or later `taskr doc set <root> goal --file <path>`. The goal is the owner's request in the owner's own words, with the acceptance. Do not paraphrase it. `taskr handover` and `taskr adopt` show it first.

The plan doc is a taskr document: `taskr doc set <root> plan --file <path>`. It holds intent only: the approach, the slices, and their order. Sequence independently verifiable slices; name each slice's check and partition shared scope into disjoint lane coverage with no gaps. Record it again when the plan changes. Put the next step in `taskr next` and a ruling in `taskr decide`, never in the plan. A spec or an issue text that several lanes share is a named document: `taskr doc set <root> plan --name <name> --file <path>`. taskr reads goals, plans, briefs, and prompt files on the host that runs the command (reports on the lane's host), and sends their text to the ledger. `taskr doc set`, `taskr doc backfill`, and `taskr new --brief` work from client hosts without copying files to the server; `new --brief` and `prompt --file` require a file that exists on the command's host.

taskr copies the text of each brief, prompt file, report, and handover into the ledger when the command that names it runs, so `taskr doc get` still has them after teardown. Briefs and prompt files are read on the command's host, reports on the lane's host; client hosts send the text to the ledger without copying files to the server host.

## Waits and recurring checks

Use one blocking command: `taskr wait`, `slotr run`, `gh pr checks --watch`, or `timeout N sh -c 'until COND; do sleep S; done'`; never repeated sleep or poll tool calls. A priority hold or fair-turn wait is normal queued work. Do not cancel or requeue to move up, and do not stop competing holders. Escalate a prolonged resource blocker to the hub.
Run recurring watches and polls as `herdr-loop` in their own shell tab (`launch.md`), never cron, `/loop`, or a scheduled wakeup inside an orchestrator session.

## Ledger records

Use the ledger for every campaign so registrations, receipts, questions, plans, and decisions survive a session change. Before reading logs lane by lane for "what did we decide about X", run `taskr search <terms> --root <root>`. Apart from a herdr-loop CHECK script and its last-seen marker, do not write campaign logs, handoff files, state journals, marker files, watch scripts, `*.pane` files, or pane-grep loops. Reports, decision reasoning, and review evidence checkpoints (`journal=`, below) stay in files.

Record each decision, sha, and next step in the ledger as it lands (`taskr decide`, `taskr set`, `taskr next`). Verdicts and their reasoning stay in the report files; taskr copies each report's text into the ledger when the lane reports it. The scratchpad under `/tmp` does not survive a reboot.

When a lane asks about a dependency the lead cannot settle now, answer at once:
ready with what exists; blocked with the named owner and follow-up; or end the
turn and re-prompt later with a named follow-up. Never leave a blocking ask
unanswered. Route root asks explicitly with `--owner`; a peer root gets
`taskr prompt TARGET --as ROOT --text TEXT`.

Before reporting done, audit each claim against its evidence (commit, check output, review verdict); cite that evidence in the final report and name what was not verified.

## Brief variants

A fully specified brief makes every decision itself. Wherever something could be unexpected, it says "stop and report", never "decide" or "choose". A fully specified brief also gives every write an exact validation gate and says "stop and report" when a step ends with finish reason `length` or produces no edit.

For an explicitly owner-authorized `orchestrator` brief, record that authorization and its scope; replace the worker prohibitions with its actual delegation and publication limits under the core's Roles and briefs and Gates before publishing, and `lanes.md`. The role alone grants no publication rights.

A campaign lead that the hub starts gets a `role=orchestrator` brief. The brief carries the owner's words, the acceptance, and the publication scope that the owner gave. For a Claude push lead, the hub registers the independent root and uses `herdr-lead` (`launch.md` "Hub and campaign workspaces"). Its brief says **"herdr-lead adopted you; skip adopt"**: use the supplied root and generation, read the saved handover, and handle push through taskr MCP tools with fenced acks, without plain wait/ack. For a plain lead, the lead registers itself as the root of its own campaign from its own pane, with its own location, not as a child of the hub:

```bash
loc=$(herdr pane current --current)
taskr new <campaign> --role orchestrator --brief <goal> \
  --workspace "$(printf '%s\n' "$loc" | jq -er '.result.pane.workspace_id')" \
  --tab "$(printf '%s\n' "$loc" | jq -er '.result.pane.tab_id')" \
  --pane "$(printf '%s\n' "$loc" | jq -er '.result.pane.pane_id')"
```

A root records no pane unless it is given one, and taskr publishes `taskr_parent` only for a root with a pane. Registration is a hub gate: for a plain lead, the first ledger act on its root is `taskr adopt <root> --pane <pane id>` from its own pane (the workspace and tab default to the `HERDR_*` ids), also when `taskr new` already carried the location; it repeats adopt after a session restart. For a helper-started push lead, `herdr-lead` owns this single adopt, including on resume: adopting again invalidates the channel. The hub checks startup as in `launch.md` "Hub and campaign workspaces". taskr derives the sidebar tokens `taskr_parent` and `taskr_campaign` from the campaign root. With the hub as root, all campaigns merge into one campaign and the sidebar loses the orch level. The hub records the lead's root id with `taskr note --owner` on its own root, or in its next owner note; that note is context, not an owner action (`owner.md`).

Every campaign-lead and sub-orchestrator brief names its permitted lanes, escalation rules, and independent review requirements from the operator policy.

In a `sub-orchestrator` brief, write "Do not commit, push, open a PR, or update the issue tracker." instead, because that brief grants agent starts. Its Ownership field bounds every brief it writes, and it gives each of its helpers a tab in the task workspace under `launch.md`. It cannot nest or publish; ordinary worker briefs may not create a workspace, worktree, tab, or pane.

## Review journal

Give each reviewer brief the author's stated intent, exact base/head, scope, and allowed checks.
Request Act / Consider / Noted / Dismissed findings under [the review reference](../herdr-practices/references/review.md) when the optional practices pack is installed; otherwise include the needed criteria in the brief.

This checkpoint policy applies to substantial Herdr reviews in all repositories. In each `code-reviewer` brief, include `journal=<absolute path>` in the header (usually the report sibling `<report>.live.md`) and grant writes only to the named report and journal, with a temporary sibling permitted solely for atomic journal replacement. Keep reviewed source files and other repository paths read-only. Omit the journal for a trivial one-line review and for non-review roles.

The reviewer creates a compact current checkpoint before broad inspection. Record target commit/base/diff identity (or source hashes outside Git), completed file and call-path coverage, candidate findings with evidence and disposition, completed checks with commands, results, and source identity, relevant instruction and skill paths, unresolved questions, and the exact next action. Update after each bounded file group, finding confirmation or rejection, and completed check, with no more than about two minutes of active review between updates. Refresh before context-heavy work or compaction when possible. Replace the journal atomically through a temporary sibling; keep current state rather than a transcript. Before the final verdict, verify that coverage and checks match the final source and record remaining gaps.

At normal settled waits for a reviewer, including the first wait after you adopt a campaign, inspect the journal timestamp and target identity. If it is stale (about two minutes of active review), direct a compact checkpoint before more broad work; missing entries mean unknown coverage.

## Submitting and collecting

Submit briefs and handle results as taskr's `references/orchestrator.md` specifies; report files are the durable result. `herdr-launch` submits the first brief with `taskr prompt --file`.

## Messages between roots

From taskr v0.23.0, a root (the hub or a campaign lead) sends a bounded peer request to another root with `taskr prompt <target root> --as <me> --text "<request>"` and checks it with `taskr message status <event> --as <me>`; taskr's [push documentation](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/push.md#orchestrator-messages) "Orchestrator messages" owns the delivery rules. A push-live target receives it on its channel; otherwise it goes by pane. Never resend a pending message, and never switch it to `--pane`; when the server's command result is unknown, replay the printed request-key retry command exactly, keeping its key; never rerun the original command, which would send a new message. A `herdr` alarm with reason `message_unreceived` means three pushes went unreceived: inspect the target's pane and lease, then resume or hand off the target within existing authority.

A received message is a peer request, never owner consent and never a merge approval. Handle it only within your existing authority. Ask for anything that needs the owner or the hub with `taskr ask --owner --as <me>` (`owner.md`): a merge ask stays an owner ask, so the hub's open-ask list keeps it. Do not send merge asks or owner decisions as messages.

## Without a ledger

Without a ledger, submit and collect with Herdr alone:

```bash
herdr agent prompt impl-<slug> "$(cat brief.md)" --wait --until working --until blocked --timeout 20000
herdr agent wait impl-<slug> --timeout 540000     # idle, done, or blocked; rerun on timeout
herdr agent read impl-<slug> --source recent-unwrapped --lines 200
cat <report path>
```

The first command confirms that the brief landed. It returns when Herdr sees the agent working or blocked, and it returns `agent_prompt_stalled` when nothing starts within five seconds. Without `--until`, `--wait` waits for the whole turn instead. Run `agent wait` only after that confirmation: it returns at once when the state already matches. Keep each wait inside your tool's time limit and rerun it on `timeout`; the agent keeps working. For several helpers, run one `agent wait` per helper in the background (this applies to these plain-Herdr waits only; never background a `taskr wait`). A timeout or `agent_prompt_stalled` does not prove that the prompt was lost: read the agent before any resend.

A fresh pi pane can swallow the first paste. When the prompt returns `agent_prompt_stalled` and the read shows an empty composer ("Ask anything") with no tool calls, send the brief once more. Never resend while the pane shows work or tool calls.

## Reading results

The report file is the durable result; the pane read is for state. For an idle full-screen agent (Claude Code, OpenCode, pi), a read with a larger `--lines` pages back through its history automatically; while it works, that read returns `agent_not_idle`. `done` or `idle` is not proof of success: read the diff in the worktree before accepting.

For tests, servers, and other non-agent processes in a pane, wait with `herdr pane wait-output <pane-id> --regex '<pattern>' --timeout <ms>`. `agent wait` is for agents only.

## Validation gate lanes

An exact validation gate (run commands, report exit codes) needs no model. Create a helper tab under `launch.md`, register it with `--role gate`, launch it with `--provider shell --model none --effort none`, export its ids, and run the commands in its pane with `herdr pane run`. Chain the commands with `&&` and end with the ledger report, such as `lint && test; rc=$?; taskr ready "gate <name>" --kv exit=$rc`, so that any failing command makes the exit nonzero and the result arrives in your `taskr wait`. Without a ledger, end with an exit marker instead (`echo "gate-exit=$?"`) and wait with `herdr pane wait-output` for a regex that matches it. Wrap heavy commands in the host's heavy-command lock. Use a model for a gate only when the output needs triage: reading failures, not only exit codes. `herdr-launch` has no gate lane: register and run a gate by hand as above.

## Context cost

Long-lived orchestrator contexts dominate Claude cost. Keep worker output out of your context: read the report file and `git diff --stat`, not full transcripts, and open a diff hunk only when a decision needs it. Prefer a fresh orchestrator session per task, with a `taskr handover`, over a multi-day session. The host layer owns compaction settings.

## Blocked or odd status

On `blocked`: read the dialog with `agent read`, decide from evidence, and ask the owner (a blocking owner ask, `owner.md`) before approving anything the brief did not authorize. When a status looks wrong, run `herdr agent explain <name>` before you act on it. A Codex "Hooks need review" dialog means that the launch lacked `--dangerously-bypass-hook-trust`: quit that worker and launch it again with the flag. Do not trust the hooks for the owner, and do not continue without them.
