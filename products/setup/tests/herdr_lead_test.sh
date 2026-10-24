#!/bin/sh
# All control-plane commands are fakes; jq and filesystem operations are real.
set -eu
repo_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd -P)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/herdr-lead-test.XXXXXX")
trap 'find "$scratch" -depth -delete' 0
trap 'exit 1' 1 2 3 15
mkdir "$scratch/stubs"
cat >"$scratch/stubs/herdr" <<'EOF'
#!/bin/sh
line=herdr
for arg in "$@"; do line="$line|$arg"; done
printf '%s\n' "$line" >>"$CALLS"
[ "${FAIL:-}" != "$1 $2" ] || exit 1
case "$1 $2" in
    'agent list')
        agents='{"result":{"agents":[]}}'
        [ ! -f "$CASE/waited-idle" ] || [ -z "${AGENTS_AFTER_WAIT:-}" ] || AGENTS=$AGENTS_AFTER_WAIT
        [ -f "$CASE/stopped" ] && AGENTS=
        printf '%s\n' "${AGENTS:-$agents}" ;;
    'pane send-keys')
        case " $* " in *' ctrl+c '*) [ "${STOP_IGNORED:-}" = 1 ] || : >"$CASE/stopped" ;; esac ;;
    'pane get') printf '%s\n' '{"result":{"pane":{"workspace_id":"wA","tab_id":"wA:t1"}}}' ;;
    'pane process-info')
        fg=400
        [ "${BUSY:-}" != "$4" ] || fg=500
        printf '{"result":{"process_info":{"foreground_process_group_id":%s,"shell_pid":400}}}\n' "$fg"
        ;;
    'pane run')
        # Execute setup in an isolated shell to prove worker IDs really clear,
        # cwd quoting works, and the Codator functions forward their argv.
        case $4 in
            *'unset TASKR_TASK TASKR_LAUNCH'*)
                TASKR_TASK=99 TASKR_LAUNCH=100 sh -c "$4 && test -z \"\${TASKR_TASK-}\" && test -z \"\${TASKR_LAUNCH-}\" && test \"\$FLEET_ROLE/\$CLAUDE_CODE_SANDBOXED\" = worker/1 && test \"\$PWD\" = \"\$WORK\" && claude --version && codex --version" || exit 1
                ;;
        esac
        ;;
    'pane wait-output') [ "${ENV_BAD:-}" != 1 ] || exit 1 ;;
    'agent start')
        : >"$CASE/started"
        [ "${DIALOG:-}" != 1 ] || { printf '%s\n' '{"error":{"code":"agent_not_ready"}}' >&2; exit 1; }
        ;;
    'agent prompt')
        case $4 in *'taskr handover --as 42'*)
            [ "${NO_HANDOVER:-}" = 1 ] || : >"$CASE/handover"
            exit 0 ;;
        esac
        printf '%s' "$4" >"$CASE/prompt-sent"
        case ${PROMPT_RESULT:-} in
            stall) printf '%s\n' 'agent_prompt_stalled' >&2; exit 1 ;;
            stall-other-exit) printf '%s\n' 'agent_prompt_stalled'; exit 2 ;;
            fail) printf '%s\n' 'agent_not_ready' >&2; exit 1 ;;
        esac
        ;;
    'agent get')
        n=0
        [ ! -f "$CASE/agent-gets" ] || n=$(cat "$CASE/agent-gets")
        n=$((n + 1))
        printf '%s\n' "$n" >"$CASE/agent-gets"
        status=$(printf '%s\n' "${GET_STATUSES:-idle}" | cut -d ' ' -f "$n")
        printf '{"result":{"agent":{"agent_status":"%s"},"type":"agent"}}\n' "$status"
        ;;
    'agent wait')
        [ "$5" != idle ] || : >"$CASE/waited-idle"
        if [ "$5" = working ]; then
            n=0
            [ ! -f "$CASE/working-waits" ] || n=$(cat "$CASE/working-waits")
            n=$((n + 1))
            printf '%s\n' "$n" >"$CASE/working-waits"
            result=$(printf '%s\n' "${WAIT_RESULTS:-0}" | cut -d ' ' -f "$n")
            exit "$result"
        fi
        ;;
    'pane read')
        if [ -f "$CASE/prompt-sent" ]; then
            case ${PROMPT_VISIBLE:-} in
                1) printf '%s\n' 'lead-push-gen-7' ;;
                disappears)
                    if [ ! -f "$CASE/working-waits" ]; then printf '%s\n' 'lead-push-gen-7'; fi
                    ;;
                old-generation) printf '%s\n' 'lead-push-gen-70' ;;
            esac
        else
            printf '%s\n' "${DIALOG_SCREEN:-Loading development channels
❯ 1. I am using this for local development
  2. Exit
Enter to confirm · Esc to cancel}"
        fi
        ;;
