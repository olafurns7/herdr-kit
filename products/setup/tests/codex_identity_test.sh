#!/bin/sh
# Identity adapter unit/self-checks; no live agents, ledger or Herdr server.
set -eu
exec python3 -B "$(dirname "$0")/codex_identity_test.py"
