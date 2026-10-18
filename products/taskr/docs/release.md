# Releasing

Releases are built on a maintainer's machine. There are no GitHub Actions:
nothing builds or publishes on push.

The released `taskr` is the Rust build (`crates/taskr`). The contract tests in
`tools/contract/` compare it with stored golden fixtures.

## release.sh

Run from `products/taskr`; releases publish to `olafurns7/herdr-kit`.
For a build that needs the Darwin half, first set the Mac command explicitly:

```sh
export TASKR_RELEASE_SSH='ssh -o BatchMode=yes -o ConnectTimeout=10 user@mac-host'
```

Replace `user@mac-host` with your Mac's SSH login. If this variable is unset,
empty or whitespace-only, release builds (including `--dry-run` and
`--darwin-only`) exit 2 before building. `--darwin-from DIR` and `--promote` do not need it.

```sh
./release.sh taskr/v1.2.3 --dry-run         # build and check everything; publish nothing
./release.sh taskr/v1.2.3                   # build, tag, push the tag, create a stable release
./release.sh taskr/v1.2.3 --darwin-only     # only build the darwin binaries on the Mac, into dist-darwin/
./release.sh taskr/v1.2.3 --darwin-from DIR # release with darwin binaries built by hand
```

The tag is `taskr/vMAJOR.MINOR.PATCH`; `vMAJOR.MINOR.PATCH` is also accepted
and normalized to that tag. The binary reports `vMAJOR.MINOR.PATCH`; a `dev`
or other build version is refused. The script runs on Linux and:

1. **Preflight.** Refuses a dirty worktree (a dry run skips this check). Without
   `--dry-run` it also refuses an existing tag and a `HEAD` that is not in
   `origin/main`. It checks the tools below and, unless `--darwin-from` is
   supplied, the Mac's ssh login. On a missing tool it prints the install block
   from `tools/contract/README.md`.
2. **Gate.** `cargo fmt --all --check`, then `cargo clippy --locked --workspace
   --all-targets -- -D warnings` and `cargo test --locked --workspace`, each
   with default features and with `--features taskr/contract`. Tests run with a
   scratch HOME and ledger under
   `.scratch/release/`, without task identity or any inherited `TASKR_CONTRACT_*`
   filter.
3. **Linux half.** `tools/release-rust.sh` with the release version: static
   `taskr` and `taskr-tui` musl binaries for x86_64 and aarch64 through
   `cargo zigbuild`, a refusal of any test-only hook, and a version smoke of
   the native `taskr` binary. Cargo, Rustup and workspace paths are remapped
   with `--remap-path-prefix`; binaries containing `/home/`, `/Users/` or the
   builder's login are refused.
4. **Darwin half**, on the Mac over ssh (there is no CI): `mktemp -d` there,
   `git archive HEAD` into it, then `tools/release-rust.sh` with the same
   version, which smokes the native arm64 `taskr`. All four binaries are
   copied back, checked against the Mac's `SHA256SUMS`, and the Mac directory
   is removed.
   darwin/amd64 gets a static check only (`file` reports an x86_64 Mach-O),
   on an arm64 Mac without Rosetta. `TASKR_RELEASE_SSH` replaces the
   whole remote command and has no default; tests set it to a local fake, so they never reach
   the Mac.
5. **Assets.** The binaries are renamed `taskr-{linux,darwin}-{amd64,arm64}`
   and `taskr-tui-{linux,darwin}-{amd64,arm64}`. Just before the checksums,
   all eight must have the expected `file` type (Linux ELF must be statically
   linked) and contain no test-only hook (`TASKR_FROZEN_NOW`,
   `TASKR_CONTRACT_`, `--contract-`) or builder path (`/home/`, `/Users/`),
   whether built here or imported.
   Only `taskr` embeds the release version; the TUI package version remains `0.0.0`.
   `taskr` linux/amd64 must print `vX.Y.Z` from `--json version` (linux/arm64
   too when `qemu-aarch64` is installed).
6. **Packaging.** `SKILL.md`, `install.sh`, `skill.tar.gz` (the skill and
   `references/`) and `plugin.tar.gz` (the Herdr plugin, stamped with the
   version), plus one `SHA256SUMS` over all twelve, in `dist/`. Both tarballs
   use numeric owner/group 0/0.
7. **Publish**, without `--dry-run`: tag, push the tag, and
   `gh release create` as a stable release.

- After publication, update the `taskr/v0.25.1` example pins to the published tag
  in the repository [README.md](../../../README.md) and
  [QUICKSTART.md](../../../QUICKSTART.md), the taskr [README.md](../README.md),
  [AGENTS_SETUP.md](../AGENTS_SETUP.md), [docs/install.md](install.md), and
  [products/setup/README.md](../../setup/README.md). Update the `TASKR_VERSION`
  default in [products/setup/bootstrap.sh](../../setup/bootstrap.sh) and its
  pin assertions in [bootstrap_test.sh](../../setup/tests/bootstrap_test.sh)
  and [install_test.sh](../../setup/tests/install_test.sh) to the same tag.

The thirteen assets are the eight platform binaries, `SKILL.md`, `install.sh`,
`skill.tar.gz`, `plugin.tar.gz` and `SHA256SUMS`. The installer installs both
binaries beside each other; `TASKR_NO_TUI=1` skips the TUI. All downloaded binaries must
pass checksum verification before
either installed binary is replaced; each replacement uses a temporary file
in the install directory and a rename.

