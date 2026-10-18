# Validate and release

This file explains how to test slotr locally and how to publish a release.

## Validate locally

The integration tests use a fake systemd driven by Python 3.11+ stdlib.
Run from `products/slotr`.

```sh
sh -n install.sh
dash -n install.sh
sh tests/install_sh_test.sh
SH=dash dash tests/install_sh_test.sh
sh -n scripts/release.sh
sh scripts/notices.sh --check
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build -p slotr --release --locked --target x86_64-unknown-linux-musl
file ../../target/x86_64-unknown-linux-musl/release/slotr
```

On a shared host, wrap cargo commands in its heavy pool, for example
`slotr run --pool heavy --kind build --purpose 'slotr release' -- cargo test --locked`.
When slotr is not installed, use the host's lock, for example
`flock /path/to/heavy.lock cargo test --locked`.

Only `SLOTR_TEST=1` enables the test seams: `SLOTR_PROC_ROOT`,
`SLOTR_CGROUP_ROOT`, `SLOTR_SYSTEMD_RUN`, `SLOTR_SYSTEMCTL`, and
`SLOTR_CLOCK_OFFSET_FILE` (seconds added to the CLOCK_BOOTTIME clock used
for state timing). Production ignores all of them.

## Release

Releases are built locally on a clean Linux x86_64 checkout:

```sh
sh scripts/release.sh slotr/vX.Y.Z
sh scripts/release.sh slotr/vX.Y.Z --publish
```

- VERSION must match the version in `Cargo.toml`. The release tag is
  `slotr/vX.Y.Z`; asset names retain plain `X.Y.Z`. A mismatch exits 2 before building.
- The script refuses dirty trees, an existing local `slotr/vX.Y.Z` tag, and
  hosts other than Linux x86_64, with exit 2.
- Before building, `sh scripts/notices.sh --check` must confirm that the
  checked-in notices match the locked target dependencies. Run
  `sh scripts/notices.sh` to refresh them from cached crate and toolchain
  license files after a dependency update.
- The build remaps cargo home, rustup home, and checkout paths. After packing,
  the script refuses builder paths or the login name in the binary strings
  or archive listing.
- Assets in `dist/` are `slotr-VERSION-x86_64-unknown-linux-musl.tar.gz` and
  its `.sha256` file. The release also includes `install.sh` and
  `config.example.toml` from the checkout, in both create and upload paths.
  The archive holds three regular files in this order: `LICENSE`,
  `THIRD-PARTY-NOTICES`, and `slotr`, with numeric owner and group 0.
- `--publish` explicitly creates a GitHub release in `olafurns7/herdr-kit`
  with `gh`, targeting the exact local HEAD that was built. If the release
  already exists, it only attaches the assets to it and does not check or change its target.
- There are no GitHub Actions workflows.

The taskr installer's `--slotr` flag runs `install.sh` from the checkout,
which installs the latest release unless `SLOTR_VERSION` is set; see
[agent-setup.md](agent-setup.md).

Run release commands from `products/slotr`; Cargo output is under the repository root `target/`.