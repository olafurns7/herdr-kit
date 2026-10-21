# herdr-kit

herdr-kit runs on [Herdr](https://herdr.dev/docs/install/), the terminal multiplexer
for coding agents; Herdr 0.9.0 or newer is required for fleet orchestration and
the taskr plugin. taskr's standalone ledger CLI, including `taskr wait`, also
works without Herdr or the taskr daemon.

New here? Follow the [whole-kit quickstart](QUICKSTART.md) to launch your first delegated agent.

Monorepo containing [taskr](products/taskr) (task ledger and TUI),
[slotr](products/slotr) (resource admission queue), and [setup](products/setup)
(fleet installation assets). Each product keeps its own version and release tag:
`taskr/vX.Y.Z` or `slotr/vX.Y.Z`.

Install taskr CLI/TUI and its canonical skill from an exact product release.
From a herdr-kit checkout:

```sh
TASKR_VERSION=taskr/v0.26.0 sh products/taskr/install.sh
TASKR_VERSION=taskr/v0.26.0 sh products/taskr/install.sh --fleet
```

`--fleet` adds public orchestration/fleet skills, configured launch helpers,
minimal hooks, and staged integration assets. `--practices`, `--verification`,
`--slotr`, `--radar`, `--codator` (wrappers only), and `--notifications` are
separate opt-ins. Core services and the taskr plugin require manual activation;
the radar opt-in manages its external plugin. Binary users need no
Node, npm, Turbo, or Cargo. See [setup installation](products/setup/README.md)
for source staging, existing-file protection, and private lane/skill overlays.

## Optional: Codator

[Codator](https://github.com/olafurns7/codator) keeps several Codex and Claude Code
subscription accounts under local labels and starts the eligible account with
the most quota left. When `~/.local/bin/codator` is executable, `herdr-launch`
and `herdr-lead` forward `codex`/`claude` through it; `herdr-lead --resume`
requires it, but it is optional otherwise. Install v0.10.0, then sign in to each
account under a label (`NAME`):

```sh
(
set -eu
version=${CODATOR_VERSION:-v0.10.0}
installer=$(mktemp)
trap 'rm -f "$installer"' EXIT
curl -fsSL -o "$installer" "https://github.com/olafurns7/codator/releases/download/$version/install.sh"
CODATOR_VERSION="$version" sh "$installer"
)
codator login codex NAME
codator login claude NAME
```

Use the login command for each provider you use; see
[Codator's README](https://github.com/olafurns7/codator#quick-start) for the rest.

## Contributing

Contributors need Rust 1.99.0 and npm 11.8.0. From the repository root:

```sh
cargo build --locked --workspace
npm ci
npx turbo run build test lint
sh tools/gate.sh
```

`sh tools/gate.sh` runs all of these; the individual commands are for focused
checks. Turbo 2.5.8 is pinned as a contributor-only task runner; Rust task caching
is disabled. The gate uses a scratch HOME and runs the workspace checks with and
without taskr's contract feature, release/installer tests, setup tests and privacy
checks. On a shared host, run it through the configured slotr heavy pool:

```sh
slotr run --pool heavy --kind build --campaign YOUR_CAMPAIGN \
  --purpose 'herdr-kit gate' -- sh tools/gate.sh
```

## License

MIT; see [LICENSE](LICENSE). taskr and slotr also carry their own copy, and
third_party/ keeps each upstream license. Release binaries of slotr ship
[THIRD-PARTY-NOTICES](products/slotr/THIRD-PARTY-NOTICES).