### Stable releases and promotion

New releases are stable immediately. `install.sh` without `TASKR_VERSION`
enumerates all release pages and selects the highest complete stable taskr
version, skipping drafts, prereleases and other products. To pin a version when
invoking the installer directly, use `TASKR_VERSION=taskr/vX.Y.Z` (see
[install](install.md#2-install)).

Stable publication also refuses a version older than the newest stable taskr release.

`--promote` is only for a release published as a prerelease earlier:

```sh
./release.sh taskr/v1.2.3 --promote # gh release edit taskr/v1.2.3 --prerelease=false --latest=false
```

It enumerates stable taskr releases (`gh api --paginate`) and refuses a version
older than the highest one. It does not set the repository-wide `latest` flag.
`TASKR_RELEASE_GH` replaces the `gh` command; tests set it to a fake.

`sh tools/release-test.sh` runs the refusal tests (imported Darwin checksums,
architectures, test-only hooks and builder paths, Rust path remapping and
builder-login guards, numeric 0/0 tarball ownership, Linux static linkage,
missing or whitespace-only SSH commands, an older `--promote`) and checks complete
packaging in all three
Darwin modes and stable publication with fakes.

### Without the Mac

If the Mac is unreachable, build the binaries there by hand later, or on any
Apple-silicon Mac with the Rust toolchain:

```sh
TASKR_VERSION=taskr/v1.2.3 tools/release-rust.sh    # on the Mac, from a checkout of the release commit
```

Copy `dist-rust/taskr-rust-aarch64-apple-darwin`,
`dist-rust/taskr-rust-x86_64-apple-darwin`,
`dist-rust/taskr-tui-rust-aarch64-apple-darwin`,
`dist-rust/taskr-tui-rust-x86_64-apple-darwin` and `dist-rust/SHA256SUMS` into a
directory and pass it with `--darwin-from DIR`. It is checked (checksums,
Mach-O type, release version for `taskr` only, no test-only hook or
`/home/` or `/Users/` builder path) before the gate.
`./release.sh <tag> --darwin-only` does the same build over ssh into
`dist-darwin/`.

### Prerequisites

On the Linux host: `git`, Rust from `rust-toolchain.toml` (1.99.0 with the
musl and darwin targets), `zig` and `cargo-zigbuild` (user-space install in
`tools/contract/README.md`), `python3`, `file`, `strings`, `sha256sum`, `tar`,
`ssh`. Publishing needs an authenticated `gh`. On the Mac: Rust from
`rust-toolchain.toml` with both darwin targets, and
`python3` (the command-line tools).

### Rollback

The previous release stays the rollback. On a host:

1. download `taskr-<os>-<arch>` from the previous release (or
   `TASKR_VERSION=taskr/vX.Y.Z sh install.sh`, replacing X.Y.Z with the previous version);
2. swap it in for the installed `taskr`;
3. run `taskr daemon --restart`.

## tools/release-rust.sh

```sh
TASKR_VERSION=taskr/v1.2.3 tools/release-rust.sh
```

It takes no arguments; the `TASKR_VERSION` default is
`git describe --match 'taskr/v[0-9]*' --always --dirty`, with the `taskr/`
prefix removed. It builds `taskr` and `taskr-tui` for the
host's platform family (Linux: static musl x86_64 and aarch64; macOS: arm64
and x86_64). It preserves caller `RUSTFLAGS` and appends `--remap-path-prefix`
for Cargo home, Rustup home and the workspace. It refuses test-only hooks,
`/home/`, `/Users/` or the builder's login in either binary
(`release-rust: builder path or login name found`), runs the native `taskr` once
(`version`, `help`, `status`) in an empty scratch home, and writes
`dist-rust/taskr-rust-<target>`, `dist-rust/taskr-tui-rust-<target>` and
`dist-rust/SHA256SUMS`. It creates no tag, uploads nothing and does not touch
the installed binary. `release.sh` runs it for both halves.

## taskr-tui

`taskr-tui` ships for the same four platforms as `taskr` and is installed by
`install.sh` by default. It needs no runtime flags; `TASKR_BIN` optionally
overrides the taskr CLI path. To build from source instead, use
`cargo install --locked --path crates/taskr-tui`.

Its screenshots under `docs/images/` come from the synthetic fixture:

```sh
cargo run -p taskr-tui --example frames -- frames   # text and ANSI frames
python3 crates/taskr-tui/tools/png.py frames        # PNGs (needs Pillow and DejaVu Sans Mono)
```

The four README images are the 120x40 glance and campaign frames, dark and
light, reduced to a 256-color palette to keep them small:

```sh
python3 - <<'PY'
from PIL import Image
for frame, name in [("glance-120x40", "glance-dark"), ("glance-120x40-light", "glance-light"),
                    ("campaign-120x40", "campaign-dark"), ("campaign-120x40-light", "campaign-light")]:
    image = Image.open(f"frames/{frame}.png").quantize(colors=256, dither=Image.Dither.NONE)
    image.save(f"docs/images/{name}.png", optimize=True)
PY
```

`cargo test -p taskr-tui` compares every frame with `crates/taskr-tui/tests/golden/`.
