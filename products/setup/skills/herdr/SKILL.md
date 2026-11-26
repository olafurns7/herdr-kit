---
name: herdr
description: herdr0, the herdr command subset on the herdr-core backend (HERDR_KIT_BACKEND=hc). Load before any herdr command on a host whose herdr-kit install chose hc. Never run hc itself.
---

# herdr on the herdr-core backend (herdr0)

herdr-kit installed this skill because this host runs the herdr-core backend
(`HERDR_KIT_BACKEND=hc`). Here `herdr` is `herdr0`, a herdr-kit program that
serves the Herdr 0.9 commands herdr-kit uses on top of herdr-core. It keeps the
same id shapes (`wN`, `wN:tN`, `wN:pN`) and `--machine`. herdr-kit does not
install the herdr-core manual, because its commands are for the owner only.

## Rules

- Never run `hc`, in any form: no `hc server`, no `herdr-core`, no hc
  installer, and no bare `hc` command, not even for help. The first `hc` call
  starts a core, and that core keeps the caller's environment. The owner
  starts the core and every root.
- Use only the commands below, through `herdr`.
- Exit 3 with `herdr0: no hc core running; the owner starts it`: stop and tell
  the owner.
- Exit 2 with `herdr0: unsupported: ...`: herdr0 has no form of that command.
  Do not work around it with hc; follow the herdr-core backend rules below.

## Commands

This list comes from herdr0's usage (`herdr --help` prints it):

```text
  workspace create|list|rename|close     tab create|list|get|rename|close
  pane current|get|list|read|run|send-text|send-keys|wait-output|process-info|report-metadata
  agent start|label|prompt|wait|list|get|read      machine list      notification show (no-op)
  herdr0 enter [--workspace W --tab T --pane P | --new-workspace LABEL] -- ARGV...
  herdr0 app-labels [--workspace W | --pane P] [--apply]
```

`agent label` and `app-labels` are herdr0's own. `agent label NAME --kind K
--pane P [-- AGENT_ARGS...]` names an agent its pane's shell runs itself
(`herdr-hub` uses it for the hub). `app-labels` lists the Herdr app labels
(`herdr.space`, `herdr.tab`, `where`) herdr0's terminals should carry; with
`--apply` it writes the missing ones. Every terminal herdr0 creates already has
them, so the Herdr desktop app shows it. Closing a space, tab or pane in the
app ends its terminals.

Unsupported (exit 2): plugin, server, worktree, status, agent explain, --skill.

Global flags: `--machine M` and `--json`. Flags and output follow the Herdr 0.9
forms in the herdr-orchestration skill.

## Where the rules are

Read the "herdr-core backend" section of the herdr-orchestration skill's
`launch.md` (`~/.agents/skills/herdr-orchestration/launch.md`). It covers the
switch, Codex `--no-daemon`, the hub start form, worktrees without the worktree
plugin, and where status, notifications, reboot and teardown differ.
