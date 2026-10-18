# Validate and release

This file explains how to test slotr locally and how to publish a release.

## Validate locally

The integration tests use a fake systemd driven by Python 3.11+ stdlib.

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

A shared host may wrap the cargo commands in its own lock
(`flock /path/to/heavy.lock cargo test --locked`).

Only `SLOTR_TEST=1` enables the test seams: `SLOTR_PROC_ROOT`,
`SLOTR_CGROUP_ROOT`, `SLOTR_SYSTEMD_RUN`, `SLOTR_SYSTEMCTL`, and
`SLOTR_CLOCK_OFFSET_FILE` (seconds added to the CLOCK_BOOTTIME clock used
for state timing). Production ignores all of them.

## Release

Releases are built locally on a clean Linux x86_64 checkout:

```sh
cd products/slotr
sh scripts/release.sh slotr/vX.Y.Z
sh scripts/release.sh slotr/vX.Y.Z --publish
```

- VERSION must match the version in `Cargo.toml`. The release tag is
  `slotr/vX.Y.Z`; asset names retain plain `X.Y.Z`. A mismatch exits 2 before building.
- The script refuses dirty trees, an existing local `slotr/vX.Y.Z` tag, and
  other hosts, with exit 2.
- Before building, `sh scripts/notices.sh --check` must confirm that the
  checked-in notices match the locked target dependencies. Run
  `sh scripts/notices.sh` to refresh them from cached crate and toolchain
  licence files after a dependency update.
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

herdr-setup's installer installs a pinned release; see
[agent-setup.md](agent-setup.md).

Run release commands from `products/slotr`; Cargo output is under the repository root `target/`.

## Upgrading to 0.3.1 (S1)

This slice adds managed campaign weights, a default 30-second quiet handoff,
a 300-second fairness floor, positive-weight hard caps, and audited manual
stops. Priority/campaign yield stops are removed. Memory accounting remains
based on declared cost minus observed anonymous memory; measured sizing and
automatic replay are not part of this release.

Runtime schema stays 1. New scheduling clocks and stop intent are sidecars
under the same lock, so older writers cannot discard them when rewriting
state.json. Managed policy is separate persistent state. No schema migration
or forced drain is required for S1, but **0.3.0 supervisors still running can
still yield-stop until they finish**. Let admitted work finish naturally;
never stop another campaign to install this change. Verify old processes have
finished before claiming the scheduling-stop guarantee for the whole host.
Do not activate managed overrides while relying on 0.3.0 admission: it cannot
read them or enforce new holds/fairness. Roll out new request producers together.

For rollback, withdraw only owned queued requests and let 0.3.1 supervisors
finish. Remove new priority keys from config presented to 0.3.0 (its loader
rejects unknown keys). Keep policy, events and sidecars for inspection;
0.3.0 ignores them, so its legacy priority/yield behavior returns. Do not reset
runtime state to simulate a drain. Source/fake-manager tests are not real
systemd runtime proof; the release lead owns any authorized real-unit smoke.