esac
EOF
cat >"$scratch/stubs/taskr" <<'EOF'
#!/bin/sh
[ "${1:-}" != --json ] || shift
line=taskr
for arg in "$@"; do line="$line|$arg"; done
printf '%s|env=%s/%s\n' "$line" "${TASKR_TASK:-}" "${TASKR_LAUNCH:-}" >>"$CALLS"
[ "${FAIL:-}" != "taskr $1" ] || exit 1
case "$1 $2" in
    'campaign 42')
        campaign='{"root":{"id":42,"name":"example","role":"orchestrator","status":"open"}}'
        printf '%s\n' "${CAMPAIGN:-$campaign}" ;;
    'consumer show')
        if [ -f "$CASE/started" ] || [ "${LEASE:-}" = live ]; then
            printf '{"live":true,"holder":"%s","session_gen":%s}\n' "${HOLDER:-mcp-channel-123}" "${LEASE_GEN:-7}"
        elif [ "${LEASE:-}" = bad ]; then
            printf '{}\n'
        else
            printf '{"live":false}\n'
        fi
        ;;
    'adopt 42')
        printf 'handover text\n'
        printf 'taskr adopt: task 42 now at pane wA:p1 (was wA:p2); event 7\n' >&2
        config=$HOME/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
        case ${CONFIG_EXISTS:-} in
            1) printf 'keep\n' >"$config" ;;
            directory) mkdir "$config" ;;
            symlink) ln -s "$CASE/missing" "$config" ;;
        esac
        ;;
    'log 42')
        printf '{"record":"event","id":%s,"kind":"adopt"}\n' "${LATEST:-7}"
        printf '{"record":"event","id":3,"kind":"handover"}\n'
        [ ! -f "$CASE/handover" ] || printf '{"record":"event","id":6,"kind":"handover"}\n'
        ;;
    'glance ') printf '{"caller_host":"%s"}\n' "${CALLER_HOST:-}" ;;
    'new lead-push-check-7') printf '{"task_id":43}\n' ;;
    'launch 43') printf '{"launch_id":8}\n' ;;
    'ready lead push check 7')
        [ "${TASKR_TASK:-}/${TASKR_LAUNCH:-}" = 43/8 ] || exit 1
        printf '{"event_id":9}\n'
        ;;
    'delivery ls') printf '{"attempts":[{"event_id":9,"session_gen":%s,"received_at":%s}]}\n' "${DELIVERY_GEN:-7}" "${RECEIVED:-\"now\"}" ;;
    'inbox --as')
        if [ -n "${ACKS:-}" ]; then
            n=0
            [ ! -f "$CASE/inbox-reads" ] || n=$(cat "$CASE/inbox-reads")
            n=$((n + 1))
            printf '%s\n' "$n" >"$CASE/inbox-reads"
            ACK=$(printf '%s\n' "$ACKS" | cut -d ' ' -f "$n")
            [ -n "$ACK" ] || ACK=${ACKS##* }
            printf '{"acked_event_id":%s,"events":[{"id":2},{"id":3}]}\n' "$ACK"
        else
            printf '{"acked_event_id":%s}\n' "${ACK:-9}"
        fi ;;
    'close 43') printf '{"task_id":43}\n' ;;
    *) exit 1 ;;
esac
EOF
cat >"$scratch/stubs/date" <<'EOF'
#!/bin/sh
n=0
[ ! -f "$CASE/clock" ] || n=$(cat "$CASE/clock")
n=$((n + 30))
printf '%s\n' "$n" >"$CASE/clock"
printf '%s\n' "$n"
EOF
printf '%s\n' '#!/bin/sh' 'printf "sleep|%s\n" "$1" >>"$CALLS"' >"$scratch/stubs/sleep"
printf '%s\n' '#!/bin/sh' 'echo "real Claude must not run" >&2' 'exit 99' >"$scratch/stubs/claude"
chmod 0755 "$scratch/stubs/"*

count=0
fail() { printf 'FAIL: %s: %s\n' "$name" "$1" >&2; exit 1; }
new_case() {
    name=$1
    count=$((count + 1))
    case_dir=$scratch/$count
    home=$case_dir/home
    work=$case_dir/"work ' quoted"
    mkdir -p "$home/.local/bin" "$work"
    cat >"$home/.local/bin/codator" <<'EOF'
#!/bin/sh
printf 'codator|%s|%s\n' "$1" "$2" >>"$CALLS"
EOF
    chmod 0755 "$home/.local/bin/codator"
    : >"$case_dir/calls"
    FAIL= LEASE= LATEST= LEASE_GEN= AGENTS= BUSY= ENV_BAD= DIALOG= ACK= RECEIVED= DELIVERY_GEN= CONFIG_EXISTS= HOLDER= CAMPAIGN=
    DIALOG_SCREEN= STOP_IGNORED= NO_HANDOVER= ACKS= AGENTS_AFTER_WAIT= CALLER_HOST=
    PROMPT_RESULT= PROMPT_VISIBLE= WAIT_RESULTS= GET_STATUSES= ENTER_DELAYS=
    printf 'case: %s\n' "$name"
}
run() {
    rc=0
    env -i HOME="$home" PATH="$scratch/stubs:/usr/bin:/bin" WORK="$work" CASE="$case_dir" CALLS="$case_dir/calls" \
        FAIL="$FAIL" LEASE="$LEASE" LATEST="$LATEST" LEASE_GEN="$LEASE_GEN" AGENTS="$AGENTS" BUSY="$BUSY" \
        ENV_BAD="$ENV_BAD" DIALOG="$DIALOG" DIALOG_SCREEN="$DIALOG_SCREEN" ACK="$ACK" RECEIVED="$RECEIVED" DELIVERY_GEN="$DELIVERY_GEN" \
        CONFIG_EXISTS="$CONFIG_EXISTS" HOLDER="$HOLDER" CAMPAIGN="$CAMPAIGN" TASKR_TASK=99 TASKR_LAUNCH=100 FLEET_ROLE=hub \
        PROMPT_RESULT="$PROMPT_RESULT" PROMPT_VISIBLE="$PROMPT_VISIBLE" WAIT_RESULTS="$WAIT_RESULTS" \
        GET_STATUSES="$GET_STATUSES" HERDR_LEAD_ENTER_DELAYS="$ENTER_DELAYS" STOP_IGNORED="$STOP_IGNORED" NO_HANDOVER="$NO_HANDOVER" ACKS="$ACKS" \
        AGENTS_AFTER_WAIT="$AGENTS_AFTER_WAIT" CALLER_HOST="$CALLER_HOST" \
        sh "$repo_dir/bin/herdr-lead" "$@" >"$case_dir/out" 2>"$case_dir/err" || rc=$?
}
expect_rc() { [ "$rc" = "$1" ] || fail "exit $rc, expected $1: $(cat "$case_dir/err")"; }
has() { grep -F -- "$2" "$case_dir/$1" >/dev/null || fail "$1 missing $2"; }
lacks() { if grep -F -- "$2" "$case_dir/$1" >/dev/null; then fail "$1 unexpectedly contains $2"; fi; }
start() { run start 42 wA:p1 --worktree "$work" "$@"; }

