#!/bin/sh
# Runs install.sh against scratch HOMEs with stub herdr, gh, taskr, codator,
# npm, and curl on PATH. It needs no network and never touches the real HOME.
# Usage: sh tests/install_test.sh   (INSTALL_SH=dash runs install.sh under dash)
set -eu

install_sh=${INSTALL_SH:-sh}
repo_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd -P)
jq_bin=$(command -v jq) || {
    printf '%s\n' 'FAIL: jq is required to run the installer'
    exit 1
}

root=$(mktemp -d "${TMPDIR:-/tmp}/herdr-setup-test.XXXXXX")
trap 'rm -rf "$root"' 0
trap 'exit 1' 1 2 3 15

stub_dir=$root/stubs
mkdir -p "$stub_dir/bin"
ln -s "$jq_bin" "$stub_dir/bin/jq"

# Every stub logs its argv to $STUB_LOG_DIR/<name>.log.
write_logging_stub() {
    stub_name=$1
    stub_body=$2
    cat >"$stub_dir/bin/$stub_name" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >>"\$STUB_LOG_DIR/$stub_name.log"
$stub_body
EOF
    chmod 0755 "$stub_dir/bin/$stub_name"
}

write_logging_stub herdr '
case "$*" in
    "plugin list --plugin hhdebb.herdr-radar --json")
        if [ -f "$HOME/radar-source.json" ]; then
            cat "$HOME/radar-source.json"
        else
            printf "%s\n" "{\"result\":{\"plugins\":[]}}"
        fi
        ;;
    "plugin install olafurns7/herdr-radar --ref "*)
        [ "${STUB_RADAR_FAIL:-0}" = 0 ] || exit 1
        jq -n --arg commit "$5" '\''{result:{plugins:[{plugin_id:"hhdebb.herdr-radar",source:{kind:"github",owner:"olafurns7",repo:"herdr-radar",resolved_commit:$commit}}]}}'\'' >"$HOME/radar-source.json"
        ;;
    "plugin config-dir hhdebb.herdr-radar")
        if [ "${STUB_RADAR_IO:-}" = mkdir ]; then
            printf "%s\n" "$HOME/radar-blocked/config"
        else
            printf "%s\n" "$HOME/radar-config"
        fi
        ;;
    "plugin action invoke hhdebb.herdr-radar.state-stop")
        pid_dir=$HOME/.local/state/herdr/plugins/hhdebb.herdr-radar
        if [ "${STUB_RADAR_IO:-}" = pid-read ]; then
            mkdir -p "$pid_dir"
            printf "%s\n" "$STUB_RADAR_PID" >"$pid_dir/animator.pid"
        else
            rm -f "$pid_dir/animator.pid"
        fi
        ;;
    "plugin action invoke hhdebb.herdr-radar.state-start")
        pid_dir=$HOME/.local/state/herdr/plugins/hhdebb.herdr-radar
        mkdir -p "$pid_dir"
        count=0
        [ ! -f "$pid_dir/starts" ] || count=$(cat "$pid_dir/starts")
        count=$((count + 1))
        printf "%s\n" "$count" >"$pid_dir/starts"
        if [ "${STUB_RADAR_RETRY:-0}" != always ] && { [ "${STUB_RADAR_RETRY:-0}" = 0 ] || [ "$count" -gt 1 ]; }; then
            printf "%s\n" "$STUB_RADAR_PID" >"$pid_dir/animator.pid"
        fi
        ;;
    "plugin action invoke hhdebb.herdr-radar.configure")
        ! grep -qx "space_owner = true" "$HOME/radar-config/config.toml" ||
            printf "%s\n" '\''$space_owner'\'' >>"$HOME/.config/herdr/config.toml"
        ;;
esac
case "$1" in
    --skill) printf "%s\n" "---" "name: herdr" "---" "stub upstream skill${STUB_HERDR_SKILL:+ $STUB_HERDR_SKILL}" ;;
esac
exit 0'
write_logging_stub gh '
case "$1 ${2:-}" in
    "auth status") [ "${STUB_GH_AUTH:-ok}" = ok ]; exit $? ;;
esac
case "${2:-}" in
    "repos/olafurns7/herdr-taskr/contents/install.sh?ref="*)
        printf "%s\n" "${2#*ref=}" >>"$STUB_LOG_DIR/gh-ref.log"
        cat "$STUB_DIR/taskr-install.sh"
        exit 0 ;;
