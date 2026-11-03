#!/usr/bin/env python3
"""The hub board over HTTP: synthetic ledger, HOME and tailnet only (the contract hub
fixture treats [::1] as the tailnet and 127.0.0.1 as loopback)."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import http.client
import json
from pathlib import Path
import select
import sqlite3
import subprocess
import tempfile

FAKE_TS = '''#!/bin/sh
# whois answers by the mode file: admitted (default), another login, or a tagged node.
mode=$(cat "$(dirname "$0")/mode" 2>/dev/null)
login=owner@example.com; tags='[]'
case "$mode" in user) login=other@example.com ;; tag) tags='["tag:ci"]' ;; esac
case "$1" in
whois) printf '{"Node":{"StableID":"host-a-node","Name":"host-a.example.ts.net.","Tags":%s},"UserProfile":{"LoginName":"%s"}}\\n' "$tags" "$login" ;;
*) exit 2 ;;
esac
'''


class Hub:
    def __init__(self, binary, tmp, node='hub-node'):
        self.binary, self.tmp = binary, Path(tmp)
        self.bin = self.tmp / 'bin'
        self.bin.mkdir(parents=True, exist_ok=True)
        (self.bin / 'tailscale').write_text(FAKE_TS)
        (self.bin / 'tailscale').chmod(0o755)
        home = self.tmp / 'home'
        (home / '.local/state/taskr').mkdir(parents=True, exist_ok=True)
        self.db = self.tmp / 'hub.db'
        self.env = {'PATH': f'{self.bin}:/usr/bin:/bin', 'LANG': 'C.UTF-8', 'TZ': 'UTC', 'HOME': str(home),
                    'TASKR_DB': str(self.db), 'TASKR_CONTRACT_TAILNET': '1', 'TASKR_CONTRACT_ORACLE': '1',
                    'HERDR_SOCKET_PATH': str(self.tmp / 'absent.sock')}
        self.log = open(self.tmp / f'hub-{node}.log', 'wb')
        self.proc = subprocess.Popen([str(binary), '--contract-hub', node], env=self.env,
                                     stdout=subprocess.PIPE, stderr=self.log)
        assert select.select([self.proc.stdout], [], [], 15)[0], 'hub fixture did not announce its listeners'
        self.tailnet = self.proc.stdout.readline().decode().strip()
        self.loopback = self.proc.stdout.readline().decode().strip()
        assert self.tailnet.startswith('http://[::1]:') and self.loopback.startswith('http://127.0.0.1:'), (self.tailnet, self.loopback)

    def mode(self, mode):
        (self.bin / 'mode').write_text(mode)

    def taskr(self, *args):
        p = subprocess.run([str(self.binary), '--json', *args], env=self.env, capture_output=True, timeout=20)
        assert p.returncode == 0, (args, p.stdout, p.stderr)
        return json.loads(p.stdout)

    def close(self):
        self.proc.terminate()
        self.proc.wait(timeout=5)
        self.proc.stdout.close()
        self.log.close()


def request(url, path, method='GET', headers=None, body=None):
    host, port = url.removeprefix('http://').rsplit(':', 1)
    c = http.client.HTTPConnection(host.strip('[]'), int(port), timeout=20)
    try:
        c.request(method, path, body=body, headers=headers or {})
        r = c.getresponse()
        return r.status, {k.lower(): v for k, v in r.getheaders()}, r.read()
    finally:
        c.close()


def seed(hub):
    root = hub.taskr('new', 'board-root', '--role', 'orchestrator')['task_id']
    lane = hub.taskr('new', 'board-lane', '--role', 'implementer', '--parent', str(root))['task_id']
    hub.taskr('set', str(lane), 'work=1', 'pr=41')
    with sqlite3.connect(hub.db) as db:
        for w, title, state in ((1, 'A1 board live', 'backlog'), (2, 'A2 shipped', 'done')):
            db.execute("insert into work_items(id,root_id,title,priority,revision,state,creator,created_at) values(?,?,?,0,1,?,'{}','2026-11-03T18:00:00.000Z')", (w, root, title, state))
            db.execute("insert into work_revisions(work_id,revision,contract,creator,created_at) values(?,1,?,'{}','2026-11-03T18:00:00.000Z')", (w, json.dumps({'intent': 'Ship it. parked: owner pick 1.'})))
    hub.taskr('ask', 'approve W1 today', '--owner', '--as', str(root))
    hub.taskr('note', 'ALARM 18:30Z STUCK #41 in queue', '--as', str(root))
    return root


def run(binary):
    results = {}
    with tempfile.TemporaryDirectory(prefix='taskr-board-') as tmp:
        hub = Hub(binary, Path(tmp) / 'a')
        try:
            seed(hub)
            # Pages: GET only, typed, with the strict CSP and the hub's headers.
            for path, kind in (('/board', 'text/html'), ('/board.js', 'text/javascript'), ('/board.css', 'text/css')):
                status, headers, body = request(hub.tailnet, path)
                assert status == 200 and headers['content-type'].startswith(kind), (path, status, headers)
                assert "default-src 'none'" in headers['content-security-policy'] and "script-src 'self'" in headers['content-security-policy']
                assert headers['cache-control'] == 'no-store' and headers['x-frame-options'] == 'DENY', headers
            assert b'/board.js' in request(hub.tailnet, '/board')[2]
            assert b'innerHTML' not in request(hub.tailnet, '/board.js')[2], 'the page renders text only'
            # Concurrent first loads share one child read.
            with ThreadPoolExecutor(max_workers=6) as pool:
                loads = list(pool.map(lambda _: request(hub.tailnet, '/api/board'), range(6)))
            assert {s for s, _, _ in loads} == {200}, [(s, b) for s, _, b in loads]
            assert len({b for _, _, b in loads}) == 1, 'one shared result'
            board = json.loads(loads[0][2])
            tickets = {t['w']: t for t in board['tickets']}
            assert (tickets[1]['column'], tickets[1]['pr'], tickets[1]['pr_state']) == ('in_progress', 41, ''), tickets[1]
            assert tickets[2]['column'] == 'done' and tickets[1]['asks'], tickets
            assert board['groups'][0]['parked'] == 'owner pick 1', board['groups']
            assert [a['event'] for a in board['alarms']] == ['STUCK'] and board['asks'][0]['text'] == 'approve W1 today', board
            # Within 15 s a ledger change is not read again.
            with sqlite3.connect(hub.db) as db:
                db.execute("update work_items set state='done' where id=1")
            again = request(hub.tailnet, '/api/board')
            assert again[0] == 200 and again[2] == loads[0][2], 'cached for 15 s'
            results['pages_and_cached_board'] = True
            # Methods, loopback, other logins and tags, Host, and RPC's browser refusal.
            status, headers, _ = request(hub.tailnet, '/board', 'POST', body=b'x')
            assert (status, headers.get('allow')) == (405, 'GET'), (status, headers)
            assert request(hub.tailnet, '/api/board', 'HEAD')[0] == 405
            for path in ('/board', '/board.js', '/board.css', '/api/board'):
                status, _, body = request(hub.loopback, path)
                assert status == 403 and b'tailnet nodes only' in body, (path, status, body)
            for mode in ('user', 'tag'):
                hub.mode(mode)
                status, _, body = request(hub.tailnet, '/api/board')
                assert status == 403 and b'not admitted' in body, (mode, status, body)
            hub.mode('')
            assert request(hub.tailnet, '/board', headers={'Host': 'evil.example:80'})[0] == 421
            assert request(hub.tailnet, '/boards')[0] == 404
            rpc = request(hub.tailnet, '/api/rpc', 'POST', {'Origin': hub.tailnet, 'Content-Type': 'application/json', 'X-Taskr-RPC': '1'}, b'{}')
            assert rpc[0] == 403 and b'browser requests are refused' in rpc[2], rpc
            results['admission'] = True
        finally:
            hub.close()
        # The server's own tailnet node is refused, as /api/rpc refuses it.
        own = Hub(binary, Path(tmp) / 'b', node='host-a-node')
        try:
            for path in ('/board', '/api/board'):
                status, _, body = request(own.tailnet, path)
                assert status == 403 and b'this node is the server' in body, (path, status, body)
            results['own_node_refused'] = True
        finally:
            own.close()
    return results


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--rust', type=Path, required=True)
    args = p.parse_args()
    print(json.dumps(run(args.rust.resolve())))
