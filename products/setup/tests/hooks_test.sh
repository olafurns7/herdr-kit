#!/bin/sh
# Ledger hooks stay silent; clock context and owner-dialog diagnostics survive.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
work=$(mktemp -d)
trap 'rm -rf "$work"' 0
mkdir -p "$work/home/.local/bin"
printf '%s\n' '#!/bin/sh' 'echo synthetic-output' 'echo synthetic-error >&2' 'exit "${SYNTHETIC_EXIT:-2}"' >"$work/home/.local/bin/taskr"
chmod 0755 "$work/home/.local/bin/taskr"
for harness in claude codex; do
    case $harness in claude) template=$root/claude/settings.json ;; codex) template=$root/codex/hooks.json ;; esac
    jq -r '.hooks | to_entries[] | select(.key != "PreToolUse" and .key != "PostToolUse") | .value[].hooks[] | select(.command | contains("clock:") | not) | .command' "$template" >"$work/commands"
    while IFS= read -r command; do
        HOME="$work/home" HERDR_ENV=1 TASKR_LAUNCH=synthetic sh -c "$command" >"$work/out" 2>"$work/err"
        [ ! -s "$work/out" ] && [ ! -s "$work/err" ] || exit 1
    done <"$work/commands"
done
python3 - "$root/claude/settings.json" "$work/home" <<'PY'
import json, os, re, subprocess, sys, tempfile
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
for tool in ('Agent', 'Task', 'Workflow', 'advisor', 'pre_advisor', 'advisor_extra', 'Bash', 'TaskCreate', 'Read'):
    matching = [g for g in groups if re.search(g['matcher'], tool)]
    assert bool(matching) == (tool in ('Agent', 'Task', 'Workflow', 'advisor')), tool
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

# Clock hooks return one compact JSON line synchronously and never block a turn.
for harness, template in (
    ('claude', Path(sys.argv[1])),
    ('codex', Path(sys.argv[1]).parent.parent / 'codex/hooks.json'),
):
    hooks = json.loads(template.read_text())['hooks']
    clock = {}
    for event in ('UserPromptSubmit', 'PostToolUse'):
        candidates = [hook for group in hooks.get(event, []) for hook in group['hooks']
                      if 'clock:' in hook['command']]
        assert len(candidates) == 1, (harness, event, 'missing clock hook')
        hook, = candidates
        assert not hook.get('async'), (harness, event, 'clock must be synchronous')
        clock[event] = hook['command']
        if event == 'PostToolUse':
            group, = [g for g in hooks[event] if hook in g['hooks']]
            assert group['matcher'] == '^Bash$', (harness, group)

    with tempfile.TemporaryDirectory(dir=sys.argv[2], prefix='clock.') as scratch:
        scratch = Path(scratch)
        bin_dir = scratch / 'bin'
        bin_dir.mkdir()
        # Freeze the UTC minute, then advance it without wall-clock waiting.
        minute_file = scratch / 'minute'
        minute_file.write_text('2026-01-02T03:04Z\n')
        date = bin_dir / 'date'
        date.write_text('#!/bin/sh\n[ "$*" = "-u +%Y-%m-%dT%H:%MZ" ] || exit 2\ncat "$CLOCK_TEST_MINUTE"\n')
        date.chmod(0o755)
        runtime = scratch / 'runtime'
        runtime.mkdir()
        fallback = scratch / 'tmp'
        fallback.mkdir()
        env = {'HOME': sys.argv[2], 'PATH': f'{bin_dir}:' + os.environ['PATH'],
               'HERDR_ENV': '1', 'XDG_RUNTIME_DIR': str(runtime), 'TMPDIR': str(fallback),
               'CLOCK_TEST_MINUTE': str(minute_file)}

        def run(event, payload='{"session_id":"session-a"}', override=None):
            run_env = {k: v for k, v in (env | (override or {})).items() if v is not None}
            result = subprocess.run(['sh', '-c', clock[event]], env=run_env,
                                    input=payload, text=True, capture_output=True)
            assert result.returncode == 0 and not result.stderr, (harness, event, result)
            if result.stdout:
                assert len(result.stdout.splitlines()) == 1, result
                assert json.loads(result.stdout) == {'hookSpecificOutput': {
                    'hookEventName': event,
                    'additionalContext': 'clock: ' + minute_file.read_text().strip() + ' (UTC, from date -u)',
                }}, result
            return result.stdout

        assert run('UserPromptSubmit')
        assert run('UserPromptSubmit'), 'every prompt gets the clock'
        assert run('PostToolUse')
        assert not run('PostToolUse'), 'same session is throttled within the minute'
        assert run('PostToolUse', '{"session_id":"session-b"}')
        assert not run('PostToolUse', '{"session_id":"session-b"}')
        assert {p.name for p in runtime.iterdir()} == {'herdr-clock-session-a', 'herdr-clock-session-b'}
        minute_file.write_text('2026-01-02T03:05Z\n')
        assert run('PostToolUse'), 'a later minute emits again'
        assert run('PostToolUse', '{"session_id":"../session/c:$unsafe"}')
        assert (runtime / 'herdr-clock-sessioncunsafe').is_file(), 'session id is sanitized'
        assert run('PostToolUse', '{"session_id":"tmp-session"}', {'XDG_RUNTIME_DIR': ''})
        assert (fallback / 'herdr-clock-tmp-session').is_file(), 'TMPDIR is the runtime fallback'
        before = {p: p.read_bytes() for p in runtime.iterdir()}
        for outside in ('0', '', None):
            for event in clock:
                assert not run(event, override={'HERDR_ENV': outside})
        assert {p: p.read_bytes() for p in runtime.iterdir()} == before
        for payload in ('', '{broken', '{}', 'null', '[]', '{"session_id":null}',
                        '{"session_id":42}', '{"session_id":""}', '{"session_id":"../"}'):
            for event in clock:
                run(event, payload)
        assert all(re.fullmatch(r'herdr-clock-[A-Za-z0-9_-]+', p.name) for p in runtime.iterdir())
        assert any(p.name.startswith('herdr-clock-ppid-') for p in runtime.iterdir()), 'missing ids use parent pid'
        for event in clock:
            # An unavailable stamp directory must not fail the tool or prompt.
            run(event, override={'XDG_RUNTIME_DIR': str(scratch / 'absent')})
print('Claude and Codex clock hook behavior passed')
PY
printf '%s\n' 'taskr hook failure boundaries passed'
