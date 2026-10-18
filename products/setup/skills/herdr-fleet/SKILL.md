---
name: herdr-fleet
description: "Optional host integration for Herdr: shared skills, lane configuration, account-manager forwarding, taskr roles, resource admission, and staged service assets. Load alongside herdr-orchestration."
---

# Herdr fleet

Load `local.md` if it exists beside this file
(`~/.agents/skills/herdr-fleet/local.md`). It contains the operator's private
policy, accounts, host names, and restrictions. The installer detects this
overlay without reading or replacing it. Never copy it into a public artifact.
`--fleet` appends private `~/.config/herdr-fleet/instructions.local.md` (operator model policy and bans) to the public global instructions for existing Claude/Codex directories.

## Installation

The canonical installer is `products/taskr/install.sh` in herdr-kit.
Default installation includes the taskr CLI and TUI, the taskr skill, and a staged Herdr plugin.
`--fleet` adds orchestration/fleet skills, launch helpers, minimal taskr hooks,
and staged service assets. `products/setup/install.sh` delegates to that entry
point with `--fleet`. Core services and the taskr plugin require manual activation; the explicit radar
opt-in manages its external plugin.

Skills live at `~/.agents/skills/<name>`. Existing Claude, Codex, and OpenCode
config directories receive loader links. Regenerate the upstream Herdr skill
with `herdr --skill` after a Herdr upgrade; the installer does this when available.

The shared placer keeps `~/.local/state/herdr-setup/installed` and
`~/.config/herdr-setup/keep`. Identical files are skipped; recorded, unedited
files update; edited files receive `.herdr-setup` sidecars. Resolve those by hand.
Use `--keep .codex/hooks.json`, for example, to preserve a machine's settings.

## Lanes

`herdr-launch`, `herdr-lead` and `herdr-hub` read the same generic `lanes.json`.
An optional `~/.config/herdr-fleet/lanes.json` replaces the entire table.
The installer never reads or writes the private table. Configure model choices,
role prefixes, and restrictions there, and describe narrative policy in `local.md`.
Unknown or malformed lanes fail before registration or agent startup.

## Optional tools

`--codator` selects the cdx/cdl forwarding wrappers; it never installs Codator.
`--practices` and `--verification` add the optional skill packs. `--slotr`,
`--notifications` (public Moshi hook fragments), and `--radar` (the external
pinned plugin) are separate opt-ins; missing optional tools do not block the core.
Launch helpers forward Codex and Claude through an existing `~/.local/bin/codator`; they
never install it or read its accounts. For a resume, preserve the recorded account and native session.
Do not bypass an account manager's refusal or inspect credential files.

If a host uses slotr, write its exact pool/kind command in the worker brief.
Keep local admission configuration and notification credentials outside this repo.
The default hook templates call taskr and block in-process delegation; public notification fragments are
selected explicitly. Private notification additions belong in locally kept settings.

## taskr roles

`--taskr-hub` (implies `--fleet`) stages the tailnet dashboard address and user service.
`--taskr-client http://hub.example.ts.net:7788` (implies `--fleet`) stages a client URL.
Existing ledgers and incompatible roles require manual reconciliation.
The installer prints activation steps and never starts or restarts a service.