esac
case "$1 ${2:-} ${4:-} ${5:-}" in
    "release download --repo olafurns7/herdr-slotr")
        slotr_tag=$3
        printf "%s\n" "$slotr_tag" >>"$STUB_LOG_DIR/gh-slotr.log"
        while [ "$#" -gt 1 ]; do
            [ "$1" != --dir ] || slotr_dir=$2
            shift
        done
        /bin/cp "$STUB_DIR/slotr-assets/$slotr_tag"/* "$slotr_dir"/
        exit $? ;;
esac
exit 1'
write_logging_stub codator 'exit 0'
write_logging_stub npm 'exit 0'
write_logging_stub curl 'exit 1'
write_logging_stub systemctl 'exit 1'
write_logging_stub uname '
case "${1:-}" in
    -m) printf "%s\n" "${STUB_UNAME_M:-arm64}" ;;
    *) printf "%s\n" "${STUB_UNAME:-Darwin}" ;;
esac'
write_logging_stub sleep 'exit 0'
write_logging_stub cp '
case "${STUB_RADAR_IO:-}:$*" in
    backup:*pre-radar-*) exit 1 ;;
esac
exec /bin/cp "$@"'
write_logging_stub cat '
case "${STUB_RADAR_IO:-}:$*" in
    pid-read:*hhdebb.herdr-radar/animator.pid)
        if [ ! -f "$HOME/.local/state/herdr/plugins/hhdebb.herdr-radar/starts" ]; then
            printf "%s\n" "stub pid read failed" >&2
            exit 1
        fi
        ;;
esac
exec /bin/cat "$@"'

# The stub taskr installer behaves like the real one: it honours
# TASKR_INSTALL_DIR, TASKR_SKILL_DIR, TASKR_NO_PLUGIN and TASKR_NO_TUI, and it overwrites
# the skill unconditionally.
cat >"$stub_dir/taskr-install.sh" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' "${TASKR_VERSION:-unset}" >>"$STUB_LOG_DIR/taskr-installer.log"
printf 'skill_dir=%s install_dir=%s no_plugin=%s no_tui=%s\n' "${TASKR_SKILL_DIR:-default}" \
    "${TASKR_INSTALL_DIR:-default}" "${TASKR_NO_PLUGIN:-0}" "${TASKR_NO_TUI:-0}" >>"$STUB_LOG_DIR/taskr-installer-env.log"
sh "$STUB_DIR/make-taskr.sh" "${TASKR_INSTALL_DIR:-$HOME/.local/bin}" "${TASKR_VERSION:?}"
skill_dir=${TASKR_SKILL_DIR:-$HOME/.agents/skills/taskr}
mkdir -p "$skill_dir/references/nested"
printf '%s\n' '---' 'name: taskr' '---' 'stub taskr skill' >"$skill_dir/SKILL.md"
printf '%s\n' 'stub taskr format' >"$skill_dir/references/format.md"
printf '%s\n' 'stub taskr orchestration' >"$skill_dir/references/orchestrator.md"
printf '%s\n' 'stub taskr recovery' >"$skill_dir/references/nested/recovery.md"
EOF
cat >"$stub_dir/make-taskr.sh" <<'EOF'
#!/bin/sh
set -eu
mkdir -p "$1"
# Current releases emit compact version output; older ones emit JSON.
version_prefix='j1 '
[ "$2" != v0.5.0 ] || version_prefix=
cat >"$1/taskr" <<STUB
#!/bin/sh
printf '%s\n' "\$*" >>"\$STUB_LOG_DIR/taskr.log"
case "\$1" in
    version) printf '%s{"version":"%s","ok":true}\n' "$version_prefix" "$2" ;;
esac
exit 0
STUB
chmod 0755 "$1/taskr"
EOF

STUB_GH_AUTH=
STUB_HERDR_SKILL=
RUN_TASKR_VERSION=
RUN_PATH=
RUN_HERDR_ENV=
STUB_UNAME=
STUB_UNAME_M=
RUN_SLOTR_VERSION=
STUB_RADAR_FAIL=
STUB_RADAR_RETRY=
STUB_RADAR_IO=
failures=0
case_count=0
case_name=

fail() {
    printf '  FAIL [%s] %s\n' "$case_name" "$1"
    failures=$((failures + 1))
}

# new_case NAME: a fresh HOME, project root, and log directory.
new_case() {
    case_name=$1
    case_count=$((case_count + 1))
    case_dir=$root/case$case_count
    home=$case_dir/home
    logs=$case_dir/logs
    project=$case_dir/project
    mkdir -p "$home" "$logs" "$project"
    printf '%s\n' "case: $case_name"
}

# run_install ARGS...: runs install.sh with a clean environment; sets $out, $rc.
run_install() {
    run_status=0
    env -i HOME="$home" PATH="${RUN_PATH:-$stub_dir/bin:$home/.local/bin:/usr/bin:/bin}" \
        TMPDIR="$case_dir" STUB_LOG_DIR="$logs" STUB_DIR="$stub_dir" \
        ${STUB_GH_AUTH:+STUB_GH_AUTH=$STUB_GH_AUTH} \
        ${STUB_HERDR_SKILL:+STUB_HERDR_SKILL=$STUB_HERDR_SKILL} \
        ${RUN_TASKR_VERSION:+TASKR_VERSION=$RUN_TASKR_VERSION} \
        ${RUN_HERDR_ENV:+HERDR_ENV=$RUN_HERDR_ENV} \
        ${STUB_UNAME:+STUB_UNAME=$STUB_UNAME} \
        ${STUB_UNAME_M:+STUB_UNAME_M=$STUB_UNAME_M} \
        ${RUN_SLOTR_VERSION:+SLOTR_VERSION=$RUN_SLOTR_VERSION} \
        ${STUB_RADAR_FAIL:+STUB_RADAR_FAIL=$STUB_RADAR_FAIL} \
        ${STUB_RADAR_RETRY:+STUB_RADAR_RETRY=$STUB_RADAR_RETRY} \
        ${STUB_RADAR_IO:+STUB_RADAR_IO=$STUB_RADAR_IO} \
        STUB_RADAR_PID="$$" \
        SETUP_DIR="$repo_dir" "$install_sh" "$stub_dir/radar-runner.sh" "$@" >"$case_dir/out" 2>&1 || run_status=$?
    out=$(cat "$case_dir/out")
    rc=$run_status
}

expect_rc() {
    [ "$rc" -eq "$1" ] || fail "exit $rc, expected $1; output: $out"
}

expect_out() {
    printf '%s\n' "$out" | grep -F -- "$1" >/dev/null || fail "output lacks: $1"
}

expect_no_out() {
    if printf '%s\n' "$out" | grep -E -- "$1" >/dev/null; then
        fail "output matches /$1/: $(printf '%s\n' "$out" | grep -E -- "$1" | head -n 3)"
    fi
}

expect_link() {
    link_path=$1
    link_target=$2
    if [ ! -L "$link_path" ]; then
        fail "$link_path is not a symlink"
    elif [ "$(readlink "$link_path")" != "$link_target" ]; then
        fail "$link_path -> $(readlink "$link_path"), expected $link_target"
    elif [ ! -f "$link_path/SKILL.md" ]; then
        fail "$link_path does not resolve to a SKILL.md"
    fi
}

expect_absent() {
    if [ -e "$1" ] || [ -L "$1" ]; then
        fail "$1 exists"
    fi
}

expect_file_content() {
    if [ ! -f "$1" ]; then
        fail "$1 is missing"
    elif [ "$(cat "$1")" != "$2" ]; then
        fail "$1 holds '$(cat "$1")', expected '$2'"
    fi
}

expect_no_log() {
    if [ -s "$logs/$1.log" ]; then
        fail "$1 was called: $(head -n 3 "$logs/$1.log")"
    fi
}

# sha256 of a file, with the same tool order as install.sh under the test PATH.
sha() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum <"$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 <"$1" | cut -d ' ' -f 1
    else
        openssl dgst -sha256 <"$1" | sed 's/.*= *//'
    fi
}

