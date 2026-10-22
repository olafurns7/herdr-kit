# Workflow: ledger, brief variants, reviews, and results

Load when: registering a campaign root or changing its plan; splitting work into small verified tasks; writing an orchestrator, sub-orchestrator, or code-reviewer brief; at settled waits on a reviewer; running a head gate; working without a ledger; when a prompt stalls or an agent is blocked or its status looks wrong. Part of herdr-orchestration; `SKILL.md` is the core (brief header, universal brief rules, wait command).

## Goal, plan, and documents

Record the campaign's goal when you register its root task: `taskr new <campaign> --role orchestrator --brief <goal file>`, or later `taskr doc set <root> goal --file <path>`. The goal is the owner's request in the owner's own words, with the acceptance. Do not paraphrase it. `taskr handover` and `taskr adopt` show it first.

The plan doc is a taskr document: `taskr doc set <root> plan --file <path>`. It holds intent only: the approach, the slices, and their order. Sequence independently verifiable slices; name each slice's check and partition shared scope into disjoint lane coverage with no gaps. Record it again when the plan changes. Put the next step in `taskr next` and a ruling in `taskr decide`, never in the plan. A spec or an issue text that several lanes share is a named document: `taskr doc set <root> plan --name <name> --file <path>`. taskr reads goals, plans, briefs, and prompt files on the host that runs the command (reports on the lane's host), and captures their text and handovers when the command that names them runs, so `taskr doc get` keeps them after teardown. `taskr doc set`, `taskr doc backfill`, and `taskr new --brief` work from client hosts without copying files to the server; `new --brief` and `prompt --file` require a file that exists on the command's host.

## Loop verification

For velocity, a lead or sub-orchestrator splits work into small gated tasks, runs fast implementers in parallel, and has one verifier review the work of several.

- **Small task:** one concern, about 150 changed lines or fewer, files disjoint from its siblings (siblings may share a task worktree only on disjoint files). Its brief carries an exact Validation gate from checks the repository already has: an existing end-to-end, eval or QA check, extended where needed (red before, green after), or typecheck, lint or build plus recorded runtime proof. Never a new standalone unit, component, helper, tooling or change-detector test. No usable gate means it is not a small task.
- **Lanes** (`lanes.md` and the operator overlay decide): fast implementers on the cheapest allowed implementation lanes, such as `sonnet-high` or `terra-medium`, and the OpenCode free lanes only for simple tasks with an exact gate. One verifier per 3-5 tasks on `opus-medium` or `sol-medium` (`rev-`), always the other provider: a verifier never verifies its own model. Money, security, the critical turn path, architecture, diagnosis, and anything without an exact gate stay on strong implementers (`opus-high`, `sol-high`).
- **Loop:** the implementer changes, runs its Validation, commits, and reports `ready` with the head and gate output. The verifier reads the diff against the brief, reruns the Validation, rejects any new standalone test, and returns READY or a FIX list with `file:line`. The same implementer fixes; the verifier reviews only the delta. After 3 loops without READY, reassign to a strong implementer.
- **Fold:** a verifier READY lets the lead fold the task into its PR or stack. PR gates are unchanged: a head gate, a full review at the PR head by a provider other than the implementer's (a verifier READY counts when it covered the full PR diff), and the publish rules in `SKILL.md`. Keep the queue full: start the next task as one turns READY and close finished lanes at once.

## Waits and recurring checks

Use one blocking command: `taskr wait`, `slotr run`, `gh pr checks --watch`, or `timeout N sh -c 'until COND; do sleep S; done'`; never repeated sleep or poll tool calls. A priority hold or fair-turn wait is normal queued work. Do not cancel or requeue to move up, and do not stop competing holders. Escalate a prolonged resource blocker to the taskr hub.
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

For an explicitly owner-authorized `orchestrator` brief, record that authorization and its scope; replace the worker prohibitions with its actual delegation and publication limits under the core's "Roles and briefs" and "Gates before publishing" sections and `lanes.md`. The role alone grants no publication rights.

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

