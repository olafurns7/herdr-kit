#!/bin/sh
# herdr-hub and herdr-lead start forms under HERDR_KIT_BACKEND=hc, run to their end
# through the real herdr0 over tests/fake_hc. No real hc, core, Herdr, taskr or Claude runs.
set -eu
setup=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
exec python3 -B - "$setup" <<'PY'
import json, os, re, shutil, socket, subprocess, sys, tempfile, threading
from pathlib import Path

setup = Path(sys.argv[1])
scratch = Path(tempfile.mkdtemp(prefix='hc-start-test.'))
# The agent temp folder must stay within 24 bytes: /tmp/h.XXX/agent-UID.
def short_tmp():
    import secrets
    for _ in range(100):
        path = Path('/tmp/h.' + secrets.token_hex(2)[:3])
        try:
            path.mkdir(mode=0o700)
            return path
        except FileExistsError:
            continue
    raise SystemExit('no short temp folder')
agent_tmp_base = short_tmp()

def executable(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(0o755)

# A live core only has to accept connections: herdr0 checks the socket before each hc call.
core_path = scratch / 'core.sock'
core = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
core.bind(str(core_path))
core.listen(64)
def serve():
    while True:
        try:
            conn, _ = core.accept()
        except OSError:
            return
        conn.close()
threading.Thread(target=serve, daemon=True).start()

stubs = scratch / 'stubs'
# fake_hc keeps sent text on the screen; this wrapper also runs text sent to a terminal
# at its shell (no on_send rule) in one sh per terminal, replaying earlier lines, and
# appends the output, so markers and env checks print as in a real shell.
executable(stubs / 'hc-shell', '''#!/usr/bin/env python3
import json, os, subprocess, sys
state = os.environ["FAKE_HC_STATE"]
argv = sys.argv[1:]
p = subprocess.run([os.environ["FAKE_HC"], *argv], capture_output=True, text=True)
sys.stdout.write(p.stdout)
sys.stderr.write(p.stderr)
if p.returncode == 0 and argv[0] == "send" and "--wait" not in argv and "--" in argv:
    i = argv.index("--")
    addr, text = [a for a in argv[1:i] if not a.startswith("-")][0], argv[i + 1]
    with open(state + "/state.json") as f:
        s = json.load(f)
    rec = s["terminals"][addr]
    if not any(r["match"] in text for r in s.get("on_send", [])) and (rec.get("foreground") or {}).get("name") == "zsh":
        lines = os.path.join(state, "sh-" + addr)
        prev = open(lines).read() if os.path.exists(lines) else ""
        r = subprocess.run(["sh", "-c", prev + "printf '%s\\\\n' __hc_shell__\\n" + text + "\\n"],
                           capture_output=True, text=True, cwd=rec["cwd"])
        with open(lines, "a") as f:
            f.write(text + "\\n")
        rec["screen"] += r.stdout.split("__hc_shell__\\n", 1)[-1]
        with open(state + "/state.json.tmp", "w") as f:
            json.dump(s, f)
        os.replace(state + "/state.json.tmp", state + "/state.json")
sys.exit(p.returncode)
''')
executable(stubs / 'taskr', r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
case = Path(os.environ["CASE"])
argv = sys.argv[1:]
if argv[:1] == ["--json"]:
    argv = argv[1:]
with open(case / "taskr-calls", "a") as f:
    f.write(json.dumps(argv) + "\n")
cmd = argv[0]
def live():
    sent = Path(os.environ["FAKE_HC_STATE"], "calls.jsonl")
    return (case / "cdl").exists() or (sent.exists() and "lead-push-gen-7" in sent.read_text())
if cmd == "campaign":
    print(json.dumps({"root": {"id": 42, "name": "example", "role": "orchestrator", "status": "open",
                               "pane_id": "wOLD9:p1", "host": ""}}))
elif cmd == "log":
    print(json.dumps({"record": "event", "id": 7 if (case / "adopted").exists() else 5, "kind": "adopt"}))
elif cmd == "consumer":
    print(json.dumps({"live": True, "holder": "mcp-channel-1", "session_gen": 7} if live() else {"live": False}))
elif cmd == "adopt":
    (case / "adopted").write_text(json.dumps(argv))
    print("handover text")
    print(f"taskr adopt: task 42 now at pane {argv[argv.index('--pane') + 1]} (was wOLD9:p1); event 7", file=sys.stderr)
elif cmd == "glance":
    print(json.dumps({"caller_host": ""}))
elif cmd == "new":
    print(json.dumps({"task_id": 43}))
elif cmd == "launch":
    print(json.dumps({"launch_id": 8}))
elif cmd == "ready":
    print(json.dumps({"event_id": 9}))
elif cmd == "delivery":
    print(json.dumps({"attempts": [{"event_id": 9, "session_gen": 7, "received_at": "now"}]}))
elif cmd == "inbox":
    print(json.dumps({"acked_event_id": 9, "events": []}))
elif cmd == "close":
    print(json.dumps({"task_id": 43}))
elif cmd == "note":
    (case / "note").write_text(json.dumps(argv))
else:
    sys.exit(1)
''')
# Deadlines advance 30 s per read, so a failure ends fast; the happy path never sleeps.
executable(stubs / 'date', '#!/bin/sh\nn=0\n[ ! -f "$CASE/clock" ] || n=$(cat "$CASE/clock")\nn=$((n + 30))\nprintf "%s\\n" "$n" >"$CASE/clock"\nprintf "%s\\n" "$n"\n')
executable(stubs / 'sleep', '#!/bin/sh\nexit 0\n')
cdl = '''#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
case = Path(os.environ["CASE"])
(case / "cdl").write_text(json.dumps(sys.argv[1:]))
for _ in range(2000):
    if (case / "note").exists():
        break
    time.sleep(0.01)
'''
count = 0

def new_case(name):
    global count
    count += 1
    print(f'case {count}: {name}', flush=True)
    case = scratch / str(count)
    home = case / 'home'
    bindir = home / '.local/bin'
    bindir.mkdir(parents=True)
    for f in ('herdr-hub', 'herdr-lead', 'herdr-lanes.sh'):
        shutil.copy2(setup / 'bin' / f, bindir)
    executable(bindir / 'cdl', cdl)
    executable(bindir / 'codator', '#!/bin/sh\nexit 0\n')
    shim = home / '.local/lib/herdr-kit/hc-bin'
    shim.mkdir(parents=True)
    (shim / 'herdr').symlink_to(setup / 'bin/herdr0')
    (home / '.config/herdr-fleet').mkdir(parents=True)
    (home / '.config/herdr-fleet/lanes.json').write_text(json.dumps(
        {'opus-high': {'kind': 'claude', 'model': 'example-opus', 'effort': 'high', 'prefixes': ['orch', 'hub']}}))
    state = case / 'fake'
    state.mkdir()
    work = case / 'work'
    work.mkdir()
    env = dict(HOME=str(home), PATH=f'{shim}:{stubs}:/usr/bin:/bin', CASE=str(case), SHELL='/bin/sh',
               HERDR_KIT_BACKEND='hc', HC_SOCKET=str(core_path), HC_BIN=str(stubs / 'hc-shell'),
               FAKE_HC=str(setup / 'tests/fake_hc'), FAKE_HC_STATE=str(state),
               XDG_STATE_HOME=str(home / '.local/state'), XDG_CONFIG_HOME=str(home / '.config'),
               HERDR_TEST_AGENT_TMP_BASE=str(agent_tmp_base), TMPDIR=str(case))
    return case, home, work, state, env

def herdr(env, *args):
    p = subprocess.run(['herdr', *args], env=env, capture_output=True, text=True, timeout=60)
    assert p.returncode == 0, (args, p.stdout, p.stderr)
    return json.loads(p.stdout)['result']

def hub_terminal(env, work):
    """The owner's root terminal: herdr0 enter labels it as a workspace's first pane."""
    made = herdr(env, 'workspace', 'create', '--label', 'hub', '--cwd', str(work))['root_pane']
    return made

def in_pane(env, p):
    return dict(env, HERDR_ENV='1', HERDR_PANE_ID=p['pane_id'], HERDR_TAB_ID=p['tab_id'],
                HERDR_WORKSPACE_ID=p['workspace_id'], HC_TERMINAL_ID=p['terminal_id'])

def fake_state(state):
    return json.loads((state / 'state.json').read_text())

def on_send(state, rules):
    s = fake_state(state)
    s['on_send'] = rules
    (state / 'state.json').write_text(json.dumps(s))

def sent(state, addr):
    return [c[c.index('--') + 1] for c in map(json.loads, (state / 'calls.jsonl').read_text().splitlines())
            if c[0] == 'send' and addr in c[:c.index('--')]]

try:
    # (a) The hub comes back by typing its start form into an hc shell terminal.
    case, home, work, state, env = new_case("hub: 'herdr-hub ROOT --lane LANE --account ACCOUNT -- --resume SID' in an hc shell")
    hub = hub_terminal(env, work)
    p = subprocess.run(['sh', str(home / '.local/bin/herdr-hub'), '42', '--lane', 'opus-high', '--account', 'acct1',
                        '--', '--resume', 'sid-hub'], env=in_pane(env, hub), cwd=work, capture_output=True,
                       text=True, timeout=120)
    assert p.returncode == 0, (p.stdout, p.stderr)
    adopted = json.loads((case / 'adopted').read_text())
    assert adopted == ['adopt', '42', '--workspace', hub['workspace_id'], '--tab', hub['tab_id'],
                       '--pane', hub['pane_id']], adopted
    args = json.loads((case / 'cdl').read_text())
    assert args[:2] == ['--account', 'acct1'], args
    assert args[args.index('--') - 2:args.index('--')] == ['--resume', 'sid-hub'], args
    assert '--dangerously-load-development-channels' in args
    result = home / '.local/state/herdr-orch/lead-push/42/hub-check-7.result'
    assert result.read_text() == 'live\n', (result.read_text(), (result.parent / 'hub-check-7.log').read_text())
    # The hub's own terminal carries its agent (herdr pane get no longer says unknown) and its app space and tab.
    lb = fake_state(state)['terminals'][hub['terminal_id']]['labels']
    assert (lb.get('agent'), lb.get('agent_name'), lb.get('agent_session')) == ('claude', 'hub-42', 'sid-hub'), lb
    assert re.fullmatch(r'sp[0-9a-f]{32}', lb['herdr.space']) and re.fullmatch(r'tb[0-9a-f]{32}', lb['herdr.tab']), lb
    assert lb['where'] == 'hub / 1', lb
    labels = [c for c in map(json.loads, (state / 'calls.jsonl').read_text().splitlines()) if c[0] == 'label']
    assert labels == [['label', '--json', hub['terminal_id'], 'agent=claude', 'agent_name=hub-42',
                       'agent_session=sid-hub']], labels

    # (b) A lead starts in a new hc shell terminal of the hub's workspace.
    case, home, work, state, env = new_case("lead: 'herdr-lead start ROOT PANE --worktree DIR --lane LANE --account ACCOUNT --resume SID'")
    hub = hub_terminal(env, work)
    lead = herdr(in_pane(env, hub), 'tab', 'create', '--workspace', hub['workspace_id'], '--cwd', str(work),
                 '--label', 'lead')['root_pane']
    on_send(state, [{'match': '--dangerously-load-development-channels', 'status': 'idle', 'foreground': 'claude',
                     'screen': 'claude is ready\n'},
                    {'match': 'lead-push-gen-7', 'status': 'working', 'turn': 1}])
    p = subprocess.run(['sh', str(home / '.local/bin/herdr-lead'), 'start', '42', lead['pane_id'], '--worktree', str(work),
                        '--lane', 'opus-high', '--account', 'acct1', '--resume', 'sid-lead'],
                       env=in_pane(env, hub), cwd=work, capture_output=True, text=True, timeout=120)
    assert p.returncode == 0, (p.stdout, p.stderr)
    assert 'herdr-lead: 42 live at 7 (event 9 received and acked)' in p.stdout, p.stdout
    adopted = json.loads((case / 'adopted').read_text())
    assert adopted == ['adopt', '42', '--workspace', lead['workspace_id'], '--tab', lead['tab_id'],
                       '--pane', lead['pane_id']], adopted
    typed = sent(state, lead['terminal_id'])
    assert any('herdr_lead_ok=' in t for t in typed), typed
    start = [t for t in typed if '--dangerously-load-development-channels' in t]
    assert len(start) == 1 and start[0].startswith('claude --account acct1 --resume sid-lead --model example-opus'), start
    assert any('lead-push-gen-7' in t for t in typed), typed
    t = fake_state(state)['terminals'][lead['terminal_id']]
    assert t['labels'].get('agent') == 'claude' and t['labels'].get('agent_name') == 'orch-example', t['labels']
    # The lead's tab is a second app tab in the hub's app space.
    hub_labels = fake_state(state)['terminals'][hub['terminal_id']]['labels']
    assert t['labels']['herdr.space'] == hub_labels['herdr.space'], (t['labels'], hub_labels)
    assert t['labels']['herdr.tab'] != hub_labels['herdr.tab'] and t['labels']['where'] == 'hub / lead', t['labels']
    assert 'herdr-lead-env=//worker/1/' in t['screen'], t['screen']

    # An rc file that puts ~/.local/bin first makes herdr Herdr 0.9: both refuse before any call.
    for which, argv in (('herdr-hub', ['42', '--lane', 'opus-high', '--account', 'acct1', '--', '--resume', 'sid-hub']),
                        ('herdr-lead', ['start', '42', 'wA:p2', '--worktree', '/', '--lane', 'opus-high',
                                        '--account', 'acct1', '--resume', 'sid-lead'])):
        case, home, work, state, env = new_case(f'{which} refuses when herdr is not the hc shim')
        executable(home / '.local/bin/herdr', '#!/bin/sh\necho herdr-0.9 "$@" >>"$CASE/herdr09"\n')
        env = dict(env, PATH=f"{home / '.local/bin'}:{env['PATH']}")
        located = dict(env, HERDR_ENV='1', HERDR_PANE_ID='wA:p1', HERDR_TAB_ID='wA:t1', HERDR_WORKSPACE_ID='wA')
        p = subprocess.run(['sh', str(home / '.local/bin' / which), *argv], env=located, cwd=work,
                           capture_output=True, text=True, timeout=60)
        assert p.returncode == 2, (p.returncode, p.stdout, p.stderr)
        want = (f"{which}: HERDR_KIT_BACKEND=hc but herdr is {home / '.local/bin/herdr'}, "
                f"not {home / '.local/lib/herdr-kit/hc-bin/herdr'}: put {home / '.local/lib/herdr-kit/hc-bin'} first on PATH")
        assert want in p.stderr, p.stderr
        for made in ('herdr09', 'taskr-calls', 'fake/calls.jsonl'):
            assert not (case / made).exists(), made
finally:
    core.close()
    shutil.rmtree(scratch, ignore_errors=True)
    shutil.rmtree(agent_tmp_base, ignore_errors=True)

print(f'PASS: {count} hc start cases')
PY
