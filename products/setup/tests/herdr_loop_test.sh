#!/bin/sh
# Real shell loop/launcher, scratch HOME and fake control plane; no live calls.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
exec python3 - "$repo_dir" "${LOOP_SH:-sh}" <<'PY'
import json, os, select, shutil, signal, subprocess, sys, tempfile
from pathlib import Path

repo = Path(sys.argv[1])
helper = repo / 'products/setup/bin/herdr-loop'
shell = shutil.which(sys.argv[2])
with tempfile.TemporaryDirectory(prefix='herdr-loop-test.') as scratch:
    base = Path(scratch)
    stubs = base / 'bin'
    stubs.mkdir()
    if sys.argv[2] == 'bashposix':
        wrapper = stubs / 'bashposix'
        wrapper.write_text('#!/bin/sh\nexec bash --posix "$@"\n')
        wrapper.chmod(0o755)
        shell = str(wrapper)
    stub = r'''#!/usr/bin/env python3
import json, os, signal, sys
from pathlib import Path
work = Path(os.environ['LOOP_TEST'])
kind = Path(sys.argv[0]).name
plan = json.loads((work / 'plan').read_text())
def read(name, default):
    p = work / name
    return p.read_text() if p.exists() else default
def log(name, value):
    with (work / name).open('a') as f: f.write(json.dumps(value) + '\n')
def hold(phase):
    if os.environ.get('LOOP_HOLD') == phase:
        (work / 'held-pid').write_text(str(os.getpid()))
        os.write(int(os.environ['LOOP_NOTIFY_FD']), (phase + '\n').encode())
        os.read(int(os.environ['LOOP_RELEASE_FD']), 1)
def launched(brief, report, name):
    text = Path(brief).read_text()
    assert text.startswith('HERDR-BRIEF role=researcher\n')
    assert 'report=' + report in text and text.count('report=') == 1
    assert 'orchestrator=lead (root 42, watcher)' in text
    assert 'fresh handler tab' in text
    for header in ('Objective:', 'Acceptance:', 'Validation:'): assert header in text
    items = Path(brief).parent / 'items'
    assert str(items) in text
    log('launches', {'name': name, 'items': items.read_text(), 'tick': int(read('tick', '0'))})
    n = int(read('launch-count', '0')) + 1
    (work / 'launch-count').write_text(str(n))
    return 100 + n
step = json.loads(read('current', '{}'))
if kind == 'loop-check':
    n = int(read('tick', '0'))
    step = plan[min(n, len(plan) - 1)]
    (work / 'tick').write_text(str(n + 1))
    (work / 'current').write_text(json.dumps(step))
    log('checks', n)
    hold('check')
    sys.stdout.write(step.get('out', ''))
    sys.exit(step.get('rc', 0))
elif kind == 'sleep':
    log('sleeps', int(sys.argv[1]))
    (work / 'sleep-pid').write_text(str(os.getpid()))
    hold('sleep')
    if int(read('tick', '0')) >= len(plan): os.kill(os.getppid(), signal.SIGTERM)
elif kind == 'herdr-launch':
    if (work / 'real-launch').exists():
        os.execv(os.environ['LOOP_SHELL'], [os.environ['LOOP_SHELL'], os.environ['LOOP_LAUNCH'], *sys.argv[1:]])
    if sys.argv[1:] == ['--lanes']:
        print('Lane table: synthetic')
        if not (work / 'no-lane').exists(): print('sonnet-medium\tclaude\tclaude-sonnet-5-5\tmedium\trs')
        sys.exit(0)
    assert sys.argv[1] == 'sonnet-medium' and sys.argv[2].startswith('rs-watch-') and sys.argv[5] == '42'
    assert 'TASKR_TASK' not in os.environ and 'TASKR_LAUNCH' not in os.environ
    task = launched(sys.argv[3], sys.argv[4], sys.argv[2])
    if step.get('launch_rc'): sys.exit(step['launch_rc'])
    print('task=%s launch=1 tab=wA:t1 pane=wA:p1' % task)
elif kind == 'taskr':
    assert 'TASKR_TASK' not in os.environ and 'TASKR_LAUNCH' not in os.environ
    log('taskr-calls', sys.argv[1:])
    if sys.argv[1] == 'new':
        task = launched(sys.argv[sys.argv.index('--brief') + 1], sys.argv[sys.argv.index('--report') + 1], sys.argv[2])
        assert sys.argv[sys.argv.index('--parent') + 1] == '42'
        print('n1 %s' % task)
    elif sys.argv[1] == 'launch': print('l1 {"launch_id":1}')
    elif sys.argv[1] == 'prompt':
        if '--file' in sys.argv:
            log('handler-prompts', sys.argv[1:])
            print('p1 1')
        else:
            assert sys.argv[2:4] == ['42', '--text']
            log('notices', sys.argv[4])
            sys.exit(step.get('notice_rc', 0))
    else:
        assert sys.argv[1:4] == ['--json', 'status', '--tree'] and sys.argv[5] == '--all'
        if step.get('status_rc'): sys.exit(step['status_rc'])
        if not step.get('missing_task'):
            print(json.dumps({'id': int(sys.argv[4]), 'status': step.get('status', 'open')}))
        print(json.dumps({'record': 'daemon', 'daemon': 'none'}))
elif kind == 'herdr':
    assert (work / 'real-launch').exists(), 'only real herdr-launch may call herdr'
    log('herdr-calls', sys.argv[1:])
    command = sys.argv[1:3]
    if command == ['pane', 'current']: print('{"workspace_id":"wA"}')
    elif command == ['tab', 'create']: print('{"tab_id":"wA:t1","pane_id":"wA:p1"}')
    elif command == ['agent', 'start']:
        hold('launch')
        sys.exit(step.get('agent_rc', 0))
    elif command not in (['pane', 'run'], ['pane', 'wait-output']): raise AssertionError(sys.argv)
else: raise AssertionError(kind)
'''
    for name in ('loop-check', 'sleep', 'herdr-launch', 'taskr', 'herdr'):
        p = stubs / name
        p.write_text(stub)
        p.chmod(0o755)

    count = 0
    def case(plan):
        global count
        count += 1
        work = base / str(count)
        work.mkdir()
        (work / 'plan').write_text(json.dumps(plan))
        (work / 'brief').write_text('HERDR-BRIEF role=researcher\norchestrator=lead (root 42, watcher) report=/old/report.md\nTask topology: watcher, not handler\nObjective: Digest the items.\nAcceptance: Concise source-grounded report.\nValidation: Verify sources.\n')
        return work
    def state(w): return w / 'home/.local/state/herdr-loop/watch'
    def records(w, name):
        p = w / name
        return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []
    def items(w): return [x['items'] for x in records(w, 'launches')]
    def env(w):
        return dict(os.environ, HOME=str(w / 'home'), HERDR_ENV='1', LOOP_TEST=str(w),
                    PATH=str(stubs) + os.pathsep + os.environ['PATH'], TASKR_TASK='999', TASKR_LAUNCH='888',
                    LOOP_SHELL=shell, LOOP_LAUNCH=str(repo / 'products/setup/bin/herdr-launch'))
    def args(w): return [shell, str(helper), '--every', '1', 'watch', 'exec loop-check', str(w / 'brief'), '42']
    def reap(p):
        # Kill the owned test process group too: broken reap guards must fail, not hang.
        try: os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError: pass
        p.communicate(timeout=3)
    def absent(pid):
        try: os.kill(pid, 0)
        except ProcessLookupError: return
        raise AssertionError('child survived: %s' % pid)
    def run(w, rc=0, extra=None, keep_lock=False):
        p = subprocess.Popen(args(w) if extra is None else [shell, str(helper)] + extra,
                             env=env(w), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             text=True, start_new_session=True)
        try:
            out, err = p.communicate(timeout=10)
            assert p.returncode == rc, (p.returncode, out, err)
            if not keep_lock: assert not (state(w) / 'lock').exists()
            if (w / 'sleep-pid').exists(): absent(int((w / 'sleep-pid').read_text()))
        finally: reap(p)
        return out, err

    w = case([{}, {}, {}]); run(w)
    assert items(w) == [] and records(w, 'sleeps') == [60, 60, 60]
    w = case([{'out': 'release v1\n'}, {'out': 'release v1\n', 'status': 'closed'}, {}, {'out': 'release v1\n'}])
    run(w)
    (w / 'tick').write_text('0'); run(w)
    assert items(w) == ['release v1\n']

    w = case([{'out': 'A'}, {'out': 'B', 'status': 'ready'}, {'out': 'C', 'status': 'done'},
              {'out': 'D', 'status': 'failed'}, {'out': 'D', 'status': 'closed'},
              {'out': 'D', 'status': 'closed'}, {'out': 'D', 'status': 'closed'}])
    run(w)
    assert items(w) == ['A', 'B', 'C', 'D']
    assert [x['tick'] for x in records(w, 'launches')] == [1, 5, 6, 7]
    assert not list((state(w) / 'queue').iterdir())

    w = case([{'out': 'A'}, {'out': 'B'}]); run(w)
    assert items(w) == ['A'] and [p.read_text() for p in (state(w) / 'queue').iterdir()] == ['B']
    (w / 'tick').write_text('0')
    (w / 'plan').write_text(json.dumps([{'out': 'B', 'status': 'closed'}])); run(w)
    assert items(w) == ['A', 'B']

    w = case([{'rc': 1, 'out': 'partial'}] * 5 + [{}] + [{'rc': 1}] * 3); run(w)
    assert items(w) == [] and records(w, 'sleeps') == [120, 240, 480, 480, 480, 60, 120, 240, 480]
    assert len(records(w, 'notices')) == 2
    w = case([{'rc': 1}] * 2 + [{'rc': 1, 'notice_rc': 5}] + [{'rc': 1}] * 2); run(w)
    assert len(records(w, 'notices')) == 2 and len(records(w, 'checks')) == 5
    assert records(w, 'sleeps') == [120, 240, 480, 480, 480]
    w = case([{'rc': 1}] * 2 + [{'rc': 1, 'notice_rc': 6}] + [{}]); run(w, 6)
    assert len(records(w, 'checks')) == 3

    w = case([{}])
    payload = '$(touch "' + str(w / 'executed') + '"); `touch "' + str(w / 'executed2') + '"`\n\'"\\ $HOME * ; & | < >\n'
    (w / 'plan').write_text(json.dumps([{'out': payload}]))
    run(w)
    assert items(w) == [payload] and not (w / 'executed').exists() and not (w / 'executed2').exists()
    assert ((state(w) / 'runs/000000000001/items').stat().st_mode & 0o777) == 0o600

    for failure in ({'status_rc': 5}, {'missing_task': True}):
        w = case([{'out': 'A'}, dict(out='B', **failure), dict(out='B', **failure), {'out': 'B', 'status': 'closed'}]); run(w)
        assert items(w) == ['A', 'B'] and records(w, 'sleeps') == [60, 120, 240, 60]
        # Status failures notify independently, retry a failed notice, and reset on a good read.
        bad = dict(out='B', **failure)
        plan = ([{'out': 'A'}] + [bad] * 2 + [dict(bad, notice_rc=5)] + [bad] * 5
                + [{'out': 'B', 'status': 'open'}] + [bad] * 3 + [{'out': 'B', 'status': 'closed'}])
        w = case(plan); run(w)
        assert items(w) == ['A', 'B'] and len(records(w, 'checks')) == len(plan)
        assert len(records(w, 'notices')) == 3  # failed attempt, first streak, second streak
        assert all('status-read failures' in x for x in records(w, 'notices'))
        assert records(w, 'sleeps') == [60, 120, 240] + [480] * 6 + [60, 120, 240, 480, 60]
    w = case([{'out': 'A'}, {'out': 'B', 'status_rc': 6}, {}]); run(w, 6)
    assert items(w) == ['A'] and len(records(w, 'checks')) == 2

    w = case([{'out': 'A', 'launch_rc': 1}]); run(w, 1)
    assert (state(w) / 'starting').exists() and len(records(w, 'notices')) == 1
    run(w, 2)
    assert len(items(w)) == 1
    w = case([{'out': 'A', 'launch_rc': 2}]); run(w, 2)
    assert not (state(w) / 'starting').exists()
    assert [p.read_text() for p in (state(w) / 'queue').iterdir()] == ['A']
    (w / 'tick').write_text('0'); (w / 'plan').write_text(json.dumps([{'out': 'A'}])); run(w)
    assert (state(w) / 'active').read_text().strip() == '102'

    for header in ('Task topology:', 'Objective:', 'Acceptance:', 'Validation:'):
        w = case([{'out': 'A'}]); (w / 'real-launch').touch()
        (w / 'brief').write_text('\n'.join(x for x in (w / 'brief').read_text().splitlines() if not x.startswith(header)) + '\n')
        run(w, 2)
        assert not records(w, 'checks') and not records(w, 'herdr-calls') and len(records(w, 'notices')) == 1
    w = case([{}]); (w / 'brief').write_text((w / 'brief').read_text().replace('role=researcher', 'role=researcherX'))
    run(w, 2); assert not records(w, 'checks')
    w = case([{}]); (w / 'real-launch').touch()
    table = json.loads((repo / 'products/setup/lanes.json').read_text()); del table['sonnet-medium']
    dest = w / 'home/.config/herdr-fleet/lanes.json'; dest.parent.mkdir(parents=True); dest.write_text(json.dumps(table))
    run(w, 2); assert not records(w, 'checks') and len(records(w, 'notices')) == 1

    # Real launcher accepts the generated report, role, topology and lane fields.
    w = case([{'out': 'A'}]); (w / 'real-launch').touch(); run(w)
    assert items(w) == ['A'] and len(records(w, 'handler-prompts')) == 1
    assert (state(w) / 'active').read_text().strip() == '101'

    w = case([{'out': 'A'}]); state(w).mkdir(parents=True); (state(w) / 'sequence').write_text('5'); run(w)
    assert records(w, 'launches')[0]['name'] == 'rs-watch-6'
    w = case([{}]); lock = state(w) / 'lock'; lock.mkdir(parents=True)
    dead = subprocess.Popen([shell, '-c', 'exit 0']); dead.wait(timeout=3)
    (lock / 'pid').write_text(str(dead.pid)); run(w)
    w = case([{}]); lock = state(w) / 'lock'; lock.mkdir(parents=True); (lock / 'pid').write_text(str(os.getpid()))
    out, err = run(w, 2, keep_lock=True)
    assert (lock / 'pid').read_text() == str(os.getpid())
    assert 'refused to start: already running (PID %s)' % os.getpid() in err
    assert not records(w, 'notices')

    w = case([{}])
    for bad in ('0', '-1', '1.5', '999999999999999999999'):
        run(w, 2, ['--every', bad, 'watch', 'exec loop-check', str(w / 'brief'), '42'])
    run(w, 2, ['../escape', 'exec loop-check', str(w / 'brief'), '42'])

    # Held CHECK has a real grandchild; held launch uses real herdr-launch step 6.
    for phase, agent_rc in (('sleep', 0), ('check', 0), ('launch', 0), ('launch', 1)):
        w = case([{'out': 'A' if phase != 'sleep' else '', 'agent_rc': agent_rc}])
        if phase == 'launch': (w / 'real-launch').touch()
        notify_r, notify_w = os.pipe(); release_r, release_w = os.pipe()
        e = dict(env(w), LOOP_HOLD=phase, LOOP_NOTIFY_FD=str(notify_w), LOOP_RELEASE_FD=str(release_r))
        command = args(w)
        if phase == 'check': command[5] = 'loop-check; exec printf tail'
        p = subprocess.Popen(command, env=e, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             text=True, start_new_session=True, pass_fds=(notify_w, release_r))
        os.close(notify_w); os.close(release_r)
        try:
            assert select.select([notify_r], [], [], 5)[0], 'child did not reach ' + phase
            assert os.read(notify_r, 64) == (phase + '\n').encode()
            p.terminate()
            if phase != 'sleep':
                try: p.wait(timeout=0.2)
                except subprocess.TimeoutExpired: pass
                else: raise AssertionError('TERM interrupted ' + phase)
                os.write(release_w, b'x')
            out, err = p.communicate(timeout=3)
            assert p.returncode == agent_rc, (phase, out, err)
            assert not (state(w) / 'lock').exists()
            absent(int((w / 'held-pid').read_text()))
            if phase == 'check':
                assert items(w) == [] and [x.read_text() for x in (state(w) / 'queue').iterdir()] == ['Atail']
            if agent_rc:
                assert (state(w) / 'starting').exists() and not (state(w) / 'active').exists()
                assert len(records(w, 'notices')) == 1 and 'stopped (exit 1)' in records(w, 'notices')[0]
                assert not records(w, 'handler-prompts')
            else:
                assert not records(w, 'notices')
            if phase == 'launch' and not agent_rc:
                assert (state(w) / 'active').read_text().strip() == '101'
                assert not (state(w) / 'starting').exists() and len(records(w, 'handler-prompts')) == 1
        finally:
            # Bound communicate even if a broken watcher leaves a pipe-holding child.
            held = w / 'held-pid'
            if held.exists():
                try: os.kill(int(held.read_text()), signal.SIGKILL)
                except ProcessLookupError: pass
            reap(p)
            os.close(notify_r); os.close(release_w)

    print('herdr-loop: acceptance cases passed (%s; fake control plane, real launcher)' % sys.argv[2])
PY
