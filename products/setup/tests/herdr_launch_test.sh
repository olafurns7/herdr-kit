#!/bin/sh
# Runs bin/herdr-launch against stub herdr and taskr, with real jq.
# Never touches the live Herdr
# server, the live taskr ledger, or the real HOME.
# Usage: [HERDR_LAUNCH_RESERVED_ONLY=1] sh tests/herdr_launch_test.sh
set -eu

repo_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd -P)
launch_sh=$repo_dir/bin/herdr-launch

root=$(mktemp -d "${TMPDIR:-/tmp}/herdr-launch-test.XXXXXX")
trap 'rm -rf "$root"' 0
trap 'exit 1' 1 2 3 15

stub_dir=$root/stubs
mkdir -p "$stub_dir"

# Both stubs append "<name> <argv joined by |>" to one calls log, so a case
# can assert the exact calls and their order. STUB_FAIL names one call
# ("herdr tab", "taskr new", ...) that exits 1.
cat >"$stub_dir/herdr" <<'EOF'
#!/bin/sh
line=herdr
for arg in "$@"; do line="$line|$arg"; done
printf '%s\n' "$line" >>"$STUB_CALLS"
[ "${STUB_FAIL:-}" != "herdr $1 $2" ] || exit 1
case "$1 $2" in
    "pane current") printf '%s\n' "${STUB_CURRENT:-{\"result\":{\"pane\":{\"pane_id\":\"wA:p1\",\"tab_id\":\"wA:t1\",\"workspace_id\":\"wA\"}}}}" ;;
    "tab create")
        [ "$4" != wZ ] || exit 1
        [ -z "${STUB_TAB:-}" ] || { printf '%s\n' "$STUB_TAB"; exit 0; }
        n=7
        case $8 in *-push) [ "${STUB_PUSH_FAIL:-}" != tab ] || exit 1; n=8 ;; esac
        printf '{"result":{"root_pane":{"agent_session":{"kind":"id"},"pane_id":"%s:p%s","tab_id":"%s:t%s","workspace_id":"%s"},"tab":{"label":"x","tab_id":"%s:t%s","workspace_id":"%s"}}}\n' "$4" $n "$4" $n "$4" "$4" $n "$4"
        ;;
    "pane get")
        [ -z "${STUB_PANE_GET:-}" ] || { printf '%s\n' "$STUB_PANE_GET"; exit 0; }
        printf '{"result":{"pane":{"cwd":"%s","foreground_cwd":"/elsewhere","pane_id":"%s","tab_id":"%s:t5","workspace_id":"%s"}}}\n' \
            "${STUB_PANE_CWD:-$PWD}" "$3" "${STUB_PANE_WS:-wA}" "${STUB_PANE_WS:-wA}"
        ;;
    "pane process-info") printf '{"result":{"process_info":{"foreground_process_group_id":%s,"foreground_processes":[{"name":"zsh","pid":400}],"pane_id":"%s","shell_pid":400}}}\n' "${STUB_FG:-400}" "$4" ;;
    "pane wait-output")
        [ "${STUB_ENV_OK:-1}" = 1 ] || exit 1
        if [ -f "$STUB_CALLS.setup" ]; then
            [ "$(cat "$STUB_CALLS.env")" = "$5" ] || exit 1
            [ -f "$(cat "$STUB_CALLS.setup-path")" ] || exit 1
        fi ;;
    "pane run")
        case $3 in *:p8) [ "${STUB_PUSH_FAIL:-}" != run ] || exit 1 ;; esac
        case $4 in '. '*)
            source_command=$4
            eval "set -- ${4#'. '}"
            [ "$#" -eq 1 ] || exit 1
            printf '%s\n' "$1" >"$STUB_CALLS.setup-path"
            cp "$1" "$STUB_CALLS.setup"
            ls -ld "${1%/*}" | cut -c1-10 >"$STUB_CALLS.setup-dir-mode"
            ls -l "$1" | cut -c1-10 >"$STUB_CALLS.setup-file-mode"
            (
                eval "$source_command" || exit 1
                printf '%s\n' "herdr-launch-env=$TASKR_TASK/$TASKR_LAUNCH/$FLEET_ROLE/$herdr_launch_ok" >"$STUB_CALLS.env"
                printf '%s\n' "$PATH" >"$STUB_CALLS.path"
                printf '%s\n' "${TASKR_DB-unset}" >"$STUB_CALLS.db"
                if [ "${STUB_SOURCE_PROBE:-0}" = 1 ]; then
                    codex 'two words' 'literal$arg'
                    claude 'two words' 'literal$arg'
                fi
            ) || exit 1
            ;; esac ;;
    "pane read") printf '%s\n' "${STUB_SCREEN:-}" ;;
    "agent start") [ "${STUB_START_OK:-1}" = 1 ] || exit 1; printf '%s\n' "$3" >"$STUB_CALLS.started" ;;
    "agent list")
        if [ -f "$STUB_CALLS.started" ]; then
            [ -z "${STUB_NATIVE:-}" ] || { printf '%s\n' "$STUB_NATIVE"; exit 0; }
            printf '{"result":{"agents":[{"name":"%s","pane_id":"wA:p5","agent_session":{"value":"synthetic-native"}}]}}\n' "$(cat "$STUB_CALLS.started")"
        else printf '%s\n' '{"result":{"agents":[]}}'; fi ;;
esac
exit 0
EOF
cat >"$stub_dir/taskr" <<'EOF'
#!/bin/sh
line=taskr
for arg in "$@"; do line="$line|$arg"; done
printf '%s\n' "$line" >>"$STUB_CALLS"
[ "${STUB_FAIL:-}" != "taskr $1" ] || exit 1
[ "${STUB_FAIL:-}" != "taskr $1 ${2:-}" ] || exit 1
case $1 in
    --json) case "$2 $3" in
        'work list') cat "$STUB_RESERVED" ;;
        'work observe')
            [ "${STUB_FAIL:-}" != 'taskr work observe' ] || exit 1
            if [ -n "${STUB_OBSERVE:-}" ]; then printf '%s\n' "$STUB_OBSERVE";
            else printf '%s\n' '{"launch_id":3579,"task_id":2468,"native_session":"synthetic-native","dispatch":{"pane":"wA:p5"}}'; fi ;;
        *) exit 1 ;; esac ;;
    new) printf '%s\n' "${STUB_NEW:-n1 2468}" ;;
    launch) printf '%s\n' "${STUB_LAUNCH:-l1 {\"launch_id\":3579,\"status\":\"open\"}}" ;;
esac
exit 0
EOF
ln -s "$(command -v jq)" "$stub_dir/jq"
chmod 0755 "$stub_dir/herdr" "$stub_dir/taskr"
# Reserved-mode socket preflight is isolated too; no connection to a live server.
printf '%s\n' '#!/bin/sh' 'cat >/dev/null' 'exit "${STUB_SERVER_RC:-0}"' >"$stub_dir/python3"
chmod 0755 "$stub_dir/python3"

failures=0
case_count=0
case_name=

fail() {
    printf '  FAIL [%s] %s\n' "$case_name" "$1"
    failures=$((failures + 1))
}

# new_case NAME [ROLE]: a fresh HOME, work directory, brief, and calls log.
new_case() {
    case_name=$1
    case_count=$((case_count + 1))
    case_dir=$root/case$case_count
    home=$case_dir/home
    mcp_config=$home/.local/state/herdr-orch/lane-push/taskr-mcp-2468.json
    work=$case_dir/work
    mkdir -p "$home/.opencode/bin" "$work"
    printf '%s\n' '#!/bin/sh' '[ "$1" = --version ] || exit 99' 'echo synthetic-opencode' >"$home/.opencode/bin/opencode"
    chmod 0755 "$home/.opencode/bin/opencode"
    work=$(CDPATH= cd "$work" && pwd -P)
    run_path=$stub_dir:/usr/bin:/bin
    run_tmpdir=$case_dir/artifacts
    mkdir "$run_tmpdir"
    ws=wA
    tcwd=$work
    brief=$work/brief.md
    report=$work/report.md
    printf '%s\n' "HERDR-BRIEF role=${2:-implementer}" "report=$report" \
        'Task topology: workspace=wA' 'Objective: test launch' 'Ownership: test files' \
        'Acceptance: launch succeeds' 'Validation: stub calls' >"$brief"
    : >"$case_dir/calls"
    STUB_FAIL= STUB_CURRENT= STUB_TAB= STUB_PANE_WS= STUB_PANE_CWD= STUB_PANE_GET= STUB_FG= STUB_ENV_OK= STUB_NEW= STUB_LAUNCH=
    STUB_SCREEN= STUB_START_OK= STUB_PUSH_FAIL= STUB_SERVER_RC=0 STUB_NATIVE= STUB_OBSERVE=
    STUB_INIT_RC=0 STUB_INIT_NATIVE=synthetic-native
    STUB_SOURCE_PROBE=0
    STUB_RESERVED=$case_dir/reserved.json
    printf '%s\n' "case: $case_name"
}

# run_launch ARGS...: runs herdr-launch in $work; sets $out, $err, $rc.
run_launch() {
    run_status=0
    (cd "$work" && env -i HOME="$home" PATH="$run_path" TMPDIR="$run_tmpdir" \
        STUB_CALLS="$case_dir/calls" STUB_FAIL="$STUB_FAIL" STUB_CURRENT="$STUB_CURRENT" \
        STUB_TAB="$STUB_TAB" STUB_PANE_WS="$STUB_PANE_WS" \
        STUB_PANE_CWD="$STUB_PANE_CWD" STUB_PANE_GET="$STUB_PANE_GET" STUB_FG="$STUB_FG" \
        STUB_ENV_OK="$STUB_ENV_OK" STUB_NEW="$STUB_NEW" STUB_LAUNCH="$STUB_LAUNCH" \
        STUB_SCREEN="$STUB_SCREEN" STUB_START_OK="$STUB_START_OK" STUB_PUSH_FAIL="$STUB_PUSH_FAIL" \
        STUB_RESERVED="$STUB_RESERVED" STUB_SERVER_RC="$STUB_SERVER_RC" \
        STUB_NATIVE="$STUB_NATIVE" STUB_OBSERVE="$STUB_OBSERVE" \
        STUB_INIT_RC="$STUB_INIT_RC" STUB_INIT_NATIVE="$STUB_INIT_NATIVE" \
        STUB_SOURCE_PROBE="$STUB_SOURCE_PROBE" \
        TASKR_FORMAT=json "$@" \
        >"$case_dir/out" 2>"$case_dir/err") || run_status=$?
    out=$(cat "$case_dir/out")
    err=$(cat "$case_dir/err")
    rc=$run_status
}

expect_rc() {
    [ "$rc" -eq "$1" ] || fail "exit $rc, expected $1; stderr: $err"
}

expect_out() {
    [ "$out" = "$1" ] || fail "stdout '$out', expected '$1'"
}

