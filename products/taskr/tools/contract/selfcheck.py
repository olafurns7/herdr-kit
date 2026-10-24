#!/usr/bin/env python3
"""Check fixture cloning and logical row/FTS comparison without any taskr binary."""
from pathlib import Path
import argparse
import contextlib
import io
import sqlite3
import sys
from unittest.mock import patch
import tempfile
import run as contract
import golden

with tempfile.TemporaryDirectory(prefix='taskr-contract-check-') as scratch:
    directory = Path(scratch)
    fixtures = contract.fixtures(directory, None)
    source = fixtures['busy']
    copy = directory / 'copy.db'
    contract.clone(source, copy)
    assert contract.logical(source) == contract.logical(copy)
    with sqlite3.connect(copy) as db:
        db.execute("update tasks set name='different' where id=1")
    assert contract.logical(source) != contract.logical(copy)
    with sqlite3.connect(copy) as db:
        db.execute('create virtual table search_fts using fts5(body)')
        db.execute("insert into search_fts(body) values ('one two three')")
    before = contract.logical(copy)
    assert 'search_fts' in before['tables']
    assert not any(name.startswith('search_fts_') for name in before['tables'])
    with sqlite3.connect(copy) as db:
        db.execute("insert into search_fts(search_fts) values('optimize')")
    assert contract.logical(copy) == before
    with sqlite3.connect(copy) as db:
        db.execute("update search_fts set body='changed' where rowid=1")
    assert contract.logical(copy) != before
    # An empty Rust-only table (schema 2) is invisible; a row in it is not.
    with sqlite3.connect(copy) as db:
        db.execute('create table subscriptions(id integer primary key, target text, fired_at text)')
        db.execute('create index subscriptions_open on subscriptions(target) where fired_at is null')
    empty = contract.logical(copy, legacy_projection=True)
    assert 'subscriptions' not in empty['tables'] and not any('subscriptions' in row[2] for row in empty['schema'])
    with sqlite3.connect(copy) as db:
        db.execute("insert into subscriptions(target) values ('7')")
    assert 'subscriptions' in contract.logical(copy, legacy_projection=True)['tables']
    # Schema 3's delivery attempts, likewise; the populated subscriptions stay visible.
    with sqlite3.connect(copy) as db:
        db.execute('create table delivery_attempts(id integer primary key, task_id integer, received_at text)')
        db.execute('create index delivery_attempts_task on delivery_attempts(task_id)')
    empty = contract.logical(copy, legacy_projection=True)
    assert 'delivery_attempts' not in empty['tables'] and not any('delivery_attempts' in row[2] for row in empty['schema'])
    assert 'subscriptions' in empty['tables']
    with sqlite3.connect(copy) as db:
        db.execute("insert into delivery_attempts(task_id) values (7)")
    assert 'delivery_attempts' in contract.logical(copy, legacy_projection=True)['tables']
    # Schema 4's consumer leases, likewise.
    with sqlite3.connect(copy) as db:
        db.execute('create table consumer_leases(task_id integer primary key, fence integer)')
    empty = contract.logical(copy, legacy_projection=True)
    assert 'consumer_leases' not in empty['tables'] and not any('consumer_leases' in row[2] for row in empty['schema'])
    assert 'delivery_attempts' in empty['tables']
    with sqlite3.connect(copy) as db:
        db.execute("insert into consumer_leases(task_id, fence) values (7, 1)")
    assert 'consumer_leases' in contract.logical(copy, legacy_projection=True)['tables']
    # v5 projection preserves the historical ledger, while full state retains every table.
    v5 = directory / 'v5.db'
    contract.clone(source, v5)
    with sqlite3.connect(v5) as db:
        for path in contract.V5_DDL:
            db.executescript(path.read_text())
        db.execute('pragma user_version=5')
    full = contract.logical(v5)
    assert set(contract.V5_TABLES) <= set(full['tables'])
    assert contract.logical(v5, legacy_projection=True, require_v5=True) == contract.logical(source, legacy_projection=True)
    # The legacy fixture itself is deliberately unmigrated; do not require v5 there.
    try:
        contract.logical(source, legacy_projection=True, require_v5=True)
    except RuntimeError:
        pass
    else:
        raise AssertionError('successful migration accepted an unmigrated fixture')
    with sqlite3.connect(v5) as db:
        db.execute('create virtual table search_fts using fts5(body)')
        db.execute("insert into search_fts(body) values ('original')")
        db.execute("insert into meta(key,value) values('v5-negative-control','original')")
    baseline = contract.logical(v5, legacy_projection=True)
    mutations = (
        "update tasks set name='v5 changed old task' where id=1",
        "update meta set value='changed' where key='v5-negative-control'",
        "update search_fts set body='changed' where rowid=1",
        "insert into work_campaigns(root_id,enabled,max_active,max_review_pending) values(1,0,1,1)",
    )
    for index, sql in enumerate(mutations):
        trial = directory / f'v5-row-{index}.db'
        contract.clone(v5, trial)
        with sqlite3.connect(trial) as db:
            db.execute(sql)
        changed = contract.logical(trial, legacy_projection=True)
        assert changed != baseline, sql
        if index == 3:
            assert changed['tables']['work_campaigns']['rows'] == 1
            assert any(row[2] == 'work_campaigns' for row in changed['schema'])
    # Missing/changed tables, missing indexes and wrong versions cannot hide behind projection.
    for index, sql in enumerate((
        'drop table work_proofs',
        'alter table work_items add column unexpected text',
        'drop index work_campaign',
        'pragma user_version=4',
    )):
        trial = directory / f'v5-schema-{index}.db'
        contract.clone(v5, trial)
        with sqlite3.connect(trial) as db:
            db.execute(sql)
        contract.logical(trial)  # full diagnostic state remains available
        try:
            contract.logical(trial, legacy_projection=True)
        except RuntimeError:
            pass
        else:
            raise AssertionError(f'v5 schema drift was projected away: {sql}')
    binary = Path('/usr/bin/python3').resolve()
    golden.session = golden.Session('selfcheck', True, directory, binary, binary)
    assert golden.normalize(str(directory / 'copy.db')) == '<tmp>/copy.db'
    assert golden.normalize('/synthetic/taskr-code/file') == '/synthetic/taskr-code/file'
    golden.session.ports.add(12345)
    assert golden.normalize('http://127.0.0.1:12345/') == 'http://127.0.0.1:<port>/'
    assert golden.normalize('http://127.0.0.1:0/') == 'http://127.0.0.1:0/'
    assert golden.normalize('http://127.0.0.1:7788/') == 'http://127.0.0.1:7788/'
    assert golden.normalize({'pid': 0, 'new_pid': True, 'daemon_pid': '0', 'lock': '0\n'}) == {'pid': 0, 'new_pid': True, 'daemon_pid': '0', 'lock': '0\n'}
    assert golden.normalize('http://[::1]:12345/') == 'http://[::1]:<port>/'
    assert golden.normalize('http://[::1]:54321/') == 'http://[::1]:54321/'
    assert golden.normalize('http://[::1]:0/') == 'http://[::1]:0/'
    assert golden.normalize('http://[::1]:1/') == 'http://[::1]:1/'
    assert golden.normalize('{"pid":123,"count":123}') == '{"pid":"<pid>","count":123}'
    golden.observe('exit and bytes', (6, b'refused\n', b''))
    golden.finish()
    golden.session = golden.Session('selfcheck', False, directory, binary, binary)
    golden.observe('exit and bytes', (6, b'refused\n', b''))
    golden.finish()
    golden.session = golden.Session('selfcheck', False, directory, binary, binary)
    try:
        golden.observe('exit and bytes', (0, b'refused\n', b''))
    except AssertionError:
        pass
    else:
        raise AssertionError('changed exit was accepted')
    golden.session = golden.Session('selfcheck', False, directory, binary, binary)
    try:
        golden.finish()
    except AssertionError:
        pass
    else:
        raise AssertionError('missing observation was accepted')
    golden.session = golden.Session('run', True, directory, binary, binary)
    assert golden.normalize(golden.LOCAL_HOST) == golden.LOCAL_HOST
    assert golden.normalize('{"server_host":"'+golden.LOCAL_HOST+'"}') == '{"server_host":"<local-host>"}'
    golden.observe('gzip bytes', (0, b'dev\n', b''))
    golden.finish()
    packed = (directory / 'run.json.gz').read_bytes()
    import gzip,json
    lines = gzip.decompress(packed).splitlines()
    assert len(lines) == len(golden.session.observations)+3
    assert all('provenance' in json.loads(line.rstrip(b',')) for line in lines[2:-1])
    assert golden.session.observations[0]['provenance'] == 'Rust rerecord'
    # Preserve historical producer provenance when a Rust rerecord is unchanged.
    legacy = json.loads(gzip.decompress(packed))
    legacy['observations'][0]['provenance'] = 'historical producer'
    golden.session = golden.Session('run', True, directory, binary, binary)
    golden.session.expected[0]['provenance'] = 'historical producer'
    golden.observe('gzip bytes', (0, b'dev\n', b''))
    golden.finish()
    historical = (directory / 'run.json.gz').read_bytes()
    assert json.loads(gzip.decompress(historical)) == legacy
    golden.session = golden.Session('run', True, directory, binary, binary)
    golden.observe('gzip bytes', (0, b'dev\n', b''))
    golden.finish()
    assert historical == (directory / 'run.json.gz').read_bytes()
    assert golden.session.observations[0]['provenance'] == 'historical producer'
    golden.session = golden.Session('run', False, directory, binary, binary)
    golden.observe('gzip bytes', (0, b'dev\n', b''))
    golden.finish()
    with sqlite3.connect(copy) as db:
        db.execute("update tasks set name='changed again' where id=1")
    assert golden.digest(contract.logical(copy, normalized=True)) != golden.digest(before)
    golden.session = golden.Session('replay', True, directory, binary, binary)
    record = {'v': 1, 'lock': '42\n', 'identity': {'pid': 42, 'uid': 1000, 'start_time': '99'}}
    golden.observe('Go records', record)
    golden.finish()
    producer_bytes = (directory / 'replay.json').read_bytes()
    golden.session = golden.Session('replay', True, directory, binary, binary)
    golden.observe('Go records', {'identity': {'start_time': '99', 'uid': 1000, 'pid': 42}, 'lock': '42\n', 'v': 1})
    golden.finish()
    assert (directory / 'replay.json').read_bytes() == producer_bytes
    golden.session = golden.Session('replay', False, directory, binary, binary)
    golden.observe('Go records', record)
    assert golden.replay('Go records', {'<pid>': 7, '<pid>\n': '7\n', '<uid>': 3, '<proc-start>': '55'}) == {'v': 1, 'lock': '7\n', 'identity': {'pid': 7, 'uid': 3, 'start_time': '55'}}
    golden.finish()
    golden.session = golden.Session('replay', False, directory, binary, binary)
    try:
        golden.observe('Go records', {**record, 'v': True})
    except AssertionError:
        pass
    else:
        raise AssertionError('JSON boolean was accepted as an integer version')
    for argv in ([], ['--expect', str(directory), '--go', str(binary)],
                 ['--record', str(directory / 'missing')]):
        parser = argparse.ArgumentParser()
        parser.add_argument('--go', type=Path)
        parser.add_argument('--rust', type=Path, default=binary)
        with patch.object(sys, 'argv', ['selfcheck', *argv]), contextlib.redirect_stderr(io.StringIO()):
            try:
                golden.parse(parser, __file__)
            except SystemExit as error:
                assert error.code == 2
            else:
                raise AssertionError(f'unsupported CLI mode accepted: {argv}')
    parser = argparse.ArgumentParser()
    parser.add_argument('--rust', type=Path, default=binary)
    with patch.object(sys, 'argv', ['selfcheck', '--record', str(binary)]):
        args = golden.parse(parser, __file__)
    assert args.go == args.rust == binary and golden.session.oracle_is_rust
    golden.session = None
print('fixture clone, row mismatch, visible FTS, shadow exclusion: PASS')