new_case 'unsupported lane has no calls or state'
start --jobs --lane sol-high
expect_rc 2
has err 'Claude leads only'
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'
[ ! -e "$home/.local/state" ] || fail 'created state'

for command in check stop; do
    new_case "$command refuses --jobs before any effects"
    if [ "$command" = check ]; then run check 42 --jobs; else run stop 42 wA:p1 --jobs; fi
    expect_rc 2
    [ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'
    [ ! -e "$home/.local/state" ] || fail 'created state'
done

new_case 'jobs does not accept owner-input permission'
start --jobs --allow-ask 17
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'
[ ! -e "$home/.local/state" ] || fail 'created state'

new_case 'missing worktree'
run start 42 wA:p1
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'

new_case 'invalid resume has no calls'
start --resume null --account example
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'

new_case 'missing prompt file refused in step 1 before mutation'
start --prompt-file "$case_dir/missing"
expect_rc 2
has out 'herdr-lead: step 1:'
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'
[ ! -e "$home/.local/state" ] || fail 'created state'

new_case 'prompt-file directory refused before mutation'
start --prompt-file "$case_dir"
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'

new_case 'multiline prompt-file path refused before mutation'
prompt_path="$case_dir/prompt
file"
printf 'Brief.\n' >"$prompt_path"
start --prompt-file "$prompt_path"
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'made a control-plane call'

new_case 'closed root refused'
CAMPAIGN='{"root":{"id":42,"name":"example","role":"orchestrator","status":"closed"}}'
start
expect_rc 1
lacks calls 'herdr|pane|run'
lacks calls 'taskr|adopt'

new_case 'busy target refused'
BUSY=wA:p1
start
expect_rc 1
lacks calls 'taskr|adopt'

new_case 'busy predecessor refused'
BUSY=wA:p2
start --predecessor-pane wA:p2
expect_rc 1
lacks calls 'taskr|adopt'

new_case 'live predecessor lease times out before clearing environment or adopt'
LEASE=live
start --predecessor-pane wA:p2
expect_rc 1
has err 'still live after 75 s'
lacks calls 'unset TASKR_TASK'
lacks calls 'taskr|adopt'

new_case 'name collision refused'
AGENTS='{"result":{"agents":[{"name":"orch-example","pane_id":"wA:p9"}]}}'
start
expect_rc 1
lacks calls 'taskr|adopt'

new_case 'recorded plain lead pane is checked without predecessor flag'
CAMPAIGN='{"root":{"id":42,"name":"example","role":"orchestrator","status":"open","pane_id":"wA:p2"}}'
AGENTS='{"result":{"agents":[{"name":"manual-lead","pane_id":"wA:p2"}]}}'
start
expect_rc 1
has err 'agent name or target/predecessor pane is occupied'
lacks calls 'herdr|pane|run'
lacks calls 'taskr|adopt'
[ ! -e "$home/.local/state" ] || fail 'created state before rejecting predecessor'

new_case 'unverified environment refused'
ENV_BAD=1
start
expect_rc 1
lacks calls 'taskr|adopt'

new_case 'conflicting adopt generation stops without agent start'
LATEST=8
start
expect_rc 1
has err 'generation conflict after adopt'
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config=- agent=orch-example lease=free check_task=-'
lacks calls 'herdr|agent|start'
[ "$(cat "$home/.local/state/herdr-orch/lead-push/42/handover-7.md")" = 'handover text' ] || fail 'handover lost'

new_case 'existing generation config never overwritten'
CONFIG_EXISTS=1
start
expect_rc 1
has err 'generation config exists'
[ "$(cat "$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json")" = keep ] || fail 'config overwritten'
lacks calls 'herdr|agent|start'

for existing in directory symlink; do
    new_case "existing config $existing refused"
    CONFIG_EXISTS=$existing
    start
    expect_rc 1
    has err 'generation config exists'
    lacks calls 'herdr|agent|start'
done

new_case 'failure after adopt prints complete recovery state'
FAIL='agent prompt'
start
expect_rc 1
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
has err 'agent=orch-example lease=unknown check_task=-'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
[ "$(grep -c 'herdr|agent|start' "$case_dir/calls")" = 1 ] || fail 'started twice'

new_case 'post-adopt failure keeps lock; a rerun reclaims it from the exited owner'
FAIL='taskr log'
start
expect_rc 1
lock_dir=$home/.local/state/herdr-orch/lead-push/42/start.lock
[ -d "$lock_dir" ] || fail 'post-adopt lock was removed'
read -r owner_pid owner_host <"$lock_dir/owner" || fail 'lock has no owner'
[ "$owner_host" = "$(uname -n)" ] || fail 'lock owner host'
# The fake ledger repeats generation 7; a real rerun adopts a new one.
rm -f "$home/.local/state/herdr-orch/lead-push/42/handover-7.md" "$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json"
FAIL=
start
expect_rc 0
has err "reclaimed start.lock from exited pid $owner_pid"
[ ! -d "$lock_dir" ] || fail 'successful rerun kept lock'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 2 ] || fail 'rerun did not adopt'