expect_record() {
    grep -F -x -e "$1 $2" "$home/.local/state/herdr-setup/installed" >/dev/null 2>&1 \
        || fail "the record lacks '$1 $2'"
}

expected_hook_command() {
    printf '[ -n "${TASKR_LAUNCH:-}" ] && "$HOME/.local/bin/taskr" hook %s %s >/dev/null 2>&1; true\n' "$1" "$2"
}

check_template_hooks() {
    hook_template=$1
    hook_harness=$2
    shift 2
    for hook_event in "$@"; do
        hook_expected=$(expected_hook_command "$hook_harness" "$hook_event")
        hook_actual=$(jq -r --arg event "$hook_event" --arg expected "$hook_expected" \
            '.hooks[$event][]?.hooks[]? | select(.command == $expected) | .command' "$hook_template")
        if [ "$hook_actual" != "$hook_expected" ]; then
            fail "$hook_template lacks the exact $hook_harness $hook_event command"
            continue
        fi
        if [ "$hook_harness" = claude ]; then
            jq -e --arg event "$hook_event" --arg expected "$hook_expected" \
                '[.hooks[$event][]?.hooks[]? | select(.command == $expected)] | length == 1 and .[0].async == true' \
                "$hook_template" >/dev/null 2>&1 || fail "$hook_harness $hook_event is not async"
        else
            jq -e --arg event "$hook_event" --arg expected "$hook_expected" \
                '[.hooks[$event][]?.hooks[]? | select(.command == $expected)] | length == 1 and .[0].timeout == 2' \
                "$hook_template" >/dev/null 2>&1 || fail "$hook_harness $hook_event does not have timeout 2"
        fi
    done
}

