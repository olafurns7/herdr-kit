#!/bin/sh
# kit-pr against a gh stub on PATH; no network and no real gh.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
kit_pr=$repo_dir/products/setup/bin/kit-pr
scratch=$(mktemp -d "${TMPDIR:-/tmp}/kit-pr-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"
cat >"$scratch/bin/gh" <<'STUB'
#!/bin/sh
printf '%s\n' "$*" >>"$STUB_DIR/calls"
if [ -n "${STUB_RC:-}" ]; then
    printf 'GraphQL: Could not resolve to a PullRequest with the number of 99999.\nsecond stderr line\n' >&2
    exit "$STUB_RC"
fi
cat "$STUB_DIR/pr.json"
STUB
chmod 755 "$scratch/bin/gh"
PATH=$scratch/bin:$PATH
STUB_DIR=$scratch
export PATH STUB_DIR
count=0
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
pr() { printf '%s\n' "$1" >"$scratch/pr.json"; }
run() {
    count=$((count + 1))
    : >"$scratch/calls"
    rc=0
    sh "$kit_pr" "$@" >"$scratch/out" 2>"$scratch/err" || rc=$?
}
expect() {
    [ "$rc" -eq 0 ] || fail "case $count: exit $rc: $(cat "$scratch/err")"
    printf '%s\n' "$1" >"$scratch/want"
    cmp -s "$scratch/want" "$scratch/out" || {
        printf 'want:\n%s\ngot:\n' "$1" >&2; cat "$scratch/out" >&2; fail "case $count"
    }
    [ ! -s "$scratch/err" ] || fail "case $count: stderr: $(cat "$scratch/err")"
}
sha=0123456789abcdef0123456789abcdef01234567
base='"number":7,"headRefOid":"'$sha'"'

# Open, all checks pass (CheckRun success/neutral/skipped and a StatusContext).
pr '{'"$base"',"state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","reviewDecision":"APPROVED","statusCheckRollup":[
 {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
 {"__typename":"CheckRun","name":"lint","status":"COMPLETED","conclusion":"NEUTRAL"},
 {"__typename":"CheckRun","name":"docs","status":"COMPLETED","conclusion":"SKIPPED"},
 {"__typename":"StatusContext","context":"ci/legacy","state":"SUCCESS"}]}'
run 7
expect "#7 OPEN head=0123456789ab checks=pass mergeable=MERGEABLE review=APPROVED draft=no"
grep -qx 'pr view 7 --json number,state,headRefOid,isDraft,mergeable,reviewDecision,statusCheckRollup' "$scratch/calls" ||
    fail "case $count: gh call: $(cat "$scratch/calls")"
run 7 --checks
expect "#7 OPEN head=0123456789ab checks=pass mergeable=MERGEABLE review=APPROVED draft=no"

# Merged, no checks, no review decision; --repo is passed through.
pr '{'"$base"',"state":"MERGED","isDraft":false,"mergeable":"UNKNOWN","reviewDecision":"","statusCheckRollup":[]}'
run 7 --repo example/widgets
expect "#7 MERGED head=0123456789ab checks=none mergeable=UNKNOWN review=none draft=no"
grep -qx 'pr view 7 --repo example/widgets --json number,state,headRefOid,isDraft,mergeable,reviewDecision,statusCheckRollup' "$scratch/calls" ||
    fail "case $count: gh call: $(cat "$scratch/calls")"
pr '{'"$base"',"state":"MERGED","isDraft":false,"mergeable":"UNKNOWN","reviewDecision":null,"statusCheckRollup":null}'
run --repo=example/widgets 7
expect "#7 MERGED head=0123456789ab checks=none mergeable=UNKNOWN review=none draft=no"

# Draft with one pending check among passes; --checks names only the pending one.
pr '{'"$base"',"state":"OPEN","isDraft":true,"mergeable":"CONFLICTING","reviewDecision":"REVIEW_REQUIRED","statusCheckRollup":[
 {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
 {"__typename":"CheckRun","name":"e2e","status":"IN_PROGRESS","conclusion":""},
 {"__typename":"StatusContext","context":"deploy/preview","state":"PENDING"}]}'
run 7
expect "#7 OPEN head=0123456789ab checks=pending mergeable=CONFLICTING review=REVIEW_REQUIRED draft=yes"
run 7 --checks
expect "#7 OPEN head=0123456789ab checks=pending mergeable=CONFLICTING review=REVIEW_REQUIRED draft=yes
pending e2e
pending deploy/preview"

# Any failure wins over pending.
pr '{'"$base"',"state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","reviewDecision":"CHANGES_REQUESTED","statusCheckRollup":[
 {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
 {"__typename":"CheckRun","name":"e2e","status":"QUEUED","conclusion":null},
 {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
 {"__typename":"CheckRun","name":"slow","status":"COMPLETED","conclusion":"TIMED_OUT"},
 {"__typename":"StatusContext","context":"ci/legacy","state":"ERROR"}]}'
run 7
expect "#7 OPEN head=0123456789ab checks=fail mergeable=MERGEABLE review=CHANGES_REQUESTED draft=no"
run --checks 7
expect "#7 OPEN head=0123456789ab checks=fail mergeable=MERGEABLE review=CHANGES_REQUESTED draft=no
pending e2e
fail test
fail slow
fail ci/legacy"

# --body asks gh for the body and prints it after the line (and after --checks).
pr '{'"$base"',"state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","reviewDecision":"","body":"Line one.\n\nLine three.","statusCheckRollup":[
 {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"}]}'
run 7 --body
expect "#7 OPEN head=0123456789ab checks=fail mergeable=MERGEABLE review=none draft=no

Line one.

Line three."
grep -qx 'pr view 7 --json number,state,headRefOid,isDraft,mergeable,reviewDecision,statusCheckRollup,body' "$scratch/calls" ||
    fail "case $count: gh call: $(cat "$scratch/calls")"
run 7 --checks --body
expect "#7 OPEN head=0123456789ab checks=fail mergeable=MERGEABLE review=none draft=no
fail test

Line one.

Line three."

# gh failure: stderr whole, gh's exit code, no status line.
for code in 1 4; do
    STUB_RC=$code; export STUB_RC
    run 99999
    unset STUB_RC
    [ "$rc" -eq "$code" ] || fail "case $count: exit $rc, want $code"
    [ ! -s "$scratch/out" ] || fail "case $count: stdout on failure: $(cat "$scratch/out")"
    printf 'GraphQL: Could not resolve to a PullRequest with the number of 99999.\nsecond stderr line\n' >"$scratch/want"
    cmp -s "$scratch/want" "$scratch/err" || fail "case $count: stderr: $(cat "$scratch/err")"
done

# Usage errors exit 2 without calling gh.
for args in '' 'abc' '7 8' '7 --repo' '7 --bogus'; do
    # shellcheck disable=SC2086
    run $args
    [ "$rc" -eq 2 ] || fail "case $count ($args): exit $rc, want 2"
    [ ! -s "$scratch/calls" ] || fail "case $count ($args): gh was called"
    grep -q '^kit-pr: usage:' "$scratch/err" || fail "case $count ($args): $(cat "$scratch/err")"
done

printf 'PASS: %s kit-pr cases\n' "$count"