new_case 'generation conflict lock is never reclaimed'
LATEST=8
start
expect_rc 1
LATEST=
start
expect_rc 1
has err 'another start needs reconciliation (start.lock owner: '
has err ' hold)'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'rerun adopted after a generation conflict'

new_case 'lock without owner or with a live owner is refused'
lock_dir=$home/.local/state/herdr-orch/lead-push/42/start.lock
mkdir -p "$lock_dir"
start
expect_rc 1
has err 'another start needs reconciliation (start.lock owner: not recorded)'
printf '%s %s\n' "$$" "$(uname -n)" >"$lock_dir/owner"
start
expect_rc 1
has err "another start needs reconciliation (start.lock owner: $$ "
lacks calls 'taskr|adopt'
[ -d "$lock_dir" ] || fail 'refused start removed the lock'

new_case 'single-line prompt names the absolute file and waits for working'
printf 'Additional brief.\n\nSecond line.\n' >"$case_dir/prompt"
test_cwd=$PWD
cd "$case_dir"
start --prompt-file ./prompt
cd "$test_cwd"
expect_rc 0
[ "$(wc -l <"$case_dir/prompt-sent")" -eq 0 ] || fail 'prompt contains a newline'
case $(cat "$case_dir/prompt-sent") in
    *" Then read $case_dir/prompt and follow it. lead-push-gen-7") ;;
    *) fail 'prompt does not end with absolute prompt-file instruction and generation token' ;;
esac
has calls '|--wait|--until|working|--timeout|30000'
lacks calls 'Additional brief.'

for result in stall stall-other-exit; do
    new_case "visible prompt recovers from $result with one Enter"
    PROMPT_RESULT=$result PROMPT_VISIBLE=1
    start
    expect_rc 0
    has calls 'herdr|pane|read|wA:p1|--source|visible|--lines|40'
    [ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 1 ] || fail 'did not send exactly one Enter'
    has calls 'herdr|agent|wait|orch-example|--until|working|--timeout|10000'
    [ "$(grep -c 'herdr|agent|prompt|' "$case_dir/calls")" = 1 ] || fail 'resent prompt text'
    [ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
    [ "$(grep -c 'herdr|agent|start' "$case_dir/calls")" = 1 ] || fail 'started twice'
done

new_case 'hidden stalled prompt fails without Enter or check task'
PROMPT_RESULT=stall
start
expect_rc 1
has err 'step 7 failed: lead prompt not submitted (inspect before retry)'
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
has err 'agent=orch-example lease=unknown check_task=-'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|new'
[ "$(grep -c 'herdr|agent|prompt|' "$case_dir/calls")" = 1 ] || fail 'resent prompt text'

new_case 'old-generation prompt echo fails without Enter'
PROMPT_RESULT=stall PROMPT_VISIBLE=old-generation
start --resume saved-session --account example
expect_rc 1
has err 'step 7 failed: lead prompt not submitted (inspect before retry)'
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
has err 'agent=orch-example lease=unknown check_task=-'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|new'

new_case 'all four Enters dropped fail with recovery state'
PROMPT_RESULT=stall PROMPT_VISIBLE=1 WAIT_RESULTS='1 1 1 1'
start
expect_rc 1
has err 'step 7 failed: lead prompt not submitted (inspect before retry)'
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
has err 'agent=orch-example lease=unknown check_task=-'
lacks calls 'taskr|new'
[ "$(grep '^sleep|' "$case_dir/calls")" = "$(printf 'sleep|5\nsleep|10\nsleep|10\nsleep|10')" ] || fail 'wrong four-attempt delays'
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 4 ] || fail 'did not send exactly four Enters'
[ "$(grep -c 'herdr|agent|prompt|' "$case_dir/calls")" = 1 ] || fail 'resent prompt text'

new_case 'first Enter dropped, second works through step 8'
PROMPT_RESULT=stall PROMPT_VISIBLE=1 WAIT_RESULTS='1 0'
start --resume saved-session --account example
expect_rc 0
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 2 ] || fail 'did not send exactly two Enters'
[ "$(grep -c 'herdr|pane|read|wA:p1|--source|visible|--lines|40' "$case_dir/calls")" = 2 ] || fail 'did not recheck visibility'
[ "$(grep -c 'herdr|agent|prompt|' "$case_dir/calls")" = 1 ] || fail 'resent prompt text'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
[ "$(grep -c 'herdr|agent|start' "$case_dir/calls")" = 1 ] || fail 'started twice'
[ "$(grep '^sleep|' "$case_dir/calls")" = "$(printf 'sleep|5\nsleep|10')" ] || fail 'wrong retry delays'
has out 'herdr-lead: step 8:'
has out 'herdr-lead: 42 live at 7 (event 9 received and acked)'
has calls 'taskr|close|43|--outcome|accepted'

new_case 'already working before second Enter succeeds with one Enter'
PROMPT_RESULT=stall PROMPT_VISIBLE=1 WAIT_RESULTS='1 0' GET_STATUSES='idle working'
start
expect_rc 0
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 1 ] || fail 'did not send exactly one Enter'
[ "$(grep -c 'herdr|agent|get|orch-example' "$case_dir/calls")" = 2 ] || fail 'did not check state twice'
[ "$(grep -c 'herdr|pane|read|wA:p1|--source|visible|--lines|40' "$case_dir/calls")" = 1 ] || fail 'read screen after agent was working'
[ "$(cat "$case_dir/working-waits")" = 1 ] || fail 'waited again after agent was working'
has out 'herdr-lead: 42 live at 7 (event 9 received and acked)'
has calls 'taskr|close|43|--outcome|accepted'

