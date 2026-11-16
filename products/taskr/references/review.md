# taskr for a reviewer lane

A reviewer lane needs only this file and the core [SKILL.md](../SKILL.md); it does not load the orchestration skill. Output codes: [format.md](format.md).

1. Fold `taskr got ATTEMPT`, from the prompt's `First taskr got N`, into your first read as one command (for example, `taskr got N && taskr doc get D`). A hook may have recorded it: `g1 ... dup` is fine.
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
(`researcher`, `architect`) should start a gather helper for a bounded read they
do not need to judge, unless their brief says `Gather helpers: none`, when any of
these holds:

- the PR touches more than 40 files or 1,500 changed lines: a helper lists the
  callers, siblings and tests of the changed exports outside the diff;
- a log, transcript, CI run or report set to scan is larger than 200 KB or
  2,000 lines;
- three or more outside sources (spec, docs, other PRs or issues) must be quoted;
- history to list spans more than 20 commits, PRs or ledger events.

Below these sizes, read it yourself: a helper costs about 2 minutes and a full
agent process. Use only gather lanes from `herdr-launch --lanes`, under the
operator's model policy. A helper returns quotes and rows with path:line or a
command; it never classifies, ranks, judges, reviews, implements, orchestrates
or writes a finding. The reviewer alone writes findings; the researcher or
architect alone writes its analysis. Read the diff and every line a finding
rests on yourself, and mark a gathered fact you did not re-read as G in the
report.

Use only `herdr-launch`; no native subagents or advisor calls. One command
writes the helper brief (role=researcher, `Gather helpers: none`, QUESTION as
its Objective) to `gather/<task>/<slug>.brief.md` beside your report (`<task>`
is your task ID and `<slug>` is SLUG; the report is `<slug>.md` there), and
starts `gather-SLUG` under your task in a new tab of your workspace. The lane is
`--lane`, else `HERDR_GATHER_LANE`, else `haiku-gather`:

```sh
herdr-launch --gather callers 'List callers and tests of foo() outside the diff, with path:line'
taskr wait --for ready,done,fail,ask
```

It prints the task and tab IDs, the report path and the two close commands. At
most 3 children may be open, including children that are ready or done but not
closed. Your brief may set a lower cap (a busy shared host allows one); keep to
it, since the launcher checks only the cap of 3. A lead that wants a seat to use
more helpers starts that seat on a host with room. A helper cannot start a
helper. Workers cannot use `--reserved` or `--machine`. Other worker roles
cannot start helpers; orchestrator and sub-orchestrator rules stay the same. For
a brief of your own, copy a generated `<slug>.brief.md` to BRIEF and edit it;
the launcher needs `HERDR-BRIEF role=researcher`, the `orchestrator=...
report=REPORT` line, and the `Task topology:`, `Objective:`, `Acceptance:` and
`Validation:` lines. Then run `herdr-launch LANE gather-SLUG BRIEF REPORT
"$TASKR_TASK"`, where LANE is a gather lane and BRIEF and REPORT are absolute
paths.

Read the helper's report. Open the source for every fact a finding rests on;
mark any other fact you cite without re-reading as G. Answer a helper's ask with
`taskr answer ASK_ID TEXT`. After handling each event, run `taskr ack EVENT_ID
--as "$TASKR_TASK"`, then wait again as needed. In a push lane, end the turn and
handle the pushed events with their fence as [Push lane](../SKILL.md#push-lane)
says; do not use a plain wait or ack. After the helper finishes and you check
its report, close its ledger task and its owned tab (use `rejected` if you
reject the report):

```sh
taskr close CHILD --outcome accepted
herdr tab close TAB
```

Count a seat's helpers:
`taskr --json status --tree SEAT --all | jq -s '[.[] | select(.parent_id == SEAT and (.name // "" | startswith("gather-")))] | length'`.