strip_taskr_hooks() {
    jq -S '(.hooks |= with_entries(.value |= map(select(all(.hooks[]?; ((.command // "") | contains("TASKR_LAUNCH") | not))))))' "$1"
}

expect_no_sidecar() {
    expect_absent "$1.herdr-setup"
}

# write_record LINE...: writes the last-installed record by hand.
write_record() {
    mkdir -p "$home/.local/state/herdr-setup"
    printf '%s\n' "$@" >"$home/.local/state/herdr-setup/installed"
}

write_keep() {
    mkdir -p "$home/.config/herdr-setup"
    printf '%s\n' "$@" >"$home/.config/herdr-setup/keep"
}

stub_herdr_skill() {
    printf '%s\n' '---' 'name: herdr' '---' "stub upstream skill${1:+ $1}"
}

old_config='model = "gpt-old"'
new_file() {
    printf '%s\n' "$2" >"$case_dir/$1"
    printf '%s\n' "$case_dir/$1"
}

skills="herdr herdr-orchestration herdr-fleet taskr"
orchestration_refs="lanes.md launch.md workflow.md owner.md teardown.md recovery.md"

cat >"$stub_dir/radar-runner.sh" <<'RUNNER'
#!/bin/sh
set -eu
home_dir=$HOME
. "$SETUP_DIR/lib/radar.sh"
files_only=0
for arg in "$@"; do [ "$arg" != --files-only ] || files_only=1; done
if ! command -v jq >/dev/null 2>&1; then
    [ "$files_only" -eq 1 ] || manual_radar
    printf '%s\n' 'ERROR: jq is required; install it and retry.' >&2
    exit 1
fi
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/radar-test.XXXXXX")
trap 'rm -rf "$work_dir"' 0
herdr_config_dest=$home_dir/.config/herdr/config.toml
if [ "$files_only" -eq 0 ]; then
    if command -v herdr >/dev/null 2>&1 && [ "${HERDR_ENV:-}" = 1 ]; then
        install_radar
    else
        manual_radar
    fi
fi
printf '%s\n' core-continues
RUNNER
radar_commit=$(sed -n 's/^RADAR_COMMIT=//p' "$repo_dir/lib/radar.sh")
[ -n "$radar_commit" ] || { printf '%s\n' 'FAIL: cannot read pinned RADAR_COMMIT'; exit 1; }
radar_settings=$(printf '%s\n' 'parent_token = "taskr_parent"' 'parent_label_token = "taskr_campaign"' 'space_owner = false' 'attention_token = "taskr_owner_ask"' 'space_logo_names = false')
radar_install="plugin install olafurns7/herdr-radar --ref $radar_commit -y"
radar_calls() {
    grep '^plugin .*hhdebb.herdr-radar\|^plugin install olafurns7/herdr-radar' "$logs/herdr.log" || :
}
expect_radar_calls() {
    [ "$(radar_calls)" = "$1" ] || fail "radar calls differ: $(radar_calls)"
}
seed_radar() {
    jq -n --arg commit "$1" '{result:{plugins:[{plugin_id:"hhdebb.herdr-radar",source:{kind:"github",owner:"olafurns7",repo:"herdr-radar",resolved_commit:$commit}}]}}' >"$home/radar-source.json"
    mkdir -p "$home/radar-config" "$home/.config/herdr"
    printf '%s\n' "$radar_settings" >"$home/radar-config/config.toml"
    printf '%s\n' '# owner config' '$space_owner' >"$home/.config/herdr/config.toml"
}
# An owner who keeps space_owner = true and has no $space_owner cell yet.
seed_radar_owner() {
    seed_radar "$1"
    printf '%s\n' "$radar_settings" | sed 's/^space_owner = false$/space_owner = true/' >"$home/radar-config/config.toml"
    printf '%s\n' '# no radar cell' >"$home/.config/herdr/config.toml"
}
radar_read_calls=$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' 'plugin config-dir hhdebb.herdr-radar')
radar_apply_calls=$(printf '%s\n' 'plugin action invoke hhdebb.herdr-radar.state-stop' 'plugin action invoke hhdebb.herdr-radar.state-start' 'plugin action invoke hhdebb.herdr-radar.configure')
RUN_HERDR_ENV=1

