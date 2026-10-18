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
        printf '%s\n' "${AGENTS:-$agents}" ;;
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
        [ "${DIALOG:-}" != 1 ] || exit 1
        ;;
    'agent prompt')
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
    'log 42') printf '{"record":"event","id":%s,"kind":"adopt"}\n' "${LATEST:-7}" ;;
    'new lead-push-check-7') printf '{"task_id":43}\n' ;;
    'launch 43') printf '{"launch_id":8}\n' ;;
    'ready lead push check 7')
        [ "${TASKR_TASK:-}/${TASKR_LAUNCH:-}" = 43/8 ] || exit 1
        printf '{"event_id":9}\n'
        ;;
    'delivery ls') printf '{"attempts":[{"event_id":9,"session_gen":%s,"received_at":%s}]}\n' "${DELIVERY_GEN:-7}" "${RECEIVED:-\"now\"}" ;;
    'inbox --as') printf '{"acked_event_id":%s}\n' "${ACK:-9}" ;;
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
    DIALOG_SCREEN=
    PROMPT_RESULT= PROMPT_VISIBLE= WAIT_RESULTS= GET_STATUSES= ENTER_DELAYS=
    printf 'case: %s\n' "$name"
}
run() {
    rc=0
    env -i HOME="$home" PATH="$scratch/stubs:/usr/bin:/bin" WORK="$work" CASE="$case_dir" CALLS="$case_dir/calls" \
        FAIL="$FAIL" LEASE="$LEASE" LATEST="$LATEST" LEASE_GEN="$LEASE_GEN" AGENTS="$AGENTS" BUSY="$BUSY" \
        ENV_BAD="$ENV_BAD" DIALOG="$DIALOG" DIALOG_SCREEN="$DIALOG_SCREEN" ACK="$ACK" RECEIVED="$RECEIVED" DELIVERY_GEN="$DELIVERY_GEN" \
        CONFIG_EXISTS="$CONFIG_EXISTS" HOLDER="$HOLDER" CAMPAIGN="$CAMPAIGN" TASKR_TASK=99 TASKR_LAUNCH=100 \
        PROMPT_RESULT="$PROMPT_RESULT" PROMPT_VISIBLE="$PROMPT_VISIBLE" WAIT_RESULTS="$WAIT_RESULTS" \
        GET_STATUSES="$GET_STATUSES" HERDR_LEAD_ENTER_DELAYS="$ENTER_DELAYS" \
        sh "$repo_dir/bin/herdr-lead" "$@" >"$case_dir/out" 2>"$case_dir/err" || rc=$?
}
expect_rc() { [ "$rc" = "$1" ] || fail "exit $rc, expected $1: $(cat "$case_dir/err")"; }
has() { grep -F -- "$2" "$case_dir/$1" >/dev/null || fail "$1 missing $2"; }
lacks() { if grep -F -- "$2" "$case_dir/$1" >/dev/null; then fail "$1 unexpectedly contains $2"; fi; }
start() { run start 42 wA:p1 --worktree "$work" "$@"; }

new_case 'unsupported lane has no calls or state'
start --lane sol-high
expect_rc 2
has err 'Claude leads only'
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

new_case 'post-adopt failure keeps lock and blocks blind re-adoption'
FAIL='taskr log'
start
expect_rc 1
[ -d "$home/.local/state/herdr-orch/lead-push/42/start.lock" ] || fail 'post-adopt lock was removed'
start
expect_rc 1
has err 'another start needs reconciliation'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'blind rerun adopted again'

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
config=$home/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
jq -e '.mcpServers.taskr == {command:"taskr",args:["mcp","--channel","--as","42","--session","7"]}' "$config" >/dev/null || fail 'bad config'
[ "$(ls -l "$config" | cut -c 1-10)" = -rw------- ] || fail 'config mode is not 0600'
[ -z "$(find "$case_dir" -name .mcp.json -print)" ] || fail 'wrote .mcp.json'
[ "$(grep -c 'taskr|adopt' "$case_dir/calls")" = 1 ] || fail 'adopted twice'
[ "$(grep -c 'herdr|agent|start' "$case_dir/calls")" = 1 ] || fail 'started twice'

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
printf 'PASS: %s herdr-lead cases\n' "$count"
