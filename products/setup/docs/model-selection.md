# Model selection

`herdr-launch`, `herdr-lead` and `herdr-hub` read the lane table from
`~/.config/herdr-fleet/lanes.json` if present, else the checkout's
`products/setup/lanes.json` when run from source, else the installed
`~/.local/share/herdr-setup/lanes.json`.

Read `lanes.md` beside this file once installed
([source](https://github.com/olafurns7/herdr-kit/blob/main/products/setup/skills/herdr-orchestration/lanes.md))
for the fields and public model rules before choosing a lane. Record reasons
for escalation. Keep operator-specific bans, account mappings, quota policy
and model preferences in the private table and the herdr-fleet skill's private
`~/.agents/skills/herdr-fleet/local.md` (never committed).
Review must remain independent of implementation.

These OpenCode lanes are for simple, bounded tasks with an exact validation
gate: read-only gathering, source-grounded digests, mechanical edits, lint or
format fixes, and doc copy. Prefer `minimax-free`, then `glm-free`; use paid
`glm-flash` when both free lanes fail or are unavailable. Free lanes may read
and edit private repository code within these limits. Never use these lanes
for critical work, review or approval, a recommendation or adjudication,
diagnosis, or a write path without an exact validation gate.
