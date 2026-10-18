---
name: taskr-verification
description: Repeat the isolated Linux taskr CLI pilot and identify surfaces it leaves unrun.
---

# taskr verification map

Status: **Linux CLI pilot recorded twice before the public import;
other surfaces unrun.** The recorded revision predates this repository's
history; rerun the recipe at your commit and record its revision and binary
hash.

Follow the [verification skill](../../setup/skills/herdr-verification/SKILL.md)
and [feature-map contract](../../setup/skills/herdr-verification/references/feature-map.md).
This is one product journey, not full taskr coverage. Index:
[CLI journey](#cli-journey), [observations](#observations),
[unrun features](#unrun-features), [cleanup and evidence](#cleanup-and-evidence).

## Ownership and doctor

Use only a disposable HOME and explicit TASKR_DB. No taskr daemon, live ledger,
config, spool, agent, account, or live Herdr socket participates. The primary
surface is the real CLI binary built from this checkout, without the contract
feature. `version` reports `dev`, so record the source revision and binary hash
as well; version alone cannot establish checkout identity. `status` must report
`daemon: none` before the first write.

The existing [golden driver](../tools/contract/README.md) establishes the scratch
environment pattern. The transcript imports its existing
[Herdr fixture](../crates/taskr/tests/daemon_cell.py) rather than adding a driver.
Only that fake transport receives the prompt. The fixture's socket is linked at
the default path **inside scratch HOME**, keeping every `HERDR_*` variable unset.

Before **each** taskr command, `check` verifies scratch HOME/DB and absence of
inherited `TASKR_TASK`, `TASKR_LAUNCH`, and `HERDR_*`. Worker commands alone
receive the IDs returned by this scratch ledger via command-local `env`.
They never inherit the real worker's IDs.

Prerequisites: Linux, the pinned Rust toolchain already installed, cached Cargo
dependencies, Python 3, Bash, jq, and GNU cp. Reflinks are optional:
`--reflink=auto` falls back to a full copy.
Missing tools or an offline build failure block this recipe; do not substitute an installed
taskr or download a toolchain. Run the blocks serially in the same Bash shell
from the repository root.

The recorded build command was `cargo build --locked --offline -p taskr`.
This setup copies the Cargo cache and directs build writes into owned scratch;
Rustup reads the installed toolchain with auto-install disabled:

```bash
set -euo pipefail
repo=$PWD
build=$(mktemp -d /tmp/taskr-verification-build.XXXXXX)
mkdir -p "$build/home" "$build/cargo"
cp -a --reflink=auto "${CARGO_HOME:-$HOME/.cargo}/registry" "${CARGO_HOME:-$HOME/.cargo}/git" "$build/cargo/"
env -i HOME="$build/home" PATH="${CARGO_HOME:-$HOME/.cargo}/bin:/usr/bin:/bin" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" RUSTUP_AUTO_INSTALL=0 CARGO_HOME="$build/cargo" CARGO_TARGET_DIR="$build/target" TMPDIR="$build" cargo build --locked --offline -p taskr
bin="$build/target/debug/taskr"
sha256sum "$bin"
```

Debug builds in different scratch paths may have different hashes. Retain the
hash printed by your own build.

## CLI journey

Purpose: register a root and worker lane, record a launch, deliver a prompt through
the fake transport, receipt it, capture a ready report, ask/answer/consume a
blocking question, finish and accept the lane, then inspect its log and documents.
`launch` records identity; it starts no agent.

Source entry points:
[CLI writes](../crates/taskr/src/write/mod.rs),
[prompt delivery](../crates/taskr/src/write/prompt.rs),
[worker identity and receipts](../crates/taskr-core/src/store/worker.rs),
[CLI reads](../crates/taskr/src/read/mod.rs).
Normal states exercised: empty ledger → open → ready → blocking ask answered →
done → closed/accepted. UI focus, hover, loading, and disabled states are
inapplicable to this CLI path. Error, replay, and stale-launch paths are unrun.

Run with `repo` and `bin` from the build block. Each invocation starts fresh
state and captures returned IDs. Python setup and CLI journey failures enter cleanup.
The fixture socket's existence is the startup readiness predicate.

```bash
evidence=$(mktemp -d /tmp/taskr-verify-evidence.XXXXXX)
printf 'evidence directory: %s\n' "$evidence"
sha256sum "$bin" > "$evidence/binary.sha256"
git rev-parse HEAD > "$evidence/source-revision.txt"
scratch=$(mktemp -d /tmp/taskr-verify.XXXXXX)
env -i scratch="$scratch" HOME="$scratch" TASKR_DB="$scratch/ledger.db" TASKR_TMP_BASE="$scratch/taskr-tmp" TMPDIR="$scratch" SQLITE_TMPDIR="$scratch" PATH="$scratch/bin:/usr/bin:/bin" LANG=C.UTF-8 TZ=UTC PYTHONDONTWRITEBYTECODE=1 repo="$repo" bin="$bin" python3 - <<'PY' 2>&1 | tee "$evidence/transcript.txt"
import os, shutil, subprocess, sys
from pathlib import Path
home = Path(os.environ["HOME"])
fake = None
try:
    sys.path.insert(0, os.environ["repo"] + "/products/taskr/crates/taskr/tests")
    from daemon_cell import Herdr
    fake = Herdr(home)
    (home / ".config/herdr").mkdir(parents=True)
    (home / ".config/herdr/herdr.sock").symlink_to(fake.path)
    assert (home / ".config/herdr/herdr.sock").is_socket()
    result = subprocess.run(["/bin/bash"], input=r"""set -euo pipefail
cd "$scratch"
check() {
  test "$HOME" = "$scratch"
  test "$TASKR_DB" = "$scratch/ledger.db"
  test -z "$(env | grep -E '^(TASKR_TASK|TASKR_LAUNCH|HERDR_[^=]*)=' || true)"
  printf 'isolation: scratch HOME/DB; no inherited worker or Herdr identity\n'
}
check
"$bin" version
check
doctor=$("$bin" status)
printf '%s\n' "$doctor"
test "$doctor" = 'j1 {"daemon":"none","rec":"daemon"}'
check
root_json=$("$bin" --json new verify-root --role orchestrator)
printf '%s\n' "$root_json"
root=$(printf '%s' "$root_json" | jq -r .task_id)
printf '# Scratch verification brief\n' > "$scratch/brief.md"
check
lane_json=$("$bin" --json new verify-lane --role implementer --parent "$root" --brief "$scratch/brief.md" --cwd "$scratch" --pane wFixture:p1)
printf '%s\n' "$lane_json"
lane=$(printf '%s' "$lane_json" | jq -r .task_id)
check
launch_json=$("$bin" --json launch "$lane" --provider fixture --model fixture --effort medium)
printf '%s\n' "$launch_json"
launch=$(printf '%s' "$launch_json" | jq -r .launch_id)
check
prompt_json=$("$bin" --json prompt "$lane" --text 'Verify the scratch CLI journey.' --receipt-timeout 0)
printf '%s\n' "$prompt_json"
prompt=$(printf '%s' "$prompt_json" | jq -r .attempt_id)
check
env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" got "$prompt"
printf 'Scratch CLI report: prompt received.\n' > "$scratch/report.md"
check
env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" ready 'CLI slice ready' --report "$scratch/report.md"
check
ask_json=$(env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" --json ask 'May I finish the scratch journey?' --blocking)
printf '%s\n' "$ask_json"
ask=$(printf '%s' "$ask_json" | jq -r .ask_id)
check
answer_json=$("$bin" --json answer "$ask" 'Yes, finish the scratch journey.')
printf '%s\n' "$answer_json"
answer=$(printf '%s' "$answer_json" | jq -r .answer_id)
check
env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" wait --for answer --timeout 0
check
env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" ack "$answer" --as "$lane"
check
env TASKR_TASK="$lane" TASKR_LAUNCH="$launch" "$bin" done 'Scratch CLI journey complete'
check
"$bin" close "$lane" --outcome accepted
check
"$bin" log "$lane"
check
"$bin" status --tree "$root"
check
"$bin" doc ls "$lane"
check
cat "$scratch/calls"
test ! -e "$HOME/.local/state/taskr/daemon.json"
test ! -e "$HOME/.local/state/taskr/client-daemon.json"
check
"$bin" close "$root"
""", text=True)
finally:
    if fake:
        fake.close()
    shutil.rmtree(home)
assert not fake.thread.is_alive() and not home.exists()
print("cleanup: fixture stopped; scratch removed")
sys.exit(result.returncode)
PY
test -s "$evidence/transcript.txt"
printf 'evidence retained at %s\n' "$evidence"
```

## Observations

Both fresh runs exited **0**, including cleanup. CLI transcripts matched byte
for byte after normalizing random scratch prefixes and event timestamps.
The build's per-run SHA-256 line is excluded from this claim and retained
separately. Excerpts below are real output;
omissions are marked `...`.

```text
isolation: scratch HOME/DB; no inherited worker or Herdr identity
j1 {"version":"dev","ok":true}
...
j1 {"daemon":"none","rec":"daemon"}
...
{"name":"verify-root","ok":true,"status":"open","task_id":1}
...
{"name":"verify-lane","ok":true,"status":"open","task_id":2}
...
{"agent_name":"verify-lane","launch_id":1,"ok":true,"pane_id":"wFixture:p1","task_id":2}
...
{"attempt_id":1,"ok":true,"outcome":"activity_observed","outcome_event_id":2,"target":"wFixture:p1","task_id":2}
...
g1 3 1
...
r1 4
...
{"ask_id":5,"event_id":5,"kind":"ask","ok":true,"recipient_task_id":1,"status":"ready","task_id":2}
...
{"answer_id":6,"ask_id":5,"asker_waiting":false,"delivered":false,"ok":true,"task_id":2}
...
k1 6
...
d1 7
...
c1 2
...
j1 {"at":"2026-10-17T04:28:33.877Z","d":{"from_status":"done","outcome":"accepted"},"i":8,"k":"closed","n":"verify-lane","rec":"event","t":2}
...
m1 {"closed":1,"all":"--all"}
...
c1 1
cleanup: fixture stopped; scratch removed
```

`delivered:false` is expected: answer stored without `--prompt`, then consumed
by CLI wait. These observations are CLI read-backs, not independent database or
deployment checks.
`doc ls` shows captured brief, prompt, and report documents of 29, 31, and 37
bytes, respectively.

The fixture call log contained exactly this delivery:

```text
["agent", "prompt", "wFixture:p1", "First taskr got 1. Verify the scratch CLI journey.", "--wait", "--until", "working", "--until", "blocked", "--timeout", "20000"]
```

Root creation also emitted the real warnings that no goal and no lead pane were
recorded. This scratch root deliberately has no agent or pane to adopt. No live
Herdr delivery or agent receipt follows from the fake `activity_observed` result.

## Unrun features

The following tests were **inspected as source pointers, not run** in this pilot.
Their existence is not a claim of passing coverage. The table gives the missing
prerequisite and the evidence needed to extend the map.

| Feature / source | Existing test, if any | Status, missing prerequisite, and what would prove it |
| --- | --- | --- |
| Live Herdr prompt and hook receipt: [delivery](../crates/taskr/src/herdr.rs) | [hooks.rs](../crates/taskr/tests/hooks.rs); [safety.rs](../crates/taskr/tests/safety.rs), `receipts_are_once_only_bound_to_the_task_launch_and_prompt_kind` | **Unrun:** only a fake transport and explicit CLI got were used. Requires an authorized disposable real Herdr session and agent; retain actual receipt, hook identity, result, and exact owned teardown. |
| TUI: [main](../crates/taskr-tui/src/main.rs) | [golden.rs](../crates/taskr-tui/tests/golden.rs), `frames_match_their_golden_files` | **Unrun:** no PTY/UI driver in this slice. Requires a disposable terminal session; exercise navigation and answer submission against scratch ledger and retain frames plus ledger read-back. Golden frames alone do not prove live input. |
| Daemon lifecycle/events: [daemon](../crates/taskr/src/daemon/mod.rs) | [daemon_cell.py](../crates/taskr/tests/daemon_cell.py), lifecycle cell; [daemon_restart_cell.py](../crates/taskr/tests/daemon_restart_cell.py) | **Unrun:** importing the fixture does not run either cell or start taskr daemon. Requires an owned scratch daemon; prove usable readiness, event observation, process identity, and teardown without touching the live daemon. |
| Push transports and malformed bindings: [push](../crates/taskr/src/mcp/push.rs) | [push.rs](../crates/taskr/tests/push.rs), `a_missing_wrong_or_malformed_binding_fails_before_the_take`; [channel.rs](../crates/taskr/tests/channel.rs), `an_idle_event_is_pushed_once_then_received_acked_and_the_next_follows` | **Unrun:** no push consumer/native adapter. Requires isolated adapter and launch identity; prove delivery, fenced receive/ack, malformed-binding refusal, and restart behavior. Synthetic tests leave native adapter integration unproved. |
| The taskr hub/client RPC, spool, and unknown outcomes: [RPC](../crates/taskr/src/net/rpc.rs) | [hub_cell.py](../crates/taskr/tests/hub_cell.py); [mcp.rs](../crates/taskr/tests/mcp.rs), `client_mode_keeps_queued_unknown_and_replay_apart`; [channel.rs](../crates/taskr/tests/channel.rs), `lost_replies_reconcile_with_their_own_keys_and_a_receipt_may_precede_submitted` | **Unrun:** explicit TASKR_DB bypassed client routing. Requires an isolated hub/client and existing cut-reply fixture; retain committed/queued/unknown distinctions, same-key reconciliation, and cleanup. Real tailnet/auth needs separate authorized proof. |
| MCP tools: [server](../crates/taskr/src/mcp.rs) | [mcp.rs](../crates/taskr/tests/mcp.rs), `framing_and_lifecycle_answer_every_request_and_keep_serving`, `startup_identity_is_fixed_and_bad_combinations_exit_2` | **Unrun:** no MCP client session. Requires an owned stdio session; prove initialization, tool framing, identity refusal and shutdown, then real client integration separately. |
| macOS: [platform code](../crates/taskr/src/daemon/macos.rs) | [macos_daemon_cell.py](../crates/taskr/tests/macos_daemon_cell.py), native restart/identity/HUP recipe | **Unrun:** Linux only. Requires macOS and compatible binary fixtures; the cell still requires a `--go` binary, which no longer exists; it cannot run until that argument is dropped. Retain native process/signal and cleanup evidence; Linux cannot clear it. |
| Other CLI commands: search, handover/adopt, decisions, refs, subscriptions, consumer leases, tmp, migrations and failure/recovery paths | [contract corpus](../testdata/contract/cases.json), [mapping ledger](../testdata/contract/mapping.tsv), [consumer.rs](../crates/taskr/tests/consumer.rs), [tmp.rs](../crates/taskr/tests/tmp.rs) | **Unrun:** outside the single journey. Select existing mapped cases per user purpose, check current executable targets (historical `todo` rows prove nothing), and retain fresh action/result/reset evidence. |
| Setup/install/release and slotr | Separate [setup](../../setup/README.md) and [slotr](../../slotr/README.md) products | **Unrun:** this map grants no installation, release, or sibling-product proof. Each needs its own authorized product journey and existing tests. |

## Cleanup and evidence

Each run prints its durable evidence directory, `/tmp/taskr-verify-evidence.XXXXXX`.
`$evidence/transcript.txt` captures combined stdout/stderr with no live identities;
`binary.sha256` and `source-revision.txt` identify the build. Keep these artifacts
for review; the recipe checks the transcript remains nonempty after fixture and
scratch cleanup. They survive build cleanup too (checked below). `/tmp` is local
retention until removed or rebooted; archive the directory with the review report
for longer retention. Do not include live secrets or identities in captures.

Remove the owned build directory, preserving evidence:

```bash
python3 - "$build" <<'PY'
import shutil, sys
from pathlib import Path
build = Path(sys.argv[1])
assert build.parent == Path("/tmp") and build.name.startswith("taskr-verification-build.")
shutil.rmtree(build)
assert not build.exists()
PY
test -s "$evidence/transcript.txt"
```
