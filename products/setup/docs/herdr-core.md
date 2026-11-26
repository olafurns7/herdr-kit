# Moving the fleet to herdr-core

This runbook moves a fleet from Herdr 0.9 to herdr-core (`hc`) in one short
stop. herdr-kit keeps its `herdr` commands: under `HERDR_KIT_BACKEND=hc`, the
`herdr0` program serves them over hc. Agents read the rules in the
orchestration skill's [launch.md](../skills/herdr-orchestration/launch.md)
"herdr-core backend"; this file is for the owner.

Placeholders: `HOST` is a machine's name, `TARGET` its ssh target, `ROOT` a
taskr root id, `DIR` a working directory, `CHECKOUT` a herdr-kit checkout,
`SHIM` a scratch directory, `SRC` a repository's source checkout, `WT` a
new task worktree, `LABEL` a workspace label, `LANE`,
`ACCOUNT` and `SID` a root's recorded lane, account and native session,
`PANE` a herdr-shaped pane id (`wN:pN`), `FILE` a handover path,
`INSTALLED` an installed file with a staged sidecar, and
`FLAGS` the flags of that host's last herdr-kit install.

**Owner** marks a step only the owner runs, from a plain terminal outside any
agent session (never from an agent, an hc terminal or a Herdr pane). No agent
ever runs `hc server`, `herdr-core`, the hc installer or a bare `hc` command:
the first `hc` call starts a core that keeps the caller's environment.

## Before the stop

Herdr 0.9 and hc keep separate binaries, sockets and state, so these steps run
beside normal work, days before the stop.

1. Merge the herdr-kit release with the hc backend. Do not install it yet.
2. **Owner**, each host: install hc.

   ```sh
   curl -fsSL https://releases.herdr.dev/core/install.sh | sh
   hc --version        # hc 0.1.22 or later
   herdr --version     # still Herdr 0.9
   ```

   hc must be in `~/.local/bin` or `/usr/local/bin`: the taskr unit's hc
   drop-in has a fixed `PATH` of the `herdr0` shim directory,
   `~/.local/bin`, `/usr/local/bin`, `/usr/bin` and `/bin`.

3. **Owner**, each host: start the core with a clean environment. A core keeps
   the environment it starts with, and so does every terminal it runs. Set
   `PATH` to the directories that hold the agent CLIs, their wrappers,
   `taskr`, and the package managers the fleet uses, in the order the
   Herdr 0.9 server uses today.

   ```sh
   env -i HOME="$HOME" USER="$USER" LOGNAME="$USER" SHELL="$SHELL" LANG=C.UTF-8 \
     XDG_RUNTIME_DIR="/run/user/$(id -u)" \
     PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin" \
     "$HOME/.local/bin/hc" server start
   hc server status --json   # running; keeps_running_after_logout: true
   ```

   If `keeps_running_after_logout` is false, run the
   `sudo loginctl enable-linger -- USER` line that hc prints, once.
4. **Owner**, the ledger host: add each other host.

   ```sh
   hc machine add TARGET --name HOST
   hc machine ls                         # HOST connected
   hc exec --on HOST -- uname -n         # prints HOST's name
   ```

5. **Owner**, each host: put the `herdr0` shim first on PATH in hc shells.
   Append this one line at the end of `~/.zshrc`, `~/.bashrc` and `~/.profile`
   (a bash login shell's stock `~/.profile` sources `~/.bashrc`, then puts
   `~/.local/bin` first again):

   ```sh
   if [ "${HERDR_KIT_BACKEND:-}" = hc ]; then PATH="$HOME/.local/lib/herdr-kit/hc-bin:$PATH"; fi
   ```

   It does nothing until the switch is set (only `herdr0 enter` sets it), and
   a rollback may leave it. Without it a login shell's rc files can put Herdr
   0.9's `herdr` first, and under the switch `herdr-launch`, `herdr-lead` and
   `herdr-hub` then refuse with exit 2.
6. **Owner**, each host: probe from a checkout of the release branch at
   `CHECKOUT`; do not install the release before the stop. Make a scratch
   shim directory `SHIM` with `ln -s CHECKOUT/products/setup/bin/herdr0
   SHIM/herdr`, open an hc shell terminal and attach to it:

   ```sh
   hc run --bg --name probe --keep --no-parent --cwd DIR -- \
     CHECKOUT/products/setup/bin/herdr0 enter --new-workspace probe -- "$SHELL" -l
   hc attach probe
   # inside:
   export PATH="SHIM:$PATH"
   command -v herdr      # SHIM/herdr
   ```

   Then use `herdr` commands only: a Claude and a Codex (`--no-daemon`) agent
   each report `idle` and `working` in `herdr agent get`; a permission prompt
   shows `blocked`; `herdr agent wait` returns; `herdr pane wait-output`
   matches output printed after its start. Save each result, then
   `hc kill probe` and `hc rm probe`. Do not start the stop until both agents
   report status.

