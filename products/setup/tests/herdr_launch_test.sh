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
# The agent temp folder (README.md "Temp files") goes under a short fake base,
# never /var/tmp: the folder must stay within 24 bytes.
agent_tmp_base=$(mktemp -d /tmp/h.XXX)
trap 'rm -rf "$root" "$agent_tmp_base"' 0
agent_tmp_dir=$agent_tmp_base/agent-$(id -u)
agent_tmp_setup=$(HERDR_TEST_AGENT_TMP_BASE=$agent_tmp_base && . "$repo_dir/bin/herdr-lanes.sh" && agent_tmp_command)
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
if [ "$1" = --machine ]; then
    [ "$2" = "${STUB_MACHINE_LABEL:-devbox}" ] || exit 1
    shift 2
fi
[ "${STUB_FAIL:-}" != "herdr $1 $2" ] || exit 1
case "$1 $2" in
    "machine list") printf '%s\n' "$STUB_MACHINES" ;;
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
        [ "${STUB_EXEC:-0}" != 1 ] || { grep -qF -- "$5" "$STUB_CALLS.screen"; exit; }
        case $5 in herdr-launch-probe-*) [ "${STUB_PROBE:-ok}" != wait-fail ] || exit 1; exit 0 ;; esac
        [ "${STUB_ENV_OK:-1}" = 1 ] || exit 1
        if [ -f "$STUB_CALLS.setup" ]; then
            [ "$(cat "$STUB_CALLS.env")" = "$5" ] || exit 1
            [ -f "$(cat "$STUB_CALLS.setup-path")" ] || exit 1
        fi ;;
    "pane run")
        # STUB_EXEC=1: one shell runs every pane command in order, and the
        # screen shows each typed command before the output, as a pane does.
        if [ "${STUB_EXEC:-0}" = 1 ]; then
            printf '%s\n' "$4" >>"$STUB_CALLS.shell"
            { cat "$STUB_CALLS.shell"; sh "$STUB_CALLS.shell" 2>&1; } >"$STUB_CALLS.screen"
            exit 0
        fi
        case $4 in ' probe_marker='*)
            [ "${STUB_PROBE:-ok}" != run-fail ] || exit 1
            printf '%s\n' "$4" >"$STUB_CALLS.probe-command"
            if [ -n "${STUB_TARGET_HOME:-}" ]; then
                HOME="$STUB_TARGET_HOME" PATH="$STUB_TARGET_BIN:/usr/bin:/bin" sh -c "$4" >"$STUB_CALLS.probe"
                exit 0
            fi
            marker=${4#' probe_marker='}; marker=${marker%%;*}
            printf '%s\n' "$marker" "${STUB_CWD_OK:-1}" "${STUB_COMMON:-/target/main/.git}" \
                "${STUB_CLI-/target/bin/codex}" "${STUB_CODATOR:-0}" "$STUB_ACCOUNT" "$marker-end" >"$STUB_CALLS.probe"
            ;;
        esac
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
    "pane read")
        [ "${STUB_EXEC:-0}" != 1 ] || { cat "$STUB_CALLS.screen"; exit 0; }
        if [ -f "$STUB_CALLS.probe" ]; then
            case ${STUB_PROBE:-ok} in
                read-fail) exit 1 ;;
                unreadable) printf '%s\n' 'no probe output' ;;
                truncated) sed '$d' "$STUB_CALLS.probe" ;;
                json) jq -Rs '{result:{output:.}}' <"$STUB_CALLS.probe" ;;
                *) cat "$STUB_CALLS.probe" ;;
            esac
        else printf '%s\n' "${STUB_SCREEN:-}"; fi ;;
    # STUB_START_OK=once fails the first agent start of a case, then succeeds.
    "agent start")
        if [ "${STUB_START_OK:-1}" = once ] && [ ! -f "$STUB_CALLS.start-failed" ]; then
            : >"$STUB_CALLS.start-failed"
            exit 1
        fi
        [ "${STUB_START_OK:-1}" = 1 ] || [ "${STUB_START_OK:-1}" = once ] || exit 1
        printf '%s\n' "$3" >"$STUB_CALLS.started" ;;
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
        'status --tree') cat "$STUB_CALLER" ;;
        'doc ls')
            [ "${STUB_FAIL:-}" != 'taskr doc ls' ] || exit 1
            cat "$STUB_CALLER_DOCS" ;;
        'work list') cat "$STUB_RESERVED" ;;
        # The row of $STUB_RESERVED's .work[] with work_id $4, as taskr prints one.
        'work show')
            jq -ce --argjson id "$4" '.work[] | select(.work_id == $id) | . + {ok: true}' "$STUB_RESERVED" && exit 0
            printf '%s\n' '{"error":"work does not exist","kind":"rejected"}'
            echo 'taskr work: work does not exist' >&2
            exit 6 ;;
        'work observe')
            [ "${STUB_FAIL:-}" != 'taskr work observe' ] || exit 1
            if [ -n "${STUB_OBSERVE:-}" ]; then printf '%s\n' "$STUB_OBSERVE";
            else printf '%s\n' '{"launch_id":3579,"task_id":2468,"native_session":"synthetic-native","dispatch":{"pane":"wA:p5"}}'; fi ;;
        *) exit 1 ;; esac ;;
    doc)
        [ "$2" = get ] && [ "$3" = 99 ] || exit 1
        cat "$STUB_CALLER_CAPTURE" ;;
    new) printf '%s\n' "${STUB_NEW:-n1 2468}" ;;
    # STUB_PROMPT: got (default), no_receipt or rejected, as --confirm reports them.
    prompt) case ${STUB_PROMPT:-got} in
        no_receipt) printf '%s\n' '{"error":"prompt attempt 9: activity observed but no taskr got receipt within 60000 ms","kind":"no_receipt","outcome":"no_receipt","receipt":false}'; exit 5 ;;
        rejected) printf '%s\n' '{"error":"prompt attempt 9: rejected; inspect the agent before any resend","kind":"herdr","outcome":"rejected"}'; exit 5 ;;
        esac ;;
    # A got from another launch never counts; STUB_LOG_GOT=1 adds this launch's.
    log) printf '%s\n' '{"kind":"got","launch_id":1,"task_id":2468}'
        [ "${STUB_LOG_GOT:-0}" != 1 ] || printf '%s\n' '{"kind":"got","launch_id":3579,"task_id":2468}' ;;
    launch) printf '%s\n' "${STUB_LAUNCH:-l1 {\"launch_id\":3579,\"status\":\"open\"}}" ;;
esac
exit 0
EOF
ln -s "$(command -v jq)" "$stub_dir/jq"
# Step 7's receipt poll sleeps; the stub returns at once and logs nothing.
printf '%s\n' '#!/bin/sh' 'exit 0' >"$stub_dir/sleep"
chmod 0755 "$stub_dir/herdr" "$stub_dir/taskr" "$stub_dir/sleep"
# Reserved-mode socket preflight is isolated too; no connection to a live server.
printf '%s\n' '#!/bin/sh' "[ \"\${1:-}\" != -c ] || exec '$(command -v python3)' \"\$@\"" \
    'cat >/dev/null' 'exit "${STUB_SERVER_RC:-0}"' >"$stub_dir/python3"
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
    # The fake lists models.txt beside it for "models --standalone" only.
    cat >"$home/.opencode/bin/opencode" <<'EOF'
#!/bin/sh
case $* in
    --version) echo synthetic-opencode ;;
    'models --standalone') [ -z "${STUB_CALLS-}" ] || echo 'opencode|models|--standalone' >>"$STUB_CALLS.opencode"
        cat "${0%/*}/models.txt" ;;
    *) exit 99 ;;
esac
EOF
    chmod 0755 "$home/.opencode/bin/opencode"
    printf '%s\n' opencode/glm-5.3-flash opencode/glm-5.3-flash-free opencode/minimax-m3.1-flash-free >"$home/.opencode/bin/models.txt"
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
    STUB_SOURCE_PROBE=0 STUB_PROMPT= STUB_LOG_GOT= STUB_EXEC=0
    STUB_MACHINES='[{"id":"m1","label":"devbox","target":"u@Box.example.com","enabled":true}]'
    STUB_MACHINE_LABEL=devbox STUB_PROBE=ok STUB_CWD_OK=1 STUB_COMMON=/target/main/.git STUB_CLI=/target/bin/codex STUB_CODATOR=0
    STUB_TARGET_HOME= STUB_TARGET_BIN= STUB_ACCOUNT='{}'
    STUB_RESERVED=$case_dir/reserved.json
    STUB_CALLER=$case_dir/caller.json STUB_CALLER_BRIEF=$case_dir/caller.md
    STUB_CALLER_DOCS=$case_dir/caller-docs.json STUB_CALLER_CAPTURE=$case_dir/caller-captured.md
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
        STUB_CALLER="$STUB_CALLER" STUB_CALLER_BRIEF="$STUB_CALLER_BRIEF" \
        STUB_CALLER_DOCS="$STUB_CALLER_DOCS" STUB_CALLER_CAPTURE="$STUB_CALLER_CAPTURE" \
        STUB_NATIVE="$STUB_NATIVE" STUB_OBSERVE="$STUB_OBSERVE" \
        STUB_INIT_RC="$STUB_INIT_RC" STUB_INIT_NATIVE="$STUB_INIT_NATIVE" \
        STUB_SOURCE_PROBE="$STUB_SOURCE_PROBE" STUB_PROMPT="$STUB_PROMPT" STUB_LOG_GOT="$STUB_LOG_GOT" \
        STUB_EXEC="$STUB_EXEC" HERDR_TEST_AGENT_TMP_BASE="$agent_tmp_base" \
        STUB_MACHINES="$STUB_MACHINES" STUB_MACHINE_LABEL="$STUB_MACHINE_LABEL" \
        STUB_PROBE="$STUB_PROBE" STUB_CWD_OK="$STUB_CWD_OK" STUB_COMMON="$STUB_COMMON" \
        STUB_CLI="$STUB_CLI" STUB_CODATOR="$STUB_CODATOR" STUB_ACCOUNT="$STUB_ACCOUNT" \
        STUB_TARGET_HOME="$STUB_TARGET_HOME" STUB_TARGET_BIN="$STUB_TARGET_BIN" \
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

# The lane flags: full permissions, no sandbox (owner decision, 2026-10-07),
# and the W2305 lean launch: built-in tools, MCP servers, skills and plugins
# cut to what workers use.
# claude_settings [orch]: the --settings JSON; orch keeps the herdr skills.
claude_settings() {
    lean_skills=',"skillOverrides":{"herdr":"off","herdr-orchestration":"off","herdr-fleet":"off"}'
    [ "${1-}" != orch ] || lean_skills=
    printf '%s' "{\"agentPushNotifEnabled\":false,\"autoMemoryEnabled\":false,\"promptSuggestionEnabled\":false,\"awaySummaryEnabled\":false,\"autoCompactEnabled\":true,\"idleCompaction\":false,\"disableBundledSkills\":true,\"syncClaudeAiSkills\":false,\"disableClaudeAiConnectors\":true,\"enabledPlugins\":{\"posthog@claude-plugins-official\":false,\"cc-plugin-you-should-know@builtin\":false,\"slack@claude-plugins-official\":false,\"stripe@claude-plugins-official\":false,\"cc-plugin-easel@builtin\":false,\"cc-plugin-plugin-authoring@builtin\":false}$lean_skills}"
}
# claude_flags EFFORT [MODEL] [orch] [mcp] [DENY]; mcp (rs- and arch- lanes, W2316)
# drops --strict-mcp-config; DENY (W3332) is the "|--disallowedTools|NAME"
# text that follows --tools.
claude_flags() {
    strict_mcp='|--strict-mcp-config'
    [ "${4-}" != mcp ] || strict_mcp=
    printf '%s' "--model|${2:-claude-opus-5-5}|--effort|$1|--dangerously-skip-permissions|--tools|Bash,Read,Edit,Write,Grep,Glob,ToolSearch,Skill,WebFetch,WebSearch${5-}$strict_mcp|--settings|$(claude_settings "${3-}")"
}
# codex_flags MODEL EFFORT TRUST
codex_flags() {
    printf '%s' "|-m|$1|-c|model_reasoning_effort=\"$2\"|-c|check_for_update_on_startup=false|--dangerously-bypass-approvals-and-sandbox|-c|projects={$3}|--dangerously-bypass-hook-trust"
}
setup=' export FLEET_ROLE=worker TASKR_TASK=2468 TASKR_LAUNCH=3579'
# Claude review seats and gather lanes get a 5-minute prompt cache; every other
# Claude lane unsets it, so a reused pane does not keep it (W3438).
seat_setup="$setup CLAUDE_CODE_SANDBOXED=1 CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1 CLAUDE_CODE_PROMPT_CACHE_TTL=5m"
claude_setup=" unset CLAUDE_CODE_PROMPT_CACHE_TTL;$setup CLAUDE_CODE_SANDBOXED=1 CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1"
forward=' && codex() { "$HOME/.local/bin/codator" codex "$@"; } && claude() { "$HOME/.local/bin/codator" claude "$@"; }'
env_check='echo "herdr-launch-env=$TASKR_TASK/$TASKR_LAUNCH/$FLEET_ROLE/$herdr_launch_ok"'

# The calls of steps 2 to 7 for task 2468, launch 3579 in pane P (tab T),
# workspace $ws, cwd $tcwd.
tail_calls() {
    printf '%s\n' \
        "taskr|new|$1|--parent|42|--role|$2|--workspace|$ws|--tab|$4|--pane|$3|--cwd|$tcwd|--brief|$brief|--report|$report" \
        "taskr|launch|2468|--provider|$5|--model|$6|--effort|$7" \
        "herdr|pane|run|$3|$8 && $agent_tmp_setup && herdr_launch_ok=3579" \
        "herdr|pane|run|$3|$env_check" \
        "herdr|pane|wait-output|$3|--match|herdr-launch-env=2468/3579/worker/3579|--timeout|15000" \
        "herdr|agent|start|$1|--kind|$5|--pane|$3|--timeout|90000|--$9" \
        "taskr|prompt|2468|--file|$brief|--confirm|--confirm-timeout|60000"
}
tab_calls() {
    [ -n "${2-}" ] || printf '%s\n' "herdr|pane|current|--current"
    printf '%s\n' "herdr|tab|create|--workspace|$ws|--cwd|$tcwd|--label|$1|--env|FLEET_ROLE=worker|--env|DISABLE_AUTO_UPDATE=true|--no-focus"
}

if [ "${HERDR_LAUNCH_RESERVED_ONLY:-0}" != 1 ]; then
# Only synthetic usage files are read; Codator must never be invoked to pick.
account_codator() {
    mkdir -p "$1/.local/bin"
    printf '%s\n' '#!/bin/sh' 'echo unexpected-codator-call >&2; exit 99' >"$1/.local/bin/codator"
    chmod 0755 "$1/.local/bin/codator"
}
usage_fixture() {
    python3 - "$1" "$2" "$3" "${4:-usable}" "${5:-0}" "${6:-72}" <<'PY'
import datetime, json, os, pathlib, sys, time
path = pathlib.Path(sys.argv[1]) / '.local/share/codator/claude' / sys.argv[2] / '.claude-usage.json'
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps({'outcome': sys.argv[4], 'snapshot': {'rate_limits': {
    'seven_day': {'utilization': 20, 'resets_at': (datetime.datetime.now(datetime.timezone.utc)
        + datetime.timedelta(hours=float(sys.argv[6]))).isoformat()},
    'model_scoped': [{'display_name': 'Fable', 'utilization': json.loads(sys.argv[3])}]}}}))
os.chmod(path, 0o600)
mtime = time.time() - int(sys.argv[5])
os.utime(path, (mtime, mtime))
PY
}
# Replace individual limits without changing the rest of a synthetic snapshot.
usage_limits() {
    python3 - "$home/.local/share/codator/claude/$1/.claude-usage.json" "$2" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data['snapshot']['rate_limits'].update(json.loads(sys.argv[2]))
path.write_text(json.dumps(data))
PY
}
expect_account_start() {
    grep -F '|agent|start|' "$case_dir/calls" | grep -F -- "|--|--account|$1|$2" >/dev/null \
        || fail 'account is not the leading native argument pair'
    printf '%s\n' "$out" | grep -F " account=$1" >/dev/null || fail 'chosen account not reported'
    # An automatic pick may warn once about the protect list (W3190); the
    # protect cases check that line itself.
    rest=$(printf '%s\n' "$err" | grep -Exv 'herdr-launch: no protect list \((missing|malformed)\): ~/\.config/herdr-fleet/accounts\.json' || :)
    [ -z "$rest" ] || fail "unexpected stderr: $err"
}
# expect_protect_warning missing|malformed: stderr is exactly that one warning.
expect_protect_warning() {
    [ "$err" = "herdr-launch: no protect list ($1): ~/.config/herdr-fleet/accounts.json" ] \
        || fail "stderr '$err', expected the $1 protect warning"
}
# acct NAME WEEK_USED FABLE_USED|- RESET [FIVE_HOUR_USED]: a fresh usable
# snapshot. RESET is hours from now or @EPOCH; - leaves out the Fable row.
acct() {
    python3 - "$home" "$@" <<'PY'
import datetime, json, pathlib, sys, time
home, name, week, fable, reset = sys.argv[1:6]
stamp = float(reset[1:]) if reset.startswith('@') else time.time() + float(reset) * 3600
limits = {'seven_day': {'utilization': json.loads(week), 'resets_at':
          datetime.datetime.fromtimestamp(stamp, datetime.timezone.utc).isoformat()}, 'model_scoped': []}
if fable != '-':
    limits['model_scoped'].append({'display_name': 'Fable', 'utilization': json.loads(fable)})
if len(sys.argv) > 6:
    limits['five_hour'] = {'utilization': json.loads(sys.argv[6])}
path = pathlib.Path(home) / '.local/share/codator/claude' / name / '.claude-usage.json'
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps({'outcome': 'usable', 'snapshot': {'rate_limits': limits}}))
PY
}
# pick LANE NAME TAIL: a reviewer launch on LANE picks NAME and the final
# line ends with " account=NAME TAIL".
pick() {
    run_launch sh "$launch_sh" "$1" rev-cart "$brief" "$report" 42
    expect_rc 0
    expect_account_start "$2" --model
    case $out in *" account=$2 $3") ;; *) fail "$1 final line '$out' does not end with account=$2 $3" ;; esac
}
protect() {
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' "$1" >"$home/.config/herdr-fleet/accounts.json"
}