new_case 'radar: fresh host installs pin, settings, backup and ordered actions'
mkdir -p "$home/.config/herdr"
printf '%s\n' '# owner config' >"$home/.config/herdr/config.toml"
run_install "$project"
expect_rc 0
expect_file_content "$home/radar-config/config.toml" "$radar_settings"
expect_file_content "$home/.config/herdr/config.toml" '# owner config'
expect_file_content "$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)" '# owner config'
expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' "$radar_install" 'plugin config-dir hhdebb.herdr-radar' "$radar_apply_calls")"
: >"$logs/herdr.log"
run_install "$project"
expect_rc 0
expect_out 'SKIP herdr-radar (already configured)'
expect_radar_calls "$radar_read_calls"
expect_file_content "$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)" '# owner config'

new_case 'radar: second run at pin skips actions and writes no radar file'
seed_radar "$radar_commit"
run_install "$project"
expect_rc 0
cp "$home/radar-config/config.toml" "$case_dir/settings-before"
cp "$home/.config/herdr/config.toml" "$case_dir/herdr-before"
touch -t 200001010000 "$home/radar-config/config.toml" "$home/.config/herdr/config.toml"
touch "$case_dir/time-marker"
: >"$logs/herdr.log"
run_install "$project"
expect_rc 0
expect_out 'SKIP herdr-radar (already configured)'
[ "$(printf '%s\n' "$out" | grep -c '^SKIP herdr-radar')" -eq 1 ] || fail 'expected one radar SKIP'
expect_radar_calls "$radar_read_calls"
cmp -s "$home/radar-config/config.toml" "$case_dir/settings-before" || fail 'settings changed'
cmp -s "$home/.config/herdr/config.toml" "$case_dir/herdr-before" || fail 'Herdr config changed'
for radar_file in "$home/radar-config/config.toml" "$home/.config/herdr/config.toml"; do
    [ ! "$radar_file" -nt "$case_dir/time-marker" ] || fail 'radar file was written'
done
expect_absent "$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)"

new_case 'radar: another commit stops before replacement'
seed_radar old-commit
run_install "$project"
expect_rc 0
expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' 'plugin action invoke hhdebb.herdr-radar.state-stop' "$radar_install" 'plugin config-dir hhdebb.herdr-radar' "$radar_apply_calls")"

new_case 'radar: missing key is inserted above unchanged colors table'
seed_radar "$radar_commit"
STUB_UNAME=Linux
printf '%s\n' 'parent_token = "taskr_parent"' 'space_owner = true' '[colors]' 'active_row_bg_light = "owner"' >"$home/radar-config/config.toml"
run_install "$project"
expect_rc 0
expect_file_content "$home/radar-config/config.toml" "$(printf '%s\n' 'parent_token = "taskr_parent"' 'space_owner = true' 'parent_label_token = "taskr_campaign"' 'attention_token = "taskr_owner_ask"' 'space_logo_names = false' '[colors]' 'active_row_bg_light = "owner"')"
STUB_UNAME=

new_case 'radar: differing keys are kept with one KEEP line each'
seed_radar "$radar_commit"
printf '%s\n' "$radar_settings" | sed -e 's/"taskr_parent"/"owner_choice"/' -e 's/"taskr_owner_ask"/"owner_ask"/' -e 's/^space_logo_names = false$/space_logo_names = true/' >"$home/radar-config/config.toml"
cp "$home/radar-config/config.toml" "$case_dir/settings-before"
run_install "$project"
expect_rc 0
for radar_key in parent_token attention_token space_logo_names; do
    expect_out "KEEP herdr-radar $radar_key"
done
[ "$(printf '%s\n' "$out" | grep -c '^KEEP herdr-radar')" -eq 3 ] || fail 'expected three radar KEEPs'
cmp -s "$home/radar-config/config.toml" "$case_dir/settings-before" || fail 'owner value changed'
expect_radar_calls "$radar_read_calls"