## The stop

1. Hub: freeze. No root starts a new lane; merges continue.
2. Leads: drain. Seats and helpers finish; each writer reaches `ready` or is
   closed `abandoned` with a handover line. `taskr status` shows no open lane
   with a pane.
3. Each root: save its state.

   ```sh
   taskr handover --as ROOT --out FILE
   ```

   Record each root's lane, account and native session (`SID`) for step 7.
4. Each root: exit its agent (Ctrl+C twice). `herdr agent list` shows no root.
5. **Owner**, each host: install herdr-kit with the hc backend.

   ```sh
   cd DIR && git pull --ff-only
   sh products/setup/install.sh FLAGS --herdr-backend hc
   ```

   Then resolve the staged sidecars. A changed installed file (a hand patch,
   say) is left in place and the new text is staged beside it as
   `INSTALLED.herdr-setup`; the installer's last block lists every such sidecar.
   Agents keep reading the old `INSTALLED` until it is resolved. For each one
   under "Staged sidecars":

   ```sh
   diff -u INSTALLED INSTALLED.herdr-setup
   mv INSTALLED.herdr-setup INSTALLED
   ```

   The next install finds `INSTALLED` identical and records it (`SKIP`). A
   file that must keep a local change goes in `~/.config/herdr-setup/keep`
   instead (`--keep`), and the installer stops staging it.

   A sidecar under "Settings sidecars" (`~/.claude/settings.json`,
   `~/.codex/hooks.json`, `~/.codex/config.toml`) is the kit's template, not a
   replacement: merge the keys and hooks that QUICKSTART.md names into the file
   by hand, keep your own settings, and never move it into place. It stays
   listed until the file matches it or is kept. A sidecar under "Stale
   sidecars" holds other content or is a symlink, so the new text was not
   staged: compare it, remove it, and install again.

6. **Owner**, each host: run the taskr unit with the hc drop-in, as the
   installer printed; with hc the Herdr plugin no longer starts the daemon, on
   the ledger host or on a client. `enable --now` is not enough: it leaves a
   running daemon on Herdr 0.9.

   ```sh
   systemctl --user daemon-reload && systemctl --user enable taskr.service && systemctl --user restart taskr.service
   taskr daemon --status     # ledger host: fresh, supervised
   ```

   A client (a host with `~/.local/state/taskr/server.url`) runs `taskr daemon`
   without `--stay`, and its status has no `supervised` key. There,
   `taskr --json daemon --status` shows `"mode":"client"`, `"running":true` and
   a `pid` equal to `systemctl --user show -p MainPID --value taskr.service`.

7. **Owner**: start the hub, then each active lead. Run these lines only in
   an hc shell terminal, never a plain terminal, where `herdr` is still
   Herdr 0.9. The hub gets its own shell terminal; type its start line there:

   ```sh
   hc run --bg --name hub --keep --no-parent --cwd DIR -- \
     herdr0 enter --new-workspace LABEL -- "$SHELL" -l
   hc attach hub
   # inside:
   command -v herdr      # ~/.local/lib/herdr-kit/hc-bin/herdr
   herdr-hub ROOT --lane LANE --account ACCOUNT -- --resume SID
   # or, when that session started with Code Mode:
   env HERDR_HUB_CODE_MODE=1 herdr-hub ROOT --lane LANE --account ACCOUNT -- --resume SID
   ```

   A lead's `PANE` is an hc shell terminal at its prompt, such as the root
   pane of `herdr tab create`. From a second hc shell terminal (`ops`, made
   the same way), create it and start the lead:

   ```sh
   command -v herdr      # ~/.local/lib/herdr-kit/hc-bin/herdr
   herdr tab create --workspace "$HERDR_WORKSPACE_ID" --cwd DIR --label lead-ROOT --no-focus \
     | jq -er '.result.root_pane.pane_id'     # PANE
   herdr-lead start ROOT PANE --worktree DIR --lane LANE --account ACCOUNT --resume SID
   # or, when that session started with Code Mode:
   env HERDR_LEAD_CODE_MODE=1 herdr-lead start ROOT PANE --worktree DIR --lane LANE --account ACCOUNT --resume SID
   ```

   Start one root per Codator account at a time: a second start on an
   account whose first start has not finished fails with `account is busy`.
   Start the next root on that account after the first one is up.

   Under the switch the three launchers refuse with exit 2 when
   `command -v herdr` is not the hc-bin shim. Each root: `taskr consumer show
   --as ROOT` shows a live holder, and one pushed prompt is acked. Parked
   roots stay down until needed.
