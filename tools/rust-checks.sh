#!/bin/sh
set -eu
# Tmp permission fixtures require a deterministic, non-group-writable default.
umask 022
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
case ${1:-} in test|lint) ;; *) echo "usage: tools/rust-checks.sh test|lint" >&2; exit 2;; esac
[ "$1" != lint ] || cargo fmt --all --check
for features in '' '--features taskr/contract'; do
    if [ "$1" = lint ]; then
        # $features is empty or two fixed words.
        cargo clippy --locked --workspace --all-targets $features -- -D warnings
    else
        cargo test --locked --workspace $features
    fi
done