for account_lane in opus-medium sol-high; do
    new_case "explicit account is passed first for $account_lane"
    account_codator "$home"
    run_launch sh "$launch_sh" --account 'review-seat' "$account_lane" impl-cart "$brief" "$report" 42
    expect_rc 0
    native_flag=--model
    [ "$account_lane" != sol-high ] || native_flag=-m
    expect_account_start 'review-seat' "$native_flag"
    case $out in *' drain='*) fail 'explicit account reported a drain pick' ;; esac
    [ ! -e "$home/.config/herdr-fleet/accounts.json" ] || fail 'fixture has a protect list'
    [ -z "$err" ] || fail "explicit account with a missing protect list warned: $err"
done
for invalid_account in 'review seat' '-seat' '_seat' 'a.b' 'éclair' 'seat/name' \
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' "$(printf 'seat\nline')"; do
    new_case 'invalid explicit account is refused before any calls'
    run_launch sh "$launch_sh" --account "$invalid_account" opus-medium impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err '--account needs one valid name'
    expect_no_calls
done
for account_arg in empty missing; do
    new_case "--account rejects $account_arg value"
    if [ "$account_arg" = empty ]; then
        run_launch sh "$launch_sh" --account '' opus-medium impl-cart "$brief" "$report" 42
    else run_launch sh "$launch_sh" --account; fi
    expect_rc 2
    expect_err '--account needs NAME'
    expect_no_calls
done
new_case 'explicit account refuses a target without Codator'
run_launch sh "$launch_sh" --account seat opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err '--account requires Codator on the target'
! grep -q '^taskr|' "$case_dir/calls" || fail 'registered without target Codator'

new_case 'Fable chooses soonest reset with lowest name on a tie' reviewer
account_codator "$home"
usage_fixture "$home" z-seat 10
usage_fixture "$home" a-seat 10
# Copy the exact reset so this tests the name tie rather than clock drift.
cp "$home/.local/share/codator/claude/z-seat/.claude-usage.json" "$home/.local/share/codator/claude/a-seat/.claude-usage.json"
usage_fixture "$home" later-reset 60 usable 0 96
pick fable-high a-seat drain=later

new_case 'Fable skips exhausted, stale, unusable, unreadable and malformed usage' reviewer
account_codator "$home"
usage_fixture "$home" valid 90
usage_fixture "$home" exhausted 100
usage_fixture "$home" over-limit 110
usage_fixture "$home" stale 0 usable 1900
usage_fixture "$home" unusable 0 error
usage_fixture "$home" no-fable 0
usage_fixture "$home" malformed 0
usage_fixture "$home" unreadable 0
usage_fixture "$home" missing-file 0
usage_fixture "$home" wrong-shape 0
usage_fixture "$home" wrong-number '"0"'
usage_fixture "$home" boolean false
usage_dir=$home/.local/share/codator/claude
printf '%s\n' '{"outcome":"usable","snapshot":{"rate_limits":{"model_scoped":[]}}}' >"$usage_dir/no-fable/.claude-usage.json"
printf '%s\n' '{broken' >"$usage_dir/malformed/.claude-usage.json"
printf '%s\n' '[]' >"$usage_dir/wrong-shape/.claude-usage.json"
chmod 000 "$usage_dir/unreadable/.claude-usage.json"
rm "$usage_dir/missing-file/.claude-usage.json"
mkdir "$usage_dir/missing-file/.claude-usage.json"
run_launch sh "$launch_sh" fable-high rev-cart "$brief" "$report" 42
expect_rc 0
if [ -r "$usage_dir/unreadable/.claude-usage.json" ]; then
    # Privileged test runners can read mode 000; the directory still exercises
    # a read failure. Ordinary runners also check permission denial.
    expect_account_start unreadable --model
else expect_account_start valid --model; fi
chmod 600 "$usage_dir/unreadable/.claude-usage.json"

for fallback in empty exhausted below-margin; do
    new_case "Fable falls back without an account: $fallback" reviewer
    account_codator "$home"
    [ "$fallback" != exhausted ] || usage_fixture "$home" seat 100
    [ "$fallback" != below-margin ] || usage_fixture "$home" seat 98
    run_launch sh "$launch_sh" fable-high rev-cart "$brief" "$report" 42
    expect_rc 0
    expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
    ! grep -q '|--account|' "$case_dir/calls" || fail 'fallback supplied an account'
done
new_case 'explicit Fable account overrides quota selection' reviewer
account_codator "$home"
usage_fixture "$home" automatic 0
run_launch sh "$launch_sh" --account manual fable-high rev-cart "$brief" "$report" 42
expect_rc 0
expect_account_start manual --model

new_case 'Opus picks independently of exhausted Fable quota'
account_codator "$home"
usage_fixture "$home" automatic 100
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_account_start automatic --model
case $out in *' drain=later') ;; *) fail 'later reason missing' ;; esac

# The weekly margin (3% left) and the five-hour margin (15% left) apply to
# every Claude lane. Model limits apply only to their own lanes.
for limit_case in week five-hour opus sonnet fable fable-week fable-five-hour missing-week missing-reset bad-reset \
    bad-week boolean-week bad-five-hour missing-fable; do
    new_case "picker skips an ineligible account: $limit_case" reviewer
    account_codator "$home"
    usage_fixture "$home" fallback 40
    usage_limits fallback '{"seven_day":{"utilization":20,"resets_at":"2100-01-01T00:00:00Z"}}'
    usage_fixture "$home" latest 0 usable 0 48
    drain_lane=opus-medium
    case $limit_case in
        week) usage_limits latest '{"seven_day":{"utilization":97.01,"resets_at":"2099-01-01T00:00:00Z"}}' ;;
        five-hour) usage_limits latest '{"five_hour":{"utilization":85.01}}' ;;
        opus) usage_limits latest '{"seven_day_opus":{"utilization":98}}' ;;
        sonnet)
            mkdir -p "$home/.config/herdr-fleet"
            printf '%s\n' '{"sonnet-test":{"kind":"claude","model":"claude-sonnet-5-5","effort":"high","prefixes":["rev"]}}' >"$home/.config/herdr-fleet/lanes.json"
            drain_lane=sonnet-test
            usage_limits latest '{"seven_day_sonnet":{"utilization":98}}' ;;
        fable) drain_lane=fable-high; usage_limits latest '{"model_scoped":[{"display_name":"Fable","utilization":98}]}' ;;
        fable-week) drain_lane=fable-high; usage_limits latest '{"seven_day":{"utilization":98,"resets_at":"2099-01-01T00:00:00Z"}}' ;;
        fable-five-hour) drain_lane=fable-high; usage_limits latest '{"five_hour":{"utilization":98}}' ;;
        missing-week) usage_limits latest '{"seven_day":null}' ;;
        missing-reset) usage_limits latest '{"seven_day":{"utilization":0}}' ;;
        bad-reset) usage_limits latest '{"seven_day":{"utilization":0,"resets_at":"bad"}}' ;;
        bad-week) usage_limits latest '{"seven_day":{"utilization":"0","resets_at":"2099-01-01T00:00:00Z"}}' ;;
        boolean-week) usage_limits latest '{"seven_day":{"utilization":false,"resets_at":"2099-01-01T00:00:00Z"}}' ;;
        bad-five-hour) usage_limits latest '{"five_hour":{"utilization":"0"}}' ;;
        missing-fable) drain_lane=fable-high; usage_limits latest '{"model_scoped":[]}' ;;
    esac
    run_launch sh "$launch_sh" "$drain_lane" rev-cart "$brief" "$report" 42
    expect_rc 0
    expect_account_start fallback --model
done

new_case 'exact 3 and 15 percent margins and optional null limits qualify' reviewer
account_codator "$home"
usage_fixture "$home" margin 97
usage_limits margin '{"seven_day":{"utilization":97,"resets_at":"2099-01-01T00:00:00Z"},"five_hour":{"utilization":85},"seven_day_opus":null,"seven_day_sonnet":null}'
run_launch sh "$launch_sh" fable-high rev-cart "$brief" "$report" 42
expect_rc 0
expect_account_start margin --model

new_case 'Opus ignores Sonnet limits and null five-hour limits'
account_codator "$home"
usage_fixture "$home" seat 100
usage_limits seat '{"five_hour":null,"seven_day_opus":null,"seven_day_sonnet":{"utilization":100}}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_account_start seat --model

new_case 'soonest reset wins before latest reset and ties use lowest name'
account_codator "$home"
usage_fixture "$home" latest 0 usable 0 120
usage_fixture "$home" later-soon 0 usable 0 20
usage_fixture "$home" z-soon 0 usable 0 2
usage_fixture "$home" a-soon 0 usable 0 2
cp "$home/.local/share/codator/claude/z-soon/.claude-usage.json" "$home/.local/share/codator/claude/a-soon/.claude-usage.json"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_account_start a-soon --model
case $out in *' drain=soon') ;; *) fail 'soon reason missing' ;; esac

# W2765 account rule: soonest reset at every horizon; Fable lanes need Fable
# room; other lanes prefer spare week (week left - 0.55 x Fable left >= 3).
new_case 'tonight-like table: Opus takes the soonest spare, Fable the soonest Fable room' reviewer
account_codator "$home"
acct acct-a 95 100 3
acct acct-b 93 100 5
acct acct-c 26 46 19 44
acct acct-d 86 100 104
acct acct-e 54 98 164
acct acct-f 61 93 165
pick opus-medium acct-a drain=soon
pick fable-high acct-c drain=soon
acct acct-a 98 100 3
acct acct-b 97.5 100 5
pick opus-medium acct-c drain=soon

new_case 'all accounts Fable-rich: tier B takes the largest spare and says fable_spend=1' reviewer
account_codator "$home"
acct acct-a 50 0 2
acct acct-b 45 0 30
acct acct-c 47 0 4
pick opus-medium acct-b 'drain=later fable_spend=1'
pick fable-high acct-a drain=soon

new_case 'missing Fable row: Opus counts no Fable, a Fable lane skips the account' reviewer
account_codator "$home"
acct acct-a 50 - 3
acct acct-b 50 0 2
pick opus-medium acct-a drain=soon
acct acct-a 50 - 1
pick fable-high acct-b drain=soon

new_case 'nothing resets within 24 hours: the soonest reset wins, not the latest' reviewer
account_codator "$home"
acct acct-a 20 100 48
acct acct-b 20 100 96
pick opus-medium acct-a drain=later
acct acct-a 20 50 48
acct acct-b 20 50 96
pick fable-high acct-a drain=later

new_case 'spare is week minus 0.55 x Fable: 74 and 54 is tier A, 3.25 is A, 2.7 and 0.5 are B' reviewer
account_codator "$home"
acct acct-a 26 46 30
acct acct-b 0 100 40
pick opus-medium acct-a drain=later
acct acct-a 50 15 2
acct acct-b 0 100 3
pick opus-medium acct-a drain=soon
acct acct-a 50 14 2
pick opus-medium acct-b drain=soon
acct acct-a 50 10 2
pick opus-medium acct-b drain=soon

new_case 'a Fable lane ignores spare: the soonest Fable room beats a later spare' reviewer
account_codator "$home"
acct acct-a 50 0 2
acct acct-b 20 50 3
pick fable-high acct-a drain=soon
pick opus-medium acct-b drain=soon

new_case 'Fable under 3 percent left is not counted: week 4 left and Fable 2 left is tier A' reviewer
account_codator "$home"
acct acct-a 96 98 2
acct acct-b 50 0 3
pick opus-medium acct-a drain=soon

new_case 'resets under a second apart in one minute tie by name' reviewer
account_codator "$home"
minute=$(python3 -c 'import time; print(int(time.time() // 60 * 60 + 3 * 3600))')
acct acct-b 20 50 "@$minute.2"
acct acct-a 20 50 "@$minute.84"
pick opus-medium acct-a drain=soon
pick fable-high acct-a drain=soon

new_case 'resets on both sides of a minute boundary tie by name' reviewer
account_codator "$home"
minute=$(python3 -c 'import time; print(int(time.time() // 60 * 60 + 3 * 3600))')
acct acct-a 20 50 "@$minute.84"
acct acct-b 20 50 "@$((minute - 1)).74"
pick opus-medium acct-a drain=soon
pick fable-high acct-a drain=soon

new_case 'a passed weekly reset ranks one week later and is not soon' reviewer
account_codator "$home"
acct acct-a 20 50 -0.5
acct acct-b 20 50 2
pick opus-medium acct-b drain=soon
pick fable-high acct-b drain=soon
acct acct-b 20 50 200
pick opus-medium acct-a drain=later

new_case 'five-hour window 86 percent used is not eligible' reviewer
account_codator "$home"
acct acct-a 20 100 2 86
acct acct-b 20 100 3
pick opus-medium acct-b drain=soon
acct acct-a 20 0 2 86
acct acct-b 20 0 3
pick fable-high acct-b drain=soon

new_case 'protected account with week 14 left and reserve 15 is not picked' reviewer
account_codator "$home"
protect '{"protect":["acct-a"],"reserve":15}'
acct acct-a 86 100 2
acct acct-b 20 100 3
pick opus-medium acct-b drain=soon
acct acct-a 0 85 2
acct acct-b 20 50 3
pick fable-high acct-b drain=soon
# Spare 4.9 without the reserve, -1.85 with it: tier B, so acct-b wins.
acct acct-a 50 18 2
acct acct-b 20 100 3
pick opus-medium acct-b drain=soon

new_case 'the reserve applies only to listed accounts and unknown names change nothing' reviewer
account_codator "$home"
protect '{"protect":["acct-b","nobody"],"reserve":15}'
acct acct-a 86 100 2
acct acct-b 20 100 3
pick opus-medium acct-a drain=soon

# Any accepted reserve changes this pick: 15 or 1 puts acct-a out (acct-b),
# -1 lets acct-c in (acct-c).
for bad_list in '{broken' '[]' '{"protect":"acct-a","reserve":15}' '{"protect":["acct-a","acct-c"]}' \
    '{"reserve":15}' '{"protect":["acct-a","acct-c"],"reserve":"15"}' '{"protect":["acct-a","acct-c"],"reserve":true}' \
    '{"protect":[1],"reserve":15}' '{"protect":["acct-a","acct-c"],"reserve":-1}'; do
    new_case "malformed protect list means no list: $bad_list" reviewer
    account_codator "$home"
    protect "$bad_list"
    acct acct-c 97.5 100 1
    acct acct-a 96.5 100 2
    acct acct-b 20 100 3
    pick opus-medium acct-a drain=soon
    expect_protect_warning malformed
done

for bad_list in huge-reserve deep-nesting; do
    new_case "protect file shape $bad_list means no list" reviewer
    account_codator "$home"
    case $bad_list in
        huge-reserve) protect "{\"protect\":[\"acct-a\"],\"reserve\":1$(printf '%0400d' 0)}" ;;
        deep-nesting) protect "$(python3 -c 'print("[" * 100000 + "]" * 100000)')" ;;
    esac
    acct acct-a 86 100 2
    acct acct-b 20 100 3
    pick opus-medium acct-a drain=soon
    expect_protect_warning malformed
    run_launch sh "$launch_sh" --quota
    expect_rc 0
    printf '%s\n' "$out" | grep -q '^account=acct-a .* role=soon$' || fail 'quota did not read no list'
    [ "$(printf '%s\n' "$out" | tail -n 1)" = 'protect=none reason=malformed' ] || fail 'quota did not name a malformed list'
done

new_case 'automatic pick with a missing protect list warns once and picks as before' reviewer
account_codator "$home"
acct acct-a 86 100 2
acct acct-b 20 100 3
pick opus-medium acct-a drain=soon
expect_protect_warning missing

new_case 'automatic pick with a valid protect list does not warn' reviewer
account_codator "$home"
protect '{"protect":[],"reserve":15}'
acct acct-a 86 100 2
pick opus-medium acct-a drain=soon
[ -z "$err" ] || fail "valid protect list warned: $err"

new_case 'remote automatic pick names a missing protect list'
STUB_CODATOR=1
STUB_ACCOUNT='{"account":"seat","drain":"later","protect":"missing"}'
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=devbox account=seat drain=later'
expect_protect_warning missing

