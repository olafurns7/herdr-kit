#!/bin/sh
# Heavy: callers wrap this gate in slotr run --pool heavy --kind build.
set -eu
# Tmp permission fixtures require a deterministic, non-group-writable default.
umask 022
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
root=$(pwd -P)
work=$(mktemp -d "${TMPDIR:-/tmp}/kit-gate.XXXXXX")
trap 'rm -rf "$work"' 0
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
# release.sh finds zig/cargo-zigbuild in the user-space tool cache, which the scratch HOME hides.
export PATH="$HOME/.cache/taskr-tools/bin:$HOME/.cache/taskr-tools/cargo/bin:$PATH"
export HOME="$work/home" TASKR_DB="$work/home/gate.db"
mkdir -p "$HOME"
unset TASKR_TASK TASKR_LAUNCH TASKR_BIN TASKR_FROZEN_NOW
for var in $(env | sed -n 's/^\(TASKR_CONTRACT_[A-Za-z0-9_]*\)=.*/\1/p'); do unset "$var"; done
npm ci --no-audit --no-fund
# Preserve the scratch ledger and Rust toolchain configuration in child tasks.
TURBO_TELEMETRY_DISABLED=1 npx --no-install turbo run build test lint --env-mode=loose --concurrency=1
sh tools/release-selection-test.sh
sh products/taskr/tools/release-test.sh
sh products/taskr/tools/install-herdr-config-test.sh
sh products/slotr/tests/install_sh_test.sh
for t in products/setup/tests/*_test.sh; do
    mkdir -p "$work/setup-home"
    (cd products/setup && HOME="$work/setup-home" sh "$root/$t")
done
(cd products/setup && HOME="$work/setup-home" WORKTREES_SH=dash dash tests/herdr_worktrees_test.sh)
(cd products/setup && HOME="$work/setup-home" dash tests/install_test.sh)
(cd products/setup && HOME="$work/setup-home" INSTALL_SH=dash dash tests/install_test.sh)
(cd products/setup && node tests/taskr_hook_test.mjs)
(cd products/setup && sh tests/privacy_check.sh)
echo "kit-gate: ok"
