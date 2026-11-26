#!/bin/sh
# Fake control plane and cdl; real jq/files, isolated HOME and ledger path.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
exec python3 - "$repo_dir" "${HUB_SH:-sh}" <<'PY'
import atexit, fcntl, json, os, pty, shlex, shutil, signal, subprocess, sys, tempfile, termios, time
from pathlib import Path
from unittest.mock import Mock, patch

setup = Path(sys.argv[1])
shell = shutil.which(sys.argv[2])
count = 0

def executable(path, source):
    path.write_text(source)
    path.chmod(0o755)

def stop_owned_process(process):
    # A reaped child's numeric process group is no longer an owned live reference.
    if process.poll() is not None:
        return
    try:
        identity = (os.getsid(process.pid), os.getpgid(process.pid))
        if identity != (process.pid, process.pid):
            process.kill()
            process.wait(timeout=5)
            raise AssertionError(('fixture child changed session/group', process.pid, identity))
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=5)

with patch('os.getsid', return_value=424242) as sid, patch('os.getpgid', return_value=424242) as group, \
     patch('os.killpg') as signal_group:
    child = Mock(pid=424242)
    child.poll.return_value = 0
    stop_owned_process(child)
    sid.assert_not_called()
    group.assert_not_called()
    signal_group.assert_not_called()

    child.poll.return_value = None
    stop_owned_process(child)
    signal_group.assert_called_once_with(424242, signal.SIGKILL)
    child.wait.assert_called_once_with(timeout=5)

    signal_group.reset_mock()
    child.reset_mock()
    sid.side_effect = ProcessLookupError
    stop_owned_process(child)
    signal_group.assert_not_called()
    child.wait.assert_called_once_with(timeout=5)

    sid.side_effect = None
    for identity in ((424243, 424242), (424242, 424243)):
        sid.return_value, group.return_value = identity
        child.reset_mock()
        try: stop_owned_process(child)
        except AssertionError: pass
        else: raise AssertionError('changed child identity was accepted')
        signal_group.assert_not_called()
        child.kill.assert_called_once_with()
        child.wait.assert_called_once_with(timeout=5)

    sid.return_value = group.return_value = 424242
    signal_group.side_effect = PermissionError('fixture permission failure')
    try: stop_owned_process(child)
    except PermissionError: pass
    else: raise AssertionError('permission failure was suppressed')
print('PASS: owned-process cleanup checks', flush=True)

# The agent temp folder (README.md "Temp files") goes under a short fake base,
# never /var/tmp: the folder must stay within 24 bytes.
agent_tmp_base = Path(subprocess.run(['mktemp', '-d', '/tmp/h.XXX'], check=True, capture_output=True, text=True).stdout.strip())
agent_tmp_dir = agent_tmp_base / f'agent-{os.getuid()}'
atexit.register(shutil.rmtree, agent_tmp_base, True)
with tempfile.TemporaryDirectory(prefix='herdr-hub-test.') as scratch:
    # getcwd reports the physical path when TMPDIR traverses a symlink.
    scratch = Path(scratch).resolve()
    stubs = scratch / 'stubs'
    stubs.mkdir()
    # Exercise detachment without depending on an installed util-linux setsid.
    executable(stubs / 'setsid', '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
helper = str(Path(os.environ['HOME']) / '.local/bin/herdr-hub')
if sys.argv[1:4] != [helper, '__move-helper', '42']:
    sys.exit('fixture setsid refuses an unexpected helper')
(Path(os.environ['CASE']) / 'setsid-args').write_text(json.dumps(sys.argv[1:]))
os.setsid()
os.execv(helper, sys.argv[1:])
''')
    executable(stubs / 'ps', '''#!/bin/sh
if [ "$*" != '-eww -o pid=,args=' ]; then exec /bin/ps "$@"; fi
config=$HOME/.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json
case $MODE in
    check-dropped) printf '123 claude --resume sess-hub --mcp-config %s\\n' "$config" ;;
    check-flag) printf '123 claude --mcp-config %s --dangerously-load-development-channels server:taskr\\n' "$config" ;;
    check-channels) printf '123 claude --mcp-config %s --channels server:taskr\\n' "$config" ;;
    check-other-generation) printf '123 claude --mcp-config %s/taskr-mcp-6.json\\n' "${config%/*}" ;;
esac
''')
    executable(stubs / 'awk', '''#!/bin/sh
if [ -n "${HERDR_CHECK_MCP_CONFIG:-}" ]; then
    for arg; do
        case $arg in *"$HERDR_CHECK_MCP_CONFIG"*) echo 'config path leaked into helper argv' >&2; exit 99 ;; esac
    done
    : >"$CASE/awk-env-checked"
