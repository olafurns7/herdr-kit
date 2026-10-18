# Model selection

The public lane table is `products/setup/lanes.json`. Both launch tools use it.
An optional `~/.config/herdr-fleet/lanes.json` replaces the whole public table.
See `lanes.md` beside the installed orchestration skill for its fields.

Hosts may keep a longer private history in their overlay.

Choose the smallest authorized lane that can complete the brief and its checks.
Use `gather-` for read-only gathering: scraping, docs, and release notes. It
allows Haiku 5.5, GLM 5.3 Flash, and Codex Luna. Haiku 5.5 and Codex Luna stay
barred from analysis, review, implementation, and orchestration.
An Opus 5.5 xhigh `orch-` sub-orchestrator needs a recorded reason (`taskr decide`)
or an owner request.
Record reasons for escalation. Keep operator-specific bans, account mappings,
quota policy, and model preferences in the private table and fleet local.md.
Review must remain independent of implementation.
