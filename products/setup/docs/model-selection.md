# Model selection

`herdr-launch`, `herdr-lead` and `herdr-hub` read the lane table from
`~/.config/herdr-fleet/lanes.json` if present, else the checkout's
`products/setup/lanes.json` when run from source, else the installed
`~/.local/share/herdr-setup/lanes.json`.

Read `lanes.md` beside this file once installed
([source](https://github.com/olafurns7/herdr-kit/blob/main/products/setup/skills/herdr-orchestration/lanes.md))
for the fields before choosing a lane. The lane table is the only list of
models; `herdr-launch --lanes` prints the resolved one. Record reasons for
escalation. Operator model policy (bans, preferences, which model may take
which work) has one source, the private
`~/.config/herdr-fleet/instructions.local.md` (never committed), which
`--fleet` merges into the global instructions. Keep account mappings and quota
policy in the herdr-fleet skill's private `~/.agents/skills/herdr-fleet/local.md`.
Review must remain independent of implementation.