expect_err() {
    printf '%s\n' "$err" | grep -F -- "$1" >/dev/null || fail "stderr lacks: $1; stderr: $err"
}

# expect_calls LINE...: the calls log holds exactly these lines in order.
expect_calls() {
    printf '%s\n' "$@" >"$case_dir/want"
    if ! cmp -s "$case_dir/want" "$case_dir/calls"; then
        fail "calls differ: $(diff "$case_dir/want" "$case_dir/calls" | sed -n '2,12p' | tr '\n' ' ')"
    fi
}

expect_no_calls() {
    [ ! -s "$case_dir/calls" ] || fail "calls were made: $(head -n 3 "$case_dir/calls" | tr '\n' ' ')"
}

# The lane flags: full permissions, no sandbox (owner decision, 2026-10-07).
claude_flags() {
    printf '%s' "--model|claude-opus-5-5|--effort|$1|--dangerously-skip-permissions|--settings|{\"agentPushNotifEnabled\":false,\"enabledPlugins\":{\"posthog@claude-plugins-official\":false}}"
}
# codex_flags MODEL EFFORT TRUST
codex_flags() {
    printf '%s' "|-m|$1|-c|model_reasoning_effort=\"$2\"|--dangerously-bypass-approvals-and-sandbox|-c|projects={$3}|--dangerously-bypass-hook-trust"
}
setup=' export FLEET_ROLE=worker TASKR_TASK=2468 TASKR_LAUNCH=3579'
claude_setup="$setup CLAUDE_CODE_SANDBOXED=1"
forward=' && codex() { "$HOME/.local/bin/codator" codex "$@"; } && claude() { "$HOME/.local/bin/codator" claude "$@"; }'
env_check='echo "herdr-launch-env=$TASKR_TASK/$TASKR_LAUNCH/$FLEET_ROLE/$herdr_launch_ok"'

# The calls of steps 2 to 7 for task 2468, launch 3579 in pane P (tab T),
# workspace $ws, cwd $tcwd.
tail_calls() {
    printf '%s\n' \
        "taskr|new|$1|--parent|42|--role|$2|--workspace|$ws|--tab|$4|--pane|$3|--cwd|$tcwd|--brief|$brief|--report|$report" \
        "taskr|launch|2468|--provider|$5|--model|$6|--effort|$7" \
        "herdr|pane|run|$3|$8 && herdr_launch_ok=3579" \
        "herdr|pane|run|$3|$env_check" \
        "herdr|pane|wait-output|$3|--match|herdr-launch-env=2468/3579/worker/3579|--timeout|15000" \
        "herdr|agent|start|$1|--kind|$5|--pane|$3|--timeout|90000|--$9" \
        "taskr|prompt|2468|--file|$brief"
}
tab_calls() {
    [ -n "${2-}" ] || printf '%s\n' "herdr|pane|current|--current"
    printf '%s\n' "herdr|tab|create|--workspace|$ws|--cwd|$tcwd|--label|$1|--env|FLEET_ROLE=worker|--env|DISABLE_AUTO_UPDATE=true|--no-focus"
}

if [ "${HERDR_LAUNCH_RESERVED_ONLY:-0}" != 1 ]; then
new_case 'claude lane in a new tab, with Codator forwarding'
mkdir -p "$home/.local/bin"
printf '%s\n' '#!/bin/sh' >"$home/.local/bin/codator"
chmod 0755 "$home/.local/bin/codator"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7'
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "|$(claude_flags medium)")"

new_case 'codex lane without Codator exports only the ids' reviewer
run_launch sh "$launch_sh" sol-medium rev-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls rev-cart)" \
    "$(tail_calls rev-cart reviewer wA:p7 wA:t7 codex gpt-6.1-sol medium "$setup" \
        "$(codex_flags gpt-6.1-sol medium "\"$tcwd\"={trust_level=\"trusted\"}")")"

# Public cheap lanes use OpenCode directly, including with Codator installed.
opencode_setup() {
    config=$(jq -cn --arg model "$1" '{model:$model,default_agent:"build",agents:{build:{model:$model}}}')
    printf '%s' "$setup${2-} && opencode() { '$home/.opencode/bin/opencode' \"\$@\"; } && export OPENCODE_CONFIG_CONTENT='$config'"
}
for lane_model in minimax-free:minimax-m3.1-flash-free glm-free:glm-5.3-flash-free glm-flash:glm-5.3-flash; do
    for lane_prefix in glm gather; do
        role=implementer
        [ "$lane_prefix" != gather ] || role=researcher
        new_case "${lane_model%%:*} launches $lane_prefix- through OpenCode" "$role"
        mkdir -p "$home/.local/bin"
        printf '%s\n' '#!/bin/sh' 'exit 99' >"$home/.local/bin/codator"
        chmod 0755 "$home/.local/bin/codator"
        run_launch sh "$launch_sh" "${lane_model%%:*}" "$lane_prefix-inventory" "$brief" "$report" 42
        expect_rc 0
        expect_calls "$(tab_calls "$lane_prefix-inventory")" \
            "$(tail_calls "$lane_prefix-inventory" "$role" wA:p7 wA:t7 opencode "opencode/${lane_model#*:}" high \
                "$(opencode_setup "opencode/${lane_model#*:}" "$forward")" '|--standalone|--auto')"
        [ ! -e "$home/.config/opencode/opencode.json" ] || fail 'global OpenCode config written'
        : >"$case_dir/calls"
        run_launch sh "$launch_sh" --check "${lane_model%%:*}"
        expect_rc 0
        expect_out "${lane_model%%:*}: ok"
        expect_no_calls
    done
done

new_case 'OpenCode free lane with a given pane'
run_launch sh "$launch_sh" glm-free glm-inventory "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3'
grep -q -F '|--kind|opencode|--pane|wA:p3|--timeout|90000|--|--standalone|--auto' "$case_dir/calls" || fail 'wrong OpenCode argv'
grep -q -F 'opencode/glm-5.3-flash-free' "$case_dir/calls" || fail 'free model missing'

new_case 'OpenCode absent on PATH and in its install directory fails before any call'
rm "$home/.opencode/bin/opencode"
run_launch sh "$launch_sh" glm-free glm-inventory "$brief" "$report" 42
expect_rc 2
expect_err 'missing CLI: opencode'
expect_no_calls
run_launch sh "$launch_sh" --check glm-free
expect_rc 1
expect_out 'glm-free: missing CLI: opencode'
expect_no_calls

new_case 'OpenCode uses PATH ahead of the install fallback and quotes its executable'
mkdir -p "$work/bin with 'quote"
cp "$home/.opencode/bin/opencode" "$work/bin with 'quote/opencode"
run_launch PATH="$work/bin with 'quote:$stub_dir:/usr/bin:/bin" sh "$launch_sh" glm-free glm-inventory "$brief" "$report" 42
expect_rc 0
pane_setup=$(sed -n 's/^herdr|pane|run|wA:p7|\( .*\)$/\1/p' "$case_dir/calls" | head -n 1)
for sh_cmd in 'bash --noprofile --norc -c' 'zsh -f -c'; do
    command -v "${sh_cmd%% *}" >/dev/null || continue
    got=$($sh_cmd "$pane_setup; opencode --version; $env_check")
    want='synthetic-opencode
herdr-launch-env=2468/3579/worker/3579'
    [ "$got" = "$want" ] || fail "${sh_cmd%% *}: executable/config setup failed: $got"
done

# Private pi tables retain compatibility; no public lane selects pi.
private_pi_lane() {
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' '{"glm":{"kind":"pi","model":"glm-5.3-flash","effort":"high","prefixes":["glm"]}}' >"$home/.config/herdr-fleet/lanes.json"
}

new_case 'Haiku gather launches as a researcher' researcher
run_launch sh "$launch_sh" haiku-gather gather-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-x)" \
    "$(tail_calls gather-x researcher wA:p7 wA:t7 claude claude-haiku-5-5 medium "$claude_setup" \
        '|--model|claude-haiku-5-5|--effort|medium|--dangerously-skip-permissions|--settings|{"agentPushNotifEnabled":false,"enabledPlugins":{"posthog@claude-plugins-official":false}}')"

new_case 'Luna gather launches as a researcher' researcher
run_launch sh "$launch_sh" luna-gather gather-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-x)" \
    "$(tail_calls gather-x researcher wA:p7 wA:t7 codex gpt-6-luna medium "$setup" \
        "$(codex_flags gpt-6-luna medium "\"$tcwd\"={trust_level=\"trusted\"}")")"

new_case 'opus-xhigh launches an orch- sub-orchestrator' sub-orchestrator
run_launch sh "$launch_sh" opus-xhigh orch-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls orch-x)" \
    "$(tail_calls orch-x sub-orchestrator wA:p7 wA:t7 claude claude-opus-5-5 xhigh "$claude_setup" \
        "|$(claude_flags xhigh)")"

new_case 'codex lane in a git worktree: both checkouts trusted' reviewer
git init -q "$case_dir/main"
git -C "$case_dir/main" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m init
git -C "$case_dir/main" worktree add -q "$case_dir/wt" 2>/dev/null
main=$(CDPATH= cd "$case_dir/main" && pwd -P)
tcwd=$(CDPATH= cd "$case_dir/wt" && pwd -P)
run_launch sh "$launch_sh" --cwd "$tcwd" sol-medium rev-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls rev-cart)" \
    "$(tail_calls rev-cart reviewer wA:p7 wA:t7 codex gpt-6.1-sol medium "$setup" \
        "$(codex_flags gpt-6.1-sol medium \
            "\"$tcwd\"={trust_level=\"trusted\"},\"$main\"={trust_level=\"trusted\"}")")"

new_case 'a codex cwd with a double quote stops at step 1 before taskr new' reviewer
mkdir -p "$case_dir/a\"b"
tcwd=$(CDPATH= cd "$case_dir/a\"b" && pwd -P)
run_launch sh "$launch_sh" --cwd "$tcwd" sol-medium rev-cart "$brief" "$report" 42
expect_rc 1
expect_err 'contains'
expect_calls "$(tab_calls rev-cart)"

new_case 'a claude cwd with a quote launches: no lane flag carries the cwd'
mkdir -p "$case_dir/it's"
tcwd=$(CDPATH= cd "$case_dir/it's" && pwd -P)
run_launch sh "$launch_sh" --cwd "$tcwd" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "|$(claude_flags medium)")"

new_case 'a cwd with a tab stops at step 1 before taskr new' reviewer
mkdir -p "$case_dir/a	b"
tcwd=$(CDPATH= cd "$case_dir/a	b" && pwd -P)
run_launch sh "$launch_sh" --cwd "$tcwd" sol-medium rev-cart "$brief" "$report" 42
expect_rc 1
expect_err 'contains'
expect_calls "$(tab_calls rev-cart)"

