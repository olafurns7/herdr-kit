# Recovery: outages, restarts, and resumes

Load when: before any resume and before prompting a restored pane; when your own orchestrator session ended or restarted; when the taskr server is unreachable; after a Herdr server restart, reboot, or crash. Part of herdr-orchestration; `SKILL.md` is the core.

## Your own session

Before a planned stop, handover, or context compaction, finish or back out of the current atomic step. Refresh the [campaign plan](workflow.md#goal-plan-and-documents) if intent changed and record `taskr next`; checkpoint with `taskr handover --as <me> --note "<checkpoint>"`: evidence for done work, each in-flight lane's expected next event, and proof gaps. Under "Session timers to re-arm" list each session-scoped timer (Claude CronCreate job, background loop) with its schedule and full prompt; they die with the session, and the successor re-creates them and notes the new ids. Use the existing ledger and reports, not a new checkpoint format.

On resume, read the campaign plan, ledger, and latest reports before re-deriving work; use `taskr search <terms> --root <root>` for a missing decision.

For a plain orchestrator, a new session starts with `taskr adopt <me>` and continues from the handover and inbox (`workflow.md` "Goal, plan, and documents"). Stop the predecessor's wait before adoption, then arm your own `taskr wait`.

For a running Claude push lead, `herdr-lead move <root> <pane> --worktree <campaign worktree> --prompt-file <lead brief>` does the whole restart: it asks for the handover, stops the lead, waits for the lease, then starts and proves the successor (`launch.md`); a hub restarts itself with `herdr-hub move <root>`. To do it by hand, settle the handover and quit the predecessor first (`herdr-lead stop <root> <pane>` waits for idle, quits it, and waits for the lease). Verify that no live agent occupies its pane and wait for `taskr consumer show --as <root>` to report `live:false`; quitting can leave the lease live until its 60 s expiry. A running channel keeps renewing, so waiting alone does not stop it. Start the successor on the same host with:

```bash
herdr-lead start <root> <pane> --worktree <campaign worktree> --predecessor-pane <old pane> --prompt-file <lead brief>
# Or resume the recorded nonempty, non-null Claude session under its recorded account:
herdr-lead start <root> <pane> --worktree <campaign worktree> --predecessor-pane <old pane> \
  --resume <session id> --account <account> --prompt-file <lead brief>
```

Verify the saved account/native home before resuming; Codator selects that home through `--account`, which the helper places first after `--`. The helper waits up to 75 s for a predecessor lease, adopts once, saves the handover, and prompts the lead with its generation in a single line naming the absolute prompt-file path and ending in `lead-push-gen-<gen>`; on a stalled prompt, step 7 checks that token before each of at most four Enter retries, spaced by 5 s then 10 s, and fails if Claude does not start working on it. The lead **skips adopt**, uses taskr MCP tools for pushed events and fenced acks, and never runs plain wait/ack. Wait for the received-and-acked live check before declaring recovery. `herdr-lead check <root>` alone proves only lease/generation health.

If startup fails after adoption, preserve its printed `ROOT/GEN/pane/mcp_config/agent/lease/check_task` state. Inspect the saved handover, latest adopt, agent, lease, delivery, inbox, and any check task before continuing; never blindly adopt again or start another agent. Stop the failed push lead before a plain rollback, observe `live:false`, then start a plain successor without the channel flags or helper config and let it consume the pending inbox. Inventory any project channel configuration without overwriting or deleting it. Keep the handover and per-generation config for reconciliation. There is no in-place rebind: use controlled stop-and-start or stop-and-resume.

A crash or a failure after adopt leaves `$HOME/.local/state/herdr-orch/lead-push/<root>/start.lock`; its `owner` file names the process and host. Every start takes, reclaims, releases and marks it under the short-lived `start.guard` directory, and releases or marks only a lock that is still its own (a hub's detached prover may finish after a rerun reclaimed it), so two starts never hold it at once. A rerun on the same host reclaims the lock when that process has exited, then still requires a free lease and a free target pane. It never reclaims a lock marked `hold` (a generation conflict: another adopter may be live), a lock owned by a live process or another host, or a lock without `owner` (made by an older helper). For those, prove no `herdr-lead` or `herdr-hub` run for the root is active, reconcile the printed state and the step log (`lead-start-<pid>.log` or `hub-check-<gen>.log`), then remove `owner` and `rmdir` the lock. A `start.guard` left by a killed start blocks later starts after 10 s; remove it with `rmdir` once no start for the root runs.

## When the taskr server is unreachable

On a client host, `got`, `ready`, `done`, `fail`, `decide`, `next`, `note`, and `close` print `qd1 <request key>` and exit 0 when the server is unreachable or earlier records wait in that host's spool: the record is queued, and the client daemon sends the queue in order once it reaches the server. Do not retry it or treat it as a failure. A full or unwritable spool makes them exit 5 with `retry with:`; rerun that line. Hook state updates queue the same way. Other writes (`new`, `launch`, `prompt`, `ask`, `answer`, `set`, `doc set`) still exit 5 with `retry with: taskr --request-key …`: keep each printed line (its key exists only in that output) and rerun it when the server answers. A `wait` ends with `x1 3 timeout unreachable`. Do not start a local ledger or register lanes again.

Work that cannot wait runs as a lane without the ledger. Its brief says "No ledger" and names the report path. Start it with `herdr agent start` following the core's Launch (or `launch.md`); submit and collect it as in `workflow.md` "Without a ledger". When the server is back, record it with one `taskr note --as <root> "<lane name>: report <report path>"`.

After the outage, run `taskr spool ls` on each client host and look at `refused` and `bad` entries (`spool send` sends now, `spool rm SEQ` removes one), arm the wait again, read `taskr status --tree <root>`, and read each open lane's pane: recorded state can be older than the lane. See taskr `references/recovery.md` for the rest.

## Review workers

For review workers, compaction alone continues the current task. Resuming the same review task after a restart also continues it: read its brief and latest journal first, cheaply verify target identity, and continue from the recorded next action. Reuse unchanged coverage and checks; repeat affected work when source identity changed or unresolved evidence requires it. Reuse recorded guidance paths and reread guidance only if it is missing, changed, or newly relevant, while preserving higher-priority requirements. A genuinely new task or session still follows the core's Preflight.

## After a Herdr server restart, reboot, or crash

After a Herdr server restart, Herdr may relaunch integrated agent panes itself with the plain native resume command (`codex resume <id>`, `claude --resume <id>`). That command carries none of the lane's launch flags: not its effort, its permission flags, or a host account wrapper. Do not prompt a restored helper. Quit it as below, then resume it manually with the lane's current flags and effort.

After a reboot or crash, first tear down stranded sessions as `teardown.md` "Workspace lifecycle, reboot, and crash" says; read `teardown.md` before stopping any restored or stranded helper.

## Resume rules and efforts

For an intentional resume, use the saved report and launch record. Preserve the
recorded native home, account, model, and session; validate the session as nonempty
and not literal `null`. Apply the current authorized lane settings from the
resolved table. If the account or session cannot be established, stop and report.
The examples below illustrate CLI shape; substitute the configured model/effort.

## Locate and verify the pane

From the orchestrator, use `herdr agent get "$owned_agent_name"` to resolve the owned agent's current `.result.agent.pane_id`. If it reports that the agent is no longer running, use the recorded pane ID instead. Stop and inspect other errors. With that verified `recovered_pane_id`, inspect its live workspace:

```bash
recovered=$(herdr pane get "$recovered_pane_id")
recovered_workspace_id=$(printf '%s\n' "$recovered" | jq -er '.result.pane.workspace_id')
test "$recovered_workspace_id" = "$task_workspace_id"

helper_pane_id="$recovered_pane_id"
```

Compare the resolved workspace, tab, pane, and cwd with the recorded task topology. Run `herdr pane current --current` only when the command executes inside the recovered worker pane; from the orchestrator it can resolve the hub instead. Do not trust inherited `HERDR_WORKSPACE_ID` or `HERDR_TAB_ID`, which can be stale after a pane move. Resume in the worker's own tab once its pane is at a shell prompt, or create a new helper tab under `launch.md`. Do not make a workspace or worktree for the replacement worker. Register a new launch for the restored pane (`taskr launch <task> … --pane <pane>`) and re-export both ledger ids together with `FLEET_ROLE` before the resume command. Never reuse the old launch: the ledger rejects its writes from then on.

## Stop before replacing

If the owned worker is still running or restored, save its metadata first, then, only if it still occupies the pane, send `herdr agent send-keys <owned-agent-or-current-pane-id> esc ctrl+c ctrl+c`. Read the pane and verify the interactive shell prompt before starting a replacement. If the pane is already at a shell prompt, do not send termination keys. If the shell was recreated, repeat any pane setup that a personal host skill requires before the start.

## Resume commands

Resume with the lane's current flags, which carry no sandbox: `<codex trust>`, `<claude settings>`, and the pane setup (`CLAUDE_CODE_SANDBOXED=1`, the folder-trust bypass, for Claude) are in `launch.md` "Unattended flags and trust".

```bash
# Load saved Codex metadata; stop/report if codex_sid is empty or `null`.
# codex_sid comes from CODEX_THREAD_ID or an actually present Herdr reference.
herdr agent start impl-<slug> --kind codex --pane "$helper_pane_id" --timeout 90000 -- \
  --dangerously-bypass-approvals-and-sandbox <codex trust> --dangerously-bypass-hook-trust \
  -m gpt-6.1-sol -c 'model_reasoning_effort="high"' resume "$codex_sid"
herdr agent prompt impl-<slug> "Continue where you left off. Report path: <path>" \
  --wait --until working --until blocked --timeout 20000
```

With the ledger, send that prompt with `taskr prompt <task> --text "Continue where you left off. Report path: <path>"` instead.

For Claude, use only that worker's recorded `CLAUDE_CONFIG_DIR`, session reference, and recorded model, with the lane's current effort; do not reuse Codex variables. A Claude session reference must have been reported by the native CLI or Herdr.

```bash
# Load Claude-only metadata; stop/report if claude_sid is empty or `null`.
herdr agent start rev-<slug> --kind claude --pane "$helper_pane_id" --timeout 90000 -- \
  --model claude-fable-5-1 --effort high \
  --dangerously-skip-permissions --settings '<claude settings>' \
  --resume "$claude_sid"
```

An OpenCode lane resumes its saved session with `herdr-launch`'s process-local model config:

```bash
# Use the recorded lane's model in place of this example.
herdr pane run "$helper_pane_id" ' export PATH="$PATH:$HOME/.opencode/bin" OPENCODE_CONFIG_CONTENT='\''{"model":"opencode/minimax-m3.1-flash-free","default_agent":"build","agents":{"build":{"model":"opencode/minimax-m3.1-flash-free"}}}'\'' '
herdr agent start glm-<slug> --kind opencode --pane "$helper_pane_id" --timeout 90000 -- \
  --standalone --auto --session "$opencode_sid"
```