8. One lead: run one end-to-end lane (`herdr-launch --gather ...`): `got`,
   `ready`, `done` in the ledger, then teardown closes its terminal.

## The Herdr desktop app

The Herdr app shows a terminal only when it carries `herdr.space=<id>` (and
`herdr.tab=<id>`); other terminals stay hidden. Every terminal `herdr0`
creates carries them, derived from its herdr0 ids: one herdr0 workspace is one
app space and one herdr0 tab one app tab. The app names a space after its
terminal's folder and numbers its tabs (it keeps that in a `where` label it
writes itself); herdr0's workspace and tab labels, and `herdr tab rename` or
`herdr workspace rename`, do not set those names. Under hc `herdr-hub` also
labels the hub's terminal with its agent (`agent=claude`, `agent_name=hub-ROOT`).

Terminals made by an earlier herdr-kit have no app labels. After installing
this release, label them once from an hc shell terminal (where `herdr` is
`herdr0`). Without `--apply` the command only lists each terminal, its
workspace and tab, the two labels and which of them would change:

```sh
herdr app-labels                         # dry run, all herdr0 terminals
herdr app-labels --workspace W --apply   # one workspace at a time
herdr app-labels --pane PANE --apply     # one terminal: the hub's last
herdr app-labels --apply                 # anything left
```

It writes only `herdr.space` and `herdr.tab`, and only on terminals with a
herdr0 `ws=` label; the app's own terminals have none and are never changed.
One exception: `herdr0 enter --new-workspace` run inside a terminal
the app opened moves that terminal to a new app space; the step-7 start,
`hc run --bg ... -- herdr0 enter --new-workspace ...`, makes a new terminal
and moves none.

Closing a space, tab or pane in the app ends the terminals in it, the hub's
included. Close only what you mean to end. A root ended this way starts again
with its step-7 start line.

## Task worktrees under hc

The Herdr worktree plugin does not run under hc: `herdr worktree` exits 2 and
no repository's worktree setup plugin runs. A lead creates the worktree with
`git worktree add` and opens its workspace with `herdr workspace create --cwd`
(launch.md "herdr-core backend"). When the repository's AGENTS.md names a
worktree setup plugin, run its setup script by hand from the new worktree.
For a plugin whose script reads `HERDR_PLUGIN_CONTEXT_JSON`:

```sh
cd WT
env HERDR_ENV=1 HERDR_PLUGIN_CONTEXT_JSON='{"worktree":{"checkout_path":"WT"}}' sh SRC/.herdr/setup.sh
```

Use the setup script path the repository's plugin names.

## After a reboot

hc resumes no agent. A kept terminal comes back as a plain shell without the
`herdr0 enter` environment, so `herdr` there is Herdr 0.9 or missing. In each
root's terminal, first restore it with the ids that terminal carries (its
`ws=`, `tab=` and `pane=` labels in `hc get NAME`):

```sh
exec herdr0 enter --workspace W --tab T --pane PANE -- "$SHELL" -l
```

Then start the root with its step-7 start line.

## Retire Herdr 0.9

**Owner**, after one full day on hc, each host: stop the Herdr 0.9 server
(`systemctl --user disable --now herdr` where a unit runs it, else
`herdr server stop`). Keep the Herdr 0.9 binary until the rollback window is
over.

## Rollback

Possible until Herdr 0.9 is retired; its server keeps running until then.

1. Roots: save state and exit, as in the stop's steps 3 and 4.
2. **Owner**, each host: install herdr-kit again with the same `FLAGS` and
   with `--herdr-backend herdr`, so it cannot inherit the switch (a previous commit if the release itself is
   at fault). The unit file is the same in both modes; the hc switch and the
   `herdr0` shim live in its drop-in, which no install removes. Remove it
   before the reload, or the daemon stays on hc.

   ```sh
   sh products/setup/install.sh FLAGS --herdr-backend herdr
   rm -f ~/.config/systemd/user/taskr.service.d/herdr-kit-hc.conf
   systemctl --user daemon-reload && systemctl --user restart taskr.service
   ```

   On a client the unit exists only for hc; under Herdr 0.9 the plugin starts
   the client daemon. Stop and disable the unit there instead of restarting it:

   ```sh
   rm -f ~/.config/systemd/user/taskr.service.d/herdr-kit-hc.conf
   systemctl --user disable --now taskr.service && systemctl --user daemon-reload
   ```

3. Start each root again from a Herdr 0.9 pane with the start forms: type
   `herdr-hub ROOT --lane LANE --account ACCOUNT -- --resume SID` in the hub's
   pane, and run `herdr-lead start ROOT PANE --worktree DIR --lane LANE
   --account ACCOUNT --resume SID` for each lead's shell pane.
4. **Owner**: leave the hc core running or stop it with `hc server stop`;
   nothing in Herdr 0.9 uses it.