new_case 'remote pick reports fable_spend=1 and drops other values'
STUB_CODATOR=1
STUB_ACCOUNT='{"account":"seat","drain":"later","fable_spend":1}'
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=devbox account=seat drain=later fable_spend=1'
STUB_ACCOUNT='{"account":"seat","drain":"later","fable_spend":"yes"}'
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=devbox account=seat drain=later'

new_case 'invalid snapshot account labels are skipped'
account_codator "$home"
usage_fixture "$home" valid 0 usable 0 48
for bad_label in 'bad name' '_bad' 'éclair' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
    usage_fixture "$home" "$bad_label" 0 usable 0 120
done
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_account_start valid --model

# W2725: Codator refuses an account the snapshot still called eligible with
# "codator: selected account is ineligible: <reason>" (stderr, exit 1). Only
# an auto-picked account is retried, once, on the next pick.
refusal='codator: selected account is ineligible: all applicable usage headroom is exhausted'
refused_flags() { printf '%s' "|--account|$1|$(claude_flags medium)"; }
refused_read='herdr|pane|read|wA:p7|--source|visible|--lines|40'
expect_rerun_hint() {
    expect_err "herdr-launch: to choose the account, rerun: herdr-launch --account NAME 'opus-medium' 'impl-cart' '$brief' '$report' '42'"
    [ "$(printf '%s\n' "$err" | tail -n 1)" = "herdr-launch: to choose the account, rerun: herdr-launch --account NAME 'opus-medium' 'impl-cart' '$brief' '$report' '42' (NAME from herdr-launch --quota)" ] \
        || fail "rerun hint is not the last stderr line: $err"
}

new_case 'refused auto-picked account: retry once on the next account'
account_codator "$home"
usage_fixture "$home" first-seat 0 usable 0 72
usage_fixture "$home" second-seat 0 usable 0 96
STUB_START_OK=once STUB_SCREEN=$refusal
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
[ "$(printf '%s\n' "$out" | tail -n 1)" = 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=second-seat' ] \
    || fail "final line does not name the second account: $out"
expect_err 'herdr-launch: Codator refused account first-seat; closed tab wA:t7, abandoned task 2468; retrying once on account second-seat'
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "$(refused_flags first-seat)" | sed '$d')" \
    "$refused_read" 'herdr|tab|close|wA:t7' 'taskr|close|2468|--outcome|abandoned' \
    "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "$(refused_flags second-seat)")"

new_case 'refused explicit account: no retry, fails as before'
account_codator "$home"
usage_fixture "$home" first-seat 0 usable 0 72
usage_fixture "$home" second-seat 0 usable 0 96
STUB_START_OK=0 STUB_SCREEN=$refusal
run_launch sh "$launch_sh" --account first-seat opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (herdr agent start) failed'
case $err in *rerun*) fail "explicit account printed a rerun hint: $err" ;; esac
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "$(refused_flags first-seat)" | sed '$d')"

new_case 'auto-picked start failure without the refusal line: no retry'
account_codator "$home"
usage_fixture "$home" first-seat 0 usable 0 72
usage_fixture "$home" second-seat 0 usable 0 96
STUB_START_OK=0 STUB_SCREEN='claude: some other startup error'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (herdr agent start) failed'
expect_rerun_hint
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "$(refused_flags first-seat)" | sed '$d')" \
    "$refused_read"

new_case 'refused auto-picked account with no other eligible account: rerun hint'
account_codator "$home"
usage_fixture "$home" first-seat 0 usable 0 96
usage_fixture "$home" out-seat 100 usable 0 72
usage_limits out-seat '{"seven_day":{"utilization":98,"resets_at":"2099-01-01T00:00:00Z"}}'
STUB_START_OK=0 STUB_SCREEN=$refusal
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 6 (herdr agent start) failed'
expect_rerun_hint
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "$(refused_flags first-seat)" | sed '$d')" \
    "$refused_read"

new_case 'Codex retains default despite Claude usage'
account_codator "$home"
usage_fixture "$home" claude-seat 0
run_launch sh "$launch_sh" sol-high impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
! grep -q '|--account|' "$case_dir/calls" || fail 'picked a Claude account for Codex'

new_case 'quota is read only and reports later next out stale without an alert'
usage_fixture "$home" drain-seat 10
usage_limits drain-seat '{"seven_day":{"utilization":97,"resets_at":"2099-01-01T00:00:00Z"}}'
usage_fixture "$home" next-seat 100
usage_limits next-seat '{"seven_day":{"utilization":20,"resets_at":"2099-02-01T00:00:00Z"}}'
usage_fixture "$home" pick-seat 100
usage_limits pick-seat '{"seven_day":{"utilization":20,"resets_at":"2098-01-01T00:00:00Z"}}'
usage_fixture "$home" out-seat 0
usage_limits out-seat '{"seven_day":{"utilization":98,"resets_at":"2097-01-01T00:00:00Z"}}'
usage_fixture "$home" stale-seat 0 usable 1900
usage_limits stale-seat '{"seven_day":{"utilization":30,"resets_at":"2096-01-01T00:00:00Z"}}'
# Restore the stale mtime after updating limits.
python3 - "$home/.local/share/codator/claude/stale-seat/.claude-usage.json" <<'PY'
import os, sys, time
stamp = time.time() - 1900
os.utime(sys.argv[1], (stamp, stamp))
PY
usage_fixture "$home" broken 0
printf '%s\n' '{broken' >"$home/.local/share/codator/claude/broken/.claude-usage.json"
run_launch sh "$launch_sh" --quota
expect_rc 0
expect_out 'account=broken week_used=- fable_used=- reset=- role=stale
account=drain-seat week_used=97 fable_used=10 reset=2099-01-01T00:00:00Z role=next
account=next-seat week_used=20 fable_used=100 reset=2099-02-01T00:00:00Z role=next
account=out-seat week_used=98 fable_used=0 reset=2097-01-01T00:00:00Z role=out
account=pick-seat week_used=20 fable_used=100 reset=2098-01-01T00:00:00Z role=later
account=stale-seat week_used=30 fable_used=0 reset=2096-01-01T00:00:00Z role=stale
protect=none reason=missing'
expect_no_calls

new_case 'quota labels the soon pick and makes no alert'
usage_fixture "$home" soon 0 usable 0 2
usage_fixture "$home" later 0 usable 0 96
run_launch sh "$launch_sh" --quota
expect_rc 0
printf '%s\n' "$out" | grep -q '^account=soon .* role=soon$' || fail 'soon role missing'
printf '%s\n' "$out" | grep -q '^account=later .* role=next$' || fail 'next role missing'
case $out in *'alert '*) fail 'soon pick made an alert' ;; esac
expect_no_calls

new_case 'quota marks a tier B pick with fable_spend=1 and alerts on nothing'
acct acct-a 50 0 2
acct acct-b 100 0 120
run_launch sh "$launch_sh" --quota
expect_rc 0
printf '%s\n' "$out" | grep -q '^account=acct-a week_used=50 fable_used=0 reset=.* role=soon fable_spend=1$' || fail 'tier B pick not marked'
printf '%s\n' "$out" | grep -q '^account=acct-b .* role=out$' || fail 'out role missing'
case $out in *'alert '*) fail 'quota printed an alert' ;; esac
expect_no_calls

new_case 'quota marks protected accounts with protected=1'
protect '{"protect":["acct-a","acct-b"],"reserve":15}'
acct acct-a 86 100 2
acct acct-b 20 100 3
acct acct-c 50 100 4
run_launch sh "$launch_sh" --quota
expect_rc 0
printf '%s\n' "$out" | grep -q '^account=acct-a week_used=86 fable_used=100 reset=.* role=out protected=1$' || fail 'reserve reason missing'
printf '%s\n' "$out" | grep -q '^account=acct-b .* role=soon protected=1$' || fail 'protected pick not marked'
printf '%s\n' "$out" | grep -q '^account=acct-c .* role=next$' || fail 'unprotected account marked'
expect_no_calls

new_case 'quota without Python fails clearly without calls'
mkdir -p "$case_dir/no-python"
printf '%s\n' '#!/bin/sh' 'exit 127' >"$case_dir/no-python/python3"
chmod 0755 "$case_dir/no-python/python3"
run_path=$case_dir/no-python:$run_path
run_launch sh "$launch_sh" --quota
expect_rc 2
expect_err '--quota could not read snapshots with python3'
expect_no_calls

new_case 'quota with no snapshots prints only a missing protect list'
run_launch sh "$launch_sh" --quota
expect_rc 0
expect_out 'protect=none reason=missing'
expect_no_calls

for quota_list in '{broken' '{"protect":["acct-a","acct-c"]}' valid; do
    new_case "quota names the protect list state: $quota_list"
    acct acct-a 20 100 3
    want='protect=none reason=malformed'
    if [ "$quota_list" = valid ]; then
        protect '{"protect":["acct-z"],"reserve":15}'
        want=
    else protect "$quota_list"; fi
    run_launch sh "$launch_sh" --quota
    expect_rc 0
    printf '%s\n' "$out" | sed -n 1p | grep -q '^account=acct-a week_used=20 fable_used=100 reset=.* role=soon$' \
        || fail "quota account line changed: $out"
    [ "$(printf '%s\n' "$out" | sed -n 2p)" = "$want" ] || fail "quota protect line '$out', expected '$want'"
    [ "$(printf '%s\n' "$out" | wc -l)" -eq $((${#want} ? 2 : 1)) ] || fail "quota printed extra lines: $out"
    [ -z "$err" ] || fail "quota wrote stderr: $err"
    expect_no_calls
done
for quota_flags in '--quota --machine devbox' '--machine devbox --quota' \
    '--quota --account seat' '--quota --push' '--quota --check' '--check --quota' \
    '--quota opus-medium' '--quota --workspace wA' '--quota --work 1' '--quota --reserved 2 3'; do
    new_case "quota refuses launch or remote options: $quota_flags"
    run_launch sh "$launch_sh" $quota_flags
    expect_rc 2
    expect_no_calls
done

# Discovery is the only local Herdr read; every control call must be forwarded.
expect_machine_calls() {
    bad=$(sed '/^herdr|machine|list|--json$/d; /^herdr|--machine|/d; /^taskr|/d' "$case_dir/calls")
    [ -z "$bad" ] || fail "unforwarded calls: $bad"
    [ "$(grep -c '^herdr|machine|list|--json$' "$case_dir/calls")" -eq 1 ] || fail 'discovery count'
    ! grep -q '|pane|current|' "$case_dir/calls" || fail 'used caller pane current'
    grep -q '^taskr|new|.*|--machine|box$' "$case_dir/calls" || fail 'new machine host missing'
    grep -q '^taskr|launch|.*|--machine|box$' "$case_dir/calls" || fail 'launch machine host missing'
    [ "$(grep -c '|pane|run|.* probe_marker=' "$case_dir/calls")" -eq 1 ] || fail 'probe count'
    awk '/^taskr\|new\|/ {exit !seen} /\|pane\|read\|/ {seen=1}' "$case_dir/calls" || fail 'registered before probe read'
}

for remote_kind in claude codex; do
    new_case "remote $remote_kind uses target facts and forwards every control call"
    lane=opus-medium
    [ "$remote_kind" != codex ] || lane=sol-high
    tcwd="/target/checkout 'quoted"
    STUB_PROBE=json
    STUB_CODATOR=1
    mkdir -p "$case_dir/caller-bin"
    printf '%s\n' '#!/bin/sh' 'touch "$STUB_CALLS.caller-git"; exit 99' >"$case_dir/caller-bin/git"
    chmod 0755 "$case_dir/caller-bin/git"
    run_path=$case_dir/caller-bin:$run_path
    run_launch sh "$launch_sh" --cwd "$tcwd" --machine devbox --workspace wT "$lane" impl-remote "$brief" "$report" 42
    expect_rc 0
    [ -z "$err" ] || fail "unexpected stderr: $err"
    [ ! -e "$case_dir/calls.caller-git" ] || fail 'ran git on the caller'
    expect_out 'task=2468 launch=3579 tab=wT:t7 pane=wT:p7 machine=devbox account=default'
    expect_machine_calls
    grep -q -F "$forward" "$case_dir/calls" || fail 'target Codator was not forwarded'
    if [ "$remote_kind" = codex ]; then
        grep -q -F '"/target/main"={trust_level="trusted"}' "$case_dir/calls" || fail 'target main checkout not trusted'
        grep -q -F "\"$tcwd\"={trust_level=\"trusted\"}" "$case_dir/calls" || fail 'target cwd not trusted'
    fi
    # Execute the exact generated shell probe in an isolated target fixture.
    mkdir -p "$case_dir/target/bin" "$case_dir/target/home/.local/bin"
    printf '%s\n' '#!/bin/sh' 'echo /target/main/.git' >"$case_dir/target/bin/git"
    printf '%s\n' '#!/bin/sh' >"$case_dir/target/bin/$remote_kind"
    chmod 0755 "$case_dir/target/bin/git" "$case_dir/target/bin/$remote_kind"
    HOME="$case_dir/target/home" PATH="$case_dir/target/bin:/usr/bin:/bin" \
        sh "$case_dir/calls.probe-command" >"$case_dir/probe-output"
    sed -n '3p' "$case_dir/probe-output" | grep -qx 0 || fail 'probe did not check missing target cwd'
    sed -n '4p' "$case_dir/probe-output" | grep -qx /target/main/.git || fail 'probe git fact missing'
    sed -n '5p' "$case_dir/probe-output" | grep -qx "$case_dir/target/bin/$remote_kind" || fail 'probe CLI fact missing'
    sed -n '6p' "$case_dir/probe-output" | grep -qx 0 || fail 'probe absent Codator fact missing'
    printf '%s\n' '#!/bin/sh' >"$case_dir/target/home/.local/bin/codator"
    chmod 0755 "$case_dir/target/home/.local/bin/codator"
    HOME="$case_dir/target/home" PATH="$case_dir/target/bin:/usr/bin:/bin" \
        sh "$case_dir/calls.probe-command" >"$case_dir/probe-output"
    sed -n '6p' "$case_dir/probe-output" | grep -qx 1 || fail 'probe executable Codator fact missing'
done

for remote_lane in fable-high opus-high; do
new_case "remote $remote_lane selection reads target usage in the single existing probe" reviewer
account_codator "$home"
usage_fixture "$home" caller-only 0
STUB_TARGET_HOME=$case_dir/target/home
STUB_TARGET_BIN=$case_dir/target/bin
mkdir -p "$STUB_TARGET_BIN"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$STUB_TARGET_BIN/claude"
chmod 0755 "$STUB_TARGET_BIN/claude"
account_codator "$STUB_TARGET_HOME"
usage_fixture "$STUB_TARGET_HOME" target-later 10 usable 0 96
usage_fixture "$STUB_TARGET_HOME" target-best 30 usable 0 48
run_launch sh "$launch_sh" --machine devbox --workspace wT --cwd "$work" "$remote_lane" rev-cart "$brief" "$report" 42
expect_rc 0
expect_account_start target-best --model
expect_machine_calls
! grep -q '|--account|caller-only|' "$case_dir/calls" || fail 'used caller usage'
case $out in *' drain=later') ;; *) fail 'remote later reason missing' ;; esac
done

for bad_pick in 'bad name' '_bad' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
    new_case "remote invalid picked account falls back to Codator and drops fable_spend: $bad_pick"
    STUB_CODATOR=1
    STUB_ACCOUNT=$(jq -cn --arg account "$bad_pick" '{account:$account,drain:"later",fable_spend:1}')
    run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 0
    expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=devbox account=default'
    ! grep -q '|--account|' "$case_dir/calls" || fail 'invalid account reached agent start'
done

for bad_account_line in '' garbage; do
    new_case 'remote empty or garbage account line falls back to Codator'
    STUB_CODATOR=1
    STUB_ACCOUNT=$bad_account_line
    run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 0
    expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=devbox account=default'
    [ -z "$err" ] || fail "unexpected stderr: $err"
    ! grep -q '|--account|' "$case_dir/calls" || fail 'unreadable account reached agent start'
done

new_case 'remote unavailable Python falls back to Codator' reviewer
STUB_TARGET_HOME=$case_dir/target/home
STUB_TARGET_BIN=$case_dir/target/bin
mkdir -p "$STUB_TARGET_BIN"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$STUB_TARGET_BIN/claude"
printf '%s\n' '#!/bin/sh' 'exit 127' >"$STUB_TARGET_BIN/python3"
chmod 0755 "$STUB_TARGET_BIN/claude" "$STUB_TARGET_BIN/python3"
account_codator "$STUB_TARGET_HOME"
usage_fixture "$STUB_TARGET_HOME" eligible 0
run_launch sh "$launch_sh" --machine devbox --workspace wT --cwd "$work" fable-high rev-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wT:t7 pane=wT:p7 machine=devbox account=default'
expect_machine_calls
! grep -q '|--account|' "$case_dir/calls" || fail 'missing Python supplied an account'

for target_codator in 0 1; do
    new_case "remote explicit account uses target Codator fact: $target_codator"
    # Caller always has Codator, so a missing target must still refuse.
    account_codator "$home"
    STUB_CODATOR=$target_codator
    run_launch sh "$launch_sh" --machine devbox --account remote-seat opus-medium impl-remote "$brief" "$report" 42 wA:p3
    if [ "$target_codator" = 1 ]; then
        expect_rc 0
        expect_account_start remote-seat --model
        expect_machine_calls
    else
        expect_rc 2
        expect_err '--account requires Codator on the target'
        ! grep -q '^taskr|' "$case_dir/calls" || fail 'registered without target Codator'
    fi
done

new_case 'remote given pane does not check the caller cwd or Codator'
STUB_PANE_CWD=/target/only
mkdir -p "$home/.local/bin"
printf '%s\n' '#!/bin/sh' >"$home/.local/bin/codator"
chmod 0755 "$home/.local/bin/codator"
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_machine_calls
! grep -q -F "$forward" "$case_dir/calls" || fail 'used caller Codator'

new_case 'remote no-receipt retry forwards pane read and Enter without resending'
STUB_PROMPT=no_receipt STUB_LOG_GOT=1
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 submit_retry=enter machine=devbox account=default'
expect_machine_calls
[ "$(grep -c '^taskr|prompt|' "$case_dir/calls")" -eq 1 ] || fail 'remote brief resent'
grep -q '^herdr|--machine|devbox|pane|send-keys|wA:p3|Enter$' "$case_dir/calls" || fail 'retry Enter not forwarded'

new_case 'machine id resolves to its enabled target'
STUB_MACHINE_LABEL=m1
run_launch sh "$launch_sh" --machine m1 opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 machine=m1 account=default'
expect_machine_calls

new_case 'remote label refuses two enabled profiles for the same derived host'
STUB_MACHINES='[{"id":"m1","label":"devbox","target":"u@Box","enabled":true},{"id":"m2","label":"other","target":"v@box.example.com","enabled":true}]'
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 2
expect_err 'host box matches 2 enabled machines; exactly one is required'
expect_calls 'herdr|machine|list|--json'

new_case 'remote label ignores disabled profiles for the same derived host'
STUB_MACHINES='[{"id":"m1","label":"devbox","target":"u@Box","enabled":true},{"id":"m2","label":"other","target":"v@box.example.com","enabled":false}]'
run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_machine_calls

new_case 'remote cwd must be absolute before any call'
run_launch sh "$launch_sh" --machine devbox --workspace wT --cwd relative opus-medium impl-remote "$brief" "$report" 42
expect_rc 2
expect_err '--machine needs an absolute --cwd'
expect_no_calls

for profiles in '[]' '[{"label":"devbox","target":"box","enabled":false}]' \
    '[{"label":"devbox","target":"box","enabled":true},{"id":"devbox","target":"other","enabled":true}]' \
    '[{"label":"Devbox","target":"box","enabled":true}]' \
    '[{"label":"devbox","target":"u@192.0.2.1","enabled":true}]' \
    '[{"label":"devbox","target":"u@2001:db8::1","enabled":true}]' 'invalid'; do
    new_case 'missing, disabled, ambiguous, wrong-case, IP or unreadable machine is refused'
    STUB_MACHINES=$profiles
    run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 2
    expect_err 'needs one enabled saved label or id'
    expect_calls 'herdr|machine|list|--json'
done

for flags in '--push --machine devbox' '--machine devbox --push' \
    '--reserved 2 3 --machine devbox' '--machine devbox --reserved 2 3' \
    '--machine devbox --check' '--check --machine devbox' '--machine'; do
    new_case "refuse remote incompatible flags: $flags"
    run_launch sh "$launch_sh" $flags opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 2
    expect_no_calls
done
for remote_kind in opencode pi; do
    new_case "refuse remote kind: $remote_kind"
    mkdir -p "$home/.config/herdr-fleet"
    remote_model=synthetic
    [ "$remote_kind" != opencode ] || remote_model=opencode/synthetic
    printf '{"custom":{"kind":"%s","model":"%s","effort":"high","prefixes":["impl"]}}\n' "$remote_kind" "$remote_model" >"$home/.config/herdr-fleet/lanes.json"
    run_launch sh "$launch_sh" --machine devbox custom impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 2
    expect_err 'supports claude and codex only'
    expect_no_calls
done
for flags in '' '--workspace wT' '--cwd /target/only'; do
    new_case "remote new tab needs workspace and cwd: $flags"
    run_launch sh "$launch_sh" --machine devbox $flags opus-medium impl-remote "$brief" "$report" 42
    expect_rc 2
    expect_err 'needs PANE or both --workspace and --cwd'
    expect_no_calls
done
for probe_failure in run-fail wait-fail read-fail unreadable truncated; do
    new_case "remote probe failure stops before registration: $probe_failure"
    STUB_PROBE=$probe_failure
    run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 1
    expect_err 'step 1'
    expect_err 'task=- launch=- tab=wA:t5 pane=wA:p3'
    ! grep -q '^taskr|' "$case_dir/calls" || fail 'registered after probe failure'
done
for fact in cwd cli codator; do
    new_case "invalid target fact stops before registration: $fact"
    case $fact in cwd) STUB_CWD_OK=0 ;; cli) STUB_CLI= ;; codator) STUB_CODATOR=invalid ;; esac
    run_launch sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
    expect_rc 1
    ! grep -q '^taskr|' "$case_dir/calls" || fail 'registered with invalid target fact'