new_case 'radar: Linux colors are appended once'
seed_radar "$radar_commit"
STUB_UNAME=Linux
run_install "$project"
expect_rc 0
expect_file_content "$home/radar-config/config.toml" "$(printf '%s\n' "$radar_settings" '' '[colors]' 'active_row_bg_light = ""')"
: >"$logs/herdr.log"
run_install "$project"
expect_rc 0
expect_radar_calls "$radar_read_calls"
expect_out 'SKIP herdr-radar (already configured)'
[ "$(grep -c '^\[colors\]$' "$home/radar-config/config.toml")" -eq 1 ] || fail 'colors table duplicated'
STUB_UNAME=

new_case 'radar: macOS does not append Linux colors'
seed_radar "$radar_commit"
STUB_UNAME=Darwin
run_install "$project"
expect_rc 0
expect_file_content "$home/radar-config/config.toml" "$radar_settings"
expect_radar_calls "$radar_read_calls"
STUB_UNAME=

new_case 'radar: missing live pid retries state-start once'
STUB_RADAR_RETRY=1
run_install "$project"
expect_rc 0
expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' "$radar_install" 'plugin config-dir hhdebb.herdr-radar' 'plugin action invoke hhdebb.herdr-radar.state-stop' 'plugin action invoke hhdebb.herdr-radar.state-start' 'plugin action invoke hhdebb.herdr-radar.state-start' 'plugin action invoke hhdebb.herdr-radar.configure')"
STUB_RADAR_RETRY=

new_case 'radar: outside Herdr prints standalone manual commands without radar calls'
RUN_HERDR_ENV=
run_install "$project"
expect_rc 0
expect_radar_calls ''
radar_manual=$(printf '%s\n' 'MANUAL: if a radar is already installed, stop it first and wait up to 10 seconds for animator.pid to name no live process:' 'herdr plugin action invoke hhdebb.herdr-radar.state-stop' 'MANUAL: install the patched radar inside Herdr with jq on PATH:' "herdr plugin install olafurns7/herdr-radar --ref $radar_commit -y" 'MANUAL: find the radar settings file:' 'herdr plugin config-dir hhdebb.herdr-radar' 'MANUAL: add missing top-level parent_token = "taskr_parent", parent_label_token = "taskr_campaign", space_owner = false, attention_token = "taskr_owner_ask", space_logo_names = false; preserve existing values. On Linux, add [colors] and active_row_bg_light = "" only if both are absent.' 'MANUAL: back up Herdr config once before applying:' 'test -e "$HOME/.config/herdr/config.toml.pre-radar-$(date -u +%F)" || cp "$HOME/.config/herdr/config.toml" "$HOME/.config/herdr/config.toml.pre-radar-$(date -u +%F)"' 'MANUAL: start radar and wait up to 10 seconds for a live animator.pid; repeat once if needed:' 'herdr plugin action invoke hhdebb.herdr-radar.state-start' 'MANUAL: apply the radar settings:' 'herdr plugin action invoke hhdebb.herdr-radar.configure')
printf '%s\n' "$out" | grep -F -A 12 'MANUAL: if a radar is already installed' >"$case_dir/manual" || fail 'manual guidance missing'
expect_file_content "$case_dir/manual" "$radar_manual"
printf '%s\n' "$radar_manual"

new_case 'radar: failed install warns, preserves exit code and skips actions'
RUN_HERDR_ENV=1
STUB_RADAR_FAIL=1
run_install "$project"
expect_rc 0
expect_out "WARN: herdr $radar_install failed; skipping radar setup"
expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' "$radar_install")"
expect_absent "$home/radar-config/config.toml"
STUB_RADAR_FAIL=

for radar_source_field in kind owner repo; do
    new_case "radar: different source $radar_source_field stops before replacement"
    seed_radar "$radar_commit"
    jq --arg field "$radar_source_field" '.result.plugins[0].source[$field] = "other"' "$home/radar-source.json" >"$case_dir/source.json"
    cp "$case_dir/source.json" "$home/radar-source.json"
    run_install "$project"
    expect_rc 0
    expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' 'plugin action invoke hhdebb.herdr-radar.state-stop' "$radar_install" 'plugin config-dir hhdebb.herdr-radar' "$radar_apply_calls")"
done

new_case 'radar: kept space_owner true with missing cell applies and preserves existing dated backup'
seed_radar_owner "$radar_commit"
cp "$home/radar-config/config.toml" "$case_dir/settings-before"
printf '%s\n' '# original backup' >"$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)"
run_install "$project"
expect_rc 0
expect_out 'KEEP herdr-radar space_owner'
cmp -s "$home/radar-config/config.toml" "$case_dir/settings-before" || fail 'owner true changed'
expect_file_content "$home/.config/herdr/config.toml" "$(printf '%s\n' '# no radar cell' '$space_owner')"
expect_radar_calls "$(printf '%s\n' "$radar_read_calls" "$radar_apply_calls")"
expect_file_content "$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)" '# original backup'