new_case 'generation token disappears before second Enter'
PROMPT_RESULT=stall PROMPT_VISIBLE=disappears WAIT_RESULTS=1
start
expect_rc 1
has err 'step 7 failed: lead prompt not submitted (inspect before retry)'
has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 1 ] || fail 'sent Enter after token disappeared'
lacks calls 'taskr|new'

new_case 'test delay override shortens retries but cannot exceed four Enters'
PROMPT_RESULT=stall PROMPT_VISIBLE=1 WAIT_RESULTS='1 1 1 1 0' ENTER_DELAYS='0 0 0 0 0'
start
expect_rc 1
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 4 ] || fail 'exceeded four Enter attempts'
[ "$(grep -c '^sleep|0$' "$case_dir/calls")" = 4 ] || fail 'ignored delay override'
lacks calls 'taskr|new'

new_case 'test delay override can reduce attempt count'
PROMPT_RESULT=stall PROMPT_VISIBLE=1 WAIT_RESULTS='1 0' ENTER_DELAYS=0
start
expect_rc 1
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|Enter' "$case_dir/calls")" = 1 ] || fail 'ignored shorter attempt override'
lacks calls 'taskr|new'

new_case 'other prompt failures do not try Enter recovery'
PROMPT_RESULT=fail
start
expect_rc 1
has err 'step 7 failed: prompt lead (inspect before retry)'
lacks calls 'herdr|pane|read'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|new'

new_case 'successful start, cleared identity, dialog, exact check flow'
DIALOG=1
printf 'Additional brief.\n' >"$case_dir/prompt"
start --prompt-file "$case_dir/prompt"
expect_rc 0
[ ! -d "$home/.local/state/herdr-orch/lead-push/42/start.lock" ] || fail 'successful start kept lock'
lacks err 'agent_not_ready'
has out 'herdr-lead: answered the development-channel dialog'
start_log=$(ls "$home/.local/state/herdr-orch/lead-push/42/"lead-start-*.log) || fail 'no start log'
for n in 1 2 3 4 5 6 7 8; do grep -F "step $n:" "$start_log" >/dev/null || fail "start log misses step $n"; done
grep -F '42 live at 7' "$start_log" >/dev/null || fail 'start log misses the live line'
grep -F 'agent_not_ready' "$start_log" >/dev/null || fail 'start log misses the blocked start'
for n in 1 2 3 4 5 6 7 8; do has out "herdr-lead: step $n:"; done
has out 'herdr-lead: 42 live at 7 (event 9 received and acked)'
has calls 'codator|claude|--version'
has calls 'codator|codex|--version'
has calls 'herdr-lead-env=//worker/1/'
has calls 'taskr|adopt|42|--workspace|wA|--tab|wA:t1|--pane|wA:p1|env=/'
has calls 'herdr|pane|send-keys|wA:p1|Enter'
has calls 'herdr|agent|wait|orch-example|--until|idle|--timeout|90000'
has calls '--model|claude-opus-5-5|--effort|high|--dangerously-skip-permissions|--settings|{"agentPushNotifEnabled":false,"enabledPlugins":{"posthog@claude-plugins-official":false}}'
has calls "--mcp-config|$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json|--dangerously-load-development-channels|server:taskr"
has calls 'do not run taskr adopt'
lacks calls 'Additional brief.'
has calls 'taskr|new|lead-push-check-7|--parent|42|--role|gate|env=/'
has calls 'taskr|launch|43|--provider|shell|--model|none|--effort|none|env=/'
has calls "taskr|ready|lead push check 7|--report|$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json|env=43/8"
has calls 'taskr|close|43|--outcome|accepted|env=/'
[ "$(wc -l <"$case_dir/prompt-sent")" -eq 0 ] || fail 'prompt contains a newline'
case $(cat "$case_dir/prompt-sent") in *' lead-push-gen-7') ;; *) fail 'missing end token' ;; esac
grep -F 'if the handover Note lists session timers to re-arm, create each again' "$case_dir/prompt-sent" >/dev/null || fail 'prompt does not ask to re-arm session timers'
config=$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
jq -e '.mcpServers.taskr == {command:"taskr",args:["mcp","--channel","--as","42","--session","7"]}' "$config" >/dev/null || fail 'bad config'
printf '%s\n' '{"mcpServers":{"taskr":{"command":"taskr","args":["mcp","--channel","--as","42","--session","7"]}}}' |
    cmp - "$config" || fail 'default config bytes changed'
[ "$(ls -l "$config" | cut -c 1-10)" = -rw------- ] || fail 'config mode is not 0600'
[ -z "$(find "$case_dir" -name .mcp.json -print)" ] || fail 'wrote .mcp.json'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
[ "$(grep -c 'herdr|agent|start' "$case_dir/calls")" = 1 ] || fail 'started twice'

new_case 'jobs start adds a generation-bound server without changing channel or resume argv'
start --jobs --resume 'sid with spaces' --account example
expect_rc 0
config=$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
jq -e '. == {mcpServers:{taskr:{command:"taskr",args:["mcp","--channel","--as","42","--session","7"]},"taskr-jobs":{command:"taskr",args:["mcp","--jobs","--as","42","--session","7","--binding","herdr-lead-root-42-gen-7"]}}}' "$config" >/dev/null || fail 'bad jobs config'
[ "$(ls -l "$config" | cut -c 1-10)" = -rw------- ] || fail 'config mode is not 0600'
has calls '--account|example|--resume|sid with spaces|--model|'
has calls "--mcp-config|$config|--dangerously-load-development-channels|server:taskr"
lacks calls 'server:taskr-jobs'
[ "$(cat "$home/.local/state/herdr-orch/lead-push/42/handover-7.md")" = 'handover text' ] || fail 'handover lost'

