# Moving the fleet to herdr-core

This runbook moves a fleet from Herdr 0.9 to herdr-core (`hc`) in one short
stop. herdr-kit keeps its `herdr` commands: under `HERDR_KIT_BACKEND=hc`, the
`herdr0` program serves them over hc. Agents read the rules in the
orchestration skill's [launch.md](../skills/herdr-orchestration/launch.md)
"herdr-core backend"; this file is for the owner.

Placeholders: `HOST` is a machine's name, `TARGET` its ssh target, `ROOT` a
taskr root id, `DIR` a working directory, `LABEL` a workspace label, `LANE`,
`ACCOUNT` and `SID` a root's recorded lane, account and native session,
`PANE` a herdr-shaped pane id (`wN:pN`), `FILE` a handover path, and
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

5. **Owner**, each host: probe. Open an hc shell terminal with the lead line
   of the stop's step 7, attach to it (`hc attach probe`), and use `herdr`
   commands only: a Claude and a Codex (`--no-daemon`) agent each report
   `idle` and `working` in `herdr agent get`; a permission prompt shows
   `blocked`; `herdr agent wait` returns; `herdr pane wait-output` matches
   output printed after its start. Save each result, then `hc kill probe` and
   `hc rm probe`. Do not start the stop until both agents report status.

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

6. **Owner**, the ledger host: run the taskr unit with the hc drop-in, as the
   installer printed; with hc the Herdr plugin no longer starts the daemon.
   `enable --now` is not enough: it leaves a running daemon on Herdr 0.9.

   ```sh
   systemctl --user daemon-reload && systemctl --user enable taskr.service && systemctl --user restart taskr.service
   taskr daemon --status     # fresh, supervised
   ```

7. **Owner**: start the hub, then each active lead, each in its own terminal.
   The hub's command adopts its root and resumes its session:

   ```sh
   hc run --bg --name hub --keep --no-parent --cwd DIR -- \
     herdr0 enter --new-workspace LABEL -- \
     herdr-hub move ROOT --lane LANE --account ACCOUNT -- --resume SID
   ```

   A lead gets a shell terminal the same way, then the hub (or the owner from
   that hc shell, never a plain terminal, where `herdr` is still Herdr 0.9)
   moves the lead into it:

   ```sh
   hc run --bg --name lead-ROOT --keep --no-parent --cwd DIR -- \
     herdr0 enter --new-workspace LABEL -- "$SHELL" -l
   # in that terminal: echo "$HERDR_PANE_ID" gives PANE
   herdr-lead move ROOT PANE --worktree DIR --lane LANE --account ACCOUNT --resume SID
   ```

   Each root: `taskr consumer show --as ROOT` shows a live holder, and one
   pushed prompt is acked. Parked roots stay down until needed.
8. One lead: run one end-to-end lane (`herdr-launch --gather ...`): `got`,
   `ready`, `done` in the ledger, then teardown closes its terminal.

## After a reboot

hc resumes no agent. A kept terminal comes back as a plain shell without the
`herdr0 enter` environment, so `herdr` there is Herdr 0.9 or missing. In each
root's terminal, first restore it with the ids that terminal carries (its
`ws=`, `tab=` and `pane=` labels in `hc get NAME`):

```sh
exec herdr0 enter --workspace W --tab T --pane PANE -- "$SHELL" -l
```

Then start the root with the step-7 line for it.

## Retire Herdr 0.9

**Owner**, after one full day on hc, each host: stop the Herdr 0.9 server
(`systemctl --user disable --now herdr` where a unit runs it, else
`herdr server stop`). Keep the Herdr 0.9 binary until the rollback window is
over.

## Rollback

Possible until Herdr 0.9 is retired; its server keeps running until then.

1. Roots: save state and exit, as in the stop's steps 3 and 4.
2. **Owner**, each host: install herdr-kit again with the same `FLAGS` and
   without `--herdr-backend hc` (a previous commit if the release itself is
   at fault). The unit file is the same in both modes; the hc switch and the
   `herdr0` shim live in its drop-in, which no install removes. Remove it
   before the reload, or the daemon stays on hc.

   ```sh
   sh products/setup/install.sh FLAGS
   rm -f ~/.config/systemd/user/taskr.service.d/herdr-kit-hc.conf
   systemctl --user daemon-reload && systemctl --user restart taskr.service
   ```

3. Start each root again in a Herdr 0.9 pane with `herdr-hub move` and
   `herdr-lead move ... --resume SID` (orchestration `recovery.md`).
4. **Owner**: leave the hc core running or stop it with `hc server stop`;
   nothing in Herdr 0.9 uses it.