fi
exec /usr/bin/awk "$@"
''')
    executable(stubs / 'herdr', '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
case = Path(os.environ['CASE'])
mode = os.environ.get('MODE', '')
args = sys.argv[1:]
if args[:2] != ['agent', 'list']:
    with (case / 'herdr-calls').open('a') as f:
        f.write(json.dumps([args, sorted(k for k in os.environ if k.startswith('CLAUDE'))]) + '\\n')
if args[:2] == ['agent', 'list']:
    if not mode.startswith('move'): (case / 'agent-list').touch()
    if mode == 'fail-agent-list': sys.exit(1)
    # move-replaced: another agent takes the pane after herdr-hub move captured the hub's session.
    lists = case / 'agent-lists'
    n = int(lists.read_text()) + 1 if lists.exists() else 1
    lists.write_text(str(n))
    session = 'sess-other' if mode == 'move-replaced' and n > 1 else 'sess-hub'
    agents = [] if (case / 'stopped').exists() else [{'name':'hub-claude', 'pane_id':'wA:p2' if mode == 'old-pane-live' else 'wA:p1',
                                                      'agent_session':{'value':session}}]
    print(json.dumps({'result':{'agents':agents}}))
elif args[:2] == ['pane', 'read'] and mode.startswith('consent'):
    # consent-answered: a person selects Yes after two reads; consent-again: the channel dialog was answered before it.
    reads = case / 'pane-reads'
    n = int(reads.read_text()) + 1 if reads.exists() else 1
    reads.write_text(str(n))
    channel = ['Loading development channels', '\u276f 1. I am using this for local development', '  2. Exit', 'Enter to confirm \u00b7 Esc to cancel']
    consent = ['\u256d\u2500\u2500\u256e', '\u2502 Code Mode (experimental) \u2502', '\u276f No, start without it', '  Yes, turn on Code Mode',
               'Enter to confirm \u00b7 Esc to start without it']
    # consent-late: a person selects Yes after 14 reads; read 3 fails. consent-flap: the dialog comes and goes forever.
    if mode == 'consent-late' and n == 3: sys.exit(1)
    if mode == 'consent-again' and n <= 2: lines = channel
    elif mode == 'consent-both': lines = channel + consent
    elif mode == 'consent-both-rev': lines = consent + channel
    elif mode == 'consent-flap': lines = consent if n % 2 else ['$']
    elif mode == 'consent-late': lines = consent if n <= 14 else channel if not (case / 'dialog-answered').exists() else ['$']
    elif mode == 'consent-stuck' or n <= 4: lines = consent
    elif mode == 'consent-off': lines = ['$']
    else: lines = channel if not (case / 'dialog-answered').exists() or n <= 7 else ['$']
    print(json.dumps({'result':{'lines':lines}}))
elif args[:2] == ['pane', 'read']:
    if mode in ('dialog', 'dialog-tall') and not (case / 'dialog-answered').exists():
        lines = ['Loading development channels']
        if mode == 'dialog-tall': lines += ['wrapped dialog content'] * 60
        lines += ['\u276f 1. I am using this for local development', '  2. Exit', 'Enter to confirm \u00b7 Esc to cancel']
        limit = int(args[args.index('--lines') + 1]) if '--lines' in args else 40
        print(json.dumps({'result':{'lines':lines[-limit:]}}))
    else: print(json.dumps({'result':{'lines':['$']}}))
elif args[:2] == ['pane', 'send-keys']:
    if 'Enter' in args: (case / 'dialog-answered').touch()
    if 'ctrl+c' in args and mode != 'move-stuck': (case / 'stopped').touch()
elif args[:2] == ['pane', 'process-info']:
    print(json.dumps({'result':{'process_info':{'shell_pid':400, 'foreground_process_group_id':400}}}))
elif args[:2] == ['pane', 'run']:
    (case / 'typed').write_text(args[3])
    state = Path(os.environ['HOME']) / '.local/state/herdr-orch/lead-push/42'
    # move-slow: the successor's prover reports 1200 fake seconds later, move-slow-consent 2700 (see the date stub).
    if mode.startswith('move-slow'): (case / 'typed-at').write_text((case / 'clock').read_text())
    else: (state / 'hub-check-9.result').write_text('live\\n')
elif args[:2] == ['agent', 'wait']: pass
else: sys.exit(99)
''')
    executable(stubs / 'taskr', '''#!/usr/bin/python3
import json, os, stat, sys
from pathlib import Path
args = sys.argv[1:]
if args[0] == '--json': args.pop(0)
case = Path(os.environ['CASE'])
with (case / 'calls').open('a') as f:
    f.write(json.dumps([args, os.getenv('TASKR_TASK'), os.getenv('TASKR_LAUNCH')]) + '\\n')
mode = os.environ.get('MODE', '')
cmd = args[0]
if mode == 'fail-' + cmd: sys.exit(1)
if mode == 'fail-accepted-close' and cmd == 'close' and args[-1] == 'accepted': sys.exit(1)
root = {'id':42, 'name':'hub', 'role': 'gate' if mode == 'not-root' else 'orchestrator',
        'status': 'closed' if mode == 'closed' else 'open',
        'pane_id':'wA:p2' if mode in ('old-pane-live', 'old-pane-free', 'fail-agent-list') else 'wA:p1'}
# A sub-orchestrator has a parent; taskr omits a null parent_id.
if mode == 'not-top': root['parent_id'] = 5
if cmd == 'campaign':
    print(json.dumps({'root': root}))
elif cmd == 'status':
    # status --tree lists the task, then its descendants; root-last checks the read selects by id.
    rows = [root, {'id':43, 'name':'lane', 'role':'orchestrator', 'status':'open', 'parent_id':42, 'pane_id':'wA:p9'}]
    if mode == 'no-row': rows = rows[1:]
    if mode == 'root-last': rows.reverse()
    for row in rows: print(json.dumps(row))
elif cmd == 'consumer':
    # A lease row keeps the generation it was taken at after it expires; no-lease: the root never had one.
    free = '{"live":false}' if mode == 'no-lease' else '{"live":false,"session_gen":3}'
    if mode in ('dialog', 'dialog-tall') and not (case / 'dialog-answered').exists(): print(free)
    elif mode in ('consent-stuck', 'consent-both', 'consent-both-rev', 'consent-flap'): print(free)
    elif mode == 'consent-late' and not (case / 'dialog-answered').exists(): print(free)
    # Live once the channel dialog is done: after read 7, past the last one shown.
    elif mode.startswith('consent') and (not (case / 'pane-reads').exists() or int((case / 'pane-reads').read_text()) <= 7):
        print(free)
    elif mode in ('cdl-exit-lease', 'prover-lease-conflict') and (case / 'adopted').exists():
        (case / 'waiting-lease').touch()
        print(free)
    elif mode == 'lost-lease' and (case / 'ready').exists(): print(free)
    elif mode == 'unknown': print('{}')
    elif mode == 'busy' or (case / 'adopted').exists() or mode.startswith('check-'):
        print(json.dumps({'live': True, 'holder': 'other' if mode == 'wrong-holder' else 'mcp-channel-123',
                          'session_gen': {'wrong-lease':6, 'check-stale':6, 'check-floor-gone':20}.get(mode, 7)}))
    else: print(free)
elif cmd == 'new':
    print(json.dumps({'task_id':43 if '--parent' in args else 42}))
elif cmd == 'adopt':
    (case / 'adopted').touch()
    state = Path(os.environ['HOME']) / '.local/state/herdr-orch/lead-push/42'
    if mode == 'config-exists': (state / 'taskr-mcp-7.json').write_text('keep')
    if mode == 'config-symlink': (state / 'taskr-mcp-7.json').symlink_to(case / 'missing')
    if mode == 'handover-exists': (state / 'handover-7.md').write_text('keep')
    print('handover text')
    # The root's refs line comes first; a lane's refs line follows. refs-missing: a taskr without the line.
    refs = {'hub-on':'hub=on, pr=12', 'hub-off':'pr=12', 'hub-lane':'pr=12'}.get(mode, 'none')
    if mode != 'refs-missing': print('- Refs: ' + refs)
    if mode == 'hub-lane': print('- Refs: hub=on')
    print('taskr adopt: task 42 now at pane wA:p1 (was wA:p1); event 7', file=sys.stderr)
elif cmd == 'log':
    gen = 8 if (mode == 'generation-conflict' or
                (mode == 'prover-conflict' and (case / 'ready').exists()) or
                (mode == 'prover-lease-conflict' and (case / 'waiting-lease').exists())) else 7
    if (case / 'typed').exists(): gen = 9
    # --since keeps only later events, as taskr does; an older adopt (3) and handover (4) sit below every floor.
    since = int(args[args.index('--since') + 1]) if '--since' in args else 0
    events = [{'kind':'adopt','id':3}, {'kind':'handover','id':4}, {'kind':'adopt','id':gen}]
    if mode.startswith('move'): events.append({'kind':'handover','id':8})
    if mode in ('hub-on', 'hub-off', 'refs-missing'):
        events.append({'kind':'ref','id':1,'data':{'key':'hub','value':'on'}})
    if mode == 'hub-off': events.append({'kind':'ref','id':2,'data':{'key':'hub','value':''}})
    for e in events:
        if e['id'] > since: print(json.dumps(e))
# Glance lists each active root; its host is omitted when it is the ledger host.
elif cmd == 'glance': print('{"caller_host":"","campaigns":[{"id":42,"name":"hub"}]}')
elif cmd == 'launch': print('{"launch_id":8}')
elif cmd == 'ready':
    if mode == 'prover-stdin':
        info = os.fstat(0)
        (case / 'prover-stdin').write_text(str(stat.S_ISCHR(info.st_mode) and info.st_rdev == os.stat(os.devnull).st_rdev))
    (case / 'ready').touch()
    if mode == 'queued-ready': print('{"queued":true,"request_key":"example"}')
    else: print(json.dumps({'event_id':40 if mode == 'backlog' else 9}))
elif cmd == 'delivery' and mode == 'later-push-old-gen':
    # A later event was received, but at generation 6: no proof for generation 7.
    print(json.dumps({'attempts':[{'event_id':9, 'session_gen':7, 'received_at':None},
                                  {'event_id':12, 'session_gen':6, 'received_at':'now'}]}))
elif cmd == 'delivery' and mode in ('later-push', 'later-push-unacked'):
    # The hub acked the check from its inbox before the digest hold pushed it; event 12 was pushed later.
    print(json.dumps({'attempts':[{'event_id':8, 'session_gen':7, 'received_at':'now'},
                                  {'event_id':9, 'session_gen':7, 'received_at':None},
                                  {'event_id':12, 'session_gen':7, 'received_at':'now'}]}))
elif cmd == 'delivery':
    print(json.dumps({'attempts':[{'event_id':40 if mode == 'backlog' else 9, 'session_gen':6 if mode == 'wrong-delivery' else 7,
                                  'received_at':None if mode == 'unreceived' else 'now'}]}))
elif cmd == 'inbox' and mode == 'backlog':
    # The successor acks older events, 1.5 per read, and reaches check event 40 after the old fixed window.
    reads = case / 'inbox-reads'
    n = int(reads.read_text()) + 1 if reads.exists() else 1
    reads.write_text(str(n))
    acked = min(n * 3 // 2, 40)
    print(json.dumps({'acked_event_id':acked, 'events':[{'id':i} for i in range(acked + 1, 41)]}))
elif cmd == 'inbox' and mode == 'consent-late':
    # The hub acks the check 10 reads later, past the first 600 s.
    reads = case / 'inbox-reads'
    n = int(reads.read_text()) + 1 if reads.exists() else 1
    reads.write_text(str(n))
    print(json.dumps({'acked_event_id':8 if n <= 10 else 9}))
elif cmd == 'inbox': print(json.dumps({'acked_event_id':8 if mode in ('unacked', 'cdl-exit-ack', 'later-push-unacked') else 9}))
elif cmd == 'close': print('{"task_id":43}')
elif cmd == 'note':
    # The consent note comes before the result note, which ends the cdl stub.
    (case / ('consent-note' if 'consent dialog is waiting' in args[1] else 'note')).write_text(args[1])
    print('{"event_id":10}')
else: sys.exit(99)
''')
    executable(stubs / 'date', '''#!/bin/sh
n=0
[ ! -f "$CASE/clock" ] || n=$(cat "$CASE/clock")
n=$((n + 30))
printf '%s\\n' "$n" >"$CASE/clock"
case $MODE in move-slow) slow=1200 ;; move-slow-consent) slow=2700 ;; *) slow= ;; esac
if [ -n "$slow" ] && [ -f "$CASE/typed-at" ] && [ "$n" -ge $(($(cat "$CASE/typed-at") + slow)) ]; then
    printf 'live\\n' >"$HOME/.local/state/herdr-orch/lead-push/42/hub-check-9.result"
fi
printf '%s\\n' "$n"
''')
    # Waits of a second or more are skipped; sub-second polls sleep for real, so herdr-hub's
    # 0.1 s poll loops (move helper start, start.guard) keep their wall-clock bound on a loaded host (W4667).
    executable(stubs / 'sleep', '#!/bin/sh\ncase $1 in 0.*) exec /bin/sleep "$@" ;; esac\nexit 0\n')
    # Require portable sed argv even when this fixture runs with GNU sed.
    executable(stubs / 'sed', '''#!/bin/sh
for arg; do
    [ "$arg" != -- ] || { printf 'fixture sed: -- is outside the portable profile\\n' >&2; exit 2; }
done
exec /usr/bin/sed "$@"
''')
    cdl = '''#!/usr/bin/python3
import json, os, signal, sys, time
from pathlib import Path
case = Path(os.environ['CASE'])
(case / 'cdl').write_text(json.dumps({'args':sys.argv[1:], 'task':os.getenv('TASKR_TASK'),
    'launch':os.getenv('TASKR_LAUNCH'), 'role':os.getenv('FLEET_ROLE'),
    'code_mode':os.getenv('CLAUDE_CODE_EXPERIMENTAL_CODE_MODE'), 'tmpdir':os.getenv('TMPDIR'), 'pid':os.getpid(), 'cwd':os.getcwd(), 'umask':os.umask(0)}))
mode = os.environ.get('MODE', '')
# Claude's channel flag is variadic: an untagged value after it is rejected at startup.
flag = sys.argv.index('--dangerously-load-development-channels')
for value in sys.argv[flag + 1:]:
    if value.startswith('-'): break
    if not value.startswith(('server:', 'plugin:')):
        (case / 'cdl-rejected').write_text(value)
        sys.exit(1)
if mode == 'hup':
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    os.killpg(0, signal.SIGHUP)
if mode == 'cdl-exit-lease': sys.exit(0)
# The prover must complete after exec replaced the launcher process.
for _ in range(1500):
    if mode == 'cdl-exit-ack' and (case / 'ready').exists(): sys.exit(0)
    if (case / 'note').exists(): break
    time.sleep(.01)
else: sys.exit(98)
'''
    def run(name, args=('42',), mode='', lane=None, location=None, seed=None, cdl_present=True, job_control=False):
        global count
        count += 1
        case = scratch / str(count)
        home = case / 'home'
        bindir = home / '.local/bin'
        bindir.mkdir(parents=True)
        shutil.copy2(setup / 'bin/herdr-hub', bindir)
        shutil.copy2(setup / 'bin/herdr-lanes.sh', bindir)
        shutil.copy2(setup / 'bin/herdr-lead', bindir)
        if cdl_present: executable(bindir / 'cdl', cdl)
        config = home / '.config/herdr-fleet'
        config.mkdir(parents=True)
        lanes = {'opus-high': {'kind':'claude','model':'example-opus','effort':'high','prefixes':['orch']}}
        if lane: lanes['other'] = lane
        (config / 'lanes.json').write_text(json.dumps(lanes))
        if seed: seed(home)
        env = dict(HOME=str(home), PATH=f'{stubs}:/usr/bin:/bin', CASE=str(case), MODE=mode,
                   HERDR_PANE_ID='wA:p1', HERDR_WORKSPACE_ID='wA', HERDR_TAB_ID='wA:t1',
                   TASKR_TASK='99', TASKR_LAUNCH='100', TASKR_DB=str(home / 'scratch.db'),
                   HERDR_TEST_AGENT_TMP_BASE=str(agent_tmp_base), TMPDIR='/caller/tmp')
        env.update(location or {})
        master, slave = pty.openpty()
        def child_setup():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
            # Deliberately differs from the gate mask: the helper must restore it.
            os.umask(0o002)
        started = time.monotonic()
        process = subprocess.Popen([shell, *(['-m'] if job_control else []), str(bindir / 'herdr-hub'), *args], env=env,
                                   cwd=case, text=True, stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   preexec_fn=child_setup)
        os.close(slave)
        try:
            # Reap cdl promptly, as the interactive parent shell would.
            if mode.startswith('cdl-exit-'): process.wait(timeout=5)
            out, err = process.communicate(timeout=25)
            if mode.startswith('cdl-exit-'):
                deadline = started + 5
                while not (case / 'note').exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                assert (case / 'note').exists(), 'prover did not stop within 5 s of cdl exit'
                assert time.monotonic() - started < 5
        finally:
            # Always close our PTY, including ownership or permission failures.
            try: stop_owned_process(process)
            finally: os.close(master)
        calls = [json.loads(line) for line in (case / 'calls').read_text().splitlines()] if (case / 'calls').exists() else []
        print(f'case {count}: {name}', flush=True)
        return case, home, process, out, err, calls

    def expect(case, rc):
        assert case[2].returncode == rc, (case[2].returncode, case[3], case[4])

    def success(case, extra=(), new=False, jobs=False, account=(), model='example-opus', code_mode='0', window=400000, full_log=False):
        expect(case, 0)
        work, home, process, out, err, calls = case
        state = home / '.local/state/herdr-orch/lead-push/42'
        config = state / 'taskr-mcp-7.json'
        prompt = (f'You are the hub root 42. herdr-hub adopted you at generation 7: do not run taskr adopt. '
                  f'This is a push lane: no plain taskr wait or ack. Pushed events arrive as <channel source="taskr" ...>; '
                  f'handle each with the taskr MCP tools, and ack with its fence. Read the handover at {state}/handover-7.md. '
                  f'A "hub push check 7" event will arrive: record it and ack it. Ack that check only from its push, never from taskr_inbox.'
                  f' Session timers (Claude CronCreate jobs, background loops) died with the old session: if the handover Note lists session timers to re-arm, create each again and record the new ids with a taskr note.')
        # A Code Mode hub reports when Code Mode is off, and then has the same waiting text.
        waiting = (f' Waiting: run a command whose result you need in the foreground, in the same response, with a Bash timeout that fits it. Use run_in_background only for a job over 10 minutes or when pushed events need you meanwhile; then end the turn and let its completion notification start the next one. Never sleep, poll or read a task\'s output file to check on a command, and never send a response whose only tool call is a wait or an ack.')
        if code_mode == '1':
            prompt += (' If this session has no REPL tool, Code Mode is off: record at once `taskr note "hub 7: Code Mode is off in this session" --as 42`, '
                       'then follow the waiting rule; it applies only then.')
        prompt += waiting
        actual = json.loads((work / 'cdl').read_text())
        settings = ('{"autoCompactEnabled":true,"autoCompactWindow":' + str(window) + ',"enabledPlugins":'
                    '{"posthog@claude-plugins-official":false,"slack@claude-plugins-official":false}}')
        expected = dict(args=[*account,'--model',model,'--effort','high',
                              '--settings',settings,'--mcp-config',str(config),
                              '--dangerously-load-development-channels','server:taskr',*extra,'--',prompt],
                        task=None, launch=None, role='hub', code_mode=code_mode, tmpdir=str(agent_tmp_dir),
                        pid=process.pid, cwd=str(work), umask=0o002)
        assert actual == expected, {'actual':actual, 'expected':expected}
        # The hub takes digest pushes with a 2-minute hold.
        servers = {'taskr':{'command':'taskr','args':['mcp','--channel','--as','42','--session','7'],
                            'env':{'TASKR_CHANNEL_DIGEST':'1','TASKR_CHANNEL_DIGEST_HOLD_S':'120'}}}
        if jobs:
            binding = 'herdr-hub-root-42-gen-7'
            assert 16 <= len(binding) <= 128
            servers['taskr-jobs'] = {'command':'taskr','args':['mcp','--jobs','--as','42','--session','7','--binding',binding]}
        assert config.read_text() == json.dumps({'mcpServers':servers}, separators=(',', ':')) + '\n'
        assert config.stat().st_mode & 0o777 == 0o600
        assert state.stat().st_mode & 0o777 == 0o700
        # cdl gets the per-user disk folder, not the caller's TMPDIR (README.md "Temp files").
        assert agent_tmp_dir.stat().st_mode & 0o777 == 0o700 and not agent_tmp_dir.is_symlink()
        assert (state / 'handover-7.md').stat().st_mode & 0o777 == 0o600
        assert (state / 'handover-7.md').read_text().startswith('handover text\n- Refs: ') or full_log
        # The root is read from status --tree, never campaign; every log read starts after a known adopt (W5421).
        assert not any(a[0] == 'campaign' for a,t,l in calls), calls
        assert (['status','--tree','42'] in [a for a,t,l in calls]) != new, calls
        logs = [a for a,t,l in calls if a[0] == 'log']
        full = [a for a in logs if a == ['log','42','--limit','0']]
        assert len(full) == full_log and all(a == ['log','42','--since','6','--limit','0'] for a in logs if a not in full), logs
        assert not (state / 'start.lock').exists()
        assert not (work / 'cdl-rejected').exists()
        assert (state / 'hub-check-7.result').read_text() == 'live\n'
        assert (state / 'hub-check-7.result').stat().st_mode & 0o777 == 0o600
        assert (work / 'note').read_text() == 'hub push check 7: live'
        assert sum(a[0] == 'adopt' for a,t,l in calls) == 1
        assert ['adopt','42','--workspace','wA','--tab','wA:t1','--pane','wA:p1'] in [a for a,t,l in calls]
        assert ['close','43','--outcome','accepted'] in [a for a,t,l in calls]
        for a,t,l in calls:
            assert (t,l) == (('43','8') if a[0] == 'ready' else (None,None)), (a,t,l)
        if new:
            assert calls[0][0] == ['new','example-hub','--role','orchestrator','--workspace','wA','--tab','wA:t1','--pane','wA:p1']
        assert all(a[0] != 'set' for a,t,l in calls)
        assert not (work / '.mcp.json').exists()
        # Under Herdr 0.9 the hub labels nothing: only herdr-core needs the agent label (hc_start_test.sh).
        if (work / 'herdr-calls').exists():
            labelled = [a for a,e in map(json.loads, (work / 'herdr-calls').read_text().splitlines()) if a[:2] == ['agent','label']]
            assert not labelled, labelled
        # Every file produced by the shim is confined to the scratch HOME.
        assert {p.name for p in work.iterdir()} <= {'home','calls','clock','adopted','ready','cdl','note','agent-list','agent-lists','prover-stdin','herdr-calls','dialog-answered',
                                                      'pane-reads','consent-note','inbox-reads'}
        assert out.splitlines()[0] == f'ROOT=42 GEN=7 mcp_config={config} prover_log={state}/hub-check-7.log'

    # The hub's window goes in --settings, never in an environment variable that leads would inherit.
    def hub_window(case):
        args = json.loads((case[0] / 'cdl').read_text())['args']
        return json.loads(args[args.index('--settings') + 1]).get('autoCompactWindow')
    c = run('hub caps the auto-compact window at 400000 by default')
    expect(c, 0)
    assert hub_window(c) == 400000, hub_window(c)
    success(c)
    for value in ('100000','300000','1000000'):
        c = run('hub takes HERDR_HUB_COMPACT_WINDOW ' + value, location={'HERDR_HUB_COMPACT_WINDOW':value})
        expect(c, 0)
        assert hub_window(c) == int(value), hub_window(c)
        success(c, window=int(value))
    for value in ('99999','1000001','0400000','abc','400k','-400000','10000000',''):
        for args in [('42',), ('--new','example-hub'), ('move','42')]:
            c = run(f'{args[0]} refuses HERDR_HUB_COMPACT_WINDOW {value!r} before any effect', args=args,
                    mode='move' if args[0] == 'move' else '', location={'HERDR_HUB_COMPACT_WINDOW':value})
            expect(c, 2)
            assert f"HERDR_HUB_COMPACT_WINDOW must be an integer from 100000 to 1000000 (got '{value}')" in c[4], c[4]
            assert not c[5] and not (c[0] / 'herdr-calls').exists() and not (c[0] / 'setsid-args').exists()
            assert not (c[0] / 'cdl').exists() and not (c[1] / '.local/state').exists()
    c = run('existing root may already record current pane; exact argv', args=('42','--','--resume','id with spaces',"quote'arg"))
    success(c, ('--resume','id with spaces',"quote'arg"))
    assert not (c[0] / 'agent-list').exists()
    assert c[3].splitlines()[1:] == ['To make this the hub: taskr set 42 hub=on']
    c = run('hub session disables PostHog and Slack only')
    success(c)
    c = run('hub account is the leading cdl pair', args=('42','--account',"account ' quoted",'--','--resume','saved-session'))
    success(c, ('--resume','saved-session'), account=('--account',"account ' quoted"))
    c = run('hub overrides inherited worker role', location={'FLEET_ROLE':'worker'})
    success(c)
    c = run('hub overrides an inherited Code Mode 1 without the opt-in', location={'CLAUDE_CODE_EXPERIMENTAL_CODE_MODE':'1'})
    success(c, code_mode='0')
    c = run('hub turns on Code Mode only with HERDR_HUB_CODE_MODE=1', location={'HERDR_HUB_CODE_MODE':'1'})
    success(c, code_mode='1')
    prompt = json.loads((c[0] / 'cdl').read_text())['args'][-1]
    assert prompt.count(' Waiting: ') == 1 and prompt.count('taskr note "hub 7: Code Mode is off in this session" --as 42') == 1, prompt
    c = run('hub without Code Mode gets the waiting text')
    success(c, code_mode='0')
    prompt = json.loads((c[0] / 'cdl').read_text())['args'][-1]
    assert prompt.count(' Waiting: ') == 1 and 'Code Mode' not in prompt, prompt
    c = run('hub Code Mode opt-in resumes a Code Mode session', args=('42','--','--resume','saved-session'),
            location={'HERDR_HUB_CODE_MODE':'1','CLAUDE_CODE_EXPERIMENTAL_CODE_MODE':'0'})
    success(c, ('--resume','saved-session'), code_mode='1')
    c = run('hub ignores an opt-in value other than 1', location={'HERDR_HUB_CODE_MODE':'yes'})
    success(c, code_mode='0')
    c = run('new root created then adopted', args=('--new','example-hub','--lane','opus-high'))
    success(c, new=True)
    c = run('jobs opt-in preserves channel, quoted Claude args and captured config',
            args=('42','--jobs','--','--resume','id with spaces',"quote'arg"))
    success(c, ('--resume','id with spaces',"quote'arg"), jobs=True)
    c = run('new hub opts in with generated root and adopt generation', args=('--new','example-hub','--jobs'))
    success(c, new=True, jobs=True)
    c = run('jobs keeps an existing immutable generation config', args=('42','--jobs'), mode='config-exists')
    expect(c, 1)
    assert (c[1] / '.local/state/herdr-orch/lead-push/42/taskr-mcp-7.json').read_text() == 'keep'
    assert not (c[0] / 'cdl').exists()
    c = run('existing hub ref suppresses suggestion', mode='hub-on')
    success(c)
    assert 'To make this the hub:' not in c[3]
    c = run('newer cleared hub ref restores suggestion', mode='hub-off')
    success(c)
    assert 'To make this the hub: taskr set 42 hub=on' in c[3]
    c = run("a lane's hub ref is not the root's", mode='hub-lane')
    success(c)
    assert 'To make this the hub: taskr set 42 hub=on' in c[3]
    c = run('a handover without a refs line falls back to the whole log', mode='refs-missing')
    success(c, full_log=True)
    assert 'To make this the hub:' not in c[3]
    c = run('the root row is found by id wherever status --tree lists it', mode='root-last')
    success(c)

    c = run('prover answers the development-channel dialog once', mode='dialog')
    success(c)
    sent = [a for a,e in map(json.loads, (c[0] / 'herdr-calls').read_text().splitlines()) if a[:2] == ['pane','send-keys']]
    assert sent == [['pane','send-keys','wA:p1','Enter']], sent

    # The Code Mode consent dialog: Enter would start the hub without Code Mode, so the prover sends nothing to it.
    consent_note = ("hub push check 7: Code Mode consent dialog is waiting in wA:p1: select 'Yes, turn on Code Mode' by hand "
                    '(README Code Mode, First start)')
    def sent_keys(c):
        return [a for a,e in map(json.loads, (c[0] / 'herdr-calls').read_text().splitlines()) if a[:2] == ['pane','send-keys']]
    def notes(c):
        return [a for a,t,l in c[5] if a[0] == 'note']
    enter = ['pane','send-keys','wA:p1','Enter']
    code_mode_on = {'HERDR_HUB_CODE_MODE':'1'}

    c = run('prover never answers a Code Mode consent dialog that stays past the deadline', mode='consent-stuck', location=code_mode_on)
    expect(c, 0)
    state = c[1] / '.local/state/herdr-orch/lead-push/42'
    reason = ("Code Mode consent dialog still waiting in wA:p1 after 600 s: select 'Yes, turn on Code Mode' by hand "
              '(README Code Mode, First start)')
    log = (state / 'hub-check-7.log').read_text()
    assert sent_keys(c) == [], sent_keys(c)
    assert notes(c) == [['note',consent_note,'--as','42'], ['note',f'hub push check 7: failed: {reason}','--as','42']], notes(c)
    assert (state / 'hub-check-7.result').read_text() == f'failed: {reason}\n', log
    assert log.count('consent dialog is waiting') == 1, log
    assert not any(a[0] == 'new' for a,t,l in c[5])

    c = run('prover answers the channel dialog once after a person answers the consent dialog', mode='consent-answered', location=code_mode_on)
    success(c, code_mode='1')
    assert sent_keys(c) == [enter], sent_keys(c)
    assert [a[1] for a in notes(c)] == [consent_note, 'hub push check 7: live'], notes(c)

    c = run('prover answers the channel dialog again after the consent restart', mode='consent-again', location=code_mode_on)
    success(c, code_mode='1')
    assert sent_keys(c) == [enter, enter], sent_keys(c)
    assert [a[1] for a in notes(c)] == [consent_note, 'hub push check 7: live'], notes(c)

    for mode in ('consent-both', 'consent-both-rev'):
        c = run('prover sends no key when both dialogs are on the screen: ' + mode, mode=mode, location=code_mode_on)
        expect(c, 0)
        assert sent_keys(c) == [], sent_keys(c)
        assert notes(c) == [['note',consent_note,'--as','42'], ['note',f'hub push check 7: failed: {reason}','--as','42']], notes(c)

    c = run('a late Yes gives the lease and the ack a full deadline', mode='consent-late', location=code_mode_on)
    success(c, code_mode='1')
    assert sent_keys(c) == [enter], sent_keys(c)
    assert [a[1] for a in notes(c)] == [consent_note, 'hub push check 7: live'], notes(c)
    # The ack came after the first deadline (about 660 on the test clock).
    assert int((c[0] / 'clock').read_text()) > 700, (c[0] / 'clock').read_text()

    c = run('a consent dialog that comes and goes restarts the deadline only once', mode='consent-flap', location=code_mode_on)
    expect(c, 0)
    assert sent_keys(c) == [] and (c[0] / 'note').read_text().startswith('hub push check 7: failed: '), (sent_keys(c), notes(c))
    assert [a[1] for a in notes(c)].count(consent_note) == 1, notes(c)
    assert int((c[0] / 'clock').read_text()) < 1400, (c[0] / 'clock').read_text()

    c = run('consent text with Code Mode off records no note and gets no key', mode='consent-off')
    success(c)
    assert sent_keys(c) == [] and [a[1] for a in notes(c)] == ['hub push check 7: live'], (sent_keys(c), notes(c))

    c = run('prover answers the development-channel dialog once with Code Mode on', mode='dialog', location=code_mode_on)
    success(c, code_mode='1')
    assert sent_keys(c) == [enter] and len(notes(c)) == 1, (sent_keys(c), notes(c))

    c = run('prover reads a channel dialog taller than 40 visible lines', mode='dialog-tall')
    success(c)
    sent = [a for a,e in map(json.loads, (c[0] / 'herdr-calls').read_text().splitlines()) if a[:2] == ['pane','send-keys']]
    assert sent == [['pane','send-keys','wA:p1','Enter']], sent
    assert any(a[:3] == ['pane','read','wA:p1'] and int(a[a.index('--lines') + 1]) >= 64
               for a,e in map(json.loads, (c[0] / 'herdr-calls').read_text().splitlines()) if a[:2] == ['pane','read'])

    c = run('prover waits while the successor drains an inbox backlog', mode='backlog')
    expect(c, 0)
    state = c[1] / '.local/state/herdr-orch/lead-push/42'
    assert (state / 'hub-check-7.result').read_text() == 'live\n', (state / 'hub-check-7.log').read_text()
    assert 'check event 40 waits behind 38 unacked events' in (state / 'hub-check-7.log').read_text()

    c = run('prover passes on an inbox ack and a later received push', mode='later-push')
    success(c)
    state = c[1] / '.local/state/herdr-orch/lead-push/42'
    assert 'check event 9 acked; later event 12 received' in (state / 'hub-check-7.log').read_text()
    assert not any(a[0] == 'close' and a[-1] == 'abandoned' for a,t,l in c[5])

    c = run('prover survives process-group SIGHUP', mode='hup')
    success(c)
    c = run('prover stdin is /dev/null with terminal job control', mode='prover-stdin', job_control=True)
    success(c)
    assert (c[0] / 'prover-stdin').read_text() == 'True'
    assert "can't access tty" not in c[4] and 'no job control' not in c[4]

    c = run('other recorded pane is free', mode='old-pane-free')
    success(c)
    for mode, text in [('old-pane-live','recorded hub pane is occupied'), ('fail-agent-list','read agent list')]:
        c = run(mode, mode=mode)
        expect(c, 1)
        assert text in c[4]
        assert [a[:3] for a,t,l in c[5]] == [['status','--tree','42']]
        assert not (c[1] / '.local/state').exists()

    for mode, reason, gate in [('cdl-exit-lease','cdl exited before the channel lease',False),
                               ('cdl-exit-ack','cdl exited before the ack',True)]:
        c = run(mode, mode=mode)
        expect(c, 0)
        state = c[1] / '.local/state/herdr-orch/lead-push/42'
        # After the check event exists the reason also says whether the check was acked.
        suffix = ' (check event 9: not acked' if gate else '\n'
        assert (state / 'hub-check-7.result').read_text().startswith(f'failed: {reason}{suffix}'), (state / 'hub-check-7.log').read_text()
        assert (c[0] / 'note').read_text().startswith(f'hub push check 7: failed: {reason}{suffix.rstrip()}')
        assert [a for a,t,l in c[5] if a[0] == 'close'] == ([['close','43','--outcome','abandoned']] if gate else [])
        assert ('closed check task 43 as abandoned' in (state / 'hub-check-7.log').read_text()) == gate
        assert (state / 'start.lock').is_dir()

    for mode, text in [('busy','still live after 75 s'),('closed','not an open root'),('not-root','not an open root'),
                       ('not-top','not an open root'),('no-row','read root campaign'),('fail-status','read root campaign')]:
        c = run(mode, mode=mode)
        expect(c, 1)
        assert text in c[4]
        assert not any(a[0] == 'adopt' for a,t,l in c[5])
        assert not (c[1] / '.local/state').exists()

    shutil.rmtree(agent_tmp_dir, True)
    agent_tmp_dir.symlink_to('/')
    try:
        c = run('a refused agent temp folder stops before any control-plane call')
    finally:
        agent_tmp_dir.unlink()
    expect(c, 2)
    assert f'herdr-agent-tmp: {agent_tmp_dir} must be a directory owned by you, not a symlink' in c[4], c[4]
    assert 'herdr-hub: agent temp folder refused' in c[4] and not c[5], (c[4], c[5])
    assert not (c[0] / 'cdl').exists() and not (c[1] / '.local/state').exists()

    for kind, prefixes, message in [('claude',['impl'],'allow orch'),('codex',['orch'],'Claude hubs only')]:
        c = run('reject incompatible lane', args=('42','--lane','other'),
                lane={'kind':kind,'model':'example','effort':'high','prefixes':prefixes})
        expect(c, 2)
        assert message in c[4] and not c[5]
        assert not (c[1] / '.local/state').exists()

    for args in [(), ('0',), ('42','--unknown'), ('42','--lane'), ('42','--account',''), ('42','--account'),
                 ('move','42','--account',''), ('move','42','--account'), ('--new','Bad'), ('--new','x'*33),
                 ('42','--code-mode','on'), ('--new','example-hub','--code-mode','off'), ('check','42','--code-mode','on'),
                 ('move','42','--code-mode'), ('move','42','--code-mode','yes'), ('move','42','--code-mode','')]:
        c = run('invalid arguments', args=args)
        expect(c, 2)
        assert not c[5]
        if args[-2:] in (('--code-mode','yes'), ('--code-mode','')): assert '--code-mode must be on or off' in c[4], c[4]
    for args in [('check','42','--jobs'), ('42','--jobs','--allow-ask','17'),
                 ('42','--jobs','--lane','other')]:
        c = run('unsupported jobs combination refuses before effects', args=args,
                lane={'kind':'codex','model':'example','effort':'high','prefixes':['orch']})
        expect(c, 2)
        assert not c[5] and not (c[0] / 'herdr-calls').exists()
        assert not (c[1] / '.local/state').exists()
    for prefix in [('42',), ('--new','example-hub'), ('move','42')]:
        for config_arg in [('--mcp-config','other.json'), ('--mcp-config=other.json',)]:
            c = run('jobs refuses competing Claude MCP config before creation or move',
                    args=(*prefix,'--jobs','--',*config_arg), mode='move' if prefix[0] == 'move' else '')
            expect(c, 2)
            assert '--jobs owns the generation MCP config' in c[4]
            assert not c[5] and not (c[0] / 'herdr-calls').exists()
            assert not (c[1] / '.local/state').exists()
    for variable, value in [('HERDR_PANE_ID',''), ('HERDR_TAB_ID','bad id'), ('HERDR_WORKSPACE_ID','null')]:
        c = run('invalid location', location={variable:value})
        expect(c, 2)
        assert not c[5]
    c = run('missing sibling cdl', cdl_present=False)
    expect(c, 2)
    assert not c[5]

    def seed_lock(owner=None):
        def seed(home):
            lock = home / '.local/state/herdr-orch/lead-push/42/start.lock'
            lock.mkdir(parents=True)
            if owner: (lock / 'owner').write_text(owner)
        return seed
    exited = subprocess.Popen(['true'])
    exited.wait()
    host = os.uname().nodename
    for label, owner in [('without owner', None), ('with a live owner', f'{os.getpid()} {host}\n'),
                         ('held after a generation conflict', f'{exited.pid} {host} hold\n'),
                         ('owned on another host', f'{exited.pid} other-host\n')]:
        c = run('existing lock prevents adoption: ' + label, seed=seed_lock(owner))
        expect(c, 1)
        assert 'another start needs reconciliation (start.lock owner: ' in c[4], c[4]
        assert not any(a[0] == 'adopt' for a,t,l in c[5])
    c = run('lock of an exited owner on this host is reclaimed', seed=seed_lock(f'{exited.pid} {host}\n'))
    success(c)
    assert f'reclaimed start.lock from exited pid {exited.pid}' in c[4]
    for mode, text in [('generation-conflict','generation conflict after adopt'),
                       ('config-exists','generation config exists'),('config-symlink','generation config exists'),
                       ('handover-exists','handover already exists')]:
        c = run(mode, mode=mode)
        expect(c, 1)
        assert text in c[4] and 'ROOT=42 GEN=7' in c[4]
        assert not (c[0] / 'cdl').exists()
        state = c[1] / '.local/state/herdr-orch/lead-push/42'
        assert (state / 'start.lock').is_dir()
        lines = (state / 'start.lock/owner').read_text().splitlines()
        owner = lines[0].split()
        assert owner[1:] == ([host, 'hold'] if mode == 'generation-conflict' else [host]), lines
        assert len(lines) == 1 if mode == 'generation-conflict' else lines[1].startswith('started '), lines
        if mode == 'config-exists': assert (state / 'taskr-mcp-7.json').read_text() == 'keep'
        if mode == 'handover-exists': assert (state / 'handover-7.md').read_text() == 'keep'
        if mode == 'config-symlink': assert (state / 'taskr-mcp-7.json').is_symlink()

    for mode, gate in [('fail-launch',True), ('fail-ready',True), ('queued-ready',True),
                       ('wrong-delivery',True),('unreceived',True),('unacked',True),('later-push-unacked',True),('later-push-old-gen',True),('prover-conflict',True),
                       ('fail-delivery',True),('fail-inbox',True),('lost-lease',True),
                       ('fail-new',False),('fail-accepted-close',True),('wrong-holder',False),('wrong-lease',False),
                       ('prover-lease-conflict',False)]:
        c = run('prover rejects '+mode, mode=mode)
        expect(c, 0)  # cdl exec succeeds; proof is asynchronous and fails separately.
        state = c[1] / '.local/state/herdr-orch/lead-push/42'
        assert (state / 'hub-check-7.result').read_text().startswith('failed: ')
        if mode == 'prover-lease-conflict':
            assert (state / 'hub-check-7.result').read_text() == 'failed: generation conflict while waiting for lease\n'
        assert (c[0] / 'note').read_text().startswith('hub push check 7: failed: ')
        states = {'wrong-delivery':'acked without a receipt at generation 7', 'unreceived':'acked without a receipt at generation 7',
                  'unacked':'not acked; event 9 received', 'later-push-unacked':'not acked; event 12 received',
                  'later-push-old-gen':'acked without a receipt at generation 7'}
        if mode in states:
            assert (state / 'hub-check-7.result').read_text().endswith(f'(check event 9: {states[mode]})\n'), (state / 'hub-check-7.result').read_text()
        if gate:
            assert 'closed check task 43 as abandoned' in (state / 'hub-check-7.log').read_text()
        closes = [a for a,t,l in c[5] if a[0] == 'close']
        expected = ([['close','43','--outcome','abandoned']] if gate else [])
        if mode == 'fail-accepted-close': expected.insert(0, ['close','43','--outcome','accepted'])
        assert closes == expected, closes
        assert (state / 'start.lock').is_dir()

    def herdr_calls(case):
        path = case[0] / 'herdr-calls'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    def move_log(case):
        # The first line names the Code Mode decision; with Code Mode on the consent warning is next; then the helper log.
        lines = case[3].splitlines()
        assert lines[0].startswith('herdr-hub: move: Code Mode '), case[3]
        on = lines[0].startswith('herdr-hub: move: Code Mode on (')
        warning = ("herdr-hub: move: Code Mode on: the new hub's account must have accepted Code Mode once, "
                   'or the new hub waits at the consent dialog (README Code Mode, First start)')
        assert lines.count(warning) == on and (not on or lines[1] == warning), case[3]
        return Path(lines[1 + on].split('; log ')[1])
    def wait_for(path, text):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if path.exists() and text in path.read_text(): return path.read_text()
            time.sleep(.05)
        raise AssertionError((path, path.read_text() if path.exists() else None))

    refused = []
    for args in [('42',), ('--new','example-hub'), ('move','42')]:
        moving = args[0] == 'move'
        c = run('hub-only lane ' + args[0], args=(*args,'--lane','other'), mode='move' if moving else '',
                lane={'kind':'claude','model':'example-hub-model','effort':'high','prefixes':['hub']})
        if c[2].returncode != 0:
            refused.append((args, c[2].returncode, c[4]))
            continue
        if moving:
            log = move_log(c)
            wait_for(log, 'herdr-hub: moved 42: generation 9 live')
            bindir = c[1] / '.local/bin'
            assert json.loads((c[0] / 'setsid-args').read_text()) == [str(bindir / 'herdr-hub'),'__move-helper','42',
                    '--lane','other','--session','sess-hub','--code-mode','off','--']
            assert shlex.split((c[0] / 'typed').read_text()) == ['env','HERDR_HUB_CODE_MODE=0',str(bindir / 'herdr-hub'),'42','--lane','other']
        else:
            success(c, new=args[0] == '--new', model='example-hub-model')
    assert not refused, refused

    c = run('move refuses without a handover since adopt', args=('move','42'))
    expect(c, 1)
    assert 'no handover since adopt 7' in c[4], c[4]
    assert not herdr_calls(c)
    assert not (c[0] / 'setsid-args').exists()
    c = run('move refuses outside the recorded hub pane', args=('move','42'), mode='move', location={'HERDR_PANE_ID':'wA:p5'})
    expect(c, 1)
    assert 'recorded at this pane' in c[4]
    c = run('move with --no-handover is a move-only flag', args=('42','--no-handover'))
    expect(c, 2)

    c = run('move starts a detached helper that stops the hub and reruns herdr-hub',
            args=('move','42','--','--resume','id with spaces'), mode='move', location={'CLAUDECODE':'1'})
    expect(c, 0)
    state = c[1] / '.local/state/herdr-orch/lead-push/42'
    log = move_log(c)
    assert log.parent == state and 'end this turn now' in c[3], c[3]
    text = wait_for(log, 'herdr-hub: moved 42: generation 9 live')
    calls = herdr_calls(c)
    assert all(not env for a, env in calls), calls
    keys = [a for a, env in calls if a[:2] == ['pane','send-keys']]
    assert keys == [['pane','send-keys','wA:p1','esc','ctrl+c','ctrl+c']], keys
    bindir = c[1] / '.local/bin'
    assert json.loads((c[0] / 'setsid-args').read_text()) == [str(bindir / 'herdr-hub'),'__move-helper','42',
            '--lane','opus-high','--session','sess-hub','--code-mode','off','--','--resume','id with spaces']
    assert (c[0] / 'typed').read_text() == f"env HERDR_HUB_CODE_MODE=0 '{bindir}/herdr-hub' 42 --lane 'opus-high' -- '--resume' 'id with spaces'", text
    assert not any(a[0] == 'adopt' for a,t,l in c[5])
    # Move, its helper and herdr-lead stop read the root row and bounded logs only: from the lease's generation 3,
    # the handover from adopt 7, and the successor from the old generation 7 (W5421).
    reads = [json.loads(line)[0] for line in (c[0] / 'calls').read_text().splitlines()]
    assert not any(a[0] == 'campaign' for a in reads), reads
    logs = [a for a in reads if a[0] == 'log']
    for bounded in (['log','42','--since','2','--limit','0'], ['log','42','--since','7','--limit','0'], ['log','42','--since','6','--limit','0']):
        assert bounded in logs, (bounded, logs)
    assert ['log','42','--limit','0'] not in logs, logs
    assert ['status','--tree','42'] in reads, reads

    c = run('jobs move forwards opt-in across detached helper and shell-quoted successor',
            args=('move','42','--jobs','--account',"account ' quoted",'--','--resume','id with spaces',"quote'arg"), mode='move')
    expect(c, 0)
    log = move_log(c)
    wait_for(log, 'herdr-hub: moved 42: generation 9 live')
    bindir = c[1] / '.local/bin'
    assert json.loads((c[0] / 'setsid-args').read_text()) == [str(bindir / 'herdr-hub'),'__move-helper','42',
            '--lane','opus-high','--session','sess-hub','--code-mode','off','--jobs','--account',"account ' quoted",'--','--resume','id with spaces',"quote'arg"]
    assert shlex.split((c[0] / 'typed').read_text()) == ['env','HERDR_HUB_CODE_MODE=0',str(bindir / 'herdr-hub'),'42','--lane','opus-high','--jobs','--account',"account ' quoted",
                                                        '--','--resume','id with spaces',"quote'arg"]
    assert not any(a[0] == 'adopt' for a,t,l in c[5])

    # The pane shell lacks the caller's opt-in. The move reads the hub's Code Mode before the CLAUDE* strip,
    # passes it to the helper as --code-mode, and the helper types it through env both ways; --code-mode on move wins.
    for location, extra, decision, source in [
            ({'CLAUDE_CODE_EXPERIMENTAL_CODE_MODE':'1'}, (), 'on', 'CLAUDE_CODE_EXPERIMENTAL_CODE_MODE=1'),
            ({'HERDR_HUB_CODE_MODE':'1'}, (), 'on', 'HERDR_HUB_CODE_MODE=1'),
            (None, (), 'off', 'default off'),
            ({'HERDR_HUB_CODE_MODE':'yes','CLAUDE_CODE_EXPERIMENTAL_CODE_MODE':'0'}, (), 'off', 'default off'),
            ({'HERDR_HUB_CODE_MODE':'1','CLAUDE_CODE_EXPERIMENTAL_CODE_MODE':'1'}, ('--code-mode','off'), 'off', '--code-mode'),
            (None, ('--code-mode','on'), 'on', '--code-mode')]:
        c = run(f'move keeps Code Mode {decision} from {source} ({location})', args=('move','42',*extra), mode='move', location=location)
        expect(c, 0)
        assert c[3].splitlines()[0] == f'herdr-hub: move: Code Mode {decision} ({source})', c[3]
        log = move_log(c)
        text = wait_for(log, 'herdr-hub: moved 42: generation 9 live')
        bindir = c[1] / '.local/bin'
        assert json.loads((c[0] / 'setsid-args').read_text()) == [str(bindir / 'herdr-hub'),'__move-helper','42',
                '--lane','opus-high','--session','sess-hub','--code-mode',decision,'--']
        typed = (c[0] / 'typed').read_text()
        assert shlex.split(typed) == ['env',f'HERDR_HUB_CODE_MODE={int(decision == "on")}', str(bindir / 'herdr-hub'),'42','--lane','opus-high'], typed
        assert f'herdr-hub: move: run {typed}\n' in text, text
        # Model the hub pane shell: run the typed line in a shell that exports the opposite value.
        executable(bindir / 'herdr-hub', '#!/bin/sh\nprintf %s "$HERDR_HUB_CODE_MODE"\n')
        opposite = '0' if decision == 'on' else '1'
        seen = subprocess.run(['sh','-c',typed], env={'PATH':'/usr/bin:/bin','HERDR_HUB_CODE_MODE':opposite},
                              capture_output=True, text=True, check=True).stdout
        assert seen == str(int(decision == 'on')), (typed, seen)

    # A set window is typed for the new hub, as Code Mode is; without one the typed line has no window (the cases above).
    c = run('move passes HERDR_HUB_COMPACT_WINDOW to the new hub', args=('move','42'), mode='move', location={'HERDR_HUB_COMPACT_WINDOW':'300000'})
    expect(c, 0)
    text = wait_for(move_log(c), 'herdr-hub: moved 42: generation 9 live')
    bindir = c[1] / '.local/bin'
    typed = (c[0] / 'typed').read_text()
    assert shlex.split(typed) == ['env','HERDR_HUB_CODE_MODE=0','HERDR_HUB_COMPACT_WINDOW=300000',str(bindir / 'herdr-hub'),'42','--lane','opus-high'], typed
    assert f'herdr-hub: move: run {typed}\n' in text, text
    executable(bindir / 'herdr-hub', '#!/bin/sh\nprintf %s "$HERDR_HUB_COMPACT_WINDOW"\n')
    seen = subprocess.run(['sh','-c',typed], env={'PATH':'/usr/bin:/bin'}, capture_output=True, text=True, check=True).stdout
    assert seen == '300000', (typed, seen)

    c = run('move helper failure is noted on the root', args=('move','42','--no-handover'), mode='move-stuck')
    expect(c, 0)
    wait_for(c[0] / 'note', 'herdr-hub move failed: stop the hub')
    assert not (c[0] / 'typed').exists()

    c = run('move helper stops nothing when another agent took the hub pane', args=('move','42'), mode='move-replaced')
    expect(c, 0)
    log = move_log(c)
    wait_for(c[0] / 'note', 'herdr-hub move failed: stop the hub')
    assert 'wA:p1 runs hub-claude (session sess-other), not the lead of root 42' in log.read_text(), log.read_text()
    assert not [a for a, env in herdr_calls(c) if a[:2] == ['pane','send-keys']]
    assert not (c[0] / 'typed').exists()

    c = run('move helper follows the prover past 700 s', args=('move','42'), mode='move-slow')
    expect(c, 0)
    log = move_log(c)
    wait_for(log, 'herdr-hub: moved 42: generation 9 live')
    assert int((c[0] / 'clock').read_text()) - int((c[0] / 'typed-at').read_text()) >= 1200

    c = run('move helper follows a Code Mode prover past 2520 s', args=('move','42','--code-mode','on'), mode='move-slow-consent')
    expect(c, 0)
    log = move_log(c)
    wait_for(log, 'herdr-hub: moved 42: generation 9 live')
    assert int((c[0] / 'clock').read_text()) - int((c[0] / 'typed-at').read_text()) >= 2700

    nosetsid = scratch / 'nosetsid'
    nosetsid.mkdir()
    for entry in stubs.iterdir():
        if entry.name != 'setsid': (nosetsid / entry.name).symlink_to(entry)
    for bindir in ('/usr/bin', '/bin'):
        for entry in os.listdir(bindir):
            if entry != 'setsid' and not os.path.lexists(nosetsid / entry): (nosetsid / entry).symlink_to(Path(bindir) / entry)
    c = run('move without setsid fails before any helper or stop', args=('move','42'), mode='move', location={'PATH':str(nosetsid)})
    expect(c, 1)
    assert 'needs setsid' in c[4], c[4]
    assert not c[5] and not herdr_calls(c) and not (c[0] / 'setsid-args').exists()
    assert not (c[1] / '.local/state').exists()

    fake = scratch / 'fake-setsid'
    fake.mkdir()
    executable(fake / 'setsid', '#!/bin/sh\nexec "$@"\n')
    c = run('move refuses a helper that stayed in the caller process group', args=('move','42'), mode='move', location={'PATH':f'{fake}:{stubs}:/usr/bin:/bin'})
    expect(c, 1)
    assert 'move helper did not detach; nothing was stopped' in c[4] and 'is not detached' in c[4], c[4]
    assert not [a for a, env in herdr_calls(c) if a[:2] == ['pane','send-keys']]

    for mode, rc, output in [('check-live',0,'live'),('check-stale',1,'stale'),('',1,'stale'),
                             ('unknown',2,'unknown'),('fail-log',2,'unknown'),('fail-consumer',2,'unknown'),
                             ('check-floor-gone',1,'stale'),('no-lease',1,'stale')]:
        c = run('check '+mode, args=('check','42'), mode=mode)
        expect(c, rc)
        assert c[3].strip() == output
        # The lease's generation bounds the log read; without one, or with no adopt at or after it, the whole log is read.
        logs = [a for a,t,l in c[5] if a[0] == 'log']
        if mode in ('check-live', 'check-stale', ''):
            assert logs == [['log','42','--since',{'check-stale':'5','':'2'}.get(mode, '6'),'--limit','0']], logs
        if mode == 'check-floor-gone': assert logs == [['log','42','--since','19','--limit','0'], ['log','42','--limit','0']], logs
        if mode in ('no-lease', 'unknown'): assert logs == [['log','42','--limit','0']], logs
        assert not (c[1] / '.local/state').exists()

    for mode in ['check-dropped', 'check-flag', 'check-channels', 'check-other-generation']:
        c = run('check argv '+mode, args=('check','42'), mode=mode)
        expect(c, 1 if mode == 'check-dropped' else 0)
        lines = c[3].splitlines()
        assert lines[0] == ('push-dropped' if mode == 'check-dropped' else 'live'), c[3]
        assert (c[0] / 'awk-env-checked').exists(), 'check did not keep the config out of helper argv'
        if mode == 'check-dropped':
            assert len(lines) == 3, c[3]
            assert lines[1] == 'Repair: herdr-hub move 42 --lane <lane> --account <account> -- --resume <session-id>', c[3]
            assert lines[2] == 'If that session started with Code Mode: omit -- --resume; or, to keep the session, run herdr-hub move 42 --lane <lane> --account <account> --code-mode on -- --resume <session-id> (README "Code Mode")', c[3]
        assert not (c[1] / '.local/state').exists()

    # Review 7952 F1: an old prover finishes after its hub exited and a rerun reclaimed the lock. It must not
    # remove the replacement lock, or a third start enters beside the second. Runs the hub's own functions.
    count += 1
    race = scratch / 'orphan-prover'
    (race / 'bin').mkdir(parents=True)
    source = (setup / 'bin/herdr-hub').read_text()
    def function(name):
        start = source.index('\n' + name + '() ') + 1
        line = source[start:source.index('\n', start)]
        return line if line.endswith('}') else source[start:source.index('\n}', start) + 2]
    lock_block = '\n'.join(function(n) for n in ('take_guard', 'drop_guard', 'take_lock', 'own_lock', 'release_lock')
                           if '\n' + n + '() ' in source)
    prelude = '\n'.join(['set -u', 'root=42 pane=wA:p1 gen=7 mcp_config=fixture lease=unknown check_task= step=0 mode=start',
                         'tmp= adopt_err= lock= lock_owner= guard= event= check_state= check_closed=', 'state_dir=$CASE host=$(uname -n)',
                         function('cleanup'), function('fail'), function('number'), lock_block])
    mock = '''
latest_gen() { printf '7\n'; }
read_lease() { lease=live; }
current_lease() { return 0; }
dialog_shown() { return 1; }
ledger() {
    [ "$1" != --json ] || shift
    case $1 in
        new) printf '{"task_id":43}\n' ;;
        launch) printf '{"launch_id":8}\n' ;;
        inbox) printf '{"acked_event_id":9,"events":[]}\n' ;;
        delivery) printf '{"attempts":[{"event_id":9,"session_gen":7,"received_at":"now"}]}\n' ;;
        close) : >"$CASE/old-close-pending"; read -r go <"$CASE/release-old"; printf '{"task_id":43}\n' ;;
        note) : >"$CASE/old-finished" ;;
        *) return 99 ;;
    esac
}
'''
    prove = source[source.index('prove() ('):source.index('\nprover_log=')]
    executable(race / 'bin/taskr', '#!/bin/sh\nprintf \'{"event_id":9}\\n\'\n')
    (race / 'old.sh').write_text(prelude + mock + prove + '\ntake_lock\nprove &\nwait\n')
    (race / 'new.sh').write_text(prelude + '\ntrap cleanup 0\ngen=\ntake_lock\n: >"$CASE/entered-$WHO"\nread -r go <"$CASE/release-$WHO"\n')
    fds = {}
    for who in ('old', 'B', 'C'):
        os.mkfifo(race / f'release-{who}')
        fds[who] = os.open(race / f'release-{who}', os.O_RDWR)
    race_env = dict(HOME=str(race), PATH=f'{race}/bin:/usr/bin:/bin', CASE=str(race))
    procs = []
    def starter(script, who):
        log = (race / f'{who}.log').open('w')
        q = subprocess.Popen([shell, str(race / script)], env=dict(race_env, WHO=who), stdout=log, stderr=subprocess.STDOUT,
                             start_new_session=True)
        procs.append(q)
        return q
    def appears(name):
        deadline = time.monotonic() + 10
        while not (race / name).exists():
            assert time.monotonic() < deadline, ('timeout', name)
            time.sleep(.02)
    try:
        a = starter('old.sh', 'A')
        appears('old-close-pending')
        # The hub dies after its prover's last checks, while the accepted close is pending.
        os.kill(a.pid, signal.SIGKILL)
        a.wait()
        b = starter('new.sh', 'B')
        appears('entered-B')
        os.write(fds['old'], b'continue\n')
        appears('old-finished')
        assert b.poll() is None
        assert (race / 'start.lock/owner').exists(), 'old prover removed the replacement lock'
        assert (race / 'start.lock/owner').read_text().split()[0] == str(b.pid)
        c = starter('new.sh', 'C')
        assert c.wait(timeout=15) == 1, 'a third start entered beside the second'
        assert 'another start needs reconciliation (start.lock owner: ' in (race / 'C.log').read_text()
        assert not (race / 'entered-C').exists() and not (race / 'start.guard').exists()
    finally:
        for who in ('old', 'B', 'C'):
            os.write(fds[who], b'finish\n')
            os.close(fds[who])
        for q in procs:
            try: q.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stop_owned_process(q)
    print(f'case {count}: an orphan prover never removes a replacement start.lock', flush=True)

    # The generation-conflict hold mark is written only on this run's own lock.
    count += 1
    hold = scratch / 'hold-mark'
    hold.mkdir()
    replacement = f'999999 {host}\nstarted 1\n'
    (hold / 'hold.sh').write_text(prelude + '\ntrap cleanup 0\ntake_lock\ngen=7\n' +
                                  f"printf '%s' '{replacement}' >\"$lock/owner\"\n" +
                                  "fail 'generation conflict after adopt; do not re-adopt'\n")
    rc = subprocess.run([shell, str(hold / 'hold.sh')], env=dict(race_env, CASE=str(hold)), capture_output=True).returncode
    assert rc == 1 and (hold / 'start.lock/owner').read_text() == replacement, (hold / 'start.lock/owner').read_text()
    assert not (hold / 'start.guard').exists()
    print(f'case {count}: the hold mark never overwrites a replacement start.lock', flush=True)

    # Review 7981 Consider: with a stale guard the hold mark cannot be written, so the failure says to reconcile by hand.
    count += 1
    stale = scratch / 'hold-stale-guard'
    stale.mkdir()
    (stale / 'hold.sh').write_text(prelude + '\ntrap cleanup 0\ntake_lock\ngen=7\nmkdir "$state_dir/start.guard"\n' +
                                   "fail 'generation conflict after adopt; do not re-adopt'\n")
    run_stale = subprocess.run([shell, str(stale / 'hold.sh')], env=dict(race_env, CASE=str(stale)), capture_output=True, text=True)
    assert run_stale.returncode == 1, run_stale
    assert 'could not mark ' in run_stale.stderr and 'reconcile it by hand before any rerun' in run_stale.stderr, run_stale.stderr
    assert (stale / 'start.lock').is_dir() and 'hold' not in (stale / 'start.lock/owner').read_text()
    print(f'case {count}: a stale guard turns the skipped hold mark into a reconcile-by-hand message', flush=True)

    # Review 7963 F1: A gets SIGTERM after its rmdir of start.guard while B, holding the guard, has read A's owner.
    # A's exit trap must not remove B's guard, or C enters and B then reclaims C's live lock. Both launchers.
    for script in ('herdr-hub', 'herdr-lead'):
        count += 1
        race = scratch / f'guard-interrupt-{script}'
        (race / 'bin').mkdir(parents=True)
        source = (setup / 'bin' / script).read_text()
        (race / 'take.sh').write_text('\n'.join([
            'set -u', 'root=42 pane=wA:p1 gen= mcp_config=fixture lease=unknown check_task= step=0 mode=start',
            'agent= tmp= adopt_err= lock= lock_owner= guard= log_file= log_buf= event= check_state= check_closed=', 'state_dir=$CASE host=$(uname -n)',
            *(function(n) for n in ('cleanup', 'fail', 'abandon_check', 'number', 'take_guard', 'drop_guard', 'take_lock', 'own_lock', 'release_lock')
              if '\n' + n + '() ' in source),
            'note() { :; }', 'trap cleanup 0', "trap 'fail interrupted' 1 2 3 15", 'take_lock',
            ': >"$CASE/entered-$WHO"', 'read -r go <"$CASE/release-$WHO"', '']))
        # Each wrapper does the real operation, then delays only its return, once.
        for tool, who, target in (('rmdir', 'A', '*start.guard'), ('sed', 'B', '*start.lock/owner')):
            executable(race / 'bin' / tool, f'''#!/bin/sh
PATH=/usr/bin:/bin {tool} "$@"; rc=$?
case "$WHO:$*" in
    {who}:{target})
        if [ ! -e "$CASE/paused-{who}" ]; then
            : >"$CASE/paused-{who}"
            read -r go <"$CASE/go-{who}"
        fi ;;
esac
exit "$rc"
''')
        fds = {}
        for name in ('go-A', 'go-B', 'release-A', 'release-B', 'release-C'):
            os.mkfifo(race / name)
            fds[name] = os.open(race / name, os.O_RDWR)
        race_env = dict(HOME=str(race), PATH=f'{race}/bin:/usr/bin:/bin', CASE=str(race))
        procs = []
        def starter(who):
            q = subprocess.Popen([shell, str(race / 'take.sh')], env=dict(race_env, WHO=who), stdout=(race / f'{who}.log').open('w'),
                                 stderr=subprocess.STDOUT, start_new_session=True)
            procs.append(q)
            return q
        def appears(name):
            deadline = time.monotonic() + 15
            while not (race / name).exists():
                assert time.monotonic() < deadline, ('timeout', script, name)
                time.sleep(.02)
        try:
            a = starter('A')
            appears('paused-A')
            b = starter('B')
            appears('paused-B')
            assert (race / 'start.guard').is_dir()
            os.kill(a.pid, signal.SIGTERM)
            os.write(fds['go-A'], b'go\n')
            # A's cleanup waits 10 s for the guard B holds, then keeps its lock rather than touch B's guard.
            assert a.wait(timeout=30) == 1
            assert (race / 'start.guard').is_dir(), f'{script}: the interrupted start removed the guard another start holds'
            c = starter('C')
            os.write(fds['go-B'], b'go\n')
            appears('entered-B')
            assert c.wait(timeout=30) == 1, f'{script}: a third start entered'
            assert not (race / 'entered-C').exists() and b.poll() is None
            assert (race / 'start.lock/owner').read_text().split()[0] == str(b.pid)
        finally:
            for fd in fds.values():
                os.write(fd, b'finish\n')
                os.close(fd)
            for q in procs:
                try: q.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    stop_owned_process(q)
        print(f'case {count}: {script}: an interrupted guard release never frees another start\'s guard', flush=True)

print(f'PASS: {count} herdr-hub cases')
PY
