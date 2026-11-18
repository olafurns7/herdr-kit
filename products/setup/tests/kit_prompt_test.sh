#!/bin/sh
# kit-prompt against a taskr stub on PATH; no real taskr, ledger or agent.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
kit_prompt=$repo_dir/products/setup/bin/kit-prompt
scratch=$(mktemp -d "${TMPDIR:-/tmp}/kit-prompt-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"
cat >"$scratch/bin/taskr" <<'STUB'
#!/bin/sh
for a in "$@"; do printf '<%s>' "$a"; done >"$STUB_DIR/calls"
cat "$STUB_DIR/stdout"
[ ! -s "$STUB_DIR/stderr" ] || cat "$STUB_DIR/stderr" >&2
exit "${STUB_RC:-0}"
STUB
chmod 755 "$scratch/bin/taskr"
PATH=$scratch/bin:$PATH
STUB_DIR=$scratch
export PATH STUB_DIR
count=0
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
# stub STDOUT [STDERR [RC]]: what the stubbed taskr prints and returns.
stub() {
    printf '%s' "$1" >"$scratch/stdout"
    printf '%s' "${2:-}" >"$scratch/stderr"
    STUB_RC=${3:-0}
    export STUB_RC
}
run() {
    count=$((count + 1))
    rc=0
    sh "$kit_prompt" "$@" >"$scratch/out" 2>"$scratch/err" || rc=$?
}
# expect RC STDOUT STDERR: exact streams and exit code.
expect() {
    [ "$rc" -eq "$1" ] || fail "case $count: exit $rc, want $1"
    printf '%s' "$2" >"$scratch/want"
    cmp -s "$scratch/want" "$scratch/out" || {
        printf 'want stdout:\n%s\ngot:\n' "$2" >&2; cat "$scratch/out" >&2; fail "case $count"
    }
    printf '%s' "$3" >"$scratch/want"
    cmp -s "$scratch/want" "$scratch/err" || {
        printf 'want stderr:\n%s\ngot:\n' "$3" >&2; cat "$scratch/err" >&2; fail "case $count"
    }
}
push='m1 {"event":512700,"gen":3,"path":"push","received":false,"target":22273}
'

# Clean push: only the event id; arguments reach taskr prompt unchanged.
stub "$push"
run 22273 --as 22270 --text 'two words; and "quotes"' --push
expect 0 'event 512700
' ''
[ "$(cat "$scratch/calls")" = '<prompt><22273><--as><22270><--text><two words; and "quotes"><--push>' ] ||
    fail "case $count: argv $(cat "$scratch/calls")"

# Pane path: the p1 record is printed whole.
pane='p1 {"g":512702,"gok":true,"o":"submitted","oe":512701,"p":88,"path":"pane"}
'
stub "$pane"
run 22273 --as 22270 --text hi
expect 0 "$pane" ''

# A record with path pane in an m1 line is not a push.
stub 'm1 {"event":5,"gen":1,"path":"pane","target":9}
'
run 9 --text hi
expect 0 'm1 {"event":5,"gen":1,"path":"pane","target":9}
' ''

# Queued (spooled) record: qd1 is printed whole.
stub 'qd1 prompt-22273-abc
'
run 22273 --text hi
expect 0 'qd1 prompt-22273-abc
' ''

# Refusal: stdout and stderr whole with taskr's exit code.
stub 'x1 6 {"err":"target closed","k":"rejected"}
' 'taskr: target 22273 is closed
' 6
run 22273 --text hi
expect 6 'x1 6 {"err":"target closed","k":"rejected"}
' 'taskr: target 22273 is closed
'

# Non-zero exit even with a push-shaped line: whole.
stub "$push" '' 5
run 22273 --text hi
expect 5 "$push" ''

# Stderr on a successful push: whole, stderr kept.
retry='taskr: server unreachable; retrying until 2026-11-18T12:00:00Z; if interrupted, retry with: taskr prompt 22273
'
stub "$push" "$retry"
run 22273 --text hi
expect 0 "$push" "$retry"

# Unknown record shapes: whole.
for line in 'z1 {"event":7,"path":"push"}' 'm1 {"path":"push","target":9}' \
    'm1 {"event":"7","path":"push"}' 'm1 {"event":7.5,"path":"push"}' \
    '{"event":7,"gen":1,"path":"push","received":false,"target":9}' 'm1 {not json}'; do
    stub "$line
"
    run 9 --text hi
    expect 0 "$line
" ''
done

# Two lines (even two pushes): whole.
two="$push$push"
stub "$two"
run 22273 --text hi
expect 0 "$two" ''

# Empty output with exit 0: whole (nothing), exit 0.
stub ''
run 22273 --text hi
expect 0 '' ''

printf 'kit_prompt_test: %s cases passed\n' "$count"
