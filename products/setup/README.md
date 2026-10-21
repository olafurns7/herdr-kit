# Herdr setup assets

Fleet orchestration and the taskr plugin require [Herdr 0.9.0 or newer](https://herdr.dev/docs/install/).

taskr owns installation. From a herdr-kit checkout:

```sh
sh products/taskr/install.sh                         # taskr CLI/TUI + skill; stages the taskr Herdr plugin
sh products/taskr/install.sh --fleet                 # add fleet integration
sh products/taskr/install.sh --practices --verification
sh products/taskr/install.sh --fleet --files-only    # offline source staging
```

`products/setup/install.sh` is a compatibility wrapper that adds `--fleet`.
Set `TASKR_VERSION=taskr/v0.26.0` to select an exact product release. Binary
installs need no Cargo, Node, or npm. `gh` authentication or `GH_TOKEN` is
supported; anonymous downloads use curl. Unpinned curl installs also need jq.
Launch helpers require jq.

Fleet selection installs the public orchestration/fleet skills, `herdr-launch`,
`herdr-loop`, `herdr-worktrees`, `herdr-lead`, `herdr-hub` and `herdr-lanes.sh`,
the shared lane table, and minimal taskr hooks.
If Herdr is available, its generated upstream skill is staged too. OpenCode/pi hooks
are selected only when the corresponding config directory already exists.
Existing agent config directories get skill loader links. No unrelated settings
are merged automatically. Services and the taskr plugin are staged; activation
is manual. The installer never starts or restarts Herdr or a service.

This repository supplies public install assets. Hosts may override them with
private `lanes.json` and fleet `local.md` files; use those local files to check
which policy a host runs.

`--practices` and `--verification` independently select the optional skills.
Their pinned pstack LICENSE and PROVENANCE.md install under
`~/.local/share/herdr-setup/third_party/pstack`. Attribution links are rendered
for the installed layout; source links continue to resolve in this repository.
slotr is a separate `--slotr` opt-in, supported by its product installer when
available in the checkout. `--codator` installs only the kit's `cdx`/`cdl` wrappers,
which forward to `codator codex`/`codator claude` with arguments unchanged;
install [Codator, the account selector, separately](../../README.md#optional-codator).
Codator's own optional `cdx`/`cdl` shortcuts (`CODATOR_SHORTCUTS=1`) bypass every
sandbox and approval prompt. Both sources use the same names and default paths,
`~/.local/bin/cdx` and `~/.local/bin/cdl`; the Codator installer replaces an
existing file only when it already calls Codator, so pick one source for these
commands (see [Codator's shortcuts](https://github.com/olafurns7/codator#optional-shortcuts-cdx-and-cdl)).
`--notifications` merges the public
Moshi (a mobile push-notification app) hook fragments through the usual
KEEP/UPDATE/MERGE rules, preserving other
settings keys. Notification credentials and destinations remain local.
`--radar` explicitly installs/configures the external
[herdr-radar](https://github.com/hhdebb/herdr-radar)
plugin (`hhdebb.herdr-radar`) at its existing pin;
it is never imported into this repository. Files-only mode suppresses radar
control actions. An absent optional tool never blocks the default core install.

The public source allowlist is in `products/taskr/install.sh`. It excludes
private overlays, backups, live state, and unselected settings. Standalone
installation resolves setup source from the same selected taskr tag.

`herdr-hub ROOT` (or `--new NAME`) adopts the taskr hub's root in its pane and
restarts its Claude lead with the taskr push channel through the sibling `cdl`
wrapper (needs `--codator` and Codator). Usage, checks and recovery:
[launch.md](skills/herdr-orchestration/launch.md).

## Existing files

One shared placement library, `lib/placement.sh`, retains the setup contract:

- Missing files are installed with their declared mode.
- Equal content gets `SKIP`; an existing mode is preserved.
- A file matching its recorded hash gets atomic `UPDATE`, preserving its mode.
- An edited file gets `MERGE` and a `.herdr-setup` sidecar.
- Destination symlinks and conflicting sidecars are preserved. Directories are
  never overwritten as files. Existing sidecars are never replaced.
- `KEEP` entries are never written, even when the recorded content is unchanged.

Keep intentional differences in `~/.config/herdr-setup/keep`, one path relative
to HOME per line. Blank lines and comments are allowed. `--keep PATH` appends
atomically, without duplicates, and applies during that run. Absolute, empty,
multiline, `~`, `.`, `..`, and escaped paths are refused. Existing parent links
must stay inside physical HOME. A symlink or directory at the keep-list path
refuses append; unreadable lists stop installation.

`~/.local/state/herdr-setup/installed` remains the SHA-256 installation record.
Missing, unreadable, malformed, symlinked, or nonregular records cannot authorize
updates. Escaped metadata directories are ignored without reading or writing
outside HOME. An interrupted run can be rerun: equal files are recognized and
recorded, while differing unrecorded files still require a manual merge.

## Private policy and lanes

The existing `~/.agents/skills/herdr-fleet/local.md` stays private and untouched.
The public fleet skill includes it when present; the installer detects it
without opening, sourcing, copying, or printing its contents.

`--fleet` places global instructions in existing `~/.claude` and `~/.codex`
directories. Keep operator model policy and bans in the optional private
`~/.config/herdr-fleet/instructions.local.md`; the installer appends its text
to the public base using the same keep, record, and sidecar rules.

`herdr-launch`, `herdr-lead` and `herdr-hub` read `~/.config/herdr-fleet/lanes.json`
when present; this
private table replaces the whole public table. Otherwise they read the checkout's
`products/setup/lanes.json` when run from source, else the staged public
`~/.local/share/herdr-setup/lanes.json`. Each named lane has `kind`,
`model`, `effort`, and a `prefixes` array. Invalid fields and unknown lanes fail
before task registration. Values form quoted argv; no eval is used. The installer
never reads or modifies the private lane table.

`herdr-launch --check [LANE...]` checks CLI versions and Codator forwarding without
control-plane writes; add `--push` before `--check` to check push prerequisites too. An
optional `deny` object maps `model`, `provider`, and `kind` to arrays of jq regular
expressions; any match refuses resolution and is marked by `--lanes`. Optional
`deny.except` maps known prefixes to objects with the same regex fields. A denial is
exempt only when every prefix of the lane has a matching exception for that field.
Exceptions have no effect without a matching deny rule. Anchor patterns with `^` and `$`
for exact matches. An OpenCode lane uses a qualified `provider/model` model;
its `provider` is the model prefix, and an explicit value must match. Keep operator policy in
the private table; the public deny list is empty.

The public OpenCode lanes are `minimax-free` (`opencode/minimax-m3.1-flash-free`),
`glm-free` (`opencode/glm-5.3-flash-free`), and `glm-flash` (`opencode/glm-5.3-flash`).
All accept `glm-` and `gather-` names. These OpenCode lanes are for simple,
bounded tasks with an exact validation gate: read-only gathering, source-grounded digests, mechanical edits, lint or
format fixes, and doc copy. Prefer `minimax-free`, then `glm-free`; use paid
`glm-flash` when both free lanes fail or are unavailable. Free lanes may read
and edit private repository code within these limits. Never use these lanes
for critical work, review or approval, a recommendation or adjudication,
diagnosis, or a write path without an exact validation gate.

`herdr-launch` runs OpenCode directly, resolving PATH or `~/.opencode/bin/opencode`.
With v2.1.1 it uses `--standalone --auto` and process-local `OPENCODE_CONFIG_CONTENT`
to select the model and build agent without writing global config. Effort is
recorded but ignored; `--push` is refused for OpenCode.

Claude's synchronous PreToolUse hook denies `Agent`, `Task`, and `Workflow` when
`HERDR_ENV=1`, pointing to `herdr-launch`. Codex has no equivalent hook surface for
`spawn_agent`; that restriction remains instructions only.

Per-user incident intake config lives at `~/.config/herdr-fleet/intake.yaml`
(example in [intake.example.yaml](intake.example.yaml); see the herdr-practices
[incident-intake reference](skills/herdr-practices/references/incident-intake.md)).
`--fleet` stages only the placeholder
`~/.local/share/herdr-setup/intake.example.yaml` and never creates or overwrites
the private config.

## taskr roles

`--taskr-hub` stages `tailnet` in the dashboard address and a user-service
unit. `--taskr-client http://hub.example.ts.net:7788` stages the client
URL. Flags are mutually exclusive; invalid URLs are refused. Local ledgers,
hub roles, and legacy hub URLs require manual reconciliation before changing
roles. Apply the printed service/daemon steps yourself.

## Validation

From the repository root:

```sh
for t in products/setup/tests/*_test.sh; do sh "$t" || break; done
node products/setup/tests/taskr_hook_test.mjs
sh products/setup/tests/privacy_check.sh
```

The installer fixtures use scratch
HOME directories, synthetic private values, fake downloads, and fake control
commands. Run the full repository gate through the configured heavy pool.
