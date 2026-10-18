# Model selection

`herdr-launch`, `herdr-lead` and `herdr-hub` read the lane table from
`~/.config/herdr-fleet/lanes.json` if present, else the checkout's
`products/setup/lanes.json` when run from source, else the installed
`~/.local/share/herdr-setup/lanes.json`.

Read
[lanes.md](https://github.com/olafurns7/herdr-kit/blob/main/products/setup/skills/herdr-orchestration/lanes.md)
for the fields and public model rules before choosing a lane. Record reasons
for escalation. Keep operator-specific bans, account mappings, quota policy
and model preferences in the private table and the herdr-fleet skill's private
`~/.agents/skills/herdr-fleet/local.md` (never committed).
Review must remain independent of implementation.