Give each reviewer brief the author's stated intent, exact base/head, scope, and allowed checks. A code-reviewer brief names `head=OID` and the green gate receipt (attempt, command, scope, exit, log). The reviewer reads `BASE..OID`, checks that the receipt matches the head and names skipped suites, and runs no build or suite. For a check that the gate does not cover, it asks the parent with `taskr ask --blocking`; the parent runs that check in the gate lane as a new attempt and answers with its receipt. Without the receipt, the reviewer withholds READY. Gate green is evidence to inspect, not proof of correctness. A review fix goes to the done-but-open lane or a new fixer; its new head needs a new gate and review at that head.
Request Act / Consider / Noted / Dismissed findings under [the review reference](../herdr-practices/references/review.md) when the optional practices pack is installed; otherwise include the needed criteria in the brief.

This checkpoint policy applies to substantial Herdr reviews in all repositories. In each `code-reviewer` brief, include `journal=<absolute path>` in the header (usually the report sibling `<report>.live.md`) and grant writes only to the named report and journal, with a temporary sibling permitted solely for atomic journal replacement. Keep reviewed source files and other repository paths read-only. Omit the journal for a trivial one-line review and for non-review roles.

The reviewer creates a compact current checkpoint before broad inspection. Record target commit/base/diff identity (or source hashes outside Git), completed file and call-path coverage, candidate findings with evidence and disposition, completed checks with commands, results, and source identity, relevant instruction and skill paths, unresolved questions, and the exact next action. Update after each bounded file group, finding confirmation or rejection, and completed check, with no more than about two minutes of active review between updates. Refresh before context-heavy work or compaction when possible. Replace the journal atomically through a temporary sibling; keep current state rather than a transcript. Before the final verdict, verify that coverage and checks match the final source and record remaining gaps.

At normal settled waits for a reviewer, including the first wait after you adopt a campaign, inspect the journal timestamp and target identity. If it is stale (about two minutes of active review), direct a compact checkpoint before more broad work; missing entries mean unknown coverage.

## Submitting and collecting

Submit briefs and handle results as taskr's `references/orchestrator.md` specifies; report files are the durable result. `herdr-launch` submits the first brief with `taskr prompt --file`.

## Messages between roots

A root (the hub or a campaign lead) sends a bounded peer request to another root with `taskr prompt <target root> --as <me> --text "<request>"` and checks it with `taskr message status <event> --as <me>`; taskr's [push documentation](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/push.md#orchestrator-messages) "Orchestrator messages" owns the delivery rules. A push-live target receives it on its channel; otherwise it goes by pane. Never resend a pending message, and never switch it to `--pane`; when the server's command result is unknown, replay the printed request-key retry command exactly, keeping its key; never rerun the original command, which would send a new message. A `herdr` alarm with reason `message_unreceived` means three pushes went unreceived: inspect the target's pane and lease, then resume or hand off the target within existing authority.

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

## Head gate

Leaves run only the repository's Leaf checks, commit their owned paths, and report `ready --kv head=OID`. The parent (lead or sub-orchestrator) runs one clean, admitted gate per head; a gate needs no model. Set up once per campaign: `git worktree add --detach <campaign worktree>-gate` (disposable, never an editing worktree); a helper tab under `launch.md`, registered with `--role gate`, launched with `--provider shell --model none --effort none`, ids exported (`herdr-launch` has no gate lane); and the evidence folder `<campaign folder>/gates/`, outside the gate worktree. For each attempt N, pin the full OID and send this line with one `herdr pane run` (GATE_CMD and its cost from the Head gate slot; INSTALL_STEP is `true` when the gate installs dependencies itself):

```sh
L=/ABS/CAMPAIGN/gates/OID12-N.log   # absolute, outside the gate worktree, one per attempt
( cd GATE_WT && git checkout --detach --force OID && git clean -ffdxq || exit 90
  command -v slotr >/dev/null || exit 91
  exec slotr run --pool heavy --cost MIB --campaign C --purpose "gate OID12 #N" \
    --task "$TASKR_TASK" --pane "$HERDR_PANE_ID" -- sh -c 'INSTALL_STEP || exit 92; GATE_CMD'
) >"$L" 2>&1; rc=$?; test -s "$L" || rc=93
taskr ready "gate OID12 #N exit=$rc" --report "$L" --kv head=OID --kv exit=$rc --kv cmd=CMD_ID --kv clean=ffdx
```