new_case 'radar: Linux existing active-row key prevents colors insertion'
seed_radar "$radar_commit"
printf '%s\n' 'active_row_bg_light = "owner"' >>"$home/radar-config/config.toml"
STUB_UNAME=Linux
run_install "$project"
expect_rc 0
expect_radar_calls "$radar_read_calls"
expect_file_content "$home/radar-config/config.toml" "$(printf '%s\n' "$radar_settings" 'active_row_bg_light = "owner"')"
STUB_UNAME=

new_case 'radar: files-only inside Herdr makes no radar calls'
run_install --files-only "$project"
expect_rc 0
expect_radar_calls ''
expect_absent "$home/radar-config/config.toml"
expect_no_out 'MANUAL: install the patched radar'

new_case 'radar: missing jq prints manual guidance before existing prerequisite exit'
mkdir -p "$case_dir/bin"
for radar_tool in "$install_sh" dirname; do
    ln -s "$(command -v "$radar_tool")" "$case_dir/bin/$radar_tool"
done
ln -s "$stub_dir/bin/herdr" "$case_dir/bin/herdr"
RUN_PATH=$case_dir/bin
run_install "$project"
RUN_PATH=
expect_rc 1
expect_no_log herdr
expect_out 'MANUAL: install the patched radar inside Herdr with jq on PATH:'
expect_out "herdr $radar_install"
RUN_HERDR_ENV=

RUN_HERDR_ENV=1
for radar_io_case in mkdir backup create rewrite colors; do
    if [ "$(id -u)" -eq 0 ] && { [ "$radar_io_case" = rewrite ] || [ "$radar_io_case" = colors ]; }; then
        continue
    fi
    new_case "radar fix1: $radar_io_case failure warns once and continues"
    mkdir -p "$project/.herdr"
    : >"$project/.herdr/herdr-plugin.toml"
    RUN_HERDR_ENV=
    run_install --taskr-hub "$project"
    expect_rc 0
    radar_baseline_rc=$rc
    RUN_HERDR_ENV=1
    seed_radar "$radar_commit"
    case $radar_io_case in
        mkdir)
            : >"$home/radar-blocked"
            STUB_RADAR_IO=mkdir
            radar_failed_path=$home/radar-blocked/config
            ;;
        backup)
            seed_radar_owner "$radar_commit"
            STUB_RADAR_IO=backup
            radar_failed_path=$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)
            ;;
        create)
            rm "$home/radar-config/config.toml"
            mkdir "$home/radar-config/config.toml"
            radar_failed_path=$home/radar-config/config.toml
            ;;
        rewrite)
            printf '%s\n' 'space_owner = true' >"$home/radar-config/config.toml"
            chmod 0444 "$home/radar-config/config.toml"
            radar_failed_path=$home/radar-config/config.toml
            ;;
        colors)
            STUB_UNAME=Linux
            chmod 0444 "$home/radar-config/config.toml"
            radar_failed_path=$home/radar-config/config.toml
            ;;
    esac
    sh "$stub_dir/make-taskr.sh" "$home/.local/bin" v0.5.0
    : >"$logs/herdr.log"
    run_install --taskr-hub "$project"
    expect_rc "$radar_baseline_rc"
    expect_out "WARN: herdr-radar could not access $radar_failed_path; skipping radar setup"
    [ "$(printf '%s\n' "$out" | grep -c '^WARN: herdr-radar')" -eq 1 ] || fail 'expected one file WARN'
    expect_out core-continues
    expect_out core-continues
    expect_out core-continues
    expect_radar_calls "$radar_read_calls"
    STUB_RADAR_IO=
    STUB_UNAME=
done

new_case 'radar fix1: pid-read failure counts as stopped and still configures'
seed_radar_owner "$radar_commit"
STUB_RADAR_IO=pid-read
run_install "$project"
expect_rc 0
expect_no_out '^WARN: herdr-radar|stub pid read failed'
expect_radar_calls "$(printf '%s\n' "$radar_read_calls" "$radar_apply_calls")"
radar_pid_path=$home/.local/state/herdr/plugins/hhdebb.herdr-radar/animator.pid
[ "$(grep -F -x -c "$radar_pid_path" "$logs/cat.log")" -eq 2 ] || fail 'expected a failed stop-wait read and a live start-wait read'
STUB_RADAR_IO=

