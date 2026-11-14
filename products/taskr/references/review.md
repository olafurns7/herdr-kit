# taskr for a reviewer lane

A reviewer lane needs only this file and the core [SKILL.md](../SKILL.md); it does not load the orchestration skill. Output codes: [format.md](format.md).

1. Run `taskr got ATTEMPT` first, from the prompt's `First taskr got N`. A hook may have recorded it: `g1 ... dup` is fine.
2. Stay read-only. Write only the report path and the journal path that the brief names (a temporary sibling is allowed for an atomic journal replace). No source edits, commits, pushes, PRs, issue-tracker writes, installs or daemon restarts. You may write briefs and start gather helpers only under Gather helpers below.
3. Run probes and checks in a scratch directory: a scratch clone or copy, `HOME` and an explicit `TASKR_DB` under `mktemp -d`. Never touch the live ledger or another lane's worktree files.
4. Missing decision: `taskr ask "question" --blocking`; in a plain lane, `taskr wait --for answer` and `taskr ack EVENT --as $TASKR_TASK` after you handle it; in a push lane, follow [Push lane](../SKILL.md#push-lane). On exit 5, rerun the printed `--request-key` line before you wait. Client `got`/`ready`/`done`/`fail` may instead print `qd1 <request key>` and exit 0 when the server is unreachable or earlier records wait in this host's spool: queued, not failure; do not retry. A full or unwritable spool: exit 5 with `retry with:`; rerun that line. Inspect `spool ls` after an outage; states: [recovery.md](recovery.md#spool).
5. Finish: write the report, then run the seat report command below, then `taskr done "VERDICT: one line"` (or `taskr fail "reason"` when the review cannot finish). The bare `done` reuses the latest `ready` record. Reply with the report path and three lines.

```sh
taskr ready "rev-SLUG" --report PATH \
  --kv repo=OWNER/NAME --kv pr=NUMBER --kv head=FULL_40_HEX_SHA \
  --kv verdict=PASS --kv p0=0 --kv p1=0 --kv p2=0 --kv p3=0
```

Use the brief's own PR and the reviewed head. Set `verdict` to `PASS` or `FAIL`
and each severity count to the number of open findings at that head; `PASS`
requires p0=p1=0. For a review without a PR, keep `repo`, `head` and the verdict
fields, and replace `--kv pr=NUMBER` with `--kv pr=none --kv reason=TEXT`.

The report states: the verdict; each finding with file:line, a failure case and a fix; the checks that hold; every command run with its exit code; the source identity reviewed (commit or hashes); and each probe path with the exact command to rerun it. Keep probes outside the repository at the evidence paths named by the brief; list every artifact left there. Ask for an external evidence path if one is missing.

## Gather helpers

Reviewer lanes (`code-reviewer`, `reviewer`, `code-validator`) and analysis lanes
(`researcher`, `architect`) may use helpers for large inputs, history, or many
files. Use only gather lanes from `herdr-launch --lanes`, under the operator's
model policy. They gather read-only facts: never analysis, review, a verdict,
implementation, or orchestration. The reviewer alone writes findings. The
researcher or architect alone writes its analysis.

Use only `herdr-launch`; no native subagents or advisor calls. Set GATHER_LANE,
BRIEF, and REPORT to the selected lane and absolute paths in your campaign
folder. Write a helper brief there with this shape:

```text
HERDR-BRIEF role=researcher
orchestrator=<your task name>   report=<absolute report path>
Task topology: workspace=<your task workspace>; herdr-launch creates the tab
Objective: Gather <specific quotes or rows> from <bounded sources> read-only.
Non-goals: No analysis, review, verdict, implementation, or orchestration.
Gather helpers: none
Skills to load: taskr
Acceptance: Return facts with path:line or exact command evidence; no findings.
Validation: Check each quote or row against its source.
Progress: taskr SKILL.md worker rules
Do not edit source, commit, push, open PRs, update issues, or start agents.
Write only the report path, then taskr ready with --report and taskr done.
```

Launch in your own task workspace (omit PANE), with your task as PARENT:

```sh
herdr-launch "$GATHER_LANE" gather-history "$BRIEF" "$REPORT" "$TASKR_TASK"
taskr wait --for ready,done,fail,ask
```

Keep the returned task and tab IDs. At most 3 children may be open, including
children that are ready or done but not closed. A helper cannot start a helper.
Your own brief may forbid helpers with the line `Gather helpers: none`.
Workers cannot use `--reserved` or `--machine`. Other worker roles cannot start
helpers; orchestrator and sub-orchestrator rules stay the same.

Read the helper's report. Open the source and check each fact before you cite
it. Answer a helper's ask with `taskr answer ASK_ID TEXT`. After handling each
event, run `taskr ack EVENT_ID --as "$TASKR_TASK"`, then wait again as needed.
In a push lane, end the turn and handle the pushed events with their fence as
[Push lane](../SKILL.md#push-lane) says; do not use a plain wait or ack.
After the helper finishes and you check its report, close its ledger task and
its owned tab (use `rejected` if you reject the report):

```sh
taskr close CHILD --outcome accepted
herdr tab close TAB
```