new_case 'jobs never overwrites a captured generation config'
CONFIG_EXISTS=1
start --jobs
expect_rc 1
[ "$(cat "$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json")" = keep ] || fail 'config overwritten'
lacks calls 'herdr|agent|start'

for border in plain boxed json; do
    new_case "narrow development-channel dialog: $border"
    DIALOG=1
    DIALOG_SCREEN='Loading development
channels
❯ 1. I am using this for local development
  2. Exit
Enter to confirm · Esc to cancel'
    if [ "$border" = boxed ]; then
        DIALOG_SCREEN='╭────────────────────╮
│ Loading development│
│ channels           │
│ ❯ 1. I am using this for local │
│ development        │
│   2. Exit          │
│ Enter to confirm · Esc to cancel │
╰────────────────────╯'
    fi
    if [ "$border" = json ]; then
        DIALOG_SCREEN='{"result":{"lines":["│ Loading development│","│ channels           │","❯ 1. I am using this for local development","  2. Exit","Enter to confirm · Esc to cancel"]}}'
    fi
    start
    expect_rc 0
    has calls 'herdr|pane|send-keys|wA:p1|Enter'
done

new_case 'failed start without development-channel dialog'
DIALOG=1 DIALOG_SCREEN='Unrelated failure'
start
expect_rc 1
has err 'agent start failed without development-channel dialog'
lacks calls 'herdr|pane|send-keys'
lacks calls 'herdr|agent|prompt'

for screen in 'Previous command output: Loading development
channels failed; shell is ready' \
    '{"result":{"lines":["Unrelated failure"],"metadata":"Loading development channels ❯ 1. I am using this for local development Enter to confirm"}}' \
    'Loading development channels' \
    '❯ 1. I am using this for local development' \
    'Loading development channels
❯ 1. I am using this for local development' \
    'Enter to confirm
Loading development channels
❯ 1. I am using this for local development' \
    'Loading development channels
❯ 2. Exit
Enter to confirm · Esc to cancel'; do
    new_case 'failed start refuses incomplete dialog, log or non-visible JSON text'
    DIALOG=1 DIALOG_SCREEN=$screen
    start
    expect_rc 1
    has err 'agent start failed without development-channel dialog'
    lacks calls 'herdr|pane|send-keys'
    lacks calls 'herdr|agent|prompt'
done

new_case 'private lane replaces public lead defaults'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{"custom":{"kind":"claude","model":"synthetic-claude","effort":"medium","prefixes":["orch"]}}' >"$home/.config/herdr-fleet/lanes.json"
start --lane custom
expect_rc 0
has calls '--model|synthetic-claude|--effort|medium'

new_case 'private replacement does not fall back for missing lane'
mkdir -p "$home/.config/herdr-fleet"
printf '%s\n' '{}' >"$home/.config/herdr-fleet/lanes.json"
start
expect_rc 2
[ ! -s "$case_dir/calls" ] || fail 'invalid table made control-plane calls'

for first_model in 'bad model' good-model; do
    new_case 'two-document lane table fails before control-plane calls'
    mkdir -p "$home/.config/herdr-fleet"
    printf '%s\n' "{\"custom\":{\"kind\":\"claude\",\"model\":\"$first_model\",\"effort\":\"medium\",\"prefixes\":[\"orch\"]}}" \
        '{"custom":{"kind":"claude","model":"good-model","effort":"medium","prefixes":["orch"]}}' >"$home/.config/herdr-fleet/lanes.json"
    start --lane custom
    expect_rc 2
    [ ! -s "$case_dir/calls" ] || fail 'JSON stream made control-plane calls'
done

new_case 'resume puts account first and keeps the session'
start --resume saved-session --account example
expect_rc 0
has calls '--|--account|example|--resume|saved-session|--model|claude-opus-5-5'

new_case 'live check waits while the successor drains an inbox backlog'
ACKS='1 2 3 4 5 6 7 8 8 9'
start
expect_rc 0
has out 'herdr-lead: check event 9 waits behind 2 unacked events'
has out 'herdr-lead: 42 live at 7 (event 9 received and acked)'

new_case 'live check still fails when acks stop advancing'
ACKS='1 2 3'
start
expect_rc 1
has err 'check event was not received and acked within 240 s (acked through 3; backlog 2)'
lacks calls 'taskr|close'

for mode in unreceived unacked wrong-generation wrong-holder; do
    new_case "live check refuses $mode"
    case $mode in
        unreceived) RECEIVED=null ;;
        unacked) ACK=8 ;;
        wrong-generation) DELIVERY_GEN=6 ;;
        wrong-holder) HOLDER=other ;;
    esac
    start
    expect_rc 1
    lacks calls 'taskr|close'
    has err 'ROOT=42 GEN=7 pane=wA:p1 mcp_config='
done

moved_campaign='{"root":{"id":42,"name":"example","role":"orchestrator","status":"open","pane_id":"wA:p1"}}'
old_lead='{"result":{"agents":[{"name":"orch-example","pane_id":"wA:p1","agent_session":{"value":"s1"}}]}}'
move() { run move 42 wA:p1 --worktree "$work" "$@"; }

