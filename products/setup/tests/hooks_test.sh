#!/bin/sh
# Ordinary hooks stay silent; owner-dialog hooks preserve denial and diagnostics.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
work=$(mktemp -d)
trap 'rm -rf "$work"' 0
mkdir -p "$work/home/.local/bin"
printf '%s\n' '#!/bin/sh' 'echo synthetic-output' 'echo synthetic-error >&2' 'exit "${SYNTHETIC_EXIT:-2}"' >"$work/home/.local/bin/taskr"
chmod 0755 "$work/home/.local/bin/taskr"
for harness in claude codex; do
    case $harness in claude) template=$root/claude/settings.json ;; codex) template=$root/codex/hooks.json ;; esac
    jq -r '.hooks | to_entries[] | select(.key != "PreToolUse" and .key != "PostToolUse") | .value[].hooks[].command' "$template" >"$work/commands"
    while IFS= read -r command; do
        HOME="$work/home" TASKR_LAUNCH=synthetic sh -c "$command" >"$work/out" 2>"$work/err"
        [ ! -s "$work/out" ] && [ ! -s "$work/err" ] || exit 1
    done <"$work/commands"
done
python3 - "$root/claude/settings.json" "$work/home" <<'PY'
import json, os, re, subprocess, sys
from pathlib import Path

moshi = Path(sys.argv[2]) / '.local/bin/moshi-hook'
moshi.write_text('#!/bin/sh\nprintf "moshi:%s\\n" "$1"\n')
moshi.chmod(0o755)
for harness in ('claude', 'codex'):
    notifications = Path(sys.argv[1]).parent.parent / harness / 'notifications.json'
    for groups in json.loads(notifications.read_text())['hooks'].values():
        for group in groups:
            for hook in group['hooks']:
                for role in ('hub', 'worker'):
                    env = {'HOME': sys.argv[2], 'PATH': os.environ['PATH'], 'FLEET_ROLE': role}
                    result = subprocess.run(['sh', '-c', hook['command']], env=env, text=True, capture_output=True)
                    assert result.returncode == 0 and result.stderr == '', (harness, role, result)
                    assert result.stdout == (f'moshi:{harness}-hook\n' if role == 'hub' else ''), result

groups = json.load(open(sys.argv[1]))['hooks']['PreToolUse']
for tool in ('Agent', 'Task', 'Workflow', 'Bash', 'TaskCreate', 'Read'):
    matching = [g for g in groups if re.search(g['matcher'], tool)]
    assert bool(matching) == (tool in ('Agent', 'Task', 'Workflow')), tool
    for group in matching:
        for hook in group['hooks']:
            assert not hook.get('async'), 'delegation denial must be synchronous'
            for herdr in ('1', '0', ''):
                env = {'HOME': sys.argv[2], 'PATH': os.environ['PATH']}
                if herdr:
                    env['HERDR_ENV'] = herdr
                result = subprocess.run(['sh', '-c', hook['command']], env=env,
                                        input='{"tool_name":"'+tool+'"}', text=True, capture_output=True)
                assert result.returncode == (2 if herdr == '1' else 0), (tool, herdr, result)
                assert result.stdout == '', result.stdout
                assert ('herdr-launch' in result.stderr) == (herdr == '1'), result.stderr
# Owner dialogs preserve denial and diagnostics, including roots without TASKR_LAUNCH.
settings = json.load(open(sys.argv[1]))
for event in ('PreToolUse', 'PostToolUse'):
    matching = [g for g in settings['hooks'][event] if re.search(g.get('matcher', ''), 'AskUserQuestion')]
    assert len(matching) == 1, (event, matching)
    hook, = matching[0]['hooks']
    assert matching[0]['matcher'] == '^AskUserQuestion$'
    assert not hook.get('async')
    assert hook['timeout'] <= 5
    for herdr in ('1', '0', ''):
        env = {'HOME': sys.argv[2], 'PATH': os.environ['PATH'], 'HERDR_ENV': herdr}
        result = subprocess.run(['sh', '-c', hook['command']], env=env,
                                input='{"tool_name":"AskUserQuestion"}', text=True, capture_output=True)
        assert result.returncode == (2 if herdr == '1' else 0), (event, herdr, result)
        assert ('synthetic-error' in result.stderr) == (herdr == '1'), result
        assert ('synthetic-output' in result.stdout) == (herdr == '1'), result
    if event == 'PreToolUse':
        env = {'HOME': sys.argv[2], 'PATH': os.environ['PATH'], 'HERDR_ENV': '1', 'SYNTHETIC_EXIT': '5'}
        result = subprocess.run(['sh', '-c', hook['command']], env=env, capture_output=True)
        assert result.returncode == 2, result
PY
printf '%s\n' 'taskr hook failure boundaries passed'