# The step-4 setup, run by a real shell, acknowledges only a complete setup:
# a claude() that cannot be redefined must keep the step-5 marker from matching.
new_case 'a failed claude() definition withholds the step-5 acknowledgement'
mkdir -p "$home/.local/bin"
printf '%s\n' '#!/bin/sh' >"$home/.local/bin/codator"
chmod 0755 "$home/.local/bin/codator"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
pane_setup=$(sed -n 's/^herdr|pane|run|wA:p7|\( export .*\)$/\1/p' "$case_dir/calls")
want='herdr-launch-env=2468/3579/worker/3579'
got=$(bash --noprofile --norc -c "$pane_setup; $env_check")
[ "$got" = "$want" ] || fail "a clean shell echoed '$got', expected '$want'"
got=$(bash --noprofile --norc -c "claude() { :; }; readonly -f claude; $pane_setup; $env_check" 2>/dev/null)
[ "$got" != "$want" ] || fail 'a readonly claude() still produced the step-5 marker'

# A reused pane shell may still define the old bwrap pi() wrapper; the pi
# setup must drop it so the pi executable is reached directly, in bash and zsh.
new_case 'pi setup in a shell with the old bwrap pi() reaches pi without bwrap'
private_pi_lane
run_launch sh "$launch_sh" glm glm-inventory "$brief" "$report" 42
expect_rc 0
pane_setup=$(sed -n 's/^herdr|pane|run|wA:p7|\( .*\)$/\1/p' "$case_dir/calls" | head -n 1)
mkdir -p "$case_dir/bin"
printf '%s\n' '#!/bin/sh' 'echo "pi-stub $*"' >"$case_dir/bin/pi"
printf '%s\n' '#!/bin/sh' 'echo "bwrap-called $*"' >"$case_dir/bin/bwrap"
chmod 0755 "$case_dir/bin/pi" "$case_dir/bin/bwrap"
old_pi='pi() { bwrap --ro-bind / / -- pi "$@"; }'
for sh_cmd in 'bash --noprofile --norc -c' 'zsh -f -c'; do
    command -v "${sh_cmd%% *}" >/dev/null || continue
    got=$(PATH="$case_dir/bin:$PATH" $sh_cmd "$old_pi; $pane_setup; pi --approve; $env_check")
    want='pi-stub --approve
herdr-launch-env=2468/3579/worker/3579'
    [ "$got" = "$want" ] || fail "${sh_cmd%% *}: got '$got', expected '$want'"
done

# Owner decision, 2026-10-07 (later): no lane carries a sandbox setting, a
# permission profile, an approval policy, or bwrap, in its argv or its setup.
for lane_name in opus-medium:impl-yolo sol-high:impl-yolo terra-medium:impl-yolo glm-flash:glm-yolo; do
    new_case "no sandbox in the ${lane_name%%:*} lane"
    run_launch sh "$launch_sh" "${lane_name%%:*}" "${lane_name#*:}" "$brief" "$report" 42
    expect_rc 0
    grep -q '^herdr|agent|start|' "$case_dir/calls" || fail 'no agent start'
    grep -E '^herdr\|(agent\|start|pane\|run)\|' "$case_dir/calls" \
        | grep -E '"sandbox"|permissions[.=]|default_permissions|bwrap|\|-a\||\|-s\||--sandbox\||--ask-for-approval|allowWrite' \
        && fail 'a sandbox, permission profile, approval policy, or bwrap is in the argv'
    case ${lane_name%%:*} in
        opus-*) grep -q -F -- '|--dangerously-skip-permissions|' "$case_dir/calls" || fail 'no bypass flag' ;;
        sol-* | terra-*) grep -q -F -- '|--dangerously-bypass-approvals-and-sandbox|' "$case_dir/calls" || fail 'no bypass flag' ;;
    esac
done


new_case 'given pane: workspace and shell checks, then ctrl+c' sub-orchestrator
run_launch sh "$launch_sh" opus-high orch-cart "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3'
expect_calls "herdr|pane|get|wA:p3" \
    "herdr|pane|process-info|--pane|wA:p3" "herdr|pane|send-keys|wA:p3|ctrl+c" \
    "$(tail_calls orch-cart sub-orchestrator wA:p3 wA:t5 claude claude-opus-5-5 high "$claude_setup" \
        "|$(claude_flags high)")"

new_case 'given pane in another workspace registers that workspace and tab'
STUB_PANE_WS=wB
ws=wB
run_launch sh "$launch_sh" opus-high impl-cart "$brief" "$report" 42 wB:p1
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wB:t5 pane=wB:p1'
expect_calls "herdr|pane|get|wB:p1" \
    "herdr|pane|process-info|--pane|wB:p1" "herdr|pane|send-keys|wB:p1|ctrl+c" \
    "$(tail_calls impl-cart implementer wB:p1 wB:t5 claude claude-opus-5-5 high "$claude_setup" \
        "|$(claude_flags high)")"

new_case 'given pane registers the pane cwd, not the caller cwd'
mkdir -p "$case_dir/target"
tcwd=$(CDPATH= cd "$case_dir/target" && pwd -P)
STUB_PANE_CWD=$tcwd
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
expect_rc 0
[ "$tcwd" != "$work" ] || fail 'caller and target directories are the same'
grep -q -F -- "|--cwd|$tcwd|" "$case_dir/calls" || fail 'taskr new did not receive the pane cwd'
grep -q -F -- "|--cwd|$work|" "$case_dir/calls" && fail 'taskr new received the caller cwd'

new_case 'a pane get without cwd stops at step 1'
STUB_PANE_GET='{"result":{"pane":{"foreground_cwd":"/elsewhere","pane_id":"wA:p3","tab_id":"wA:t5","workspace_id":"wA"}}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
expect_rc 1
expect_err 'step 1 ("cwd" is not in herdr pane get exactly once) failed; so far: workspace=- task=- launch=- tab=- pane=wA:p3'
expect_calls "herdr|pane|get|wA:p3"

# Misread pane cwds stop at step 1 before taskr new.
check_bad_cwd() {
    new_case "pane cwd $1 stops at step 1"
    STUB_PANE_CWD=$2
    run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
    expect_rc 1
    expect_err 'step 1 (the pane cwd could not be read'
    expect_calls "herdr|pane|get|wA:p3"
}
# Both back\slash and the escaped back\\slash exist, so only the
# backslash rule can stop the first case.
mkdir -p "$root/back\\slash" "$root/back\\\\slash" "$root/com,ma"
check_bad_cwd 'with an escaped backslash' "$root/back\\\\slash"
check_bad_cwd 'with a comma' "$root/com,ma"
check_bad_cwd 'that is not a directory' "$root/no-such-dir"

# A nested tokens key of the same name never stands in for the pane's own.
mkdir -p "$root/task,root" "$root/decoy"
new_case 'nested tokens.cwd with an unreadable real cwd stops at step 1'
STUB_PANE_GET='{"result":{"pane":{"pane_id":"wB:p1","workspace_id":"wB","tab_id":"wB:t1","cwd":"'"$root/task,root"'","tokens":{"cwd":"'"$root/decoy"'"}}}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wB:p1
expect_rc 1
expect_err 'step 1 ("cwd" is not in herdr pane get exactly once) failed'
expect_calls "herdr|pane|get|wB:p1"
for key in workspace_id tab_id; do
    new_case "nested tokens.$key stops at step 1"
    STUB_PANE_GET='{"result":{"pane":{"cwd":"'"$work"'","pane_id":"wA:p3","tab_id":"wA:t5","tokens":{"'"$key"'":"wX"},"workspace_id":"wA"}}}'
    run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
    expect_rc 1
    expect_err "step 1 (\"$key\" is not in herdr pane get exactly once) failed"
    expect_calls "herdr|pane|get|wA:p3"
done

new_case 'a pane get with a tokens map of other keys still passes'
STUB_PANE_GET='{"result":{"pane":{"cwd":"'"$work"'","foreground_cwd":"/elsewhere","pane_id":"wA:p3","tab_id":"wA:t5","tokens":{"tab_key":"k-wA:t5","ws_key":"k-wA"},"workspace_id":"wA"}}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3'

new_case '--workspace and --cwd create the tab there without asking for the current pane'
mkdir -p "$case_dir/task"
ws=wT
tcwd=$(CDPATH= cd "$case_dir/task" && pwd -P)
run_launch sh "$launch_sh" --workspace wT --cwd "$case_dir/task" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wT:t7 pane=wT:p7'
expect_calls "$(tab_calls impl-cart wT)" \
    "$(tail_calls impl-cart implementer wT:p7 wT:t7 claude claude-opus-5-5 medium "$claude_setup" "|$(claude_flags medium)")"

new_case 'an unknown --workspace stops at step 1'
ws=wZ
run_launch sh "$launch_sh" --workspace wZ opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 1 (herdr tab create) failed; so far: workspace=wZ task=- launch=- tab=- pane=-'
expect_calls "$(tab_calls impl-cart wZ)"

new_case 'a workspace missing from herdr pane current stops at step 1'
STUB_CURRENT='{"result":{"pane":{"pane_id":"wA:p1"}}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 1 (workspace id from herdr pane current)'
expect_calls "herdr|pane|current|--current"

new_case 'given pane with a foreground program is refused'
STUB_FG=900
run_launch sh "$launch_sh" opus-high impl-cart "$brief" "$report" 42 wA:p3
expect_rc 1
expect_err 'step 1 (pane wA:p3 is not at its shell prompt)'
expect_calls "herdr|pane|get|wA:p3" "herdr|pane|process-info|--pane|wA:p3"

new_case 'ids without a 1 are read whole'
STUB_NEW='n1 2468' STUB_LAUNCH='l1 {"launch_id":3579}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7'

new_case 'misleading tags: the n1 and l1 digits are never read as ids'
STUB_NEW='n1 2468' STUB_LAUNCH='l1 {"replaced_launch_id":11,"launch_id":3579,"status":"open"}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7'

new_case 'a planned or noisy new line is refused'
STUB_NEW='n1 2468 planned'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err "step 2 (no single 'n1 <task>' line in: n1 2468 planned)"
expect_err 'task=- launch=- tab=wA:t7 pane=wA:p7'

new_case 'a launch line without launch_id is refused'
STUB_LAUNCH='l1 {"replaced_launch_id":3579}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 3 (no single'
expect_err 'task=2468 launch=- tab=wA:t7 pane=wA:p7'

new_case 'tab ids disagree'
STUB_TAB='{"result":{"root_pane":{"pane_id":"wA:p7","tab_id":"wA:t7"},"tab":{"tab_id":"wA:t8"}}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 1 (tab id from herdr tab create)'
expect_calls "$(tab_calls impl-cart)"