new_case 'jobs move carries opt-in through the stop and new generation render'
CAMPAIGN=$moved_campaign AGENTS=$old_lead
move --jobs --no-handover
expect_rc 0
has calls 'herdr|pane|send-keys|wA:p1|esc|ctrl+c|ctrl+c'
config=$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
jq -e '.mcpServers | keys == ["taskr","taskr-jobs"] and .["taskr-jobs"].args == ["mcp","--jobs","--as","42","--session","7","--binding","herdr-lead-root-42-gen-7"]' "$config" >/dev/null || fail 'move lost jobs identity'

new_case 'move asks for a handover, stops the predecessor, then starts'
CAMPAIGN=$moved_campaign AGENTS=$old_lead
move
expect_rc 0
has calls 'herdr|agent|prompt|orch-example|'
has calls 'taskr handover --as 42'
has calls 'Session timers to re-arm: each CronCreate job or background loop you armed'
has calls 'herdr|agent|wait|wA:p1|--until|idle|--timeout|120000'
has calls 'herdr|pane|send-keys|wA:p1|esc|ctrl+c|ctrl+c'
has out 'herdr-lead: move: handover event 6 recorded'
has out 'herdr-lead: 42 live at 7 (event 9 received and acked)'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
[ -n "$(grep -n 'taskr handover' "$case_dir/calls" | head -1)" ] || fail 'no handover prompt'
[ "$(grep -n 'send-keys|wA:p1|esc' "$case_dir/calls" | cut -d: -f1)" -lt "$(grep -n 'taskr|adopt' "$case_dir/calls" | cut -d: -f1)" ] || fail 'adopted before stopping'
grep -F 'move: handover event 6 recorded' "$home/.local/state/herdr-orch/lead-push/42/"lead-start-*.log >/dev/null || fail 'start log misses move lines'

new_case 'move without a recorded handover stops nothing'
CAMPAIGN=$moved_campaign AGENTS=$old_lead NO_HANDOVER=1
move
expect_rc 1
has err 'orch-example recorded no handover; nothing was stopped'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|adopt'

new_case 'move --no-handover skips the prompt'
CAMPAIGN=$moved_campaign AGENTS=$old_lead
move --no-handover
expect_rc 0
lacks calls 'taskr handover --as 42'
has calls 'herdr|pane|send-keys|wA:p1|esc|ctrl+c|ctrl+c'

new_case 'move fails when the predecessor keeps running'
CAMPAIGN=$moved_campaign AGENTS=$old_lead STOP_IGNORED=1
move --no-handover
expect_rc 1
has err 'orch-example still runs in wA:p1 after 120 s'
[ "$(grep -c 'herdr|pane|send-keys|wA:p1|ctrl+c|ctrl+c' "$case_dir/calls")" = 1 ] || fail 'did not resend ctrl+c once'
lacks calls 'taskr|adopt'

new_case 'move validates a busy destination before the handover or stop'
CAMPAIGN=$moved_campaign AGENTS=$old_lead BUSY=wA:p3
run move 42 wA:p3 --worktree "$work"
expect_rc 1
has err 'target pane is not at its shell prompt'
lacks calls 'taskr handover --as 42'
lacks calls 'herdr|pane|send-keys'
run move 42 wA:p3 --worktree "$work" --no-handover
expect_rc 1
has err 'target pane is not at its shell prompt'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|adopt'

new_case 'move to another pane checks its host before the stop'
CAMPAIGN=$moved_campaign AGENTS=$old_lead ENV_BAD=1
run move 42 wA:p3 --worktree "$work" --no-handover
expect_rc 1
has err 'target is not on this host'
lacks calls 'herdr|pane|send-keys'

new_case 'move refuses an unrelated agent in the recorded pane'
CAMPAIGN=$moved_campaign AGENTS='{"result":{"agents":[{"name":"worker-unrelated","pane_id":"wA:p1"}]}}'
move
expect_rc 1
has err 'wA:p1 runs worker-unrelated (session unknown), not the lead of root 42; nothing was sent to it'
lacks calls 'herdr|agent|prompt'
move --no-handover
expect_rc 1
has err 'not the lead of root 42'
lacks calls 'herdr|pane|send-keys'
lacks calls 'taskr|adopt'

new_case 'move refuses predecessor-pane and a root without a pane'
move --predecessor-pane wA:p2
expect_rc 2
move
expect_rc 1
has err 'root has no recorded lead pane to move from'
lacks calls 'taskr|adopt'

new_case 'stop waits for a live lease to expire'
CAMPAIGN=$moved_campaign AGENTS=$old_lead LEASE=live
run stop 42 wA:p1
expect_rc 1
has err 'predecessor lease is still live after 120 s'
LEASE=
run stop 42 wA:p1
expect_rc 0
has out 'herdr-lead: wA:p1 is at its shell prompt and the lease of 42 is free'

new_case 'stop refuses a pane that is not the recorded root pane'
CAMPAIGN=$moved_campaign AGENTS=$old_lead
run stop 42 wA:p9
expect_rc 1
lacks calls 'herdr|pane|send-keys'

new_case 'stop refuses an unrelated agent in the recorded pane'
CAMPAIGN=$moved_campaign AGENTS='{"result":{"agents":[{"name":"worker-unrelated","pane_id":"wA:p1"}]}}'
run stop 42 wA:p1
expect_rc 1
has err 'wA:p1 runs worker-unrelated (session unknown), not the lead of root 42'
lacks calls 'herdr|pane|send-keys'

