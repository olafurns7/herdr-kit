#!/bin/sh
# Fake control plane and cdl; real jq/files, isolated HOME and ledger path.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
exec python3 - "$repo_dir" "${HUB_SH:-sh}" <<'PY'
import fcntl, json, os, pty, shutil, signal, subprocess, sys, tempfile, termios, time
from pathlib import Path

setup = Path(sys.argv[1])
shell = shutil.which(sys.argv[2])
count = 0

def executable(path, source):
    path.write_text(source)
    path.chmod(0o755)

with tempfile.TemporaryDirectory(prefix='herdr-hub-test.') as scratch:
    scratch = Path(scratch)
    stubs = scratch / 'stubs'
    stubs.mkdir()
    executable(stubs / 'herdr', '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
assert sys.argv[1:] == ['agent', 'list']
case = Path(os.environ['CASE'])
(case / 'agent-list').touch()
mode = os.environ.get('MODE', '')
if mode == 'fail-agent-list': sys.exit(1)
print(json.dumps({'result':{'agents':[{'pane_id':'wA:p2' if mode == 'old-pane-live' else 'wA:p1'}]}}))
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
if cmd == 'campaign':
    print(json.dumps({'root': {'id':42, 'role': 'gate' if mode == 'not-root' else 'orchestrator',
          'status': 'closed' if mode == 'closed' else 'open',
          'pane_id':'wA:p2' if mode in ('old-pane-live', 'old-pane-free', 'fail-agent-list') else 'wA:p1'}}))
elif cmd == 'consumer':
    if mode in ('cdl-exit-lease', 'prover-lease-conflict') and (case / 'adopted').exists():
        (case / 'waiting-lease').touch()
        print('{"live":false}')
    elif mode == 'lost-lease' and (case / 'ready').exists(): print('{"live":false}')
    elif mode == 'unknown': print('{}')
    elif mode == 'busy' or (case / 'adopted').exists() or mode.startswith('check-'):
        print(json.dumps({'live': True, 'holder': 'other' if mode == 'wrong-holder' else 'mcp-channel-123',
                          'session_gen': 6 if mode in ('wrong-lease', 'check-stale') else 7}))
    else: print('{"live":false}')
elif cmd == 'new':
    print(json.dumps({'task_id':43 if '--parent' in args else 42}))
elif cmd == 'adopt':
    (case / 'adopted').touch()
    state = Path(os.environ['HOME']) / '.local/state/herdr-orch/lead-push/42'
    if mode == 'config-exists': (state / 'taskr-mcp-7.json').write_text('keep')
    if mode == 'config-symlink': (state / 'taskr-mcp-7.json').symlink_to(case / 'missing')
    if mode == 'handover-exists': (state / 'handover-7.md').write_text('keep')
    print('handover text')
    print('taskr adopt: task 42 now at pane wA:p1 (was wA:p1); event 7', file=sys.stderr)
elif cmd == 'log':
    gen = 8 if (mode == 'generation-conflict' or
                (mode == 'prover-conflict' and (case / 'ready').exists()) or
                (mode == 'prover-lease-conflict' and (case / 'waiting-lease').exists())) else 7
    print(json.dumps({'kind':'adopt','id':gen}))
    if mode in ('hub-on', 'hub-off'):
        print(json.dumps({'kind':'ref','id':1,'data':{'key':'hub','value':'on'}}))
    if mode == 'hub-off': print(json.dumps({'kind':'ref','id':2,'data':{'key':'hub','value':''}}))
elif cmd == 'launch': print('{"launch_id":8}')
elif cmd == 'ready':
    if mode == 'prover-stdin':
        info = os.fstat(0)
        (case / 'prover-stdin').write_text(str(stat.S_ISCHR(info.st_mode) and info.st_rdev == os.stat(os.devnull).st_rdev))
    (case / 'ready').touch()
    if mode == 'queued-ready': print('{"queued":true,"request_key":"example"}')
    else: print('{"event_id":9}')
elif cmd == 'delivery':
    print(json.dumps({'attempts':[{'event_id':9, 'session_gen':6 if mode == 'wrong-delivery' else 7,
                                  'received_at':None if mode == 'unreceived' else 'now'}]}))
elif cmd == 'inbox': print(json.dumps({'acked_event_id':8 if mode in ('unacked', 'cdl-exit-ack') else 9}))
elif cmd == 'close': print('{"task_id":43}')
elif cmd == 'note':
    (case / 'note').write_text(args[1])
    print('{"event_id":10}')
else: sys.exit(99)
''')
    executable(stubs / 'date', '''#!/bin/sh
n=0
[ ! -f "$CASE/clock" ] || n=$(cat "$CASE/clock")
n=$((n + 30))
printf '%s\\n' "$n" >"$CASE/clock"
printf '%s\\n' "$n"
''')
    executable(stubs / 'sleep', '#!/bin/sh\nexit 0\n')
    cdl = '''#!/usr/bin/python3
import json, os, signal, sys, time
from pathlib import Path
case = Path(os.environ['CASE'])
(case / 'cdl').write_text(json.dumps({'args':sys.argv[1:], 'task':os.getenv('TASKR_TASK'),
    'launch':os.getenv('TASKR_LAUNCH'), 'pid':os.getpid(), 'cwd':os.getcwd(), 'umask':os.umask(0)}))
mode = os.environ.get('MODE', '')
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
        if cdl_present: executable(bindir / 'cdl', cdl)
        config = home / '.config/herdr-fleet'
        config.mkdir(parents=True)
        lanes = {'opus-high': {'kind':'claude','model':'example-opus','effort':'high','prefixes':['orch']}}
        if lane: lanes['other'] = lane
        (config / 'lanes.json').write_text(json.dumps(lanes))
        if seed: seed(home)
        env = dict(HOME=str(home), PATH=f'{stubs}:/usr/bin:/bin', CASE=str(case), MODE=mode,
                   HERDR_PANE_ID='wA:p1', HERDR_WORKSPACE_ID='wA', HERDR_TAB_ID='wA:t1',
                   TASKR_TASK='99', TASKR_LAUNCH='100', TASKR_DB=str(home / 'scratch.db'))
        env.update(location or {})
        master, slave = pty.openpty()
        def child_setup():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
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
            # A failed mutation must not leave its prover running after this test.
            try: os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError: pass
            process.wait()
            os.close(master)
        calls = [json.loads(line) for line in (case / 'calls').read_text().splitlines()] if (case / 'calls').exists() else []
        print(f'case {count}: {name}', flush=True)
        return case, home, process, out, err, calls

    def expect(case, rc):
        assert case[2].returncode == rc, (case[2].returncode, case[3], case[4])

    def success(case, extra=(), new=False):
        expect(case, 0)
        work, home, process, out, err, calls = case
        state = home / '.local/state/herdr-orch/lead-push/42'
        config = state / 'taskr-mcp-7.json'
        prompt = (f'You are the hub root 42. herdr-hub adopted you at generation 7: do not run taskr adopt. '
                  f'This is a push lane: no plain taskr wait or ack. Pushed events arrive as <channel source="taskr" ...>; '
                  f'handle each with the taskr MCP tools, and ack with its fence. Read the handover at {state}/handover-7.md. '
                  f'A "hub push check 7" event will arrive: record it and ack it.')
        actual = json.loads((work / 'cdl').read_text())
        assert actual == dict(args=['--model','example-opus','--effort','high','--mcp-config',str(config),
                                   '--dangerously-load-development-channels','server:taskr',*extra,prompt],
                              task=None, launch=None, pid=process.pid, cwd=str(work), umask=0o002), actual
        assert json.loads(config.read_text()) == {'mcpServers':{'taskr':{'command':'taskr',
                'args':['mcp','--channel','--as','42','--session','7']}}}
        assert config.stat().st_mode & 0o777 == 0o600
        assert state.stat().st_mode & 0o777 == 0o700
        assert (state / 'handover-7.md').stat().st_mode & 0o777 == 0o600
        assert (state / 'handover-7.md').read_text() == 'handover text\n'
        assert not (state / 'start.lock').exists()
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
        # Every file produced by the shim is confined to the scratch HOME.
        assert {p.name for p in work.iterdir()} <= {'home','calls','clock','adopted','ready','cdl','note','agent-list','prover-stdin'}
        assert out.splitlines()[0] == f'ROOT=42 GEN=7 mcp_config={config} prover_log={state}/hub-check-7.log'

    c = run('existing root may already record current pane; exact argv', args=('42','--','--resume','id with spaces',"quote'arg"))
    success(c, ('--resume','id with spaces',"quote'arg"))
    assert not (c[0] / 'agent-list').exists()
    assert c[3].splitlines()[1:] == ['To make this the hub: taskr set 42 hub=on']
    c = run('new root created then adopted', args=('--new','example-hub','--lane','opus-high'))
    success(c, new=True)
    c = run('existing hub ref suppresses suggestion', mode='hub-on')
    success(c)
    assert 'To make this the hub:' not in c[3]
    c = run('newer cleared hub ref restores suggestion', mode='hub-off')
    success(c)
    assert 'To make this the hub: taskr set 42 hub=on' in c[3]

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
        assert [a[0] for a,t,l in c[5]] == ['campaign']
        assert not (c[1] / '.local/state').exists()

    for mode, reason, gate in [('cdl-exit-lease','cdl exited before the channel lease',False),
                               ('cdl-exit-ack','cdl exited before the ack',True)]:
        c = run(mode, mode=mode)
        expect(c, 0)
        state = c[1] / '.local/state/herdr-orch/lead-push/42'
        assert (state / 'hub-check-7.result').read_text() == f'failed: {reason}\n', (state / 'hub-check-7.log').read_text()
        assert (c[0] / 'note').read_text() == f'hub push check 7: failed: {reason}'
        assert [a for a,t,l in c[5] if a[0] == 'close'] == ([['close','43','--outcome','rejected']] if gate else [])
        assert (state / 'start.lock').is_dir()

    for mode, text in [('busy','still live after 75 s'),('closed','not an open root'),('not-root','not an open root')]:
        c = run(mode, mode=mode)
        expect(c, 1)
        assert text in c[4]
        assert not any(a[0] == 'adopt' for a,t,l in c[5])
        assert not (c[1] / '.local/state').exists()

    for kind, prefixes, message in [('claude',['impl'],'allow orch'),('codex',['orch'],'Claude hubs only')]:
        c = run('reject incompatible lane', args=('42','--lane','other'),
                lane={'kind':kind,'model':'example','effort':'high','prefixes':prefixes})
        expect(c, 2)
        assert message in c[4] and not c[5]
        assert not (c[1] / '.local/state').exists()

    for args in [(), ('0',), ('42','--unknown'), ('42','--lane'), ('--new','Bad'), ('--new','x'*33)]:
        c = run('invalid arguments', args=args)
        expect(c, 2)
        assert not c[5]
    for variable, value in [('HERDR_PANE_ID',''), ('HERDR_TAB_ID','bad id'), ('HERDR_WORKSPACE_ID','null')]:
        c = run('invalid location', location={variable:value})
        expect(c, 2)
        assert not c[5]
    c = run('missing sibling cdl', cdl_present=False)
    expect(c, 2)
    assert not c[5]

    def seed_lock(home):
        (home / '.local/state/herdr-orch/lead-push/42/start.lock').mkdir(parents=True)
    c = run('existing lock prevents adoption', seed=seed_lock)
    expect(c, 1)
    assert not any(a[0] == 'adopt' for a,t,l in c[5])
    for mode, text in [('generation-conflict','generation conflict after adopt'),
                       ('config-exists','generation config exists'),('config-symlink','generation config exists'),
                       ('handover-exists','handover already exists')]:
        c = run(mode, mode=mode)
        expect(c, 1)
        assert text in c[4] and 'ROOT=42 GEN=7' in c[4]
        assert not (c[0] / 'cdl').exists()
        state = c[1] / '.local/state/herdr-orch/lead-push/42'
        assert (state / 'start.lock').is_dir()
        if mode == 'config-exists': assert (state / 'taskr-mcp-7.json').read_text() == 'keep'
        if mode == 'handover-exists': assert (state / 'handover-7.md').read_text() == 'keep'
        if mode == 'config-symlink': assert (state / 'taskr-mcp-7.json').is_symlink()

    for mode, gate in [('fail-launch',True), ('fail-ready',True), ('queued-ready',True),
                       ('wrong-delivery',True),('unreceived',True),('unacked',True),('prover-conflict',True),
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
        closes = [a for a,t,l in c[5] if a[0] == 'close']
        expected = ([['close','43','--outcome','rejected']] if gate else [])
        if mode == 'fail-accepted-close': expected.insert(0, ['close','43','--outcome','accepted'])
        assert closes == expected, closes
        assert (state / 'start.lock').is_dir()

    for mode, rc, output in [('check-live',0,'live'),('check-stale',1,'stale'),('',1,'stale'),
                             ('unknown',2,'unknown'),('fail-log',2,'unknown'),('fail-consumer',2,'unknown')]:
        c = run('check '+mode, args=('check','42'), mode=mode)
        expect(c, rc)
        assert c[3].strip() == output
        assert not (c[1] / '.local/state').exists()

print(f'PASS: {count} herdr-hub cases')
PY