# A failure at each step stops there and prints the ids so far.
check_stop() {
    new_case "failure: $1"
    STUB_FAIL=$1
    run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
    expect_rc 1
    expect_err "step $2"
    expect_err "so far: $3"
    [ -z "$out" ] || fail "stdout not empty: $out"
    [ "$(wc -l <"$case_dir/calls")" -eq "$4" ] || fail "$(wc -l <"$case_dir/calls") calls, expected $4"
    case $(tail -n 1 "$case_dir/calls") in
        "$(printf '%s' "$1" | tr ' ' '|')|"*) ;;
        *) fail "last call was $(tail -n 1 "$case_dir/calls")" ;;
    esac
}
check_stop 'herdr pane current' 1 'workspace=- task=- launch=- tab=- pane=-' 1
check_stop 'herdr tab create' 1 'workspace=wA task=- launch=- tab=- pane=-' 2
check_stop 'taskr new' 2 'workspace=wA task=- launch=- tab=wA:t7 pane=wA:p7' 3
check_stop 'taskr launch' 3 'workspace=wA task=2468 launch=- tab=wA:t7 pane=wA:p7' 4
check_stop 'herdr pane run' 4 'workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7' 5
check_stop 'herdr pane wait-output' 5 'workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7' 7
check_stop 'herdr agent start' 6 'workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7' 8
check_stop 'taskr prompt' 7 'workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7' 9

new_case 'env verification failure stops before agent start'
STUB_ENV_OK=0
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 5 (the pane shell did not echo herdr-launch-env=2468/3579/worker/3579)'
grep -q '^herdr|agent' "$case_dir/calls" && fail 'agent start ran'
grep -q '^taskr|prompt' "$case_dir/calls" && fail 'prompt ran'

# Refusals happen before any call.
for pair in rs:researcher impl:implementer rev:reviewer orch:sub-orchestrator \
    impl:mechanical-worker rev:code-reviewer arch:architect rev:code-validator; do
    brief_prefix=${pair%%:*} brief_role=${pair#*:}
    new_case "supported brief role: $pair" "$brief_role"
    case $brief_prefix in
        orch) brief_lane=opus-high ;;
        arch) brief_lane=opus-xhigh ;;
        *) brief_lane=opus-medium ;;
    esac
    if [ "$brief_prefix" != impl ]; then
        sed '/^Ownership:/d' "$brief" >"$brief.tmp"
        mv "$brief.tmp" "$brief"
    fi
    run_launch sh "$launch_sh" "$brief_lane" "$brief_prefix-cart" "$brief" "$report" 42
    expect_rc 0
    grep -q '^herdr|agent|start|' "$case_dir/calls" || fail 'no agent start'
done

for variant in orchestrator orchestrator-journal standalone-journal trailing-blank crlf duplicate; do
    new_case "report header accepted: $variant"
    case $variant in
        orchestrator) sed 's/^report=/orchestrator=orch-test   report=/' "$brief" >"$brief.tmp" ;;
        orchestrator-journal) sed 's/^report=\(.*\)/orchestrator=orch-test   report=\1   journal=\/journal.md/' "$brief" >"$brief.tmp" ;;
        standalone-journal) sed '/^report=/s/$/   journal=\/journal.md/' "$brief" >"$brief.tmp" ;;
        trailing-blank) sed '/^report=/s/$/   /' "$brief" >"$brief.tmp" ;;
        crlf) awk '{ printf "%s\r\n", $0 }' "$brief" >"$brief.tmp" ;;
        duplicate) cat "$brief" >"$brief.tmp"; printf 'report=/other/report.md\n' >>"$brief.tmp" ;;
    esac
    mv "$brief.tmp" "$brief"
    run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
    expect_rc 0
done

for pair in impl:researcher orch:orchestrator gather:implementer; do
    brief_prefix=${pair%%:*} brief_role=${pair#*:}
    new_case "role outside prefix family refused: $pair" "$brief_role"
    sed '/^Ownership:/d' "$brief" >"$brief.tmp"
    mv "$brief.tmp" "$brief"
    brief_lane=opus-medium
    [ "$brief_prefix" != orch ] || brief_lane=opus-high
    [ "$brief_prefix" != gather ] || brief_lane=haiku-gather
    run_launch sh "$launch_sh" "$brief_lane" "$brief_prefix-cart" "$brief" "$report" 42
    expect_rc 2
    expect_err "role=$brief_role"
    expect_err "prefix=$brief_prefix-"
    [ "$(wc -l <"$case_dir/err")" -eq 1 ] || fail 'expected one actionable stderr line'
    expect_no_calls
done

new_case 'mechanical-worker implementer requires Ownership' mechanical-worker
sed '/^Ownership:/d' "$brief" >"$brief.tmp"
mv "$brief.tmp" "$brief"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err 'brief needs the Ownership: header'
expect_no_calls

for bad_field in role placeholder report 'Task topology:' 'Objective:' 'Acceptance:' \
    'Validation:' 'Ownership:' mismatch first-line; do
    new_case "malformed brief: $bad_field"
    case $bad_field in
        role) sed '1s/ role=implementer//' "$brief" >"$brief.tmp"; want='HERDR-BRIEF role=' ;;
        placeholder) sed '1s/implementer/__ROLE__/' "$brief" >"$brief.tmp"; want='HERDR-BRIEF role=' ;;
        report) sed '/^report=/d' "$brief" >"$brief.tmp"; want='report=' ;;
        mismatch) sed 's|^report=.*|report=/other/report.md|' "$brief" >"$brief.tmp"; want="brief report=/other/report.md, REPORT=$report" ;;
        first-line) printf '\n' >"$brief.tmp"; cat "$brief" >>"$brief.tmp"; want='HERDR-BRIEF role=' ;;
        *) sed "/^$bad_field/d" "$brief" >"$brief.tmp"; want=$bad_field ;;
    esac
    mv "$brief.tmp" "$brief"
    # A mention in prose cannot stand in for a header.
    printf 'The prose mentions %s but supplies no header.\n' "$bad_field" >>"$brief"
    run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err "$want"
    [ "$(wc -l <"$case_dir/err")" -eq 1 ] || fail 'expected one actionable stderr line'
    expect_no_calls
done

check_refused() {
    new_case "refused: $*"
    run_launch sh "$launch_sh" "$@"
    expect_rc 2
    expect_no_calls
}
for pair in fable-high:impl-x fable-xhigh:rev-x fable-high:arch-x sonnet-high:rev-x \
    sonnet-high:arch-x sonnet-high:orch-x sonnet-medium:impl-x astra-medium:impl-x \
    astra-high:mech-x astra-medium:glm-x glm-flash:impl-x opus-medium:glm-x opus-max:impl-x \
    opus-max:arch-x sol-medium:impl-x terra-medium:rev-x opus-medium:orch-x opus-xhigh:impl-x \
    opus-xhigh:rs-x opus-high:arch-x opus-medium:mech-x terra-medium:mech-x \
    haiku-gather:impl-x haiku-gather:mech-x haiku-gather:rs-x haiku-gather:rev-x \
    haiku-gather:arch-x haiku-gather:glm-x haiku-gather:orch-x \
    luna-gather:impl-x luna-gather:mech-x luna-gather:rs-x luna-gather:rev-x \
    luna-gather:arch-x luna-gather:glm-x luna-gather:orch-x glm-free:rs-x minimax-free:rev-x minimax-free:orch-x glm-free:arch-x glm-flash:rev-x; do
    check_refused "${pair%%:*}" "${pair#*:}" "$brief" "$report" 42
    expect_err 'does not launch as'
done
check_refused opus-medium impl-x "$root/missing-brief.md" "$report" 42
expect_err 'does not exist'
check_refused opus-medium impl-x "$brief" report.md 42
check_refused opus-medium impl-x "$brief" "$report" abc
check_refused opus-medium Impl-x "$brief" "$report" 42
check_refused opus-medium impl "$brief" "$report" 42
check_refused opus-medium impl-x.y "$brief" "$report" 42
check_refused opus-medium impl-abcdefghijklmnopqrstuvwxyz0123 "$brief" "$report" 42
check_refused opus-medium foo-x "$brief" "$report" 42
check_refused opus-low impl-x "$brief" "$report" 42
expect_err "unknown lane 'opus-low'"
check_refused opus-medium impl-x "$brief" "$report" 42 ''
check_refused opus-medium impl-x "$brief" "$report"
check_refused --workspace opus-medium impl-x "$brief" "$report" 42
check_refused --workspace '' opus-medium impl-x "$brief" "$report" 42
check_refused --workspace wB opus-medium impl-x "$brief" "$report" 42 wB:p1
check_refused --cwd "$root/no-such-dir" opus-medium impl-x "$brief" "$report" 42
check_refused --cwd "$root" opus-medium impl-x "$brief" "$report" 42 wA:p3
expect_err '--cwd applies only without PANE'

new_case '--lanes lists the source table in lane order without calls'
run_launch sh "$launch_sh" --lanes
expect_rc 0
expect_out "Lane table: $repo_dir/bin/../lanes.json
$(jq -r 'del(.deny) | to_entries | sort_by(.key)[] | [.key, .value.kind, .value.model, .value.effort, (.value.prefixes | join(" "))] | @tsv' "$repo_dir/lanes.json")"
expect_no_calls

new_case 'prefix refusal lists the source lanes that allow rev-'
run_launch sh "$launch_sh" opus-high rev-cart "$brief" "$report" 42
expect_rc 2
expect_err 'lane opus-high does not launch as rev- (allowed: impl rs orch; rev- lanes: astra-high fable-high opus-max opus-medium opus-xhigh sol-medium)'
expect_no_calls

new_case '--lanes usage rejects extra arguments without calls'
run_launch sh "$launch_sh" --lanes extra
expect_rc 2
expect_err 'usage: herdr-launch --lanes'
expect_no_calls

for cheap_lane in minimax-free glm-free glm-flash; do
    new_case "OpenCode --push for $cheap_lane refuses before any call"
    run_launch sh "$launch_sh" --push "$cheap_lane" glm-inventory "$brief" "$report" 42
    expect_rc 2
    expect_err '--push is not supported for opencode; use pane prompts'
    expect_no_calls
    run_launch sh "$launch_sh" --push "$cheap_lane" glm-inventory "$brief" "$report" 42 wA:p3
    expect_rc 2
    expect_err '--push is not supported for opencode; use pane prompts'
    expect_no_calls
done

# --push: each client's adapter, validated before any call.
new_case 'pi --push loads the push extension'
private_pi_lane
ext=$work/taskr-push.ts
: >"$ext"
run_launch HERDR_PUSH_PI_EXTENSION="$ext" sh "$launch_sh" --push glm glm-inventory "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7'
expect_calls "$(tab_calls glm-inventory)" \
    "$(tail_calls glm-inventory implementer wA:p7 wA:t7 pi glm-5.3-flash high " unset -f pi 2>/dev/null;$setup" \
        "|--provider|opencode|--model|glm-5.3-flash|--thinking|high|--approve|--extension|$ext")"