done

new_case 'claude lane in a new tab, with Codator forwarding'
mkdir -p "$home/.local/bin"
printf '%s\n' '#!/bin/sh' >"$home/.local/bin/codator"
chmod 0755 "$home/.local/bin/codator"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
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
    printf '%s' " && { opencode models --standalone 2>/dev/null | grep -qxF '$1' || { echo \"herdr-launch-model-missing=\$TASKR_LAUNCH:\"'$1'\"@\$(uname -n)\"; false; }; }"
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
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 account=default'
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
cp "$home/.opencode/bin/opencode" "$home/.opencode/bin/models.txt" "$work/bin with 'quote/"
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

# W3467: the pane's own OpenCode must list the lane's exact model id before
# herdr_launch_ok is set; a missing id stops step 5 with one line naming the
# model and the host, and the agent never starts. STUB_EXEC runs the real
# setup chain against the fake opencode.
pane_host=$(uname -n)
for target in tab pane; do
    new_case "OpenCode lane with a listed model starts (new $target)"
    STUB_EXEC=1
    set -- glm-free glm-inventory "$brief" "$report" 42
    [ "$target" = tab ] || set -- "$@" wA:p3
    run_launch sh "$launch_sh" "$@"
    expect_rc 0
    grep -q '^herdr|agent|start|glm-inventory|' "$case_dir/calls" || fail 'listed model did not start'
    grep -qx 'opencode|models|--standalone' "$case_dir/calls.opencode" 2>/dev/null || fail 'the pane never listed models'

    new_case "OpenCode lane with an unlisted model stops at step 5 (new $target)"
    STUB_EXEC=1
    printf '%s\n' opencode/glm-5.3-flash opencode/glm-5.3-flash-free-beta >"$home/.opencode/bin/models.txt"
    run_launch sh "$launch_sh" "$@"
    expect_rc 1
    so_far='workspace=wA task=2468 launch=3579 tab=wA:t7 pane=wA:p7'
    [ "$target" = tab ] || so_far='workspace=wA task=2468 launch=3579 tab=wA:t5 pane=wA:p3'
    want="herdr-launch: step 5 (opencode models on $pane_host did not list opencode/glm-5.3-flash-free (missing from its catalog, or the listing failed); the agent was not started) failed; so far: $so_far"
    [ "$err" = "$want" ] || fail "stderr '$err', expected '$want'"
    if grep -E '^(herdr\|agent\|start|taskr\|prompt)\|' "$case_dir/calls" >/dev/null; then fail 'started with an unlisted model'; fi
done

# --check runs the same listing locally; an unlisted model fails the lane.
new_case 'OpenCode --check passes a listed model and fails an unlisted one'
run_launch sh "$launch_sh" --check glm-free glm-flash
expect_rc 0
expect_out 'glm-free: ok
glm-flash: ok'
grep -qx 'opencode|models|--standalone' "$case_dir/calls.opencode" 2>/dev/null || fail '--check never listed models'
printf '%s\n' opencode/glm-5.3-flash opencode/glm-5.3-flash-free-beta >"$home/.opencode/bin/models.txt"
run_launch sh "$launch_sh" --check glm-free glm-flash
expect_rc 1
expect_out 'glm-free: opencode models did not list opencode/glm-5.3-flash-free (missing from its catalog, or the listing failed)
glm-flash: ok'
expect_no_calls

new_case 'OpenCode marker from an earlier launch is not this launch'
STUB_EXEC=1
printf '%s\n' 'echo "herdr-launch-model-missing=3578:opencode/glm-5.3-flash-free@otherhost"' >"$case_dir/calls.shell"
printf '%s\n' opencode/glm-5.3-flash >"$home/.opencode/bin/models.txt"
run_launch sh "$launch_sh" glm-free glm-inventory "$brief" "$report" 42 wA:p3
expect_rc 1
expect_err "step 5 (opencode models on $pane_host did not list opencode/glm-5.3-flash-free (missing from its catalog, or the listing failed); the agent was not started) failed"
case $err in *otherhost*) fail 'read the marker of another launch' ;; esac

# Private pi tables retain compatibility; no public lane selects pi.
private_pi_lane() {
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' '{"glm":{"kind":"pi","model":"glm-5.3-flash","effort":"high","prefixes":["glm"]}}' >"$home/.config/herdr-fleet/lanes.json"
}

new_case 'Haiku gather launches as a researcher' researcher
run_launch sh "$launch_sh" haiku-gather gather-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-x)" \
    "$(tail_calls gather-x researcher wA:p7 wA:t7 claude claude-haiku-5-5 medium "$seat_setup" \
        "|$(claude_flags medium claude-haiku-5-5)")"

# W3438: only Claude review seats and gather- lanes export the 5-minute cache;
# the exact setup line the fake herdr receives decides. cache_case NAME ROLE
# LANE KIND WANT: WANT is seat (with the env) or plain (without it).
cache_case() {
    new_case "prompt cache: $1 as $2 on $3 is $5" "$2"
    run_launch sh "$launch_sh" "$3" "$1" "$brief" "$report" 42
    expect_rc 0
    case $5 in seat) cache_want=$seat_setup ;; *) cache_want=$claude_setup ;; esac
    [ "$4" = claude ] || cache_want=$setup
    cache_got=$(sed -n 's/^herdr|pane|run|wA:p7|\( .*\) && herdr_launch_ok=3579$/\1/p' "$case_dir/calls")
    case $cache_got in "$cache_want" | "$cache_want && "*) ;; *) fail "setup '$cache_got' is not '$cache_want'" ;; esac
    [ "$5" = seat ] || ! grep -qF CLAUDE_CODE_PROMPT_CACHE_TTL=5m "$case_dir/calls" || fail 'lane exports the prompt cache TTL'
}
# A writer started in a seat's reused pane runs both step-4 lines in one shell.
new_case 'prompt cache: an impl- lane in a reused seat pane has no TTL' code-reviewer
run_launch sh "$launch_sh" opus-high rev-cart "$brief" "$report" 42 wA:p3
expect_rc 0
reuse_seat=$(sed -n 's/^herdr|pane|run|wA:p3|\( .*\)$/\1/p' "$case_dir/calls" | head -n 1)
{ printf '%s\n' 'HERDR-BRIEF role=implementer'; sed 1d "$brief"; } >"$case_dir/brief.next" && cat "$case_dir/brief.next" >"$brief"
: >"$case_dir/calls"
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42 wA:p3
expect_rc 0
reuse_impl=$(sed -n 's/^herdr|pane|run|wA:p3|\( .*\)$/\1/p' "$case_dir/calls" | head -n 1)
case $reuse_seat in *CLAUDE_CODE_PROMPT_CACHE_TTL=5m*) ;; *) fail "seat setup lacks the TTL: $reuse_seat" ;; esac
got=$(sh -c "$reuse_seat; printf 'seat=%s ' \"\${CLAUDE_CODE_PROMPT_CACHE_TTL-unset}\"; $reuse_impl; printf 'impl=%s' \"\${CLAUDE_CODE_PROMPT_CACHE_TTL-unset}\"")
[ "$got" = 'seat=5m impl=unset' ] || fail "reused pane shell kept the seat TTL: $got"
for cache_role in code-reviewer reviewer code-validator; do
    cache_case rev-cart "$cache_role" opus-high claude seat
done
cache_case gather-x researcher haiku-gather claude seat
cache_case impl-cart implementer opus-medium claude plain
cache_case impl-cart mechanical-worker opus-medium claude plain
cache_case rs-facts researcher opus-medium claude plain
cache_case arch-x architect opus-xhigh claude plain
cache_case orch-x sub-orchestrator opus-xhigh claude plain
cache_case rev-cart reviewer sol-medium codex plain
cache_case gather-x researcher luna-gather codex plain
cache_case gather-inventory researcher glm-free opencode plain

new_case 'Luna gather launches as a researcher' researcher
run_launch sh "$launch_sh" luna-gather gather-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-x)" \
    "$(tail_calls gather-x researcher wA:p7 wA:t7 codex gpt-6-luna medium "$setup" \
        "$(codex_flags gpt-6-luna medium "\"$tcwd\"={trust_level=\"trusted\"}")")"

# These callers are workers, not root launchers. The ledger owns their role.
caller_fixture() {
    jq -cn --arg role "$1" --arg name "${2:-rev-parent}" \
        '{id:42,role:$role,name:$name,status:"open"}' >"$STUB_CALLER"
    printf '%s\n' 'HERDR-BRIEF role=reviewer' >"$STUB_CALLER_BRIEF"
    cp "$STUB_CALLER_BRIEF" "$STUB_CALLER_CAPTURE"
    jq -cn --arg path "$STUB_CALLER_BRIEF" \
        '{doc_id:99,task_id:42,kind:"brief",name:"",captured:true,source_path:$path}' >"$STUB_CALLER_DOCS"
}

for caller_role in reviewer researcher; do
    for gather_lane in haiku-gather luna-gather glm-free; do
        new_case "$caller_role caller may launch $gather_lane" researcher
        caller_fixture "$caller_role"
        # A closed child does not count; ready and done children still count.
        printf '%s\n' '{"id":43,"parent_id":42,"status":"ready"}' \
            '{"id":44,"parent_id":42,"status":"done"}' \
            '{"id":45,"parent_id":42,"status":"closed"}' >>"$STUB_CALLER"
        run_launch TASKR_TASK=42 sh "$launch_sh" "$gather_lane" gather-facts "$brief" "$report" 42
        expect_rc 0
        grep -q '^taskr|new|gather-facts|--parent|42|--role|researcher|' "$case_dir/calls" || fail 'helper is not ledgered under caller'
        grep -q '^herdr|agent|start|gather-facts|' "$case_dir/calls" || fail 'no gather helper start'
    done
done

for refusal in non-gather parent nested cap forbidden forbidden-reason forbidden-crlf forbidden-capital forbidden-indented forbidden-bullet forbidden-colon captured-forbidden implementer ledger malformed missing-caller docs no-row two-rows string-id uncaptured get; do
    new_case "worker gather refusal: $refusal" researcher
    caller_fixture reviewer
    helper_lane=haiku-gather helper_name=gather-facts helper_parent=42
    case $refusal in
        non-gather)
            helper_lane=opus-medium helper_name=rs-facts
            reason='may launch only gather- helpers' ;;
        parent) helper_parent=41; reason='must use PARENT=$TASKR_TASK' ;;
        nested) caller_fixture researcher gather-parent; reason='cannot start a helper' ;;
        cap)
            printf '%s\n' '{"id":43,"parent_id":42,"status":"open"}' \
                '{"id":44,"parent_id":42,"status":"ready"}' \
                '{"id":45,"parent_id":42,"status":"done"}' >>"$STUB_CALLER"
            reason='at most 3 open gather helpers' ;;
        forbidden | forbidden-reason | forbidden-crlf | forbidden-capital | forbidden-indented | forbidden-bullet | forbidden-colon | captured-forbidden)
            case $refusal in
                forbidden-capital) printf '%s\n' 'Gather helpers: None' ;;
                forbidden-indented) printf '%s\n' '  Gather helpers: none' ;;
                forbidden-bullet) printf '%s\n' '- Gather helpers: none' ;;
                forbidden-colon) printf '%s\n' 'Gather helpers : none' ;;
                forbidden-reason) printf '%s\n' 'Gather helpers: none (reason).' ;;
                forbidden-crlf) printf 'Gather helpers:\tnone\r\n' ;;
                *) printf '%s\n' 'Gather helpers: none' ;;
            esac >>"$STUB_CALLER_CAPTURE"
            [ "$refusal" = captured-forbidden ] || cp "$STUB_CALLER_CAPTURE" "$STUB_CALLER_BRIEF"
            reason='brief says Gather helpers: none' ;;
        implementer) caller_fixture implementer impl-parent; reason='caller role=implementer cannot start helpers' ;;
        ledger) STUB_FAIL='taskr --json status'; reason='could not read caller task and children' ;;
        malformed) printf '%s\n' 'not JSON' >"$STUB_CALLER"; reason='could not read caller role and name' ;;
        missing-caller) printf '%s\n' '{"record":"daemon"}' >"$STUB_CALLER"; reason='could not read caller role and name' ;;
        docs) STUB_FAIL='taskr doc ls'; reason='could not read caller brief record' ;;
        no-row)
            jq '.task_id = 41' "$STUB_CALLER_DOCS" >"$case_dir/updated"
            mv "$case_dir/updated" "$STUB_CALLER_DOCS"
            reason='could not read captured caller brief id' ;;
        two-rows)
            cat "$STUB_CALLER_DOCS" "$STUB_CALLER_DOCS" >"$case_dir/updated"
            mv "$case_dir/updated" "$STUB_CALLER_DOCS"
            reason='could not read captured caller brief id' ;;
        string-id)
            jq '.doc_id |= tostring' "$STUB_CALLER_DOCS" >"$case_dir/updated"
            mv "$case_dir/updated" "$STUB_CALLER_DOCS"
            reason='could not read captured caller brief id' ;;
        uncaptured)
            jq '.captured = false' "$STUB_CALLER_DOCS" >"$case_dir/updated"
            mv "$case_dir/updated" "$STUB_CALLER_DOCS"
            reason='could not read captured caller brief id' ;;
        get) STUB_FAIL='taskr doc get'; reason='could not read captured caller brief' ;;
    esac
    run_launch TASKR_TASK=42 sh "$launch_sh" "$helper_lane" "$helper_name" "$brief" "$report" "$helper_parent"
    expect_rc 2
    expect_err "$reason"
    grep -q '^herdr|' "$case_dir/calls" && fail 'refused caller reached Herdr'
    grep -q '^taskr|new|' "$case_dir/calls" && fail 'refused caller registered a child'