for radar_prior_stop in no yes; do
    new_case "radar fix3: backup failure with prior stop $radar_prior_stop gives matching recovery guidance"
    if [ "$radar_prior_stop" = yes ]; then
        seed_radar_owner old-commit
    else
        seed_radar_owner "$radar_commit"
    fi
    STUB_RADAR_IO=backup
    run_install "$project"
    expect_rc 0
    expect_out "WARN: herdr-radar could not access $home/.config/herdr/config.toml.pre-radar-$(date -u +%F); skipping radar setup"
    if [ "$radar_prior_stop" = yes ]; then
        radar_recovery="WARN: herdr-radar's state process was stopped; start it with:"
        printf '%s\n' "$out" | grep -F -x -A 1 "$radar_recovery" >"$case_dir/recovery" || fail 'recovery guidance missing'
        expect_file_content "$case_dir/recovery" "$(printf '%s\n' "$radar_recovery" 'herdr plugin action invoke hhdebb.herdr-radar.state-start')"
        [ "$(printf '%s\n' "$out" | grep -F -x -c "$radar_recovery")" -eq 1 ] || fail 'expected one recovery warning'
        expect_radar_calls "$(printf '%s\n' 'plugin list --plugin hhdebb.herdr-radar --json' 'plugin action invoke hhdebb.herdr-radar.state-stop' "$radar_install" 'plugin config-dir hhdebb.herdr-radar')"
    else
        expect_no_out "state process was stopped|^herdr plugin action invoke hhdebb.herdr-radar.state-start$"
        expect_radar_calls "$radar_read_calls"
    fi
    expect_out core-continues
    STUB_RADAR_IO=
done

new_case 'radar fix1: indented table preserves table keys and inserts top-level keys'
seed_radar "$radar_commit"
printf '%s\n' 'parent_token = "taskr_parent"' '  [colors]' 'space_owner = true' >"$home/radar-config/config.toml"
run_install "$project"
expect_rc 0
expect_file_content "$home/radar-config/config.toml" "$(printf '%s\n' 'parent_token = "taskr_parent"' 'parent_label_token = "taskr_campaign"' 'space_owner = false' 'attention_token = "taskr_owner_ask"' 'space_logo_names = false' '  [colors]' 'space_owner = true')"

new_case 'radar fix1: Linux spaced colors table is preserved without duplication'
seed_radar "$radar_commit"
printf '%s\n' '  [ colors ]' >>"$home/radar-config/config.toml"
cp "$home/radar-config/config.toml" "$case_dir/settings-before"
STUB_UNAME=Linux
run_install "$project"
expect_rc 0
cmp -s "$home/radar-config/config.toml" "$case_dir/settings-before" || fail 'spaced colors table changed'
expect_radar_calls "$radar_read_calls"
STUB_UNAME=

new_case 'radar fix1: two failed starts give manual recovery and next run skips'
STUB_RADAR_RETRY=always
run_install "$project"
expect_rc 0
expect_out 'WARN: herdr-radar state process is not running; start it by hand: herdr plugin action invoke hhdebb.herdr-radar.state-start'
[ "$(grep -c '^plugin action invoke hhdebb.herdr-radar.state-start$' "$logs/herdr.log")" -eq 2 ] || fail 'expected two state-start calls'
grep -F -x 'plugin action invoke hhdebb.herdr-radar.configure' "$logs/herdr.log" >/dev/null || fail 'configure did not run'
: >"$logs/herdr.log"
run_install "$project"
expect_rc 0
expect_out 'SKIP herdr-radar (already configured)'
expect_radar_calls "$radar_read_calls"
STUB_RADAR_RETRY=

new_case 'radar fix1: owner false without cell skips actions and adds no cell'
seed_radar "$radar_commit"
cp "$home/radar-config/config.toml" "$case_dir/settings-before"
printf '%s\n' '# no radar cell' >"$home/.config/herdr/config.toml"
run_install "$project"
expect_rc 0
expect_no_out '^KEEP herdr-radar'
expect_out 'SKIP herdr-radar (already configured)'
expect_radar_calls "$radar_read_calls"
cmp -s "$home/radar-config/config.toml" "$case_dir/settings-before" || fail 'owner false changed'
expect_file_content "$home/.config/herdr/config.toml" '# no radar cell'
expect_absent "$home/.config/herdr/config.toml.pre-radar-$(date -u +%F)"
RUN_HERDR_ENV=

printf '\n%s cases, %s failures\n' "$case_count" "$failures"
[ "$failures" -eq 0 ]