Do not add `tee`: the exit status comes from the command. While a host's installed slotr predates the supervisor's exit-status file (`slotr: PASS RUN` / `slotr: FAIL RUN: exit N` lines), its client can report success for a failed unit, so a slotr run passes only when all hold: the unit is inactive; its journal (`journalctl --user -u RUN`) has no `Main process exited, code=exited, status=` or `Failed with result` line; the lane's own exit is 0; and there is a `Consumed` journal line or a program-written result file with pass counts, cited by path and sha256. A successful `ready` does not prove the log was captured: read the log path. A repository may keep a dependency cache through `git clean` only when its Head gate slot names the cache and says how its validity for the gated inputs is shown; by default `git clean -ffdxq` removes it. Exit 90 (checkout or clean), 91 (no slotr), 92 (install), 93 (no log), and a slotr stop are incomplete, not red, and no lane is charged. A host without slotr drops the slotr check and wrapper by an explicit brief line, never by fallback. Without a ledger, end with `echo "gate-exit=$rc"` and wait with `herdr pane wait-output` for it.

Run one gate at a time per campaign. When a gate ends, gate the newest head that contains every reported ready head (`git merge-base --is-ancestor`); this batches heads. The dedupe key is the full OID, the exact gate command (with scope and BASE), and the cleanup; do not run a green key again, and run a different key at the same OID as a new attempt. Record the latest `gate.head`, `gate.exit`, and `gate.log` on the root with `taskr set`. A stopped gate (exit 75 with `slotr: stopped RUN: REASON`) is incomplete: keep the run and reason, retry once for `emergency_available_mib`, `stop_available_mib`, `stop_psi_full_avg10`, or `lease_expired`; a second stop, a `manual` stop, another or unknown reason, or a configuration error is the parent's decision (raise the cost or lease, gate later, or ask the owner). A payload's own exit 75 without the slotr line is red.

On red, failing files are a hint, not proof of cause: read the failure and its dependency path. Send the failure to the one lane at fault, or each lane its part; when the cause is unclear (tooling, lockfile, a provider change that breaks a consumer), start an integration fixer with the batch's heads. Send it with `taskr prompt LANE --file gates/FAIL-OID12-N.md`: head, command, exit, failing excerpt, log path, and optionally the one focused command, under the heavy-command lock, that the leaf may run once per fix round. The leaf fixes, commits, and reports a new head. This policy caps fix rounds: after three red gates charged to one lane, the parent decides (fixer lane, smaller slice, or owner ask). On green, each lane whose reported head the gated head contains (`git merge-base --is-ancestor`) stays done but open until review at that exact gated head is READY; then close them (`accepted` or `reworked`) and tear down their tabs.

With a branch per lane, the parent merges each lane branch into the campaign branch. On a conflict, record base, head, and paths, run `git merge --abort`, and prompt the leaf: "merge campaign base BASE into your branch, resolve inside your owned paths, commit, report the new head". A conflict in paths that the leaf does not own goes to the parent or a fixer. Then merge again.

## Context cost

Long-lived orchestrator contexts dominate Claude cost. Keep worker output out of your context: read the report file and `git diff --stat`, not full transcripts, and open a diff hunk only when a decision needs it. Prefer a fresh orchestrator session per task, with a `taskr handover`, over a multi-day session. The host layer owns compaction settings.

## Blocked or odd status

On `blocked`: read the dialog with `agent read`, decide from evidence, and ask the owner (a blocking owner ask, `owner.md`) before approving anything the brief did not authorize. When a status looks wrong, run `herdr agent explain <name>` before you act on it. A Codex "Hooks need review" dialog means that the launch lacked `--dangerously-bypass-hook-trust`: quit that worker and launch it again with the flag. Do not trust the hooks for the owner, and do not continue without them.
