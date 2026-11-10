# Lane selection

Load before choosing or resuming a lane, or before splitting work into small
verified tasks (Loop verification, below). The lane table is the only list of
models: read it with `herdr-launch --lanes`. Which model may take which work is
operator policy in the private `~/.config/herdr-fleet/instructions.local.md`,
which `--fleet` merges into the global instructions; public text names no models.

`herdr-launch`, `herdr-lead` and `herdr-hub` resolve the optional private table at
`~/.config/herdr-fleet/lanes.json`, else the checkout's `products/setup/lanes.json` when run from
source, else `~/.local/share/herdr-setup/lanes.json`. A private table replaces all defaults.
Each lane has `kind` (`claude`, `codex`, `opencode`, or legacy private `pi`), `model`,
`effort` (`low` to `ultra`), optional `provider`, and allowed `prefixes`
(`impl`, `mech`, `rs`, `rev`, `arch`, `glm`, `orch`, `gather`). OpenCode's provider is
the model prefix; an explicit value must match. Other kinds default to the kind,
except legacy `pi`, which defaults to `opencode`.
The helpers validate the table and build argv without evaluating text. Unknown lanes fail
before any control-plane call. `herdr-launch --lanes` lists resolved lanes, prefixes
and the table path without launching.

Use `gather-` with a `role=researcher` brief for read-only gathering, such as
scraping, docs, or release notes. A lead's gather lanes and a reviewer or analysis
lane's gather helpers have the same model limits: read-only gathering, never
analysis, review, a verdict, implementation, or orchestration. Reviewers and
analysis lanes follow taskr `references/review.md` for launch, checks, and close.
A lane's prefixes say which roles it may take; the operator overlay may narrow
that further for a model.

OpenCode models use `provider/model` syntax. Its v2.1.1 root TUI has no model or
reasoning-effort flag: the launcher selects the model through process-local
`OPENCODE_CONFIG_CONTENT`; `effort` stays in the ledger but is ignored by the
harness. `--standalone --auto` keeps each worker's config and hook environment
in its own server and auto-approves permissions that are not explicitly denied.
No global OpenCode config is changed. OpenCode uses pane prompts; `--push` is refused.

An optional `deny` object maps `model`, `provider`, and `kind` to arrays of jq
regular expressions. A match denies the lane. Optional `deny.except` maps known
prefixes to objects with the same regex fields. An exception clears a field's
denial only if every prefix of that lane has an exception matching that field's
value. It has no effect without a matching deny rule. For example,
`"except":{"gather":{"model":["^example-small-model$"]}}`
can exempt a gather-only lane on that model, but not a lane with both `gather`
and `rs` prefixes. Invalid exception keys, shapes, and patterns fail closed.
Keep operator-specific rules in the private table; the public `deny` is empty.

The public `opus-xhigh` lane allows `orch-`; an xhigh sub-orchestrator still needs
a recorded reason (`taskr decide`) or an owner request.

Choose the smallest lane allowed by the operator's private policy and the brief
that can complete the work and its checks. Record the
actual model, effort, provider, account label when applicable, and native session
in the checkpoint. A resume preserves the recorded account/session and uses the
current authorized lane settings. Stop if those cannot be established.

Reviewers work independently of implementers. The brief defines scope, validation,
escalation, and publication authority; a lane name grants none of those rights.

A quota stop parks a rule-bound review loop. When a model that the loop's review
rule needs is out of quota, park the loop until that model returns. Do not run
rounds with fewer seats, and do not switch the writer model.

## Loop verification

For velocity, a lead or sub-orchestrator splits work into small gated tasks, runs fast implementers in parallel, and has one verifier review the work of several.

- **Small task:** one concern, about 150 changed lines or fewer, files disjoint from its siblings (siblings may share a task worktree only on disjoint files). Its brief carries an exact Validation gate from checks the repository already has: an existing end-to-end, eval or QA check, extended where needed (red before, green after), or typecheck, lint or build plus recorded runtime proof. Never a new standalone unit, component, helper, tooling or change-detector test. No usable gate means it is not a small task.
- **Lanes** (the resolved table and the operator overlay decide): fast implementers on the cheapest allowed implementation lanes, such as `sonnet-high` or `terra-medium`, and the OpenCode free lanes only for simple tasks with an exact gate. One verifier per 3-5 tasks on `opus-medium` or `sol-medium` (`rev-`), always the other provider: a verifier never verifies its own model. Money, security, the critical turn path, architecture, diagnosis, and anything without an exact gate stay on strong implementers (`opus-high`, `sol-high`).
- **Loop:** the implementer changes, runs the repository's Leaf checks, commits, and reports `ready` with the head. The parent proves the rest of the Validation gate by the repository's proof rule (`workflow.md` "Head gate"). The verifier reads the diff against the brief and that proof receipt, reruns the Leaf checks, rejects any new standalone test, and returns READY or a FIX list with `file:line`. The same implementer fixes; the verifier reviews only the delta. After 3 loops without READY, reassign to a strong implementer.
- **Fold:** a verifier READY lets the lead fold the task into its PR or stack. PR gates are unchanged: the repository's proof rule at the head, a full review at the PR head by a provider other than the implementer's (a verifier READY counts when it covered the full PR diff), and the publish rules in `SKILL.md`. Keep the queue full: start the next task as one turns READY and close finished lanes at once.
