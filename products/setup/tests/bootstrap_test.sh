#!/bin/sh
# No network or real installer: real tar fixtures, stub gh, scratch HOME/TMPDIR.
# INSTALL_SH=dash selects the shell for both standalone and piped bootstrap.
set -eu

repo_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd -P)
shell_bin=$(command -v "${INSTALL_SH:-sh}")
root=$(mktemp -d "${TMPDIR:-/tmp}/herdr-bootstrap-test.XXXXXX")
trap 'rm -rf "${root:?}"' 0
trap 'exit 1' 1 2 3 15
mkdir -p "$root/bin" "$root/fixture/github-source/products/taskr" "$root/missing/github-source" "$root/empty-bin"

cat >"$root/bin/gh" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' "$@" >"$CASE_DIR/gh-argv"
cat "$FIXTURE"
exit "$DOWNLOAD_STATUS"
EOF
chmod 0755 "$root/bin/gh"
cat >"$root/fixture/github-source/products/taskr/install.sh" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' "$#" >"$CASE_DIR/argc"
printf '%s\n' "$@" >"$CASE_DIR/argv"
pwd -P >"$CASE_DIR/cwd"
source_dir=${0%/*}
[ "$(cat "$source_dir/neighbor.txt")" = neighbor ]
printf '%s\n' extracted >"$CASE_DIR/layout"
exit "$CHILD_STATUS"
EOF
printf '%s\n' neighbor >"$root/fixture/github-source/products/taskr/neighbor.txt"
printf '%s\n' unrelated >"$root/missing/github-source/other.txt"
tar -czf "$root/good.tar.gz" -C "$root/fixture" github-source
tar -czf "$root/missing.tar.gz" -C "$root/missing" github-source
printf '%s\n' corrupt >"$root/corrupt.tar.gz"
printf '%s\n' api repos/olafurns7/herdr-kit/tarball/taskr/v0.25.1 >"$root/expected-gh-argv"

count=0
new_case() {
    count=$((count + 1))
    printf 'case %s: %s\n' "$count" "$1"
    case_dir=$root/case$count
    mkdir -p "$case_dir/home" "$case_dir/tmp" "$case_dir/caller with spaces"
    caller_dir=$(CDPATH= cd "$case_dir/caller with spaces" && pwd -P)
    fixture=$root/good.tar.gz
    download_status=0
    child_status=0
    test_path=$root/bin:/usr/bin:/bin
    mode=standalone
}

fail() {
    printf 'FAIL case %s: %s\n' "$count" "$1" >&2
    cat "$case_dir/output" >&2
    exit 1
}

run_bootstrap() {
    status=0
    (
        cd "$caller_dir"
        if [ "$mode" = piped ]; then
            cat "$repo_dir/bootstrap.sh" |
                env -i HOME="$case_dir/home" TMPDIR="$case_dir/tmp" PATH="$test_path" \
                CASE_DIR="$case_dir" FIXTURE="$fixture" DOWNLOAD_STATUS="$download_status" \
                CHILD_STATUS="$child_status" "$shell_bin" -s -- "$@"
        else
            env -i HOME="$case_dir/home" TMPDIR="$case_dir/tmp" PATH="$test_path" \
                CASE_DIR="$case_dir" FIXTURE="$fixture" DOWNLOAD_STATUS="$download_status" \
                CHILD_STATUS="$child_status" "$shell_bin" "$repo_dir/bootstrap.sh" "$@"
        fi
    ) >"$case_dir/output" 2>&1 || status=$?
    # Both temporary downloads and scratch HOME must be empty after every run.
    for entry in "$case_dir/tmp"/* "$case_dir/home"/* "$case_dir/home"/.[!.]* "$case_dir/home"/..?*; do
        if [ -e "$entry" ] || [ -L "$entry" ]; then
            fail "left files behind: $entry"
        fi
    done
}

expect_child() {
    [ "$status" -eq "$child_status" ] || fail "exit $status, expected $child_status"
    set -- --fleet "$@"
    [ "$(cat "$case_dir/argc")" -eq "$#" ] || fail 'argument count changed'
    printf '%s\n' "$@" >"$case_dir/expected-argv"
    cmp -s "$case_dir/argv" "$case_dir/expected-argv" || fail 'arguments changed'
    [ "$(cat "$case_dir/cwd")" = "$caller_dir" ] || fail 'caller cwd changed'
    [ "$(cat "$case_dir/layout")" = extracted ] || fail 'neighbor file not extracted'
    cmp -s "$case_dir/gh-argv" "$root/expected-gh-argv" || fail 'wrong gh request'
}

expect_no_child() {
    [ "$status" -ne 0 ] || fail 'failure returned success'
    [ ! -e "$case_dir/argc" ] || fail 'installer called after failure'
}

for mode_name in standalone piped; do
    new_case "$mode_name: flags, spaces and empty argument"
    mode=$mode_name
    run_bootstrap --files-only --keep '.codex/config.toml' 'project with spaces' ''
    expect_child --files-only --keep '.codex/config.toml' 'project with spaces' ''
done

new_case 'no project argument: caller cwd retained'
run_bootstrap
expect_child

new_case 'authentication/download failure'
download_status=23
run_bootstrap
expect_no_child
[ "$status" -eq 23 ] || fail 'download failure status changed'

new_case 'corrupt archive'
fixture=$root/corrupt.tar.gz
run_bootstrap
expect_no_child

new_case 'missing installer'
fixture=$root/missing.tar.gz
run_bootstrap
expect_no_child

new_case 'installer failure status'
child_status=37
run_bootstrap --files-only 'relative project'
expect_child --files-only 'relative project'

new_case 'piped installer failure status'
mode=piped
child_status=42
run_bootstrap --files-only
expect_child --files-only

new_case 'missing gh'
test_path=$root/empty-bin
run_bootstrap
expect_no_child
[ ! -e "$case_dir/gh-argv" ] || fail 'download attempted without gh'
case $(cat "$case_dir/output") in
    *'gh is required'*) ;;
    *) fail 'missing gh message' ;;
esac

new_case 'missing tar'
test_path=$root/bin
run_bootstrap
expect_no_child
[ ! -e "$case_dir/gh-argv" ] || fail 'download attempted without tar'
case $(cat "$case_dir/output") in
    *'tar is required'*) ;;
    *) fail 'missing tar message' ;;
esac

printf 'PASS: %s bootstrap cases (%s)\n' "$count" "${INSTALL_SH:-sh}"