new_case 'stop rechecks the lead session right before its keys'
CAMPAIGN=$moved_campaign AGENTS=$old_lead
AGENTS_AFTER_WAIT='{"result":{"agents":[{"name":"orch-example","pane_id":"wA:p1","agent_session":{"value":"s2"}}]}}'
run stop 42 wA:p1
expect_rc 1
has calls 'herdr|agent|wait|wA:p1|--until|idle'
has err 'wA:p1 runs orch-example (session s2), not the lead of root 42'
lacks calls 'herdr|pane|send-keys'

new_case 'stop --session names the hub to stop, whatever its name'
CAMPAIGN=$moved_campaign AGENTS='{"result":{"agents":[{"name":null,"pane_id":"wA:p1","agent_session":{"value":"s1"}}]}}'
run stop 42 wA:p1 --session s9
expect_rc 1
has err 'wA:p1 runs an unnamed agent (session s1), not the lead of root 42'
lacks calls 'herdr|pane|send-keys'
run stop 42 wA:p1 --session s1
expect_rc 0
has calls 'herdr|pane|send-keys|wA:p1|esc|ctrl+c|ctrl+c'

new_case 'stop and move refuse a root adopted on another host'
CAMPAIGN='{"root":{"id":42,"name":"example","role":"orchestrator","status":"open","pane_id":"wA:p1","host":"other.example.com"}}'
AGENTS=$old_lead
run stop 42 wA:p1
expect_rc 1
has err 'root 42 was adopted on other.example.com, not this host'
move --no-handover
expect_rc 1
has err 'root 42 was adopted on other.example.com, not this host'
lacks calls 'herdr|pane|send-keys'
CALLER_HOST=other.example.com
run stop 42 wA:p1
expect_rc 0
CAMPAIGN=$moved_campaign
run stop 42 wA:p1
expect_rc 1
has err 'root 42 was adopted on the ledger host, not this host (other.example.com)'

new_case 'check live'
LEASE=live
run check 42
expect_rc 0
has out live

new_case 'check free is stale'
run check 42
expect_rc 1
has out stale

new_case 'check old generation is stale'
LEASE=live LEASE_GEN=6
run check 42
expect_rc 1
has out stale

new_case 'check unreadable is unknown'
FAIL='taskr consumer'
run check 42
expect_rc 2
has out unknown

new_case 'check malformed is unknown'
LEASE=bad
run check 42
expect_rc 2
has out unknown
# Review 7908 F1: A reads a dead owner and pauses mid-reclaim while B and then C start. Only one may enter.
for script in herdr-lead herdr-hub; do
    new_case "$script start.lock reclaim admits one starter while another reclaims"
    race=$case_dir/race
    mkdir -p "$race/bin"
    # A pauses once, right after its first read of the owner file (cat or sed, whichever the launcher uses).
    cat >"$race/bin/cat" <<'EOF'
#!/bin/sh
PATH=/usr/bin:/bin "${0##*/}" "$@"; rc=$?
case "$WHO:$*" in
    A:*start.lock/owner)
        if [ ! -f "$RACE/paused-A" ]; then
            : >"$RACE/paused-A"
            i=0; until [ -f "$RACE/go-A" ] || [ "$i" -ge 600 ]; do i=$((i + 1)); sleep 0.05; done
        fi ;;
esac
exit "$rc"
EOF
    cp "$race/bin/cat" "$race/bin/sed"
    chmod 0755 "$race/bin/cat" "$race/bin/sed"
    {
        printf '%s\n' 'set -u' 'state_dir=$RACE root=42 host=$(uname -n) guard= lock=' \
            "number() { case \$1 in '' | *[!0-9]* | 0) return 1 ;; esac; }" \
            'fail() { printf "%s\n" "$1" >&2; [ -z "$guard" ] || rmdir -- "$guard"; exit 1; }'
        awk '/^# start.guard serializes/ { on = 1 } on { print } /^release_lock\(\) \{/ { last = 1 } last && /^}/ { exit }' "$repo_dir/bin/$script"
        printf '%s\n' 'take_lock' ': >"$RACE/entered-$WHO"' \
            'i=0; until [ -f "$RACE/release" ] || [ "$i" -ge 600 ]; do i=$((i + 1)); sleep 0.05; done'
    } >"$race/take.sh"
    mkdir "$race/start.lock"
    printf '2147483647 %s\n' "$(uname -n)" >"$race/start.lock/owner"
    starter() { env -i PATH="$race/bin:/usr/bin:/bin" RACE="$race" WHO="$1" sh "$race/take.sh" 2>"$race/$1.err" & }
    wait_file() {
        i=0
        until [ -f "$race/$1" ]; do i=$((i + 1)); [ "$i" -le 200 ] || { : >"$race/go-A"; : >"$race/release"; fail "no $1"; }; sleep 0.05; done
    }
    starter A; a=$!
    wait_file paused-A
    starter B; b=$!
    sleep 1
    [ ! -f "$race/entered-B" ] || { : >"$race/go-A"; : >"$race/release"; fail 'B entered while A was reclaiming'; }
    : >"$race/go-A"
    wait_file entered-A
    rc=0; wait "$b" || rc=$?
    [ "$rc" = 1 ] || fail "B exit $rc"
    grep -F 'another start needs reconciliation (start.lock owner: ' "$race/B.err" >/dev/null || fail "B: $(cat "$race/B.err")"
    starter C; c=$!
    rc=0; wait "$c" || rc=$?
    [ "$rc" = 1 ] || fail "C exit $rc"
    : >"$race/release"
    wait "$a" || fail 'A failed'
    [ ! -f "$race/entered-B" ] && [ ! -f "$race/entered-C" ] || fail 'a second starter entered'
    [ ! -e "$race/start.guard" ] || fail 'start.guard left behind'
done

printf 'PASS: %s herdr-lead cases\n' "$count"
