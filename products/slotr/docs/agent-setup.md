# Agent setup

This file explains how slotr is installed on a Herdr fleet host and how agents use it there.

## Quick agent setup

```sh
curl -fsSL https://raw.githubusercontent.com/olafurns7/herdr-kit/main/products/slotr/install.sh | sh -s -- --config
```

The script downloads its pinned Linux x86_64 release, verifies the checksum
and archive layout, and installs it atomically to `~/.local/bin/slotr`.
It creates `~/.config/slotr/config.toml` from the release's example only
when no config exists, keeps an existing config, then checks that file.
It prints the installed version and tells you when to add `~/.local/bin`
to `PATH`. Running it again replaces the binary.

See [install.md](install.md) for requirements, manual steps, and mirrors.
If `XDG_CONFIG_HOME` or `SLOTR_CONFIG` is set, slotr normally reads another
config path; this installer still creates and checks the file above.
See [configuration.md](configuration.md).

## herdr-setup

herdr-setup installs the pinned release with its own checks.

## Using slotr in a fleet

Configure slotr on each host where you want to use it. A host that runs heavy long-lived runtimes
lists its pools in `~/.config/slotr/config.toml`. Write the wrapped command
into the worker's brief:

```sh
slotr run --pool runtime --campaign <campaign> --purpose "<what for>" \
  --task <lead task id> --pane <lead pane> -- <command>
```

- Exit 75 with a `slotr: stopped RUN: REASON` line on stderr means slotr
  stopped the runtime (pressure or configured lease/idle cleanup). A configured `on_stop`
  hook can send a note, and `events.jsonl` normally records the reason.
  Retry only under the owner's bounded recovery instructions.
- `slotr status` shows holders, the queue, and each waiter's wait reason.
- A lead may wait for admission with
  `herdr pane wait-output <pane> --match 'slotr: admitted'`.
- `slotr run` needs a systemd user bus, so it runs in a Herdr pane, not in a
  sandboxed agent tool shell.
- Heavy one-shot commands (type checks, lint, tests, builds, installs) go
  through the host's heavy pool when it has one:
  `slotr run --pool heavy --kind tsc --campaign <campaign> --purpose "<what for>" -- <command>`.
  See [usage.md](usage.md#heavy-one-shot-commands).
- Never stop another campaign's run to enforce priority. Keep your request
  queued and ask the hub through taskr with campaign, pool and wait reason.
  The hub changes weights with `slotr priority set CAMPAIGN WEIGHT`.
- Weight changes, caps, quiet handoffs and fairness apply before admission.
  Existing runs finish unless real pressure or configured lifecycle rules stop
  them. Cancel only an owned run under its cleanup authority, with `--reason`.
- Exit 75 without a slotr stop line is the workload's own result. Inspect
  events before a bounded retry; manual cancellation is not an automatic retry.

`examples/devbox.toml` shows a `runtime` pool with hooks that send task notes
through `taskr`; see [configuration.md](configuration.md#hooks).

Monorepo releases use `slotr/vX.Y.Z`; archive names retain plain `X.Y.Z`.
Set `SLOTR_VERSION=X.Y.Z` (also accepts `vX.Y.Z` or `slotr/vX.Y.Z`) for an
exact release. Without a pin, authenticated `gh` or `curl` plus `jq` enumerate all release pages and
select the highest stable slotr version with its archive/checksum assets
(and config when requested). Binary users need no Node, Turbo or Cargo.