done

for caller_role in reviewer researcher; do
    for refusal in reserved machine pane workspace; do
        new_case "$caller_role gather refusal: $refusal" researcher
        caller_fixture "$caller_role"
        reason='a lane cannot use --reserved or --machine'
        case $refusal in
            reserved) run_launch TASKR_TASK=42 sh "$launch_sh" --reserved 2468 3579 haiku-gather gather-facts "$brief" "$report" 42 wA:p5 ;;
            machine) run_launch TASKR_TASK=42 sh "$launch_sh" --machine devbox haiku-gather gather-facts "$brief" "$report" 42 wA:p5 ;;
            pane)
                reason='a gather helper uses a new tab in the caller workspace'
                run_launch TASKR_TASK=42 sh "$launch_sh" haiku-gather gather-facts "$brief" "$report" 42 wB:p5 ;;
            workspace)
                reason='a gather helper uses a new tab in the caller workspace'
                run_launch TASKR_TASK=42 sh "$launch_sh" --workspace wB haiku-gather gather-facts "$brief" "$report" 42 ;;
        esac
        expect_rc 2
        expect_err "$reason"
        grep -q '^herdr|' "$case_dir/calls" && fail 'refused caller reached Herdr'
        grep -q '^taskr|new|' "$case_dir/calls" && fail 'refused caller registered a child'
    done
done

for disk_brief in changed missing; do
    new_case "captured brief permits gather with $disk_brief disk brief" researcher
    caller_fixture reviewer
    case $disk_brief in
        changed) printf '%s\n' 'Gather helpers: none' >>"$STUB_CALLER_BRIEF" ;;
        missing) rm "$STUB_CALLER_BRIEF" ;;
    esac
    # Only the matching row's captured document controls the opt-out.
    printf '%s\n' '{"doc_id":100,"task_id":41,"kind":"brief","name":"","captured":false}' >>"$STUB_CALLER_DOCS"
    printf '%s\n' 'Gather helpers: nonetheless allowed' 'A brief may say Gather helpers: none' >>"$STUB_CALLER_CAPTURE"
    run_launch TASKR_TASK=42 sh "$launch_sh" haiku-gather gather-facts "$brief" "$report" 42
    expect_rc 0
    grep -q '^taskr|doc|get|99$' "$case_dir/calls" || fail 'did not read matching captured brief'
    grep -q '^herdr|tab|create|--workspace|wA|' "$case_dir/calls" || fail 'helper is not in caller workspace'
    grep -q '^herdr|agent|start|gather-facts|' "$case_dir/calls" || fail 'no gather helper start'
done

# --gather writes the helper brief beside the caller's report and launches
# through the same caller checks. gather_fixture ROLE adds the report path.
gather_fixture() {
    caller_fixture "$1"
    jq -c --arg r "$work/rev.md" '.report_path = $r' "$STUB_CALLER" >"$case_dir/updated"
    mv "$case_dir/updated" "$STUB_CALLER"
    gather_brief=$work/gather/42/callers.brief.md gather_report=$work/gather/42/callers.md
}
for caller_role in reviewer researcher; do
    new_case "$caller_role --gather writes the brief and launches haiku-gather" researcher
    gather_fixture "$caller_role"
    run_launch TASKR_TASK=42 sh "$launch_sh" --gather callers 'List callers of foo
outside the diff'
    expect_rc 0
    [ "$(sed -n 2p "$gather_brief")" = "orchestrator=42   report=$gather_report" ] || fail 'brief report header'
    grep -qx 'HERDR-BRIEF role=researcher' "$gather_brief" || fail 'brief role'
    grep -qx 'Objective: Gather read-only: List callers of foo outside the diff' "$gather_brief" || fail 'question is not one Objective line'
    grep -qx 'Gather helpers: none' "$gather_brief" || fail 'helper brief allows nested helpers'
    grep -qF "taskr|new|gather-callers|--parent|42|--role|researcher|--workspace|wA|--tab|wA:t7|--pane|wA:p7|--cwd|$work|--brief|$gather_brief|--report|$gather_report" \
        "$case_dir/calls" || fail 'helper is not ledgered under caller with the written brief'
    grep -qF '|--model|claude-haiku-5-5|' "$case_dir/calls" || fail 'default lane is not haiku-gather'
    grep -qF "herdr|pane|run|wA:p7|$seat_setup && $agent_tmp_setup && herdr_launch_ok=3579" "$case_dir/calls" || fail 'helper lacks the 5-minute cache'
    expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default
report=$gather_report
taskr close 2468 --outcome accepted   # or rejected
herdr tab close wA:t7"
done

new_case '--gather: two callers with one slug get separate files' researcher
gather_fixture reviewer
run_launch TASKR_TASK=42 sh "$launch_sh" --gather callers 'QUESTION ONE'
expect_rc 0
jq -c '.id = 43' "$STUB_CALLER" >"$case_dir/updated" && mv "$case_dir/updated" "$STUB_CALLER"
jq -c '.task_id = 43' "$STUB_CALLER_DOCS" >"$case_dir/updated" && mv "$case_dir/updated" "$STUB_CALLER_DOCS"
run_launch TASKR_TASK=43 sh "$launch_sh" --gather callers 'QUESTION TWO'
expect_rc 0
for seat in 42:ONE 43:TWO; do
    grep -qx "Objective: Gather read-only: QUESTION ${seat#*:}" "$work/gather/${seat%:*}/callers.brief.md" \
        || fail "seat ${seat%:*} brief lost its question"
    grep -qF "|--parent|${seat%:*}|" "$case_dir/calls" || fail "seat ${seat%:*} not ledgered"
    grep -qF "|--brief|$work/gather/${seat%:*}/callers.brief.md|--report|$work/gather/${seat%:*}/callers.md" \
        "$case_dir/calls" || fail "seat ${seat%:*} paths not its own"
done

for lane_source in env flag; do
    new_case "--gather lane from $lane_source" researcher
    gather_fixture reviewer
    case $lane_source in
        env) run_launch TASKR_TASK=42 HERDR_GATHER_LANE=luna-gather sh "$launch_sh" --gather callers 'Q' ;;
        flag) run_launch TASKR_TASK=42 HERDR_GATHER_LANE=glm-free sh "$launch_sh" --gather callers 'Q' --lane luna-gather ;;
    esac
    expect_rc 0
    grep -qF '|--kind|codex|' "$case_dir/calls" || fail 'lane override not used'
done

for refusal in forbidden no-task slug dotdot blank long-slug no-question lane-alone args report-exists; do
    new_case "--gather refusal: $refusal" researcher
    gather_fixture reviewer
    set -- TASKR_TASK=42 sh "$launch_sh" --gather callers 'Q'
    case $refusal in
        forbidden) printf '%s\n' 'Gather helpers: none' >>"$STUB_CALLER_CAPTURE"; reason='brief says Gather helpers: none' ;;
        no-task) shift; reason='--gather needs TASKR_TASK' ;;
        slug) set -- TASKR_TASK=42 sh "$launch_sh" --gather Callers 'Q'; reason="--gather slug 'Callers' must match" ;;
        dotdot) set -- TASKR_TASK=42 sh "$launch_sh" --gather ../x 'Q'; reason="--gather slug '../x' must match" ;;
        blank) set -- TASKR_TASK=42 sh "$launch_sh" --gather callers "$(printf ' \n\t')"; reason='--gather QUESTION is only whitespace' ;;
        long-slug) set -- TASKR_TASK=42 sh "$launch_sh" --gather abcdefghijklmnopqrstuvwxyz 'Q'; reason='longer than 25' ;;
        no-question) set -- TASKR_TASK=42 sh "$launch_sh" --gather callers ''; reason='needs SLUG QUESTION' ;;
        lane-alone) set -- TASKR_TASK=42 sh "$launch_sh" --lane haiku-gather haiku-gather gather-x "$brief" "$report" 42; reason='--lane applies only with --gather' ;;
        args) set -- "$@" haiku-gather gather-x "$brief" "$report" 42; reason='--gather takes no LANE' ;;
        report-exists) mkdir -p "${gather_report%/*}"; : >"$gather_report"; reason='choose another SLUG' ;;
    esac
    run_launch "$@"
    expect_rc 2
    expect_err "$reason"
    case $refusal in slug | dotdot | blank | long-slug | no-question)
        [ ! -e "$work/gather" ] || fail 'refused --gather wrote a file' ;;
    esac
    grep -q '^herdr|' "$case_dir/calls" && fail 'refused --gather reached Herdr'
    grep -q '^taskr|new|' "$case_dir/calls" && fail 'refused --gather registered a child'
done

for caller_role in orchestrator sub-orchestrator; do
    new_case "$caller_role caller keeps non-gather launch authority"
    caller_fixture "$caller_role" orch-parent
    run_launch TASKR_TASK=42 sh "$launch_sh" opus-medium impl-fix "$brief" "$report" 42
    expect_rc 0
    grep -q '^herdr|agent|start|impl-fix|' "$case_dir/calls" || fail 'no implementer start'
done

new_case 'sub-orchestrator caller keeps remote launch authority'
caller_fixture sub-orchestrator orch-parent
run_launch TASKR_TASK=42 sh "$launch_sh" --machine devbox opus-medium impl-remote "$brief" "$report" 42 wA:p3
expect_rc 0
expect_machine_calls
grep -q '^herdr|--machine|devbox|agent|start|impl-remote|' "$case_dir/calls" || fail 'no remote implementer start'

new_case 'opus-xhigh launches an orch- sub-orchestrator' sub-orchestrator
run_launch sh "$launch_sh" opus-xhigh orch-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls orch-x)" \
    "$(tail_calls orch-x sub-orchestrator wA:p7 wA:t7 claude claude-opus-5-5 xhigh "$claude_setup" \
        "|$(claude_flags xhigh '' orch)")"

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
pane_setup=$(sed -n 's/^herdr|pane|run|wA:p7|\( .*\)$/\1/p' "$case_dir/calls" | head -n 1)
want='herdr-launch-env=2468/3579/worker/3579'
got=$(bash --noprofile --norc -c "$pane_setup; $env_check")
[ "$got" = "$want" ] || fail "a clean shell echoed '$got', expected '$want'"
got=$(bash --noprofile --norc -c "$pane_setup; printenv CLAUDE_CODE_DISABLE_ADVISOR_TOOL")
[ "$got" = 1 ] || fail "advisor disable flag was not exported to a child process: $got"
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
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 account=default'
expect_calls "herdr|pane|get|wA:p3" \
    "herdr|pane|process-info|--pane|wA:p3" "herdr|pane|send-keys|wA:p3|ctrl+c" \
    "$(tail_calls orch-cart sub-orchestrator wA:p3 wA:t5 claude claude-opus-5-5 high "$claude_setup" \
        "|$(claude_flags high '' orch)")"

new_case 'given pane in another workspace registers that workspace and tab'
STUB_PANE_WS=wB
ws=wB
run_launch sh "$launch_sh" opus-high impl-cart "$brief" "$report" 42 wB:p1
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wB:t5 pane=wB:p1 account=default'
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
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 account=default'

new_case '--workspace and --cwd create the tab there without asking for the current pane'
mkdir -p "$case_dir/task"
ws=wT
tcwd=$(CDPATH= cd "$case_dir/task" && pwd -P)
run_launch sh "$launch_sh" --workspace wT --cwd "$case_dir/task" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wT:t7 pane=wT:p7 account=default'
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
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'

new_case 'misleading tags: the n1 and l1 digits are never read as ids'
STUB_NEW='n1 2468' STUB_LAUNCH='l1 {"replaced_launch_id":11,"launch_id":3579,"status":"open"}'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'

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

# Step 7 presses Enter once, only after a --confirm no_receipt; the brief goes once.
step7_calls() {
    printf '%s\n' "$(tab_calls impl-cart)" \
        "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "|$(claude_flags medium)")" "$@"
}
step7_count() { grep -c -F -x -- "$1" "$case_dir/calls" || true; }
enter='herdr|pane|send-keys|wA:p7|Enter'
log_call='taskr|log|2468|--json|--limit|5'

new_case 'step 7: got within the first wait presses no Enter'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
expect_calls "$(step7_calls)"

new_case 'step 7: no_receipt, then a got after one Enter'
STUB_PROMPT=no_receipt STUB_LOG_GOT=1 STUB_SCREEN='pasted brief, not submitted'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 submit_retry=enter account=default'
expect_err 'pasted brief, not submitted'
expect_calls "$(step7_calls 'herdr|pane|read|wA:p7|--source|recent-unwrapped|--lines|40' "$enter" "$log_call")"

new_case 'step 7: no_receipt and no got after one Enter fails, never resends'
STUB_PROMPT=no_receipt
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
[ -z "$out" ] || fail "stdout not empty: $out"
expect_err 'step 7 (no receipt 60 s after one Enter; inspect the pane, never resend) failed'
[ "$(step7_count "$enter")" -eq 1 ] || fail "Enter pressed $(step7_count "$enter") times"
[ "$(grep -c '^taskr|prompt|' "$case_dir/calls")" -eq 1 ] || fail 'brief sent more than once'
n=$(step7_count "$log_call")
[ "$n" -ge 1 ] && [ "$n" -le 30 ] || fail "$n receipt polls, expected 1 to 30"

new_case 'step 7: rejected presses no Enter'
STUB_PROMPT=rejected
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err 'step 7 (taskr prompt) failed'
expect_calls "$(step7_calls)"

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
expect_err "--cwd '$root/no-such-dir' is not a directory"
check_refused --cwd "$root" opus-medium impl-x "$brief" "$report" 42 wA:p3
expect_err '--cwd applies only without PANE'

new_case '--lanes lists the source table in lane order without calls'
run_launch sh "$launch_sh" --lanes
expect_rc 0
expect_out "Lane table: $repo_dir/bin/../lanes.json
$(jq -r 'del(.deny, .pins) | to_entries | sort_by(.key)[] | [.key, .value.kind, .value.model, .value.effort, (.value.prefixes | join(" "))] | @tsv' "$repo_dir/lanes.json")"
expect_no_calls

new_case '--lanes shows fable-high with the hub prefix'
run_launch sh "$launch_sh" --lanes
expect_rc 0
printf '%s\n' "$out" | grep -Fx "$(printf 'fable-high\tclaude\tclaude-fable-5-2\thigh\trev hub')" >/dev/null || fail 'fable-high lacks the hub prefix'
expect_no_calls

new_case 'hub names are refused even with a hub-capable lane' orchestrator
run_launch sh "$launch_sh" fable-high hub-cart "$brief" "$report" 42
expect_rc 2
expect_err "name prefix 'hub-' is not"
expect_no_calls

new_case 'fable-high cannot launch an orch sub-orchestrator' sub-orchestrator
run_launch sh "$launch_sh" fable-high orch-cart "$brief" "$report" 42
expect_rc 2
expect_err 'lane fable-high does not launch as orch-'
expect_no_calls

