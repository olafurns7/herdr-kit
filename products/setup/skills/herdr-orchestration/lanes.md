# Lane selection

Load before choosing or resuming a lane. Lane tables hold model assignments; the
model rules below apply to every table.

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
scraping, docs, or release notes. The public table has Haiku 5.5, OpenCode,
and Codex Luna gather lanes. Haiku 5.5 and Codex Luna run only on `gather-` lanes,
never analysis, review, implementation, or orchestration.

| Lane | OpenCode model | Prefixes |
| --- | --- | --- |
| `minimax-free` | `opencode/minimax-m3.1-flash-free` | `glm gather` |
| `glm-free` | `opencode/glm-5.3-flash-free` | `glm gather` |
| `glm-flash` | `opencode/glm-5.3-flash` | `glm gather` |

These OpenCode lanes are for simple, bounded tasks with an exact validation
gate: read-only gathering, source-grounded digests, mechanical edits, lint or
format fixes, and doc copy. Prefer `minimax-free`, then `glm-free`; use paid
`glm-flash` when both free lanes fail or are unavailable. Free lanes may read
and edit private repository code within these limits. Never use these lanes
for critical work, review or approval, a recommendation or adjudication,
diagnosis, or a write path without an exact validation gate.

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
`"except":{"gather":{"model":["(?i)^(claude-)?haiku", "(?i)luna"]}}`
can exempt gather-only Haiku and Luna lanes, but not a lane with both `gather`
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