new_case 'pi --push without HERDR_PUSH_PI_EXTENSION dies before any call'
private_pi_lane
run_launch sh "$launch_sh" --push glm glm-inventory "$brief" "$report" 42
expect_rc 2
expect_err 'HERDR_PUSH_PI_EXTENSION'
expect_no_calls

new_case 'pi --push with a missing extension file dies before any call'
private_pi_lane
run_launch HERDR_PUSH_PI_EXTENSION="$work/missing.ts" sh "$launch_sh" --push glm glm-inventory "$brief" "$report" 42
expect_rc 2
expect_err 'HERDR_PUSH_PI_EXTENSION'
expect_no_calls

mcp_want='{"mcpServers":{"taskr":{"command":"taskr","args":["mcp","--channel"]}}}'
claude_push_flags() {
    printf '%s' "|$(claude_flags medium)|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr"
}
new_case 'claude --push writes a private config and adds both flags'
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config"
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "$(claude_push_flags)")"
printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'private config content is wrong'
[ "$(ls -l "$mcp_config" | cut -c 1-10)" = -rw------- ] || fail 'private config mode is not 0600'
[ ! -e "$work/.mcp.json" ] || fail 'cwd .mcp.json was written'
[ "$(find "${mcp_config%/*}" -type f | wc -l)" -eq 1 ] || fail 'temporary config was left behind'

new_case 'claude --push leaves a tracked project .mcp.json untouched'
project_mcp='{"mcpServers":{"project":{"command":"project-mcp"}}}'
printf '%s\n' "$project_mcp" >"$work/.mcp.json"
git -C "$work" init -q
git -C "$work" add .mcp.json
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config"
git -C "$work" diff --exit-code -- .mcp.json >/dev/null || fail 'tracked project config was changed'
printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'private config content is wrong'

new_case 'claude --push ignores a dangling project .mcp.json symlink'
ln -s "$work/absent.json" "$work/.mcp.json"
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
[ "$(readlink "$work/.mcp.json")" = "$work/absent.json" ] || fail 'project config symlink was changed'
[ ! -e "$work/absent.json" ] || fail 'the symlink target was created'

for existing in file directory symlink; do
    new_case "claude --push refuses an existing private config $existing"
    mkdir -p "${mcp_config%/*}"
    case $existing in
        file) printf '%s\n' "$mcp_want" >"$mcp_config" ;;
        directory) mkdir "$mcp_config" ;;
        symlink) ln -s "$work/absent.json" "$mcp_config" ;;
    esac
    run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
    expect_rc 1
    expect_err 'step 6 (push config path already exists) failed'
    expect_err "mcp_config=$mcp_config"
    grep -q '^herdr|agent' "$case_dir/calls" && fail 'agent start ran'
    case $existing in
        file) printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'existing config was changed' ;;
        directory) [ -z "$(find "$mcp_config" -type f -print)" ] || fail 'existing directory was written' ;;
        symlink) [ ! -e "$work/absent.json" ] || fail 'the symlink target was created' ;;
    esac
done

new_case 'claude --push refuses a config created just before rename'
mkdir "$case_dir/race-bin"
cat >"$case_dir/race-bin/mv" <<'EOF'
#!/bin/sh
for arg in "$@"; do destination=$arg; done
printf 'late config\n' >"$destination"
exec /bin/mv "$@"
EOF
chmod 0755 "$case_dir/race-bin/mv"
run_launch PATH="$case_dir/race-bin:$stub_dir:/usr/bin:/bin" sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (push config exists or could not be saved) failed'
[ "$(cat "$mcp_config")" = 'late config' ] || fail 'late config was overwritten'
[ "$(find "${mcp_config%/*}" -type f | wc -l)" -eq 1 ] || fail 'temporary config was left behind on failure'
grep -q '^herdr|agent' "$case_dir/calls" && fail 'agent start ran'

new_case 'claude --push answers the development-channel dialog'
STUB_START_OK=0 STUB_SCREEN='WARNING: Loading development channels
> 1. I am using this for local development'
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config"
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "$(claude_push_flags)" \
        | sed '$d')" \
    "herdr|pane|read|wA:p7|--source|visible|--lines|40" \
    "herdr|pane|send-keys|wA:p7|Enter" \
    "herdr|agent|wait|impl-cart|--until|idle|--timeout|90000" \
    "taskr|prompt|2468|--file|$brief"

new_case 'claude --push: a failed start without the dialog stops at step 6'
STUB_START_OK=0 STUB_SCREEN='Error: something else'
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (herdr agent start failed and the pane shows no development-channel dialog) failed'
grep -q '^herdr|pane|send-keys' "$case_dir/calls" && fail 'Enter was sent'
grep -q '^taskr|prompt' "$case_dir/calls" && fail 'prompt ran'

new_case 'claude --push: the dialog answered but the agent never idle stops at step 6'
STUB_START_OK=0 STUB_SCREEN='Loading development channels' STUB_FAIL='herdr agent wait'
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (the agent was not idle within 90 s after the development-channel dialog) failed'

new_case 'claude without --push: a failed start does not read the pane'
STUB_START_OK=0 STUB_SCREEN='Loading development channels'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (herdr agent start) failed'
grep -q '^herdr|pane|read' "$case_dir/calls" && fail 'the pane was read'
[ ! -e "$work/.mcp.json" ] || fail '.mcp.json was written'

codex_bin=$work/codex
printf '%s\n' '#!/bin/sh' >"$codex_bin"
chmod 0755 "$codex_bin"
sidecar="export TASKR_TASK=2468 TASKR_LAUNCH=3579 && i=0 && until taskr log 2468 --limit 1 | grep -q -F '\"native_home\":\"/'; do i=\$((i + 1)); if [ \$i -ge 360 ]; then echo 'herdr-launch: the session never ran taskr start; no push sidecar'; exit 1; fi; sleep 5; done && exec taskr push codex --codex-bin '$codex_bin'"
new_case 'codex --push starts the push sidecar in its own tab (step 8)'
printf '%s\n' 'Run taskr start first.' >>"$brief"
run_launch HERDR_PUSH_CODEX_BIN="$codex_bin" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 push_tab=wA:t8 push_pane=wA:p8'
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 codex gpt-6.1-sol high "$setup" \
        "$(codex_flags gpt-6.1-sol high "\"$tcwd\"={trust_level=\"trusted\"}")")" \
    "herdr|tab|create|--workspace|wA|--cwd|$tcwd|--label|impl-cart-push|--env|FLEET_ROLE=worker|--env|DISABLE_AUTO_UPDATE=true|--no-focus" \
    "herdr|pane|run|wA:p8|$sidecar"
grep -q -F -- "taskr push codex --codex-bin '$codex_bin'" "$case_dir/calls" || fail 'no taskr push codex --codex-bin'

# The sidecar text, run by real shells against a stub taskr whose log line
# carries a synthetic native_home, polls once and execs taskr push.
mkdir -p "$case_dir/bin"
cat >"$case_dir/bin/taskr" <<'EOF'
#!/bin/sh
printf 'taskr %s task=%s launch=%s\n' "$*" "$TASKR_TASK" "$TASKR_LAUNCH" >>"$SIDE_CALLS"
[ "$1" != log ] || printf '%s\n' 'j1 {"account":"example","native_home":"/synthetic/home","provider":"codex","t":2468}'
EOF
chmod 0755 "$case_dir/bin/taskr"
# Global zshenv can replace inherited PATH even with -f; set it after startup.
# A broken stub/loop must fail promptly and reap its own group, never poll for 30 min.
fixture_python=$(command -v python3)
run_sidecar() {
    side_path=$1
    shift
    side_shell=$(command -v "$1") || return 127
    shift
    "$fixture_python" -I - "$side_path" "$case_dir/side" "$sidecar" "$home" "$side_shell" "$@" <<'SIDECAR_PY'
import os, signal, subprocess, sys

path, calls, code, home = sys.argv[1:5]
expected = path.split(os.pathsep)[0] + '/taskr'
prefix = ('export PATH="$1" SIDE_CALLS="$2" && hash -r && '
          '[ "$(command -v taskr)" = "$3" ] || '
          '{ echo "sidecar fixture: taskr is not the selected stub" >&2; exit 125; }; ')
child = subprocess.Popen([*sys.argv[5:], prefix + code, 'sidecar-fixture', path, calls, expected],
                         env={'HOME': home, 'PATH': path, 'SIDE_CALLS': calls},
                         start_new_session=True)
try:
    try:
        result = child.wait(timeout=20)
    except subprocess.TimeoutExpired:
        print('sidecar fixture: 20s deadline exceeded; cleaning owned process group', file=sys.stderr)
        result = 124
finally:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait(timeout=3)
sys.exit(result)
SIDECAR_PY
}
for sh_cmd in 'sh -c' 'bash --noprofile --norc -c' 'zsh -f -c' 'dash -c'; do
    command -v "${sh_cmd%% *}" >/dev/null || continue
    : >"$case_dir/side"
    run_sidecar "$case_dir/bin:/usr/bin:/bin" $sh_cmd || fail "${sh_cmd%% *}: sidecar exited $?"
    want="taskr log 2468 --limit 1 task=2468 launch=3579
taskr push codex --codex-bin $codex_bin task=2468 launch=3579"
    [ "$(cat "$case_dir/side")" = "$want" ] || fail "${sh_cmd%% *}: sidecar calls: $(cat "$case_dir/side")"
done

# A session that never registers: 360 polls, 359 sleeps (stubbed), one
# timeout line, exit 1, and no taskr push.
mkdir -p "$case_dir/tbin"
cat >"$case_dir/tbin/taskr" <<'EOF'
#!/bin/sh
printf 'taskr %s\n' "$*" >>"$SIDE_CALLS"
[ "$1" != log ] || printf '%s\n' 'j1 {"provider":"codex","t":2468}'
EOF
printf '%s\n' '#!/bin/sh' 'printf "sleep %s\\n" "$*" >>"$SIDE_CALLS"' >"$case_dir/tbin/sleep"
chmod 0755 "$case_dir/tbin/taskr" "$case_dir/tbin/sleep"
for sh_cmd in 'sh -c' 'bash --noprofile --norc -c' 'zsh -f -c' 'dash -c'; do
    command -v "${sh_cmd%% *}" >/dev/null || continue
    : >"$case_dir/side"
    side_rc=0
    side_out=$(run_sidecar "$case_dir/tbin:/usr/bin:/bin" $sh_cmd) || side_rc=$?
    n=${sh_cmd%% *}
    [ "$side_rc" -eq 1 ] || fail "$n: never-registered sidecar exited $side_rc"
    [ "$side_out" = 'herdr-launch: the session never ran taskr start; no push sidecar' ] || fail "$n: output: $side_out"
    [ "$(grep -c '^taskr log 2468 --limit 1$' "$case_dir/side")" -eq 360 ] || fail "$n: polls != 360"
    [ "$(grep -c '^sleep 5$' "$case_dir/side")" -eq 359 ] || fail "$n: sleeps != 359"
    [ "$(wc -l <"$case_dir/side")" -eq 719 ] || fail "$n: unexpected calls"
    ! grep -q '^taskr push' "$case_dir/side" || fail "$n: taskr push ran"
done

new_case 'codex --push: a failed sidecar tab names the ids so far'
printf '%s\n' 'taskr start' >>"$brief"
STUB_PUSH_FAIL=tab
run_launch HERDR_PUSH_CODEX_BIN="$codex_bin" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 8 (herdr tab create) failed; so far: workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7'
case $err in *push_tab*) fail "a push tab was named: $err" ;; esac
grep -q '^taskr|prompt' "$case_dir/calls" || fail 'step 7 did not run first'

new_case 'codex --push: a failed sidecar run names the push tab'
printf '%s\n' 'taskr start' >>"$brief"
STUB_PUSH_FAIL=run
run_launch HERDR_PUSH_CODEX_BIN="$codex_bin" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 8 (herdr pane run push sidecar) failed; so far: workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7 push_tab=wA:t8'
[ -z "$out" ] || fail "stdout not empty: $out"

new_case 'codex --push without taskr start in the brief dies before any call'
run_launch HERDR_PUSH_CODEX_BIN="$codex_bin" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
expect_rc 2
expect_err 'taskr start'
expect_no_calls

check_bad_bin() {
    new_case "codex --push refuses HERDR_PUSH_CODEX_BIN $1"
    printf '%s\n' 'taskr start' >>"$brief"
    run_launch HERDR_PUSH_CODEX_BIN="$2" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err 'HERDR_PUSH_CODEX_BIN'
    expect_no_calls
}
check_bad_bin 'unset' ''
check_bad_bin 'that is relative' codex
check_bad_bin 'that is not executable' "$root"
check_bad_bin "with a quote" "$root/it's"

new_case '--push with PANE is accepted'
private_pi_lane
ext=$work/taskr-push.ts
: >"$ext"
run_launch HERDR_PUSH_PI_EXTENSION="$ext" sh "$launch_sh" --push glm glm-inventory "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3'
grep -q -F -- "|--extension|$ext" "$case_dir/calls" || fail 'no --extension'

new_case 'opus-max launches a rev- name' reviewer
run_launch sh "$launch_sh" opus-max rev-sec "$brief" "$report" 42
expect_rc 0
grep -q -F -- '|--effort|max|' "$case_dir/calls" || fail 'no --effort max'

new_case 'private lane table replaces public mappings'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"synthetic":{"kind":"codex","model":"synthetic-model","effort":"high","prefixes":["impl"]}}' >"$home/.config/herdr-fleet/lanes.json"
run_launch sh "$launch_sh" synthetic impl-cart "$brief" "$report" 42
expect_rc 0
grep -q -F '|--model|synthetic-model|--effort|high' "$case_dir/calls" || fail 'private taskr launch metadata missing'
grep -q -F '|-m|synthetic-model|-c|model_reasoning_effort="high"' "$case_dir/calls" || fail 'private native argv missing'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err "unknown lane 'opus-medium'"

new_case '--lanes and prefix alternatives use only the private replacement'
mkdir -p "$home/.config/herdr-fleet"
cat >"$home/.config/herdr-fleet/lanes.json" <<'EOF'
{
  "zeta": {"kind":"codex","model":"synthetic-z","effort":"high","prefixes":["rev","rs"]},
  "worker": {"kind":"claude","model":"synthetic-worker","effort":"medium","prefixes":["impl"]},
  "alpha": {"kind":"pi","model":"synthetic-a","effort":"low","prefixes":["rev"]}
}
EOF
run_launch sh "$launch_sh" --lanes
expect_rc 0
listing=$(printf 'alpha\tpi\tsynthetic-a\tlow\trev\nworker\tclaude\tsynthetic-worker\tmedium\timpl\nzeta\tcodex\tsynthetic-z\thigh\trev rs')
expect_out "Lane table: $home/.config/herdr-fleet/lanes.json
$listing"
expect_no_calls
run_launch sh "$launch_sh" worker rev-cart "$brief" "$report" 42
expect_rc 2
expect_err 'lane worker does not launch as rev- (allowed: impl; rev- lanes: alpha zeta)'
expect_no_calls

# A copied launcher has no source placement.sh and reads the installed table.
mkdir -p "$home/.local/bin" "$home/.local/share/herdr-setup"
cp "$launch_sh" "$repo_dir/bin/herdr-lanes.sh" "$home/.local/bin/"
mv "$home/.config/herdr-fleet/lanes.json" "$home/.local/share/herdr-setup/lanes.json"
run_launch sh "$home/.local/bin/herdr-launch" --lanes
expect_rc 0
expect_out "Lane table: $home/.local/share/herdr-setup/lanes.json
$listing"
expect_no_calls

for bad_table in '{}' '[]' \
    '{"custom":{"kind":"opencode","model":"missing-provider","effort":"high","prefixes":["glm"]}}' \
    '{"custom":{"kind":"opencode","model":"opencode/a/b","effort":"high","prefixes":["glm"]}}' \
    '{"custom":{"kind":"claude","model":"opencode/synthetic","effort":"high","prefixes":["impl"]}}' \
    '{"custom":{"kind":"shell","model":"safe","effort":"high","prefixes":["impl"]}}' \
    '{"custom":{"kind":"claude","model":"$(touch injected)","effort":"high","prefixes":["impl"]}}' \
    '{"custom":{"kind":"claude","model":"safe","effort":"wrong","prefixes":["impl"]}}' \
    '{"custom":{"kind":"claude","model":"safe","effort":"high","prefixes":["unknown"]}}' \
    '{"custom":{"kind":"claude","model":"safe","effort":"high"}}'; do
    new_case 'invalid private lane table fails before control-plane calls'
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' "$bad_table" >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err 'invalid lane table'
    expect_no_calls
    [ ! -e "$work/injected" ] || fail 'lane model was evaluated'
    run_launch sh "$launch_sh" --lanes
    expect_rc 2
    expect_err 'invalid lane table'
    expect_out ''
    expect_no_calls
done

for first_model in 'bad model' good-model; do
    new_case 'two-document lane table fails before control-plane calls'
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' "{\"custom\":{\"kind\":\"claude\",\"model\":\"$first_model\",\"effort\":\"medium\",\"prefixes\":[\"impl\"]}}" \
        '{"custom":{"kind":"claude","model":"good-model","effort":"medium","prefixes":["impl"]}}' >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_no_calls
    run_launch sh "$launch_sh" --lanes
    expect_rc 2
    expect_err 'invalid lane table'
    expect_out ''
    expect_no_calls
done

new_case '--check reports a missing CLI without control-plane calls'
run_launch sh "$launch_sh" --check opus-medium
expect_rc 1
expect_out 'opus-medium: missing CLI: claude'
expect_no_calls

new_case '--check reports each requested lane and hides version output'
mkdir -p "$work/bin"
for cli in claude codex opencode; do
    printf '%s\n' '#!/bin/sh' '[ "$1" = --version ] || exit 99' 'echo synthetic-version-text' >"$work/bin/$cli"
    chmod 0755 "$work/bin/$cli"
done
check_path=$work/bin:$stub_dir:/usr/bin:/bin
run_launch PATH="$check_path" sh "$launch_sh" --check opus-medium sol-high glm-flash
expect_rc 0
expect_out 'opus-medium: ok
sol-high: ok
glm-flash: ok'
expect_no_calls
run_launch PATH="$check_path" sh "$launch_sh" --check
expect_rc 0
[ "$(printf '%s\n' "$out" | wc -l)" -eq "$(jq 'del(.deny) | length' "$repo_dir/lanes.json")" ] || fail 'not every lane checked'
printf '%s\n' "$out" | grep -v ': ok$' >/dev/null && fail 'unexpected check output'
expect_no_calls
run_launch PATH="$check_path" sh "$launch_sh" --check unknown opus-medium
expect_rc 1
expect_out 'unknown: unknown lane
opus-medium: ok'
expect_no_calls

printf '%s\n' '#!/bin/sh' 'echo synthetic-error >&2' 'exit 1' >"$work/bin/claude"
run_launch PATH="$check_path" sh "$launch_sh" --check opus-medium
expect_rc 1
expect_out 'opus-medium: claude --version failed'
expect_no_calls

new_case '--check exercises the Codator wrapper when launch would use it'
mkdir -p "$home/.local/bin" "$work/bin"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$work/bin/codex"
printf '%s\n' '#!/bin/sh' '[ "$*" = "codex --version" ] || exit 99' 'exit 1' >"$home/.local/bin/codator"
chmod 0755 "$work/bin/codex" "$home/.local/bin/codator"
run_launch PATH="$work/bin:$stub_dir:/usr/bin:/bin" sh "$launch_sh" --check sol-high
expect_rc 1
expect_out 'sol-high: Codator codex --version failed'
expect_no_calls

new_case '--push --check checks Codex prerequisites and refuses OpenCode'
mkdir -p "$work/bin"
for cli in codex opencode; do
    printf '%s\n' '#!/bin/sh' 'exit 0' >"$work/bin/$cli"
    chmod 0755 "$work/bin/$cli"
done
check_path=$work/bin:$stub_dir:/usr/bin:/bin
run_launch PATH="$check_path" sh "$launch_sh" --push --check sol-high glm-flash
expect_rc 1
expect_out 'sol-high: missing absolute HERDR_PUSH_CODEX_BIN
glm-flash: --push is not supported for opencode; use pane prompts'
expect_no_calls
run_launch PATH="$check_path" HERDR_PUSH_CODEX_BIN="$work/bin/codex" sh "$launch_sh" --push --check sol-high glm-flash
expect_rc 1
expect_out 'sol-high: ok
glm-flash: --push is not supported for opencode; use pane prompts'
expect_no_calls
run_launch PATH="$check_path" HERDR_PUSH_CODEX_BIN="$work/absent" sh "$launch_sh" --push --check sol-high
expect_rc 1
expect_out 'sol-high: HERDR_PUSH_CODEX_BIN --version failed'
expect_no_calls

for field in model provider kind; do
    new_case "private deny $field refuses its added lane and marks the listing"
    mkdir -p "$home/.config/herdr-fleet"
    jq -n --arg field "$field" '{deny:{($field):["^synthetic", "^pi$"]},
        custom:{kind:"pi",provider:"synthetic-provider",model:"synthetic-model",effort:"high",prefixes:["impl"]}}' >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err "lane custom denied: $field"
    expect_no_calls
    run_launch sh "$launch_sh" --lanes
    expect_rc 0
    printf '%s\n' "$out" | grep -q "denied: $field" || fail 'denied lane not marked'
    expect_no_calls
    run_launch sh "$launch_sh" --check custom
    expect_rc 1
    expect_out "custom: denied: $field"
    expect_no_calls
done

for explicit_provider in false true; do
    new_case 'OpenCode provider deny uses the model prefix'
    mkdir -p "$home/.config/herdr-fleet"
    jq -n --argjson explicit "$explicit_provider" '{deny:{provider:["^opencode-go$"]},
        custom:{kind:"opencode",model:"opencode-go/x",effort:"high",prefixes:["glm"]}}
        | if $explicit then .custom.provider = "opencode-go" else . end' >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom glm-cart "$brief" "$report" 42
    expect_rc 2
    expect_err 'lane custom denied: provider'
    expect_no_calls
    run_launch sh "$launch_sh" --lanes
    expect_rc 0
    printf '%s\n' "$out" | grep -q 'denied: provider' || fail 'denied OpenCode lane not marked'
    expect_no_calls
    run_launch sh "$launch_sh" --check custom
    expect_rc 1
    expect_out 'custom: denied: provider'
    expect_no_calls
done

new_case 'OpenCode rejects a provider that differs from the model prefix'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"custom":{"kind":"opencode","provider":"opencode","model":"opencode-go/x","effort":"high","prefixes":["glm"]}}' >"$home/.config/herdr-fleet/lanes.json"
run_launch sh "$launch_sh" custom glm-cart "$brief" "$report" 42
expect_rc 2
expect_err 'opencode provider must match model prefix: opencode-go'
expect_no_calls
run_launch sh "$launch_sh" --lanes
expect_rc 2
expect_err 'opencode provider must match model prefix: opencode-go'
expect_out ''
expect_no_calls

new_case 'nonmatching deny preserves a private pi provider argv'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"deny":{"model":["^other$"]},"custom":{"kind":"pi","provider":"synthetic-provider","model":"synthetic-model","effort":"high","prefixes":["impl"]}}' >"$home/.config/herdr-fleet/lanes.json"
run_launch sh "$launch_sh" custom impl-cart "$brief" "$report" 42
expect_rc 0
grep -q -F '|--provider|synthetic-provider|--model|synthetic-model' "$case_dir/calls" || fail 'pi provider not used'

new_case 'owner-style deny exempts only gather-only Haiku and Luna' researcher
mkdir -p "$home/.config/herdr-fleet" "$work/bin"
for cli in claude codex; do
    printf '%s\n' '#!/bin/sh' '[ "$1" = --version ] || exit 99' 'exit 0' >"$work/bin/$cli"
    chmod 0755 "$work/bin/$cli"
done
jq '.deny = {model:["(?i)^(claude-)?haiku", "(?i)luna", "(?i)deepseek"],
        except:{gather:{model:["(?i)^(claude-)?haiku", "(?i)luna"]}}}
    | ."haiku-rs" = (."haiku-gather" | .prefixes = ["rs"])
    | ."haiku-mixed" = (."haiku-gather" | .prefixes = ["gather", "rs"])
    | ."deepseek-gather" = (."haiku-gather" | .model = "deepseek-synthetic")' \
    "$repo_dir/lanes.json" >"$home/.config/herdr-fleet/lanes.json"
run_launch PATH="$work/bin:$stub_dir:/usr/bin:/bin" sh "$launch_sh" --check \
    haiku-gather luna-gather haiku-rs haiku-mixed deepseek-gather
expect_rc 1
expect_out 'haiku-gather: ok
luna-gather: ok
haiku-rs: denied: model
haiku-mixed: denied: model
deepseek-gather: denied: model'
expect_no_calls
run_launch sh "$launch_sh" --lanes
expect_rc 0
listing=$(jq -r 'del(.deny) | to_entries | sort_by(.key)[] |
    [.key, .value.kind, .value.model, .value.effort, (.value.prefixes | join(" "))] +
    (if (.key | IN("haiku-rs", "haiku-mixed", "deepseek-gather")) then ["denied: model"] else [] end) | @tsv' \
    "$home/.config/herdr-fleet/lanes.json")
expect_out "Lane table: $home/.config/herdr-fleet/lanes.json
$listing"
expect_no_calls
run_launch sh "$launch_sh" haiku-gather rs-x "$brief" "$report" 42
expect_rc 2
expect_err 'does not launch as rs-'
expect_no_calls
run_launch sh "$launch_sh" haiku-gather gather-x "$brief" "$report" 42
expect_rc 0
grep -q -F '|--model|claude-haiku-5-5|--effort|medium' "$case_dir/calls" || fail 'Haiku gather argv missing'

for field in model provider kind; do
    for coverage in none gather both; do
        new_case "deny exception for $field needs every prefix: $coverage" researcher
        mkdir -p "$home/.config/herdr-fleet"
        jq -n --arg field "$field" --arg coverage "$coverage" '
            {deny:{($field):[".*"], except:{gather:{($field):[".*"]}}},
             custom:{kind:"pi",model:"synthetic",effort:"high",prefixes:["gather","rs"]}}
            | if $coverage == "none" then .deny.except.gather[$field] = ["^other$"]
              elif $coverage == "both" then .deny.except.rs = .deny.except.gather else . end' \
            >"$home/.config/herdr-fleet/lanes.json"
        run_launch sh "$launch_sh" custom gather-x "$brief" "$report" 42
        if [ "$coverage" = both ]; then
            expect_rc 0
            grep -q '^herdr|agent|start|' "$case_dir/calls" || fail 'no agent start'
        else
            expect_rc 2
            expect_err "lane custom denied: $field"
            expect_no_calls
        fi
    done
done

for rules in \
    '{"except":{"gather":{"model":["^other$"]}}}' \
    '{"model":["^other$"],"except":{"gather":{"model":["^other$"]}}}' \
    '{"model":[".*"],"provider":[".*"],"except":{"gather":{"model":[".*"]}}}'; do
    new_case 'exceptions act only on a matching denial of the same field' researcher
    mkdir -p "$home/.config/herdr-fleet"
    printf '{"deny":%s,"custom":{"kind":"pi","model":"synthetic","effort":"high","prefixes":["gather"]}}\n' \
        "$rules" >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom gather-x "$brief" "$report" 42
    case $rules in
        *provider*) expect_rc 2; expect_err 'lane custom denied: provider'; expect_no_calls ;;
        *) expect_rc 0 ;;
    esac
done

for bad_except in 'null' 'false' '[]' '"gather"' '{"unknown":{}}' \
    '{"gather":null}' '{"gather":false}' '{"gather":[]}' '{"gather":"bad"}' \
    '{"gather":{"except":{}}}' '{"gather":{"unknown":[]}}' \
    '{"gather":{"model":"bad"}}' '{"gather":{"model":["["]}}' \
    '{"gather":{"provider":[7]}}' '{"gather":{"kind":[""]}}'; do
    new_case 'invalid deny exception fails closed before control-plane calls'
    mkdir -p "$home/.config/herdr-fleet"
    printf '{"deny":{"except":%s},"custom":{"kind":"claude","model":"synthetic","effort":"high","prefixes":["gather"]}}\n' \
        "$bad_except" >"$home/.config/herdr-fleet/lanes.json"
    for mode in launch list check; do
        case $mode in
            launch) run_launch sh "$launch_sh" custom gather-x "$brief" "$report" 42 ;;
            list) run_launch sh "$launch_sh" --lanes ;;
            check) run_launch sh "$launch_sh" --check custom ;;
        esac
        expect_rc 2
        expect_err 'invalid lane table'
        expect_out ''
        expect_no_calls
    done
done

for bad_deny in 'null' 'false' '[]' '{"model":"bad"}' '{"model":["["]}' '{"model":[7]}' '{"model":[""]}' '{"unknown":[]}'; do
    new_case 'invalid deny fails closed before control-plane calls'
    mkdir -p "$home/.config/herdr-fleet"
    printf '{"deny":%s,"custom":{"kind":"claude","model":"synthetic","effort":"high","prefixes":["impl"]}}\n' "$bad_deny" >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" custom impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err 'invalid lane table'
    expect_no_calls
done

fi

reserved_snapshot() {
    jq -n --arg cwd "$tcwd" '{campaign:42,generation:7,parked:false,configuration:{enabled:true},work:[{revision:1,current_run:9,revisions:[{revision:1,contract:{checkout:{cwd:$cwd}}}],runs:[{run_id:9,round:1,state:"active",assignments:[{task_id:2468,launch_id:3579,round:1,state:"starting",stage:"executor",dispatch:{workspace:"wA",tab:"wA:t5",pane:"wA:p5"},lane:{name:"opus-medium",provider:"claude",model:"claude-opus-5-5",effort:"medium"}}]}]}]}' >"$STUB_RESERVED"
}

new_case 'reserved long PATH sources exact private setup before Step5 cleanup'
reserved_snapshot
long_tail=$(python3 -B - <<'PY'
print(':'.join('/synthetic/path ' + str(i) + "'literal$arg" for i in range(400)))
PY
)
run_path=$run_path:$long_tail
run_tmpdir="$case_dir/setup ' files"
mkdir "$run_tmpdir" "$case_dir/candidate-bin"
ln -s "$stub_dir/taskr" "$case_dir/candidate-bin/taskr"
ln -s "$case_dir/candidate-bin" "$case_dir/candidate-alias"
candidate_bin=$(CDPATH= cd "$case_dir/candidate-bin" && pwd -P)
mkdir -p "$home/.local/bin"
cat >"$home/.local/bin/codator" <<'EOF'
#!/bin/sh
printf '<%s>' "$@" >>"$STUB_CALLS.functions"
printf '\n' >>"$STUB_CALLS.functions"
EOF
chmod 0755 "$home/.local/bin/codator"
STUB_SOURCE_PROBE=1
run_launch env TASKR_DB="$case_dir/scratch ' ledger.sqlite" TASKR_WORK_EXECUTABLE="$case_dir/candidate-alias/taskr" \
    sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 0
python3 -B - "$case_dir" "$candidate_bin:$run_path" "$work" "$case_dir/scratch ' ledger.sqlite" <<'PY' || fail 'long PATH setup content/transport/cleanup'
import pathlib, shlex, sys
case, expected_path, checkout = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3])
rows = (case / 'calls').read_text().splitlines()
commands = [r.split('|', 4)[4] for r in rows if r.startswith('herdr|pane|run|')]
source = [c for c in commands if c.startswith('. ')]
assert len(source) == 1 and len(source[0].encode()) <= 512
parts = shlex.split(source[0])
assert len(parts) == 2 and parts[0] == '.'
path = pathlib.Path(parts[1])
assert checkout not in path.resolve().parents
assert not path.exists() and not path.parent.exists(), 'confirmed setup was retained'
data = (case / 'calls.setup').read_text()
assert len(data.encode()) > 10000 and "literal$arg" in data and 'codex()' in data and 'claude()' in data
assert (case / 'calls.path').read_text() == expected_path + '\n'
assert (case / 'calls.db').read_text() == sys.argv[4] + '\n'
assert (case / 'calls.env').read_text() == 'herdr-launch-env=2468/3579/worker/3579\n'
assert (case / 'calls.functions').read_text() == '<codex><two words><literal$arg>\n<claude><two words><literal$arg>\n'
assert (case / 'calls.setup-dir-mode').read_text().strip() == 'drwx------'
assert (case / 'calls.setup-file-mode').read_text().strip() == '-rw-------'
assert len(commands) == 2 and commands[1].startswith('echo "herdr-launch-env=')
PY

for delivery in source-unknown receipt-missing; do
    new_case "reserved $delivery keeps setup for diagnosis and never retries"
    reserved_snapshot
    case $delivery in source-unknown) STUB_FAIL='herdr pane run' ;; receipt-missing) STUB_ENV_OK=0 ;; esac
    run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
    expect_rc 1
    expect_err 'setup artifact='
    python3 -B - "$case_dir" <<'PY' || fail 'uncertain setup retention/no-repeat'
import pathlib, shlex, stat, sys
case = pathlib.Path(sys.argv[1])
rows = (case / 'calls').read_text().splitlines()
source = [r.split('|', 4)[4] for r in rows if r.startswith('herdr|pane|run|') and r.split('|', 4)[4].startswith('. ')]
assert len(source) == 1
path = pathlib.Path(shlex.split(source[0])[1])
assert path.is_file() and stat.S_IMODE(path.stat().st_mode) == 0o600
assert stat.S_IMODE(path.parent.stat().st_mode) == 0o700
assert str(path) in (case / 'err').read_text()
assert not any(r.startswith(('herdr|agent|start|', 'taskr|prompt|')) for r in rows)
PY
done

new_case 'reserved setup refuses a temporary directory inside the checkout'
reserved_snapshot
run_tmpdir=$work/tmp
mkdir "$run_tmpdir"
run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 1
expect_err 'private setup must be outside checkout'
if grep -F 'herdr|pane|run|' "$case_dir/calls" >/dev/null; then fail 'checkout-local setup reached pane'; fi

for identity_case in success refused changed-native; do
    new_case "reserved Codex initializer $identity_case precedes observe and work"
    reserved_snapshot
    jq '.work[0].runs[0].assignments[0].lane = {name:"sol-high",provider:"codex",model:"gpt-6.1-sol",effort:"high"}' \
        "$STUB_RESERVED" >"$case_dir/updated"
    mv "$case_dir/updated" "$STUB_RESERVED"
    mkdir "$case_dir/launch-bin"
    cp "$launch_sh" "$repo_dir/bin/herdr-lanes.sh" "$case_dir/launch-bin/"
    mkdir -p "$home/.local/share/herdr-setup"
    cp "$repo_dir/lanes.json" "$home/.local/share/herdr-setup/lanes.json"
    cat >"$case_dir/launch-bin/herdr-codex-identity" <<'EOF'
#!/bin/sh
line=identity
for arg in "$@"; do line="$line|$arg"; done
printf '%s\n' "$line|$TASKR_WORK_EXECUTABLE" >>"$STUB_CALLS"
[ "$STUB_INIT_RC" = 0 ] || exit "$STUB_INIT_RC"
printf '{"native_session":"%s","provenance_path":"/synthetic/provenance.json"}\n' "$STUB_INIT_NATIVE"
EOF
    chmod 0755 "$case_dir/launch-bin/herdr-codex-identity"
    case $identity_case in refused) STUB_INIT_RC=1 ;; changed-native) STUB_INIT_NATIVE=other ;; esac
    run_launch sh "$case_dir/launch-bin/herdr-launch" --reserved 2468 3579 sol-high impl-w3579 "$brief" "$report" 42 wA:p5
    grep -F "identity|2468|3579|42|7|impl-w3579|wA:p5|$work|$stub_dir/taskr" "$case_dir/calls" >/dev/null \
        || fail 'Codex initializer did not receive the exact reserved context'
    if [ "$identity_case" = success ]; then
        expect_rc 0
        python3 - "$case_dir/calls" <<'PY' || fail 'Codex initializer ordering'
import sys
rows = open(sys.argv[1]).read().splitlines()
def index(prefix):
    return next(i for i, row in enumerate(rows) if row.startswith(prefix))
assert index('herdr|agent|start|') < index('identity|') < index('taskr|--json|work|observe|') < index('taskr|prompt|')
PY
    else
        expect_rc 1
        if grep -E '^taskr\|(prompt\||--json\|work\|observe\|)' "$case_dir/calls" >/dev/null; then
            fail 'failed/mismatched initialization reached observation or work prompt'
        fi
    fi
done

new_case 'reserved launch skips new and launch, preserving exports/start/prompt'
reserved_snapshot
run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p5'
if grep -E '^taskr\|(new|launch)\|' "$case_dir/calls" >/dev/null; then fail 'reserved mode created another task or launch'; fi
grep -F 'herdr|agent|start|impl-cart|' "$case_dir/calls" >/dev/null || fail 'reserved mode did not start'
grep -F "taskr|prompt|2468|--file|$brief" "$case_dir/calls" >/dev/null || fail 'reserved brief not captured'
grep -F 'TASKR_LAUNCH=3579 DISABLE_AUTO_UPDATE=true' "$case_dir/calls.setup" >/dev/null || fail 'reserved identity/update environment missing'
grep -F 'unset TASKR_DB && export PATH=' "$case_dir/calls.setup" >/dev/null || fail 'reserved default context did not clear stale ledger override'
python3 - "$case_dir/calls" <<'PY' || fail 'reserved observation identity/order mismatch'
import json, sys
rows = open(sys.argv[1]).read().splitlines()
start = next(i for i, row in enumerate(rows) if row.startswith('herdr|agent|start|impl-cart|'))
observe = next(i for i, row in enumerate(rows) if row.startswith('taskr|--json|work|observe|3579|--as|42|--generation|7|--data|'))
prompt = next(i for i, row in enumerate(rows) if row.startswith('taskr|prompt|2468|'))
assert start < observe < prompt
data = json.loads(rows[observe].split('|--data|', 1)[1])
assert data['pane'] == 'wA:p5' and data['native_session'] == 'synthetic-native' and data['observed_at']
PY

for fault in missing wrong-pane wrong-name duplicate rejected-observe unconfirmed-observe; do
    new_case "reserved $fault withholds prompt after native start"
    reserved_snapshot
    case $fault in
        missing) STUB_NATIVE='{"result":{"agents":[{"name":"impl-cart","pane_id":"wA:p5","agent_session":null}]}}' ;;
        wrong-pane) STUB_NATIVE='{"result":{"agents":[{"name":"impl-cart","pane_id":"wA:p9","agent_session":{"value":"native"}}]}}' ;;
        wrong-name) STUB_NATIVE='{"result":{"agents":[{"name":"replacement","pane_id":"wA:p5","agent_session":{"value":"native"}}]}}' ;;
        duplicate) STUB_NATIVE='{"result":{"agents":[{"name":"impl-cart","pane_id":"wA:p5","agent_session":{"value":"native"}},{"name":"replacement","pane_id":"wA:p5","agent_session":{"value":"other"}}]}}' ;;
        rejected-observe) STUB_FAIL='taskr work observe' ;;
        unconfirmed-observe) STUB_OBSERVE='qd1 synthetic-request' ;;
    esac
    run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
    expect_rc 1
    if grep -F 'taskr|prompt|' "$case_dir/calls" >/dev/null; then fail 'unobserved native identity received a brief'; fi
    case $fault in rejected-observe|unconfirmed-observe) ;; *)
        if grep -F 'taskr|--json|work|observe|' "$case_dir/calls" >/dev/null; then fail 'invalid native identity reached observe'; fi ;; esac
done

new_case 'reserved scratch ledger and selected candidate CLI propagate to the shell'
reserved_snapshot
# A distinct symlinked candidate proves both CLI selection and physical PATH propagation.
mkdir -p "$case_dir/candidate-bin"
ln -s "$stub_dir/taskr" "$case_dir/candidate-bin/taskr"
ln -s "$case_dir/candidate-bin" "$case_dir/candidate-alias"
candidate_bin=$(CDPATH= cd -- "$case_dir/candidate-bin" && pwd -P)
run_launch env TASKR_DB="$case_dir/scratch.sqlite" TASKR_WORK_EXECUTABLE="$case_dir/candidate-alias/taskr" \
    sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 0
grep -F "export TASKR_DB='$case_dir/scratch.sqlite'" "$case_dir/calls.setup" >/dev/null || fail 'scratch ledger missing from reserved child environment'
grep -F "export PATH='$candidate_bin:" "$case_dir/calls.setup" >/dev/null || fail 'candidate CLI directory missing from reserved child PATH'

for alias_kind in symlink dot case; do
    new_case "reserved $alias_kind checkout spelling refuses before step2"
    case $alias_kind in
        symlink) ln -s "$work" "$case_dir/work-alias"; tcwd=$case_dir/work-alias ;;
        dot) tcwd=$work/. ;;
        case) tcwd=$case_dir/WORK; if [ ! -d "$tcwd" ]; then printf '  SKIP (case-sensitive filesystem)\n'; continue; fi ;;
    esac
    STUB_PANE_CWD=$tcwd
    reserved_snapshot
    run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
    expect_rc 1
    expect_err 'canonical physical spelling'
    if grep -E '^(taskr\||herdr\|(agent\|start|pane\|(run|send-keys)|tab\|(create|rename))\|)' "$case_dir/calls" >/dev/null; then
        fail 'noncanonical reserved checkout reached step2 or process effects'
    fi
done

for field in task_id launch_id round pane cwd model effort; do
    new_case "reserved mismatch $field fails before agent start"
    reserved_snapshot
    case $field in
        cwd) filter='.work[0].revisions[0].contract.checkout.cwd = "/other"' ;;
        pane) filter='.work[0].runs[0].assignments[0].dispatch.pane = "wA:p9"' ;;
        model|effort) filter=".work[0].runs[0].assignments[0].lane.$field = \"drift\"" ;;
        *) filter=".work[0].runs[0].assignments[0].$field = 999" ;;
    esac
    jq "$filter" "$STUB_RESERVED" >"$STUB_RESERVED.tmp"
    mv "$STUB_RESERVED.tmp" "$STUB_RESERVED"
    run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
    expect_rc 1
    if grep -E '^herdr\|(agent\|start|pane\|run)\|' "$case_dir/calls" >/dev/null; then fail 'mismatched reserved identity reached effects'; fi
done
new_case 'reserved unavailable server never calls Herdr or taskr'
reserved_snapshot
STUB_SERVER_RC=1
run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 1
expect_no_calls

new_case 'reserved requires an explicit pane'
run_launch sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_no_calls

printf '\n%s cases, %s failures\n' "$case_count" "$failures"
[ "$failures" -eq 0 ]