new_case 'prefix refusal lists the source lanes that allow rev-'
run_launch sh "$launch_sh" sol-high rev-cart "$brief" "$report" 42
expect_rc 2
expect_err 'lane sol-high does not launch as rev- (allowed: impl orch; rev- lanes: astra-high fable-high opus-high opus-max opus-medium opus-xhigh sol-medium)'
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
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
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
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config account=default"
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "$(claude_push_flags)")"
printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'private config content is wrong'
[ "$(ls -l "$mcp_config" | cut -c 1-10)" = -rw------- ] || fail 'private config mode is not 0600'
[ ! -e "$work/.mcp.json" ] || fail 'cwd .mcp.json was written'
[ "$(find "${mcp_config%/*}" -type f | wc -l)" -eq 1 ] || fail 'temporary config was left behind'

# W2316: rs- and arch- Claude lanes keep the user and project MCP servers
# (no --strict-mcp-config); a push lane still adds its taskr --mcp-config.
# Every other Claude lane stays strict.
new_case 'claude rs- lane keeps MCP servers' researcher
run_launch sh "$launch_sh" opus-medium rs-facts "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls rs-facts)" \
    "$(tail_calls rs-facts researcher wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' mcp)")"

new_case 'claude arch- lane keeps MCP servers' researcher
run_launch sh "$launch_sh" opus-xhigh arch-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls arch-x)" \
    "$(tail_calls arch-x researcher wA:p7 wA:t7 claude claude-opus-5-5 xhigh "$claude_setup" \
        "|$(claude_flags xhigh '' '' mcp)")"

new_case 'claude arch- --push lane keeps MCP servers and adds the taskr config' researcher
run_launch sh "$launch_sh" --push opus-xhigh arch-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls arch-x)" \
    "$(tail_calls arch-x researcher wA:p7 wA:t7 claude claude-opus-5-5 xhigh "$claude_setup" \
        "|$(claude_flags xhigh '' '' mcp)|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr")"
printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'private config content is wrong'

# W2317: --mcp keeps the MCP servers for one claude lane of any name; every
# other flag is unchanged, and a non-claude lane is refused before any call.
new_case 'claude impl- --mcp lane keeps MCP servers'
run_launch sh "$launch_sh" --mcp opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' mcp)")"

new_case 'claude impl- --push --mcp lane keeps MCP servers and adds the taskr config'
run_launch sh "$launch_sh" --push --mcp opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' mcp)|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr")"
printf '%s\n' "$mcp_want" | cmp -s - "$mcp_config" || fail 'private config content is wrong'

new_case 'claude arch- --mcp lane is unchanged' researcher
run_launch sh "$launch_sh" --mcp opus-xhigh arch-x "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls arch-x)" \
    "$(tail_calls arch-x researcher wA:p7 wA:t7 claude claude-opus-5-5 xhigh "$claude_setup" \
        "|$(claude_flags xhigh '' '' mcp)")"

new_case 'codex --mcp lane refuses before any call' reviewer
run_launch sh "$launch_sh" --mcp sol-medium rev-cart "$brief" "$report" 42
expect_rc 2
expect_err '--mcp supports claude lanes only; lane sol-medium is codex'
expect_no_calls

for cheap_lane in glm-free glm-flash; do
    new_case "OpenCode --mcp for $cheap_lane refuses before any call"
    run_launch sh "$launch_sh" --mcp "$cheap_lane" glm-inventory "$brief" "$report" 42
    expect_rc 2
    expect_err "--mcp supports claude lanes only; lane $cheap_lane is opencode"
    expect_no_calls
done

new_case '--mcp does not combine with --check'
run_launch sh "$launch_sh" --mcp --check opus-medium
expect_rc 2
expect_err '--mcp does not combine with --check'
expect_no_calls

# W3332: --deny-tool NAME (repeatable, claude lanes only) adds one
# --disallowedTools NAME pair per name, in order, right after --tools; every
# other element is unchanged.
pscale_write=mcp__planetscale__planetscale_execute_write_query
new_case 'claude impl- --deny-tool adds one --disallowedTools pair'
run_launch sh "$launch_sh" --deny-tool "$pscale_write" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' '' "|--disallowedTools|$pscale_write")")"

# A glob name with a matching file in the cwd and a name with parentheses:
# each stays one literal element, in the order given.
new_case 'claude --deny-tool twice keeps both names literal and in order'
: >"$work/mcp__srv__file"
run_launch sh "$launch_sh" --deny-tool 'mcp__srv__*' --mcp --deny-tool 'Bash(rm:*)' opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' mcp '|--disallowedTools|mcp__srv__*|--disallowedTools|Bash(rm:*)')")"

new_case 'haiku-gather --mcp --deny-tool lane (the W3332 first use)' researcher
run_launch sh "$launch_sh" --mcp --deny-tool "$pscale_write" haiku-gather gather-probe "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-probe)" \
    "$(tail_calls gather-probe researcher wA:p7 wA:t7 claude claude-haiku-5-5 medium "$seat_setup" \
        "|$(claude_flags medium claude-haiku-5-5 '' mcp "|--disallowedTools|$pscale_write")")"

new_case 'claude --push --deny-tool keeps the taskr config after --settings'
long_name=$(printf '%0200d' 0)
run_launch sh "$launch_sh" --push --deny-tool "$long_name" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium '' '' '' "|--disallowedTools|$long_name")|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr")"

# Without --deny-tool the argv is the one main passed, for a plain, rs-,
# push and --mcp lane.
for deny_case in plain rs push mcp; do
    deny_role=implementer deny_name=impl-cart deny_flags=$(claude_flags medium) deny_opt=
    case $deny_case in
        rs) deny_role=researcher deny_name=rs-cart deny_flags=$(claude_flags medium '' '' mcp) ;;
        push) deny_opt=--push ;;
        mcp) deny_opt=--mcp deny_flags=$(claude_flags medium '' '' mcp) ;;
    esac
    new_case "claude $deny_case lane without --deny-tool is unchanged" "$deny_role"
    deny_tail=
    [ "$deny_case" != push ] || deny_tail="|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr"
    run_launch sh "$launch_sh" $deny_opt opus-medium "$deny_name" "$brief" "$report" 42
    expect_rc 0
    expect_calls "$(tab_calls "$deny_name")" \
        "$(tail_calls "$deny_name" "$deny_role" wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
            "|$deny_flags$deny_tail")"
    ! grep -q -F -- '--disallowedTools' "$case_dir/calls" || fail 'argv names --disallowedTools'
done

# Each refusal exits 2 before any herdr or taskr call.
deny_refusal() {
    new_case "$1" "${2:-implementer}"
    shift 2
    run_launch sh "$launch_sh" "$@"
    expect_rc 2
    expect_no_calls
}
deny_charset="must be 1-200 characters of A-Za-z0-9_.:*()- and not start with -"
deny_refusal 'codex --deny-tool lane refuses' reviewer --deny-tool Edit sol-medium rev-cart "$brief" "$report" 42
expect_err '--deny-tool supports claude lanes only; lane sol-medium is codex'
deny_refusal 'OpenCode --deny-tool lane refuses' '' --deny-tool Edit glm-free glm-inventory "$brief" "$report" 42
expect_err '--deny-tool supports claude lanes only; lane glm-free is opencode'
deny_refusal '--deny-tool before --check refuses' '' --deny-tool Edit --check opus-medium
expect_err '--deny-tool does not combine with --check'
deny_refusal '--deny-tool after --check refuses' '' --check opus-medium --deny-tool Edit
expect_err '--deny-tool does not combine with --check'
deny_refusal '--deny-tool before --quota refuses' '' --deny-tool Edit --quota
expect_err '--deny-tool does not combine with --quota'
deny_refusal '--deny-tool after --quota refuses' '' --quota --deny-tool Edit
expect_err '--deny-tool does not combine with --quota'
deny_refusal '--deny-tool with --gather refuses' researcher --deny-tool Edit --gather facts 'what is x'
expect_err '--deny-tool does not combine with --gather'
deny_refusal '--deny-tool without NAME refuses' '' --deny-tool
expect_err '--deny-tool needs NAME'
for bad_name in '' 'Bash(git *)' 'Edit,Write' '--print' '-x' "$(printf 'Edit\nWrite')" 'a|b' "$(printf 'caf\303\251')" "a'b"; do
    deny_refusal "--deny-tool refuses NAME [$bad_name]" '' --deny-tool "$bad_name" opus-medium impl-cart "$brief" "$report" 42
    expect_err "$deny_charset"
done
deny_refusal '--deny-tool refuses a 201-character NAME' '' --deny-tool "${long_name}0" opus-medium impl-cart "$brief" "$report" 42
expect_err '--deny-tool NAME is longer than 200 characters'

# --plugin NAME (repeatable, claude lanes only) turns enabledPlugins NAME true
# in --settings and drops --strict-mcp-config; --compact-window N adds
# "autoCompactWindow":N. recorded_settings prints the --settings value the
# stub recorded, which must parse as JSON.
recorded_settings() {
    grep '^herdr|agent|start|' "$case_dir/calls" | tr '|' '\n' | sed -n '/^--settings$/{n;p;}'
}
expect_settings_jq() {
    settings_got=$(recorded_settings)
    printf '%s\n' "$settings_got" | jq -e . >/dev/null 2>&1 || fail "recorded --settings does not parse: $settings_got"
    printf '%s\n' "$settings_got" | jq -e "$@" >/dev/null 2>&1 || fail "recorded --settings fails $*: $settings_got"
}
slack=slack@claude-plugins-official
plugin_settings() {
    claude_settings | sed "s/\"$slack\":false/\"$slack\":true/"
}
lean_tools='--model|claude-opus-5-5|--effort|medium|--dangerously-skip-permissions|--tools|Bash,Read,Edit,Write,Grep,Glob,ToolSearch,Skill,WebFetch,WebSearch'
new_case 'claude --plugin slack turns only the slack key true and drops --strict-mcp-config'
run_launch sh "$launch_sh" --plugin "$slack" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$lean_tools|--settings|$(plugin_settings)")"
expect_settings_jq --arg s "$slack" '.enabledPlugins[$s] == true and ([.enabledPlugins[] | select(.)] | length) == 1 and (.enabledPlugins | length) == 6'
! grep -q -F -- '--strict-mcp-config' "$case_dir/calls" || fail 'argv names --strict-mcp-config'

new_case 'claude --mcp --plugin drops --strict-mcp-config'
run_launch sh "$launch_sh" --mcp --plugin "$slack" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$lean_tools|--settings|$(plugin_settings)")"
! grep -q -F -- '--strict-mcp-config' "$case_dir/calls" || fail 'argv names --strict-mcp-config'

new_case 'claude --plugin accepts a 200-character NAME'
run_launch sh "$launch_sh" --plugin "$long_name" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_settings_jq --arg n "$long_name" '.enabledPlugins[$n] == true and (.enabledPlugins | length) == 7'
! grep -q -F -- '--strict-mcp-config' "$case_dir/calls" || fail 'argv names --strict-mcp-config'

new_case 'claude --plugin with an unlisted name adds it once'
run_launch sh "$launch_sh" --plugin new-plugin@market opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_settings_jq '.enabledPlugins["new-plugin@market"] == true and (.enabledPlugins | length) == 7 and ([.enabledPlugins[] | select(.)] | length) == 1'
[ "$(recorded_settings | grep -o '"new-plugin@market"' | wc -l)" -eq 1 ] || fail 'added key is not there exactly once'
! grep -q -F -- '--strict-mcp-config' "$case_dir/calls" || fail 'argv names --strict-mcp-config'

new_case 'claude --plugin twice (and a repeat) turns both true, no duplicate key'
run_launch sh "$launch_sh" --plugin "$slack" --plugin stripe@claude-plugins-official --plugin "$slack" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_settings_jq '([.enabledPlugins | to_entries[] | select(.value) | .key] | sort) == ["slack@claude-plugins-official", "stripe@claude-plugins-official"] and (.enabledPlugins | length) == 6'
[ "$(recorded_settings | grep -o "\"$slack\"" | wc -l)" -eq 1 ] || fail 'slack key is duplicated'

new_case 'haiku-gather --plugin --deny-tool lane (the read-only Slack use)' researcher
run_launch sh "$launch_sh" --plugin "$slack" --deny-tool mcp__plugin_slack_slack__slack_send_message haiku-gather gather-slack "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls gather-slack)" \
    "$(tail_calls gather-slack researcher wA:p7 wA:t7 claude claude-haiku-5-5 medium "$seat_setup" \
        "|$(printf '%s' "$lean_tools" | sed 's/opus/haiku/')|--disallowedTools|mcp__plugin_slack_slack__slack_send_message|--settings|$(plugin_settings)")"

new_case 'claude --push --plugin keeps the taskr config and drops --strict-mcp-config'
run_launch sh "$launch_sh" --push --plugin "$slack" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$lean_tools|--settings|$(plugin_settings)|--mcp-config|$mcp_config|--dangerously-load-development-channels|server:taskr")"

new_case 'claude lane without --plugin or --compact-window keeps --strict-mcp-config and the settings'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
grep -q -F -- '|--strict-mcp-config|' "$case_dir/calls" || fail 'argv lacks --strict-mcp-config'
[ "$(recorded_settings)" = "$(claude_settings)" ] || fail 'settings changed without flags'
expect_settings_jq '([.enabledPlugins[] | select(.)] | length) == 0 and (has("autoCompactWindow") | not)'

new_case 'claude --compact-window 100000 lands in --settings'
run_launch sh "$launch_sh" --compact-window 100000 opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|$(claude_flags medium | sed 's/"autoCompactEnabled":true,/"autoCompactEnabled":true,"autoCompactWindow":100000,/')")"
expect_settings_jq '.autoCompactWindow == 100000 and ([.enabledPlugins[] | select(.)] | length) == 0'

new_case 'claude --compact-window 100000 with --plugin'
run_launch sh "$launch_sh" --compact-window 100000 --plugin "$slack" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_settings_jq --arg s "$slack" '.autoCompactWindow == 100000 and .enabledPlugins[$s] == true'
new_case 'claude --compact-window 1000000 passes'
run_launch sh "$launch_sh" --compact-window 1000000 opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_settings_jq '.autoCompactWindow == 1000000'

plugin_charset="must be 1-200 characters of A-Za-z0-9_.@:- and not start with -"
deny_refusal 'codex --plugin lane refuses' reviewer --plugin "$slack" sol-medium rev-cart "$brief" "$report" 42
expect_err '--plugin supports claude lanes only; lane sol-medium is codex'
deny_refusal 'OpenCode --plugin lane refuses' '' --plugin "$slack" glm-free glm-inventory "$brief" "$report" 42
expect_err '--plugin supports claude lanes only; lane glm-free is opencode'
deny_refusal 'codex --compact-window lane refuses' reviewer --compact-window 100000 sol-medium rev-cart "$brief" "$report" 42
expect_err '--compact-window supports claude lanes only; lane sol-medium is codex'
deny_refusal '--plugin before --check refuses' '' --plugin "$slack" --check opus-medium
expect_err '--plugin does not combine with --check'
deny_refusal '--plugin after --check refuses' '' --check opus-medium --plugin "$slack"
expect_err '--plugin does not combine with --check'
deny_refusal '--compact-window before --check refuses' '' --compact-window 100000 --check opus-medium
expect_err '--compact-window does not combine with --check'
deny_refusal '--compact-window after --check refuses' '' --check opus-medium --compact-window 100000
expect_err '--compact-window does not combine with --check'
deny_refusal '--plugin with --quota refuses' '' --plugin "$slack" --quota
expect_err '--plugin does not combine with --quota'
deny_refusal '--compact-window with --quota refuses' '' --compact-window 100000 --quota
expect_err '--compact-window does not combine with --quota'
deny_refusal '--plugin with --gather refuses' researcher --plugin "$slack" --gather facts 'what is x'
expect_err '--plugin does not combine with --gather'
deny_refusal '--compact-window with --gather refuses' researcher --compact-window 100000 --gather facts 'what is x'
expect_err '--compact-window does not combine with --gather'
deny_refusal '--plugin without NAME refuses' '' --plugin
expect_err '--plugin needs NAME'
for bad_name in '' 'slack plugin' 'a"b' 'a,b' '--print' '-x' "$(printf 'a\nb')" 'a}b' 'a*' "$(printf 'caf\303\251')"; do
    deny_refusal "--plugin refuses NAME [$bad_name]" '' --plugin "$bad_name" opus-medium impl-cart "$brief" "$report" 42
    expect_err "$plugin_charset"
done
deny_refusal '--plugin refuses a 201-character NAME' '' --plugin "${long_name}0" opus-medium impl-cart "$brief" "$report" 42
expect_err '--plugin NAME is longer than 200 characters'
deny_refusal '--compact-window without N refuses' '' --compact-window
expect_err '--compact-window needs N once'
deny_refusal '--compact-window twice refuses' '' --compact-window 100000 --compact-window 200000 opus-medium impl-cart "$brief" "$report" 42
expect_err '--compact-window needs N once'
for bad_n in abc 5 9999 10000 99999 1000001 010000 0100000 '' -100000 1e5 '100000 ' 99999999999999999999; do
    deny_refusal "--compact-window refuses N [$bad_n]" '' --compact-window "$bad_n" opus-medium impl-cart "$brief" "$report" 42
    expect_err "--compact-window N '$bad_n' must be an integer 100000-1000000"
done

new_case 'claude impl- lane stays strict'
run_launch sh "$launch_sh" opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" \
        "|--model|claude-opus-5-5|--effort|medium|--dangerously-skip-permissions|--tools|Bash,Read,Edit,Write,Grep,Glob,ToolSearch,Skill,WebFetch,WebSearch|--strict-mcp-config|--settings|$(claude_settings)")"

new_case 'claude --push leaves a tracked project .mcp.json untouched'
project_mcp='{"mcpServers":{"project":{"command":"project-mcp"}}}'
printf '%s\n' "$project_mcp" >"$work/.mcp.json"
git -C "$work" init -q
git -C "$work" add .mcp.json
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config account=default"
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
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config account=default"
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup" "$(claude_push_flags)" \
        | sed '$d')" \
    "herdr|pane|read|wA:p7|--source|visible|--lines|40" \
    "herdr|pane|send-keys|wA:p7|Enter" \
    "herdr|agent|wait|impl-cart|--until|idle|--timeout|90000" \
    "taskr|prompt|2468|--file|$brief|--confirm|--confirm-timeout|60000"

new_case 'claude --push with a picked account reads the dialog screen once'
account_codator "$home"
usage_fixture "$home" seat 0
STUB_START_OK=0 STUB_SCREEN='WARNING: Loading development channels
> 1. I am using this for local development'
run_launch sh "$launch_sh" --push opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 mcp_config=$mcp_config account=seat drain=later"
expect_protect_warning missing
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium "$claude_setup$forward" "|--account|seat$(claude_push_flags)" \
        | sed '$d')" \
    "herdr|pane|read|wA:p7|--source|visible|--lines|40" \
    "herdr|pane|send-keys|wA:p7|Enter" \
    "herdr|agent|wait|impl-cart|--until|idle|--timeout|90000" \
    "taskr|prompt|2468|--file|$brief|--confirm|--confirm-timeout|60000"

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

# Captured from the Codex 0.166.0 update dialog offering 0.169.0.
codex_update_screen='  Update available · 0.166.0 → 0.169.0
  Release notes:
  https://github.com/openai/codex/releases/lat
  est

› 1. Update now (runs `npm install -g @openai/
     codex`)
  2. Skip
  3. Skip until next version

  enter continue · esc skip'
new_case 'codex skips the update dialog once before prompting'
STUB_START_OK=0 STUB_SCREEN=$codex_update_screen
run_launch sh "$launch_sh" sol-high impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 account=default'
expect_calls "$(tab_calls impl-cart)" \
    "$(tail_calls impl-cart implementer wA:p7 wA:t7 codex gpt-6.1-sol high "$setup" \
        "$(codex_flags gpt-6.1-sol high "\"$tcwd\"={trust_level=\"trusted\"}")" | sed '$d')" \
    "herdr|pane|read|wA:p7|--source|visible|--lines|40" \
    "herdr|pane|send-keys|wA:p7|esc" \
    "herdr|agent|wait|impl-cart|--until|idle|--timeout|30000" \
    "taskr|prompt|2468|--file|$brief|--confirm|--confirm-timeout|60000"

for blocked_screen in 'Trust this directory?' \
    'Update available · 0.166.0 → 0.169.0' '  3. Skip until next version' \
    'Other dialog: Update available · 0.166.0 → 0.169.0
  3. Skip until next version'; do
    new_case "codex leaves another blocked screen alone: $blocked_screen"
    STUB_START_OK=0 STUB_SCREEN=$blocked_screen
    run_launch sh "$launch_sh" sol-high impl-cart "$brief" "$report" 42
    expect_rc 1
    expect_err 'step 6 (herdr agent start) failed'
    grep -q '^herdr|pane|send-keys' "$case_dir/calls" && fail 'Escape was sent'
    grep -q '^herdr|agent|wait' "$case_dir/calls" && fail 'idle wait ran'
    grep -q '^taskr|prompt' "$case_dir/calls" && fail 'prompt ran'
done

for failure in 'pane read' 'pane send-keys' 'agent wait'; do
    new_case "codex update dialog recovery stops on failed $failure"
    STUB_START_OK=0 STUB_SCREEN=$codex_update_screen STUB_FAIL="herdr $failure"
    run_launch sh "$launch_sh" sol-high impl-cart "$brief" "$report" 42
    expect_rc 1
    case $failure in
        'pane read') expect_err 'step 6 (herdr agent start, then herdr pane read) failed' ;;
        'pane send-keys') expect_err 'step 6 (herdr pane send-keys esc at the Codex update dialog) failed' ;;
        'agent wait') expect_err 'step 6 (the agent was not idle within 30 s after the Codex update dialog) failed' ;;
    esac
    grep -q '^taskr|prompt' "$case_dir/calls" && fail 'prompt ran'
    [ "$(grep -c '^herdr|pane|send-keys|wA:p7|esc$' "$case_dir/calls")" -le 1 ] \
        || fail 'Escape was retried'
done

codex_bin=$work/codex
printf '%s\n' '#!/bin/sh' >"$codex_bin"
chmod 0755 "$codex_bin"
sidecar="export TASKR_TASK=2468 TASKR_LAUNCH=3579 && i=0 && until taskr log 2468 --limit 1 | grep -q -F '\"native_home\":\"/'; do i=\$((i + 1)); if [ \$i -ge 360 ]; then echo 'herdr-launch: the session never ran taskr start; no push sidecar'; exit 1; fi; sleep 5; done && exec taskr push codex --codex-bin '$codex_bin'"
new_case 'codex --push starts the push sidecar in its own tab (step 8)'
printf '%s\n' 'Run taskr start first.' >>"$brief"
run_launch HERDR_PUSH_CODEX_BIN="$codex_bin" sh "$launch_sh" --push sol-high impl-cart "$brief" "$report" 42
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t7 pane=wA:p7 push_tab=wA:t8 push_pane=wA:p8 account=default'
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
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p3 account=default'
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

# pinned_table [PINS]: two impl lanes and optional pins.
pinned_table() {
    mkdir -p "$home/.config/herdr-fleet"
    printf '{"pinned":{"kind":"codex","model":"synthetic-pinned","effort":"high","prefixes":["impl"]},"other":{"kind":"codex","model":"synthetic-other","effort":"high","prefixes":["impl"]}%s}\n' \
        "${1:+,\"pins\":$1}" >"$home/.config/herdr-fleet/lanes.json"
}

new_case 'lane pin refuses a pinned name on another lane before control-plane calls'
pinned_table '{"impl-weekly":"pinned"}'
run_launch sh "$launch_sh" other impl-weekly-x "$brief" "$report" 42
expect_rc 2
expect_out ''
[ "$err" = "herdr-launch: task 'impl-weekly-x' matches pin 'impl-weekly', which requires lane 'pinned'; given lane 'other'" ] ||
    fail "pin refusal line: $err"
expect_no_calls

new_case 'lane pin accepts a pinned name on its lane'
pinned_table '{"impl-weekly":"pinned"}'
run_launch sh "$launch_sh" pinned impl-weekly-x "$brief" "$report" 42
expect_rc 0
grep -q -F '|--model|synthetic-pinned|--effort|high' "$case_dir/calls" || fail 'pinned lane metadata missing'

new_case 'lane pin leaves names without a pinned prefix unchanged'
pinned_table '{"impl-weekly":"pinned"}'
run_launch sh "$launch_sh" other impl-cart "$brief" "$report" 42
expect_rc 0
grep -q -F '|--model|synthetic-other|--effort|high' "$case_dir/calls" || fail 'unpinned lane metadata missing'

new_case 'lane pin: the longest matching pinned prefix names the refused lane'
pinned_table '{"impl-":"other","impl-weekly":"pinned"}'
run_launch sh "$launch_sh" other impl-weekly-x "$brief" "$report" 42
expect_rc 2
expect_err "matches pin 'impl-weekly', which requires lane 'pinned'; given lane 'other'"
expect_no_calls

new_case 'lane pin: nested pins start a name on the longest pin lane'
pinned_table '{"impl-":"other","impl-weekly":"pinned"}'
run_launch sh "$launch_sh" pinned impl-weekly-x "$brief" "$report" 42
expect_rc 0
grep -q -F '|--model|synthetic-pinned|--effort|high' "$case_dir/calls" || fail 'nested pin lane metadata missing'

new_case 'lane pin: nested pins refuse a third lane and name the longest pin'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"pinned":{"kind":"codex","model":"synthetic-pinned","effort":"high","prefixes":["impl"]},"other":{"kind":"codex","model":"synthetic-other","effort":"high","prefixes":["impl"]},"third":{"kind":"codex","model":"synthetic-third","effort":"high","prefixes":["impl"]},"pins":{"impl-":"other","impl-weekly":"pinned"}}' \
    >"$home/.config/herdr-fleet/lanes.json"
run_launch sh "$launch_sh" third impl-weekly-x "$brief" "$report" 42
expect_rc 2
[ "$err" = "herdr-launch: task 'impl-weekly-x' matches pin 'impl-weekly', which requires lane 'pinned'; given lane 'third'" ] ||
    fail "nested pin refusal line: $err"
expect_no_calls

new_case 'lane pin: nested pins keep the shorter pin lane for other names'
pinned_table '{"impl-":"other","impl-weekly":"pinned"}'
run_launch sh "$launch_sh" pinned impl-cart "$brief" "$report" 42
expect_rc 2
expect_err "matches pin 'impl-', which requires lane 'other'; given lane 'pinned'"
expect_no_calls

new_case 'empty pins behave like a table without pins'
pinned_table '{}'
run_launch sh "$launch_sh" other impl-weekly-x "$brief" "$report" 42
expect_rc 0

new_case '--lanes lists pins after the lanes'
pinned_table '{"impl-weekly":"pinned","impl-daily":"other"}'
run_launch sh "$launch_sh" --lanes
expect_rc 0
expect_out "Lane table: $home/.config/herdr-fleet/lanes.json
$(printf 'other\tcodex\tsynthetic-other\thigh\timpl\npinned\tcodex\tsynthetic-pinned\thigh\timpl\npin\timpl-daily\tother\npin\timpl-weekly\tpinned')"
expect_no_calls

new_case 'lane pin refuses a gather helper before its brief is written' researcher
gather_fixture reviewer
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"haiku-gather":{"kind":"claude","model":"synthetic-small","effort":"medium","prefixes":["gather"]},"luna-gather":{"kind":"codex","model":"synthetic-luna","effort":"medium","prefixes":["gather"]},"pins":{"gather-callers":"luna-gather"}}' \
    >"$home/.config/herdr-fleet/lanes.json"
run_launch TASKR_TASK=42 sh "$launch_sh" --gather callers 'Q'
expect_rc 2
expect_err "task 'gather-callers' matches pin 'gather-callers', which requires lane 'luna-gather'; given lane 'haiku-gather'"
[ ! -e "$gather_brief" ] || fail 'gather brief written before the pin refusal'
! grep -q -F 'taskr|new|' "$case_dir/calls" || fail 'refused gather helper was ledgered'
! grep -q '^herdr|' "$case_dir/calls" || fail 'refused gather helper made herdr calls'

for bad_pins in 'null' 'false' '[]' '"pinned"' '{"impl-weekly":"missing"}' '{"impl-weekly":"pins"}' \
    '{"impl-weekly":"deny"}' '{"impl-weekly":7}' '{"impl-weekly":null}' '{"Impl-weekly":"pinned"}' \
    '{"":"pinned"}' '{"impl-weekly-0123456789-0123456789":"pinned"}'; do
    new_case 'invalid lane pins fail the table load before control-plane calls'
    pinned_table "$bad_pins"
    for mode in launch list check; do
        case $mode in
            launch) run_launch sh "$launch_sh" pinned impl-weekly-x "$brief" "$report" 42 ;;
            list) run_launch sh "$launch_sh" --lanes ;;
            check) run_launch sh "$launch_sh" --check pinned ;;
        esac
        expect_rc 2
        expect_err 'invalid lane table'
        expect_out ''
        expect_no_calls
    done
done

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
# OpenCode --check also lists models; the fake lists every public OpenCode lane's id.
printf '%s\n' '#!/bin/sh' '[ "$*" != "models --standalone" ] || { jq -r '"'"'.[] | objects | select(.kind == "opencode") | .model'"'"' "$0.lanes"; exit; }' \
    '[ "$1" = --version ] || exit 99' 'echo synthetic-version-text' >"$work/bin/opencode"
cp "$repo_dir/lanes.json" "$work/bin/opencode.lanes"
check_path=$work/bin:$stub_dir:/usr/bin:/bin
run_launch PATH="$check_path" sh "$launch_sh" --check opus-medium sol-high glm-flash
expect_rc 0
expect_out 'opus-medium: ok
sol-high: ok
glm-flash: ok'
expect_no_calls
run_launch PATH="$check_path" sh "$launch_sh" --check
expect_rc 0
[ "$(printf '%s\n' "$out" | wc -l)" -eq "$(jq 'del(.deny, .pins) | length' "$repo_dir/lanes.json")" ] || fail 'not every lane checked'
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
listing=$(jq -r 'del(.deny, .pins) | to_entries | sort_by(.key)[] |
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

# --work: ticket W12 of campaign 42, depending on W3. ticket_list [JQ_FILTER]
# writes the campaign the stub's `taskr --json work show ID` reads rows from.
ticket_list() {
    jq -n '{campaign:42,generation:1,parked:false,configuration:null,work:[
        {work_id:12,campaign:42,title:"Cart totals",revision:2,state:"queued",revisions:[
            {revision:1,contract:null},
            {revision:2,contract:{intent:"Fix the cart total.",acceptance:"cart test passes\nreview READY",scope:["src/cart","tests/cart"],dependencies:[{work:3,revision:1}]}}]},
        {work_id:3,campaign:42,title:"Prices",revision:1,state:"done",revisions:[]}]}' \
        | jq "${1:-.}" >"$STUB_RESERVED"
}
show_call='taskr|--json|work|show|12'
dep_call='taskr|--json|work|show|3'

new_case '--work with --reserved is refused before any call'
ticket_list
run_launch sh "$launch_sh" --work W12 --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 2
expect_err '--work does not combine with --reserved'
expect_no_calls

for refused in 'missing:.work |= map(select(.work_id != 12)):W12 is not a ticket in campaign 42' \
    'done:.work[0].state = "done":W12 is done' \
    'cancelled:.work[0].state = "cancelled":W12 is cancelled' \
    'no spec:.work[0].revisions[1].contract = null:W12 spec lacks intent, acceptance, scope, dependencies' \
    'intent:del(.work[0].revisions[1].contract.intent):W12 spec lacks intent' \
    'acceptance:.work[0].revisions[1].contract.acceptance = "":W12 spec lacks acceptance' \
    'scope:del(.work[0].revisions[1].contract.scope):W12 spec lacks scope' \
    'dependencies:del(.work[0].revisions[1].contract.dependencies):W12 spec lacks dependencies' \
    'other campaign:.work[0].campaign = 43:W12 is not a ticket in campaign 42' \
    'open dependency:.work[1].state = "active":W12 depends on W3, which is active' \
    'missing dependency:.work |= map(select(.work_id != 3)):W12 depends on W3, which is not in campaign 42' \
    'foreign dependency:.work[1].campaign = 43:W12 depends on W3, which is not in campaign 42'; do
    new_case "--work refuses a ticket: ${refused%%:*}"
    filter=${refused#*:}
    ticket_list "${filter%:*}"
    run_launch sh "$launch_sh" --work 12 opus-medium impl-cart "$brief" "$report" 42
    expect_rc 2
    expect_err "${refused##*:}"
    case $refused in
        *dependency:*) expect_calls "$show_call" "$dep_call" ;;
        *) expect_calls "$show_call" ;;
    esac
    [ ! -e "$work/brief.W12.md" ] || fail 'refused ticket wrote a work brief'
done

new_case '--work writes the ticket into a W12 brief, sends it and sets work=12'
ticket_list
cp "$brief" "$case_dir/brief.orig"
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 work_brief=$work/brief.W12.md account=default"
cmp -s "$brief" "$case_dir/brief.orig" || fail 'the lead brief was modified'
{
    sed -n '1,3p' "$brief"
    printf '%s\n' 'Work: W12 Cart totals' 'Intent: Fix the cart total.' 'Acceptance:' \
        '- cart test passes' '- review READY' 'Scope: src/cart, tests/cart'
    sed '1,3d' "$brief"
} >"$case_dir/want-brief"
cmp -s "$case_dir/want-brief" "$work/brief.W12.md" || fail "work brief differs: $(diff "$case_dir/want-brief" "$work/brief.W12.md" | tr '\n' ' ')"
expect_calls "$show_call" "$dep_call" "$(tab_calls impl-cart)" \
    "$(brief=$work/brief.W12.md; tail_calls impl-cart implementer wA:p7 wA:t7 claude claude-opus-5-5 medium \
        "$claude_setup" "|$(claude_flags medium)" | sed '1a\
taskr|set|2468|work=12')"

new_case '--work: a refused picked account retries with a fresh W12 brief'
ticket_list
account_codator "$home"
usage_fixture "$home" first-seat 0 usable 0 72
usage_fixture "$home" second-seat 0 usable 0 96
STUB_START_OK=once STUB_SCREEN='codator: selected account is ineligible: all applicable usage headroom is exhausted'
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 work_brief=$work/brief.W12.md account=second-seat"
{
    sed -n '1,3p' "$brief"
    printf '%s\n' 'Work: W12 Cart totals' 'Intent: Fix the cart total.' 'Acceptance:' \
        '- cart test passes' '- review READY' 'Scope: src/cart, tests/cart'
    sed '1,3d' "$brief"
} >"$case_dir/want-brief"
cmp -s "$case_dir/want-brief" "$work/brief.W12.md" || fail 'the retry did not rewrite the W12 brief'
[ "$(grep -c '^taskr|close|2468|--outcome|abandoned$' "$case_dir/calls")" -eq 1 ] || fail 'the first task was not abandoned once'
[ "$(grep -c '^taskr|set|2468|work=12$' "$case_dir/calls")" -eq 2 ] || fail 'each task did not get work=12'

new_case '--work refuses an existing W12 brief before any tab'
ticket_list
printf '%s\n' 'earlier' >"$work/brief.W12.md"
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err "work brief '$work/brief.W12.md' already exists"
expect_calls "$show_call" "$dep_call"
[ "$(cat "$work/brief.W12.md")" = earlier ] || fail 'existing work brief was overwritten'

new_case '--work: a failed taskr set is a step 2 failure'
ticket_list
STUB_FAIL='taskr set'
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 1
expect_err "step 2 (taskr set 2468 work=12) failed; so far: workspace=wA task=2468 launch=- tab=wA:t7 pane=wA:p7 work_brief=$work/brief.W12.md"
if grep -E '^taskr\|launch\|' "$case_dir/calls" >/dev/null; then fail 'launch after a failed taskr set'; fi

# Past `work list`'s 1000-row cap: the list holds W1-W1000 only, W4537 and
# its dependency W999 are read by id.
new_case '--work finds a ticket past the 1000-row work list'
jq -n '{campaign:42,generation:1,parked:false,configuration:null,work:(
    [range(1; 1001) | {work_id:., campaign:42, title:"Row \(.)", revision:1, state:"done", revisions:[]}]
    + [{work_id:4537,campaign:42,title:"Late ticket",revision:1,state:"queued",revisions:[
        {revision:1,contract:{intent:"Ship the late ticket.",acceptance:["late test passes"],scope:["src/late"],dependencies:[{work:999,revision:1}]}}]}])}' \
    >"$case_dir/campaign.json"
jq '.work |= .[:1000]' "$case_dir/campaign.json" >"$case_dir/list.json"
[ "$(jq '.work | length' "$case_dir/list.json")" -eq 1000 ] || fail 'list fixture is not 1000 rows'
if jq -e '.work[] | select(.work_id == 4537)' "$case_dir/list.json" >/dev/null; then fail 'list fixture holds W4537'; fi
# The stub's work list prints the capped list, work show reads the campaign.
mkdir "$case_dir/cap-bin"
cat >"$case_dir/cap-bin/taskr" <<EOF
#!/bin/sh
if [ "\$*" = '--json work list 42' ]; then printf '%s\n' 'taskr|--json|work|list|42' >>"\$STUB_CALLS"; cat '$case_dir/list.json'; exit 0; fi
[ "\$1 \$2 \$3" != '--json work show' ] || STUB_RESERVED='$case_dir/campaign.json'
exec '$stub_dir/taskr' "\$@"
EOF
chmod 0755 "$case_dir/cap-bin/taskr"
run_path=$case_dir/cap-bin:$run_path
run_launch sh "$launch_sh" --work W4537 opus-medium impl-cart "$brief" "$report" 42
expect_rc 0
expect_out "task=2468 launch=3579 tab=wA:t7 pane=wA:p7 work_brief=$work/brief.W4537.md account=default"
{
    sed -n '1,3p' "$brief"
    printf '%s\n' 'Work: W4537 Late ticket' 'Intent: Ship the late ticket.' 'Acceptance:' \
        '- late test passes' 'Scope: src/late'
    sed '1,3d' "$brief"
} >"$case_dir/want-brief"
cmp -s "$case_dir/want-brief" "$work/brief.W4537.md" 2>/dev/null || fail 'W4537 work brief missing or different'
grep -qxF 'taskr|--json|work|show|4537' "$case_dir/calls" || fail 'W4537 not read by id'
grep -qxF 'taskr|--json|work|show|999' "$case_dir/calls" || fail 'dependency W999 not read by id'
grep -qxF 'taskr|set|2468|work=4537' "$case_dir/calls" || fail 'task did not get work=4537'

new_case '--work: a failed work show stops before any tab'
ticket_list
STUB_FAIL='taskr --json'
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err 'W12: taskr work show 12 failed'
expect_calls "$show_call"

new_case '--work: a failed dependency read stops before any tab'
ticket_list
printf '%s\n' '#!/bin/sh' '[ "$*" != "--json work show 3" ] || { echo "taskr|--json|work|show|3" >>"$STUB_CALLS"; exit 5; }' \
    "exec '$stub_dir/taskr' \"\$@\"" >"$case_dir/taskr"
mkdir "$case_dir/dep-bin" && mv "$case_dir/taskr" "$case_dir/dep-bin/taskr" && chmod 0755 "$case_dir/dep-bin/taskr"
run_path=$case_dir/dep-bin:$run_path
run_launch sh "$launch_sh" --work W12 opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err 'W12: taskr work show 3 failed'
expect_calls "$show_call" "$dep_call"
[ ! -e "$work/brief.W12.md" ] || fail 'failed dependency read wrote a work brief'

new_case '--work rejects a malformed id'
run_launch sh "$launch_sh" --work W0x opus-medium impl-cart "$brief" "$report" 42
expect_rc 2
expect_err "--work 'W0x' is not W<id> or <id>"
expect_no_calls

# W4508: agent temp files go to a per-user disk folder (README.md "Temp files").
# agent_tmp_command is the one line each launcher runs; the tests point its
# base at a short fake folder through HERDR_TEST_AGENT_TMP_BASE.
# agent_tmp_run SHELL [PATH]: run the line with a caller TMPDIR; out is the
# exported TMPDIR, err the refusal.
agent_tmp_run() {
    run_status=0
    out=$(env -i PATH="${2:-/usr/bin:/bin}" TMPDIR=/caller/tmp "$1" -c "$agent_tmp_setup && printf %s \"\$TMPDIR\"" 2>"$case_dir/err") || run_status=$?
    err=$(cat "$case_dir/err")
    rc=$run_status
}
agent_tmp_mode() { stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"; }
for agent_tmp_shell in sh zsh bash; do
    command -v "$agent_tmp_shell" >/dev/null 2>&1 || continue
    new_case "agent temp folder: created 0700 and replaces a caller TMPDIR ($agent_tmp_shell)"
    rm -rf "$agent_tmp_dir"
    agent_tmp_run "$agent_tmp_shell"
    expect_rc 0
    [ "$out" = "$agent_tmp_dir" ] || fail "TMPDIR '$out', expected '$agent_tmp_dir'"
    [ "$(agent_tmp_mode "$agent_tmp_dir")" = 700 ] || fail "mode $(agent_tmp_mode "$agent_tmp_dir")"
    chmod 0755 "$agent_tmp_dir"
    agent_tmp_run "$agent_tmp_shell"
    expect_rc 0
    [ "$(agent_tmp_mode "$agent_tmp_dir")" = 700 ] || fail 'an existing folder kept mode 755'
done

# BSD chmod (macOS) takes "--" after the mode as a file name and fails. The
# folder starts at 0755, so mode 700 proves the chmod in the line ran.
new_case 'agent temp folder: the line works with a BSD chmod (W5244)'
mkdir -p "$case_dir/bsdbin"
cat >"$case_dir/bsdbin/chmod" <<'EOF'
#!/bin/sh
for arg; do
    [ "$arg" != -- ] || { printf '%s\n' 'chmod: --: No such file or directory' >&2; exit 1; }
done
exec /bin/chmod "$@"
EOF
chmod 0755 "$case_dir/bsdbin/chmod"
rm -rf "$agent_tmp_dir"
mkdir -m 0755 "$agent_tmp_dir"
agent_tmp_run sh "$case_dir/bsdbin:/usr/bin:/bin"
expect_rc 0
[ "$out" = "$agent_tmp_dir" ] || fail "TMPDIR '$out', expected '$agent_tmp_dir'"
[ "$(agent_tmp_mode "$agent_tmp_dir")" = 700 ] || fail "mode $(agent_tmp_mode "$agent_tmp_dir")"

new_case 'agent temp folder: a symlink is refused and its target untouched'
rm -rf "$agent_tmp_dir" "$agent_tmp_base/target"
mkdir -m 0755 "$agent_tmp_base/target"
ln -s target "$agent_tmp_dir"
agent_tmp_run sh
expect_rc 1
[ -z "$out" ] || fail "TMPDIR exported after a refusal: $out"
expect_err "herdr-agent-tmp: $agent_tmp_dir must be a directory owned by you, not a symlink; not starting"
[ "$(agent_tmp_mode "$agent_tmp_base/target")" = 755 ] || fail 'the symlink target was changed'
rm -f "$agent_tmp_dir"

new_case 'agent temp folder: a non-directory is refused'
: >"$agent_tmp_dir"
agent_tmp_run sh
expect_rc 1
[ -z "$out" ] || fail "TMPDIR exported after a refusal: $out"
expect_err 'must be a directory owned by you, not a symlink; not starting'
rm -f "$agent_tmp_dir"

# A fake id makes the folder BASE/agent-x/.., which resolves to /, a real
# directory that this user does not own and that is not itself a symlink.
# A logging chmod proves the owner check refuses it, not a failed chmod.
# As root / is owned by the runner and the helper would chmod it, so skip.
new_case 'agent temp folder: a foreign-owned directory is refused'
if [ "$(id -u)" = 0 ]; then
printf '  SKIP (running as root)\n'
else
mkdir -p "$case_dir/idbin"
printf '%s\n' '#!/bin/sh' 'printf "%s\n" "x/.."' >"$case_dir/idbin/id"
printf '%s\n' '#!/bin/sh' ": >\"$case_dir/chmod-called\"" 'exec /bin/chmod "$@"' >"$case_dir/idbin/chmod"
chmod 0755 "$case_dir/idbin/id" "$case_dir/idbin/chmod"
ln -s / "$agent_tmp_base/agent-x"
agent_tmp_run sh "$case_dir/idbin:/usr/bin:/bin"
expect_rc 1
[ -z "$out" ] || fail "TMPDIR exported after a refusal: $out"
expect_err "herdr-agent-tmp: $agent_tmp_base/agent-x/.. must be a directory owned by you"
[ ! -e "$case_dir/chmod-called" ] || fail 'chmod ran on a foreign-owned folder'
rm -f "$agent_tmp_base/agent-x"
fi

new_case 'agent temp folder: over 24 bytes is refused before mkdir'
agent_tmp_long=$agent_tmp_base/abcdefghijklmn
agent_tmp_setup_saved=$agent_tmp_setup
agent_tmp_setup=$(HERDR_TEST_AGENT_TMP_BASE=$agent_tmp_long && . "$repo_dir/bin/herdr-lanes.sh" && agent_tmp_command)
agent_tmp_run sh
agent_tmp_setup=$agent_tmp_setup_saved
expect_rc 1
expect_err "herdr-agent-tmp: $agent_tmp_long/agent-$(id -u) is over 24 bytes (Codex socket path limit); not starting"
[ ! -e "$agent_tmp_long" ] || fail 'the long folder was created'

new_case 'agent temp folder: the default base is /var/tmp; a relative base is refused'
default_setup=$(unset HERDR_TEST_AGENT_TMP_BASE; . "$repo_dir/bin/herdr-lanes.sh" && agent_tmp_command)
case $default_setup in "herdr_agent_tmp='/var/tmp'/agent-\$(id -u) && "*) ;; *) fail "default line: $default_setup" ;; esac
run_status=0
(HERDR_TEST_AGENT_TMP_BASE=relative && . "$repo_dir/bin/herdr-lanes.sh" && agent_tmp_command) >/dev/null 2>"$case_dir/err" || run_status=$?
[ "$run_status" -ne 0 ] || fail 'a relative base was accepted'

# The pane runs the line, so a --machine lane creates the folder on its own
# host. STUB_EXEC runs the real setup chain for each lane kind.
for agent_tmp_lane in opus-medium:impl-tmp:implementer sol-medium:rev-tmp:reviewer glm-free:glm-tmp:implementer; do
    agent_tmp_role=${agent_tmp_lane##*:} agent_tmp_lane=${agent_tmp_lane%:*}
    new_case "agent temp folder: the pane chain exports it (${agent_tmp_lane%%:*})" "$agent_tmp_role"
    rm -rf "$agent_tmp_dir"
    STUB_EXEC=1
    printf '%s\n' opencode/glm-5.3-flash-free >"$home/.opencode/bin/models.txt"
    run_launch sh "$launch_sh" "${agent_tmp_lane%%:*}" "${agent_tmp_lane#*:}" "$brief" "$report" 42
    expect_rc 0
    grep -qF " && $agent_tmp_setup && herdr_launch_ok=3579" "$case_dir/calls.shell" || fail 'setup lacks the agent temp line'
    [ "$(agent_tmp_mode "$agent_tmp_dir")" = 700 ] || fail 'the pane did not create the folder 0700'
    replay=$(cd "$work" && env -i HOME="$home" PATH=/usr/bin:/bin TMPDIR=/caller/tmp sh -c "$(sed -n 1p "$case_dir/calls.shell") && printf %s \"\$TMPDIR\"") \
        || fail 'replayed setup failed'
    [ "$replay" = "$agent_tmp_dir" ] || fail "pane TMPDIR '$replay', expected '$agent_tmp_dir'"
done

new_case 'agent temp folder: a refused folder stops at step 5 before the agent starts'
rm -rf "$agent_tmp_dir"
ln -s / "$agent_tmp_dir"
STUB_EXEC=1
run_launch sh "$launch_sh" opus-medium impl-tmp "$brief" "$report" 42
expect_rc 1
expect_err 'step 5 (the pane shell did not echo herdr-launch-env=2468/3579/worker/3579)'
grep -qF "herdr-agent-tmp: $agent_tmp_dir must be a directory owned by you" "$case_dir/calls.screen" || fail 'the pane does not show the refusal'
if grep -E '^herdr\|agent\|start\|' "$case_dir/calls" >/dev/null; then fail 'started with a refused folder'; fi
rm -f "$agent_tmp_dir"

fi

reserved_snapshot() {
    jq -n --arg cwd "$tcwd" '{campaign:42,generation:7,parked:false,configuration:{enabled:true},work:[{revision:1,current_run:9,revisions:[{revision:1,contract:{checkout:{cwd:$cwd}}}],runs:[{run_id:9,round:1,state:"active",assignments:[{task_id:2468,launch_id:3579,round:1,state:"starting",stage:"executor",dispatch:{workspace:"wA",tab:"wA:t5",pane:"wA:p5"},lane:{name:"opus-medium",provider:"claude",model:"claude-opus-5-5",effort:"medium"}}]}]}]}' >"$STUB_RESERVED"
}

new_case 'orchestrator caller keeps reserved launch authority'
reserved_snapshot
printf '%s\n' '{"id":42,"role":"orchestrator","name":"orch-parent","status":"open"}' >"$STUB_CALLER"
run_launch TASKR_TASK=42 sh "$launch_sh" --reserved 2468 3579 opus-medium impl-cart "$brief" "$report" 42 wA:p5
expect_rc 0
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p5 account=default'
grep -q '^herdr|agent|start|impl-cart|' "$case_dir/calls" || fail 'no reserved implementer start'
if grep -E '^taskr\|(new|launch)\|' "$case_dir/calls" >/dev/null; then fail 'reserved mode created another task or launch'; fi

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
expect_out 'task=2468 launch=3579 tab=wA:t5 pane=wA:p5 account=default'
if grep -E '^taskr\|(new|launch)\|' "$case_dir/calls" >/dev/null; then fail 'reserved mode created another task or launch'; fi
grep -F 'herdr|agent|start|impl-cart|' "$case_dir/calls" >/dev/null || fail 'reserved mode did not start'
grep -F "taskr|prompt|2468|--file|$brief|--confirm|--confirm-timeout|60000" "$case_dir/calls" >/dev/null || fail 'reserved brief not captured'
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
new_case 'reserved OpenCode lane with an unlisted model stops at step 5'
reserved_snapshot
jq '.work[0].runs[0].assignments[0].lane = {name:"glm-free",provider:"opencode",model:"opencode/glm-5.3-flash-free",effort:"high"}' \
    "$STUB_RESERVED" >"$case_dir/reserved.next" && mv "$case_dir/reserved.next" "$STUB_RESERVED"
STUB_EXEC=1
printf '%s\n' opencode/glm-5.3-flash >"$home/.opencode/bin/models.txt"
run_launch sh "$launch_sh" --reserved 2468 3579 glm-free glm-cart "$brief" "$report" 42 wA:p5
expect_rc 1
expect_err "herdr-launch: step 5 (opencode models on $(uname -n) did not list opencode/glm-5.3-flash-free (missing from its catalog, or the listing failed); the agent was not started) failed; so far: workspace=wA task=2468 launch=3579 tab=wA:t5 pane=wA:p5"
expect_err 'setup artifact='
if grep -E '^(herdr\|agent\|start|taskr\|prompt)\|' "$case_dir/calls" >/dev/null; then fail 'reserved start with an unlisted model'; fi
printf '%s\n' opencode/glm-5.3-flash-free >"$home/.opencode/bin/models.txt"
: >"$case_dir/calls"; rm -f "$case_dir/calls.shell" "$case_dir/calls.opencode"
run_launch sh "$launch_sh" --reserved 2468 3579 glm-free glm-cart "$brief" "$report" 42 wA:p5
expect_rc 0
grep -q '^herdr|agent|start|glm-cart|' "$case_dir/calls" || fail 'reserved listed model did not start'
grep -qx 'opencode|models|--standalone' "$case_dir/calls.opencode" 2>/dev/null || fail 'the reserved pane never listed models'

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
