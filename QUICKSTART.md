# From a fresh machine to a delegated agent

This path uses Linux or macOS, Herdr, Claude Code and a local taskr ledger.
Run the commands in order from a normal terminal shell. Each step includes a
check; stop if it fails. An agent helping with setup should ask before changing
shell profiles, hooks or agent settings. Skill links are the allowed exception;
existing conflicting links are preserved. Hooks and settings still need the ask.
The person at the terminal starts Herdr: never start or restart its server
from inside an agent session.

## 1. Check prerequisites and get the kit

Install `curl`, `git`, `tar` and `jq` with your OS package manager, plus
`sha256sum` (Linux) or `shasum` (macOS). Then check:

```sh
for tool in curl git tar jq; do command -v "$tool"; done
command -v sha256sum || command -v shasum
git clone https://github.com/olafurns7/herdr-kit.git
cd herdr-kit
test -f products/taskr/install.sh
```

Expect a path for each tool, Git's clone progress and a silent successful
`test`. Keep this shell in the checkout through step 5. Binary installation
needs no Rust, Node, npm or Turbo.

## 2. Install Herdr

Use the [official installer](https://herdr.dev/docs/install/), saving the
script before running it:

```sh
curl -fsSL -o herdr-install.sh https://herdr.dev/install.sh &&
  test -s herdr-install.sh && sh herdr-install.sh
export PATH="$HOME/.local/bin:$PATH"
herdr --version
```

Expect `installed herdr to .../.local/bin/herdr` and a version from the check.
taskr's plugin needs Herdr 0.9.0 or newer. Keep the PATH export in each new
shell, or add it to your shell profile yourself.

## 3. Install and sign in to Claude Code

Follow [Claude Code's quickstart](https://code.claude.com/docs/en/quickstart):

```sh
curl -fsSL -o claude-install.sh https://claude.ai/install.sh &&
  test -s claude-install.sh && bash claude-install.sh
claude --version
claude
```

Expect a version ending in `(Claude Code)`. Complete the browser sign-in;
you need an account with Claude Code access. At the prompt, enter `/exit` to
return to the shell. Reopen `claude` once to check that sign-in persists, then
exit again. Do this before installing fleet links and launching a worker.
Codex also works: install its CLI and use a Codex lane from `herdr-launch --lanes`.
Optional for several accounts: [install Codator](README.md#optional-codator).

## 4. Install taskr and the fleet files

An agent must ask before running the fleet installer: it also writes hooks
and global agent instructions.

Choose the latest release on the
[Releases page](https://github.com/olafurns7/herdr-kit/releases) with a
`taskr/vX.Y.Z` tag. The example pins `taskr/v0.26.0`; use a newer published tag
if needed:

```sh
export TASKR_VERSION=taskr/v0.26.0
sh products/taskr/install.sh --fleet
taskr version
command -v taskr-tui herdr-launch
test -f "$HOME/.agents/skills/taskr/SKILL.md"
test -f "$HOME/.agents/skills/herdr-orchestration/SKILL.md"
```

Expect checksum `OK` lines, installed taskr/TUI paths, fleet `INSTALL` or
`SKIP` lines, and `plugin staged: ... (activate with herdr plugin link)`.
The checks should show the chosen taskr version, both command paths and silent
successful file tests. `MERGE` means an existing edited file was preserved;
inspect the printed `.herdr-setup` sidecar before proceeding. For an existing
`~/.claude/settings.json`, merge `permissions.defaultMode: "bypassPermissions"`,
`skipDangerousModePermissionPrompt: true` and
the sidecar's `hooks` entries (`PreToolUse`, `PostToolUse`, `SessionStart`,
`Stop`, `UserPromptSubmit` and `StopFailure`), preserving your existing settings and hooks.
This makes bypass mode the default even when starting `claude` without flags.

`--fleet` installs public skills, launch helpers and minimal taskr hooks.
It stages integration files; activation is manual. Follow the installer's
printed next steps for selected options. Detailed paths, existing-file handling,
agent links and recovery are in [taskr's setup guide](products/taskr/AGENTS_SETUP.md)
and [setup assets](products/setup/README.md).

## 5. Optional: install slotr on Linux x86_64

Skip this step on macOS or if you only want the first worker. slotr requires
a working systemd user manager; see [its requirements](products/slotr/docs/install.md).
Pick a published slotr tag separately from taskr. The example pins
`slotr/v0.3.1`; use a newer published tag if needed:

```sh
SLOTR_VERSION=slotr/v0.3.1 sh products/slotr/install.sh --config
slotr --version
slotr config check
```

Expect `Installed to .../.local/bin/slotr`, a version and a successful config
check. `--config` keeps an existing config. Pool setup and heavy-command use
are in [slotr's guide](products/slotr/README.md).

## 6. Enter Herdr and activate taskr

From this normal terminal, the person runs:

```sh
herdr
```

Expect Herdr's workspace and a shell pane. Run the remaining commands in that
shell, outside any agent session:

```sh
test "${HERDR_ENV:-}" = 1
herdr plugin link "$HOME/.local/share/taskr/plugin"
taskr daemon --restart
herdr plugin list --json
taskr daemon --status
```

Expect a successful silent environment check, the plugin `olafurns7.taskr` in
the list, and daemon status with `"running":true`, `"stale":false`,
`"dashboard":"up"` and `running_version` matching `taskr version`.
If startup is still settling, check status again. A zero exit alone does not
prove the daemon is running. This starts taskr; leave the Herdr server running.
Sidebar/radar setup and multi-machine use are optional; follow
[the taskr guide](products/taskr/AGENTS_SETUP.md) if you need them.

## 7. Delegate a small task

The public `opus-medium` lane uses Claude Code. Check it before launch:

```sh
herdr-launch --lanes
herdr-launch --check opus-medium
```

Expect a lane table and `opus-medium: ok`. A private lane table, if you already
have one, replaces the public table; choose an available Claude lane allowing
the `rs` prefix. The check verifies the CLI, not model access or billing.

Create a new demo directory (choose another name if it already exists), a root
task and a complete brief. Keep using this shell so `$root` stays set:

```sh
mkdir "$HOME/kit-first-task" && cd "$HOME/kit-first-task"
```

Stop if this fails. Then create the root task:

```sh
root=$(taskr new kit-first-check --role orchestrator --pane "$HERDR_PANE_ID" |
  sed -n 's/^n1 //p')
test -n "$root"
```

Stop if this fails. Then record the goal, write the brief and launch the worker:

```sh
printf 'Check the installed taskr guidance.\n' > goal.md
taskr doc set "$root" goal --file "$PWD/goal.md"
cat > first-brief.md <<EOF
HERDR-BRIEF role=researcher
orchestrator=kit-first-check report=$PWD/first-report.md
Task topology: use the launcher's task, tab and pane; cwd=$PWD
Objective: Read the installed taskr skill and report the receipt and completion commands.
Ownership: $PWD/first-report.md only, plus your taskr receipts and reports.
Forbidden: other file changes, daemon/server changes, commits, pushes and agent starts.
Acceptance: Report names the skill path and explains got, ready, done and fail.
Validation: Check those commands against the installed taskr SKILL.md.
Progress: At most one note; ready with the report path, then done.
Skills to load: ~/.agents/skills/taskr/SKILL.md only.
EOF
herdr-launch opus-medium rs-first-check "$PWD/first-brief.md" \
  "$PWD/first-report.md" "$root"
taskr status --tree "$root"
```

Expect `ds1` for the goal and `task=... launch=... tab=... pane=...` from the
launcher, then the child under the root in status. The initial `new` command
may remind you to record a goal; the next command does so. Herdr opens a worker
tab without changing focus. Click `rs-first-check` to see the agent work.
The launcher gives the worker permission to run commands without approval
prompts; use this small brief in the demo directory for the first run.

## 8. Read the result

From the same plain shell, wait through taskr and read the durable result:

```sh
taskr wait --as "$root" --for done,fail --timeout 60000
taskr status --tree "$root"
taskr log "$root" --tree
cat "$PWD/first-report.md"
```

Expect a `done` event (`d` in the compact wait frame), a child with status
`done`, and the requested report. A timeout prints `x1 3 timeout`: run the
wait again. A `fail` event needs investigation before counting the task as done.
A `got` receipt means delivery; `ready` means a report is available; neither
means completion. After reading a completion event, acknowledge its numeric
event ID with `taskr ack EVENT_ID --as "$root"` (replace `EVENT_ID` from the
wait output). `taskr-tui` is the interactive ledger view; press `?` for help
and `q` to quit.

For another task, write a new brief and use a unique root/agent name. Coding
agents changing this kit should read [AGENTS.md](AGENTS.md) first.

## 9. Optional: enable push delivery

Push delivery sends taskr events to an idle agent without the agent running
`taskr wait`. It is opt-in for each agent CLI and needs that CLI's push adapter.
For a new worker, add `--push` before the lane in the usual launch command:
`herdr-launch --push LANE NAME BRIEF REPORT PARENT`.
Roots can message another root with
`taskr prompt ROOT_ID --as MY_ROOT_ID --text "Review the report when ready"`;
delivery uses push when the target has a live adapter, otherwise its pane.
Replace the uppercase placeholders with your lane, task IDs and paths. See
[section 5: The MCP server and push channel](products/taskr/AGENTS_SETUP.md#the-mcp-server-and-push-channel)
and [the push guide](products/taskr/docs/push.md) for setup and checks.
