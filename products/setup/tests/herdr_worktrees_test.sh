#!/bin/sh
# Scratch repositories only; the helper must preserve refs, registration and files.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
exec python3 - "$repo_dir" "${WORKTREES_SH:-sh}" <<'PY'
import hashlib, json, os, shlex, shutil, subprocess, sys, tempfile
from pathlib import Path

helper = Path(sys.argv[1]) / 'products/setup/bin/herdr-worktrees'
shell = sys.argv[2]
with tempfile.TemporaryDirectory(prefix='herdr-worktrees-test.') as scratch:
    root = Path(scratch)
    home = root / 'home'
    home.mkdir()
    env = dict(os.environ, HOME=str(home), GIT_CONFIG_NOSYSTEM='1',
               GIT_AUTHOR_NAME='Inventory Test', GIT_AUTHOR_EMAIL='test@example.invalid',
               GIT_COMMITTER_NAME='Inventory Test', GIT_COMMITTER_EMAIL='test@example.invalid',
               GIT_TERMINAL_PROMPT='0', LC_ALL='C')
    for key in list(env):
        if key.startswith('GIT_') and key not in ('GIT_CONFIG_NOSYSTEM', 'GIT_AUTHOR_NAME',
                'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL', 'GIT_TERMINAL_PROMPT'):
            del env[key]
    real_git = shutil.which('git')
    temp_objects = root / 'temporary objects'
    temp_objects.mkdir()
    env['TMPDIR'] = str(temp_objects)
    if shell == 'bashposix':
        wrapper = root / 'bashposix'
        wrapper.write_text('#!/bin/sh\nexec bash --posix "$@"\n')
        wrapper.chmod(0o755)
        shell = str(wrapper)
    def git(cwd, *args):
        return subprocess.check_output([real_git, '-C', str(cwd), *args], env=env, stderr=subprocess.PIPE)
    def commit(path, name):
        (path / name).write_text(name + '\n')
        git(path, 'add', name)
        git(path, 'commit', '-qm', name)
    origin = root / 'origin.git'
    git(root, 'init', '--bare', str(origin))
    main = root / 'main'
    git(root, 'init', '-b', 'main', str(main))
    commit(main, 'base')
    git(main, 'remote', 'add', 'origin', str(origin))
    git(main, 'push', '-u', 'origin', 'main')
    git(main, 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/main')
    paths = [main]
    def worktree(name, branch=None, start='main', detached=False):
        path = root / name
        args = ['worktree', 'add', '-q']
        args += ['--detach'] if detached else ['-b', branch or name]
        git(main, *args, str(path), start)
        paths.append(path)
        return path
    ff = worktree('fast forward space', 'ff')
    commit(ff, 'ff')
    git(ff, 'push', 'origin', 'ff')
    git(main, 'merge', '--ff-only', 'ff')
    git(main, 'push', 'origin', 'main')
    squash = worktree('squashed')
    commit(squash, 'squash')
    git(squash, 'push', 'origin', 'squashed')
    git(main, 'merge', '--squash', 'squashed')
    git(main, 'commit', '-qm', 'squash merge')
    git(main, 'push', 'origin', 'main')
    opened = worktree('open')
    commit(opened, 'open')
    # A non-origin remote ref also counts as published.
    git(main, 'update-ref', 'refs/remotes/backup/open', git(opened, 'rev-parse', 'HEAD').decode().strip())
    unpublished = worktree('unpushed')
    commit(unpublished, 'unpushed')
    dirty = worktree('dirty')
    (dirty / 'base').write_text('changed\n')
    untracked = worktree('untracked')
    (untracked / 'new dir').mkdir()
    for name in ('one', 'two'):
        (untracked / 'new dir' / name).write_text('untracked\n')
    renamed = worktree('renamed')
    git(renamed, 'mv', 'base', 'renamed base')
    detached = worktree('detached "quoted"', detached=True)
    detached_unpushed = worktree('detached-unpushed', start='unpushed', detached=True)
    # A merge commit can add content that neither parent contributed.
    side = worktree('side')
    commit(side, 'side')
    merge = worktree('merge-content')
    commit(merge, 'merge-parent')
    git(main, 'merge', '--ff-only', 'side')
    git(main, 'merge', '--no-ff', '-m', 'publish both parents', 'merge-content')
    git(main, 'push', 'origin', 'main')
    git(merge, 'merge', '--no-ff', '--no-commit', 'side')
    commit(merge, 'unique-merge-content')
    git(merge, 'push', 'origin', 'merge-content')
    assert git(main, 'rev-list', '--count', '--no-merges', 'main..merge-content').strip() == b'0'
    # Patch IDs ignore this indentation difference; a content merge must not.
    whitespace = worktree('whitespace')
    (whitespace / 'f.py').write_text('if True:\n    a()\nb()\n')
    git(whitespace, 'add', 'f.py')
    git(whitespace, 'commit', '-qm', 'unindented')
    git(whitespace, 'push', 'origin', 'whitespace')
    (main / 'f.py').write_text('if True:\n    a()\n    b()\n')
    git(main, 'add', 'f.py')
    git(main, 'commit', '-qm', 'indented')
    git(main, 'push', 'origin', 'main')
    # A clean, unpublished head whose content is already on base stays unpushed.
    applied_unpushed = worktree('applied-unpushed', start='squashed')
    git(applied_unpushed, 'commit', '--allow-empty', '-qm', 'not published')
    ignored = worktree('ignored')
    # Per-worktree .git is a file, so the ignore rule belongs to the fixture commit.
    (ignored / '.gitignore').write_text('local/\n')
    git(ignored, 'add', '.gitignore')
    git(ignored, 'commit', '-qm', 'ignore local')
    git(ignored, 'push', 'origin', 'ignored')
    git(main, 'merge', '--ff-only', 'ignored')
    git(main, 'push', 'origin', 'main')
    (ignored / 'local').mkdir()
    (ignored / 'local/notes.md').write_text('synthetic private notes\n')
    newline = worktree('line\nbreak', branch='newline')
    twin = worktree('trailing', branch='twin')
    trailing = worktree('trailing\n', branch='trailing-newline')
    (twin / 'untracked').write_text('must not alias trailing newline path\n')
    gone = worktree('gone')
    shutil.rmtree(gone)
    locked = worktree('missing-locked')
    git(main, 'worktree', 'lock', str(locked))
    shutil.rmtree(locked)
    orphan = worktree('orphan')
    git(orphan, 'checkout', '--orphan', 'unborn')
    git(orphan, 'rm', '-rf', '.')
    (main / 'base').write_text('current edit\n')
    no_origin = root / 'no origin'
    git(root, 'init', '-b', 'main', str(no_origin))
    commit(no_origin, 'standalone')
    # A bare entry has no HEAD; linked worktrees still need inventory rows.
    bare = root / 'bare repo.git'
    git(root, 'clone', '--bare', str(origin), str(bare))
    bare_worktree = root / 'bare linked'
    git(bare, 'worktree', 'add', '--detach', str(bare_worktree), 'main')
    empty_bare = root / 'empty.git'
    git(root, 'init', '--bare', str(empty_bare))
    repositories = [main, no_origin, bare, empty_bare]
    inspected_paths = [*paths, no_origin, bare_worktree]

    # Each conflict would let a keep-ours recipe hide branch-only content.
    driver_main = root / 'driver main'
    git(root, 'init', '-b', 'main', str(driver_main))
    driver_cases = {'info': 'info.conf', 'default': 'default.conf',
                    'committed': 'committed.conf', 'union': 'union.conf'}
    mergiraf = shutil.which('mergiraf')
    if mergiraf: driver_cases['mergiraf'] = 'a.rs'
    for name in driver_cases.values():
        (driver_main / name).write_text('fn f() {}\n' if name == 'a.rs' else 'value=base\n')
    git(driver_main, 'add', '.')
    git(driver_main, 'commit', '-qm', 'driver base')
    driver_paths = {}
    for label, name in driver_cases.items():
        path = root / ('driver-' + label)
        git(driver_main, 'worktree', 'add', '-b', label, str(path), 'main')
        (path / name).write_text('fn f() {}\nfn g(){a();}\n' if name == 'a.rs' else 'branch-only=secret\n')
        git(path, 'add', name)
        git(path, 'commit', '-qm', label)
        git(driver_main, 'update-ref', 'refs/remotes/origin/' + label, git(path, 'rev-parse', 'HEAD').decode().strip())
        driver_paths[label] = path
    for name in driver_cases.values():
        (driver_main / name).write_text('fn f() {}\nfn g() {\n    a();\n}\n' if name == 'a.rs' else 'value=upstream\n')
    (driver_main / '.gitattributes').write_text('committed.conf merge=keep\n')
    git(driver_main, 'add', '.')
    git(driver_main, 'commit', '-qm', 'upstream driver conflicts and attributes')
    git(driver_main, 'update-ref', 'refs/remotes/origin/main', git(driver_main, 'rev-parse', 'HEAD').decode().strip())
    git(driver_main, 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/main')
    driver_marker = root / 'custom-driver-ran'
    keep = 'printf invoked > ' + shlex.quote(str(driver_marker)) + '; true'
    git(driver_main, 'config', 'merge.keep.driver', keep)
    git(driver_main, 'config', 'merge.extra with=equals.driver', keep)
    git(driver_main, 'config', 'merge.default', 'keep')
    attributes = 'info.conf merge=keep\nunion.conf merge=union\n'
    if mergiraf:
        git(driver_main, 'config', 'merge.mergiraf.driver', shlex.quote(mergiraf) + ' merge --git %O %A %B -p %P -l %L')
        attributes += 'a.rs merge=mergiraf\n'
    (driver_main / '.git/info/attributes').write_text(attributes)
    assert git(driver_main, 'check-attr', 'merge', '--', 'committed.conf').decode().strip().endswith(': keep')
    expected_driver_keys = git(driver_main, 'config', '--name-only', '--get-regexp', '^merge\\..*\\.driver$').decode().splitlines()
    assert 'merge.extra with=equals.driver' in expected_driver_keys
    repositories.append(driver_main)
    inspected_paths.extend([driver_main, *driver_paths.values()])


    # Catch every attempted Git write/network command, including one that fails.
    stubs = root / 'bin'
    stubs.mkdir()
    guard = stubs / 'git'
    guard.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
while args and args[0] in ('-C', '-c'):
    args = args[2:]
if args == ['rev-parse', '--local-env-vars']:
    os.execv(os.environ['INVENTORY_REAL_GIT'], ['git', *sys.argv[1:]])
assert args and args[0] in ('rev-parse', 'symbolic-ref', 'worktree', 'status', 'rev-list', 'merge-base', 'merge-tree', 'config'), sys.argv
if args[0] == 'symbolic-ref': assert '--quiet' in args and len(args) == 3, sys.argv
if args[0] == 'worktree': assert args[1:] == ['list', '--porcelain', '-z'], sys.argv
if args[0] == 'config': assert args[1] == '--get-regexp' or args[1:3] == ['--name-only', '--get-regexp'], sys.argv
assert os.environ.get('GIT_OPTIONAL_LOCKS') == '0'
assert os.environ.get('GIT_NO_LAZY_FETCH') == '1'
mode = os.environ.get('INVENTORY_FAIL')
if ((mode == 'short' and args[:2] == ['rev-parse', '--short']) or
    (mode == 'ahead' and args[:2] == ['rev-list', '--left-right']) or
    (mode == 'unpublished' and args[:2] == ['rev-list', '--max-count=1']) or
    (mode == 'ancestry' and args[0] == 'merge-base') or
    (mode == 'listing' and args[0] == 'worktree')):
    sys.exit(128)
if args[0] == 'merge-tree':
    assert Path.cwd().is_relative_to(Path(os.environ['TMPDIR'])), Path.cwd()
    assert Path(os.environ['GIT_DIR']).is_absolute() and Path(os.environ['GIT_DIR']).is_dir()
    assert sys.argv[1:3] == ['-c', 'merge.default=text'], sys.argv
    count = int(os.environ['GIT_CONFIG_COUNT'])
    keys = [os.environ['GIT_CONFIG_KEY_' + str(i)] for i in range(count)]
    assert all(os.environ['GIT_CONFIG_VALUE_' + str(i)] == 'false' for i in range(count))
    if os.environ.get('INVENTORY_DRIVER_KEYS'):
        assert set(keys) == set(json.loads(os.environ['INVENTORY_DRIVER_KEYS'])), keys
    directory = Path(os.environ['GIT_OBJECT_DIRECTORY'])
    assert directory.is_relative_to(Path(os.environ['TMPDIR'])), directory
    assert Path(json.loads(os.environ['GIT_ALTERNATE_OBJECT_DIRECTORIES'])).is_dir()
    with open(os.environ['INVENTORY_MERGE_LOG'], 'a') as log:
        log.write(str(directory) + '\\n')
    if mode == 'merge-tree': sys.exit(129)
os.execv(os.environ['INVENTORY_REAL_GIT'], ['git', *sys.argv[1:]])
''')
    guard.chmod(0o755)
    for name in ('taskr', 'herdr', 'curl', 'ssh'):
        stub = stubs / name
        stub.write_text('#!/bin/sh\necho "unexpected control-plane/network call" >&2\nexit 97\n')
        stub.chmod(0o755)
    run_env = dict(env, PATH=str(stubs) + os.pathsep + env['PATH'], INVENTORY_REAL_GIT=real_git,
                   INVENTORY_MERGE_LOG=str(root / 'merge-directories'))
    def readonly_git(cwd, *args):
        return subprocess.check_output([real_git, '-C', str(cwd), *args],
                env=dict(env, GIT_OPTIONAL_LOCKS='0', GIT_NO_LAZY_FETCH='1'), stderr=subprocess.PIPE)
    def snapshot():
        evidence = []
        for repo in repositories:
            evidence += [readonly_git(repo, 'for-each-ref'), readonly_git(repo, 'worktree', 'list', '--porcelain')]
            common = Path(readonly_git(repo, 'rev-parse', '--path-format=absolute', '--git-common-dir').decode().removesuffix('\n'))
            evidence += [(common / name).read_bytes() if (common / name).exists() else None for name in ('HEAD', 'config', 'info/attributes')]
            objects = Path(readonly_git(repo, 'rev-parse', '--path-format=absolute', '--git-path', 'objects').decode().removesuffix('\n'))
            evidence.append([(str(p.relative_to(objects)), hashlib.sha256(p.read_bytes()).digest())
                             for p in sorted(objects.rglob('*')) if p.is_file()])
        for path in inspected_paths:
            if not path.is_dir(): continue
            git_dir = Path(readonly_git(path, 'rev-parse', '--absolute-git-dir').decode().removesuffix('\n'))
            evidence.append((git_dir / 'HEAD').read_bytes())
            evidence.append([(str(p.relative_to(path)), hashlib.sha256(p.read_bytes()).digest())
                             for p in sorted(path.rglob('*')) if p.is_file() and '.git' not in p.relative_to(path).parts])
            evidence.append(readonly_git(path, '-c', 'core.fsmonitor=false', 'status', '--porcelain', '--untracked-files=all', '--ignored=matching'))
            index = Path(readonly_git(path, 'rev-parse', '--path-format=absolute', '--git-path', 'index').decode().removesuffix('\n'))
            evidence.append((index.read_bytes(), index.stat().st_mtime_ns) if index.exists() else None)
        return evidence
    def invoke(cwd, *args, extra=None, helper_shell=None, success=True):
        before = snapshot()
        result = subprocess.run([helper_shell or shell, str(helper), *map(str, args)], cwd=cwd,
                                env=dict(run_env, **(extra or {})), text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if success: assert result.returncode == 0, result.stderr
        else: assert result.returncode != 0 and not result.stdout, result
        after = snapshot()
        assert after == before, ['changed evidence item ' + str(i) for i, (b, a) in enumerate(zip(before, after)) if b != a]
        assert not list(temp_objects.iterdir()), 'temporary objects were not removed'
        assert all(not list(path.glob('.merge_file_*')) for path in inspected_paths), 'merge temp file in worktree'
        return result.stdout
    def run(cwd, *args, **kwargs): return invoke(cwd, *args, **kwargs)
    rows = json.loads(run(main, '--json'))
    by_path = {row['path']: row for row in rows}
    assert set(by_path) == set(map(str, paths))
    def row(path, state, **fields):
        value = by_path[str(path)]
        assert value['state'] == state, value
        for key, expected in fields.items(): assert value[key] == expected, value
        assert value['head'] == 'unknown' or len(value['head']) >= 7
        assert 'task' not in value
    row(main, 'current', dirty=1, untracked=0, ahead=0, behind=0, merged='yes')
    row(ff, 'merged', branch='ff', ahead=0, merged='yes')
    row(squash, 'merged', ahead=1, merged='yes')
    row(opened, 'open', ahead=1, merged='no')
    row(unpublished, 'unpushed', ahead=1, merged='no')
    row(dirty, 'dirty', dirty=1, untracked=0)
    row(untracked, 'dirty', dirty=0, untracked=2)
    row(renamed, 'dirty', dirty=1, untracked=0)
    row(detached, 'merged', branch='detached')
    row(detached_unpushed, 'unpushed', branch='detached')
    row(merge, 'open', merged='no')
    row(whitespace, 'open', merged='unknown')  # add/add conflict; never merged
    row(applied_unpushed, 'unpushed', merged='yes')
    row(ignored, 'merged', dirty=0, untracked=0, ignored=1)
    row(newline, 'merged')
    row(trailing, 'merged', untracked=0)
    row(twin, 'dirty', untracked=1)
    row(gone, 'missing', dirty='unknown', untracked='unknown', ignored='unknown', merged='unknown')
    row(locked, 'missing', dirty='unknown', untracked='unknown', ignored='unknown', merged='unknown')
    row(orphan, 'open', head='unknown', ahead='unknown', behind='unknown', merged='unknown')
    text = run(main, main)
    assert len(text.splitlines()) == len(rows)
    assert 'task=' not in text and 'safe' not in text and 'remove' not in text
    assert 'path=' + json.dumps(str(ff)) in text
    assert 'branch="detached"' in text
    assert {line.rsplit('state=', 1)[1] for line in text.splitlines()} == {r['state'] for r in rows}
    # Current means the invoking worktree, even when REPO names another worktree.
    other = json.loads(run(ff, '--json', main))
    assert next(r for r in other if r['path'] == str(ff))['state'] == 'current'
    assert next(r for r in other if r['path'] == str(main))['state'] == 'dirty'
    single = json.loads(run(root, '--json', no_origin))[0]
    assert single['state'] == 'unpushed' and single['merged'] == 'unknown', single
    assert single['ahead'] == single['behind'] == 'unknown'
    assert 'merged=unknown' in run(root, no_origin)
    assert json.loads(run(root, '--json', empty_bare)) == []
    bare_rows = json.loads(run(root, '--json', bare))
    assert [r['path'] for r in bare_rows] == [str(bare_worktree)], bare_rows
    # Inherited hook/submodule Git context must not redirect the inventory.
    inherited = json.loads(run(main, '--json', extra={
        'GIT_DIR': str(main / '.git'), 'GIT_WORK_TREE': str(main),
        'GIT_INDEX_FILE': str(main / '.git/index'), 'GIT_OBJECT_DIRECTORY': str(main / '.git/objects'),
    }))
    assert inherited == rows
    assert json.loads(run(trailing, '--json', main))[[r['path'] for r in rows].index(str(trailing))]['state'] == 'current'
    # Every formerly swallowed Git failure must fail under plain bash too.
    for failure in ('short', 'ahead', 'unpublished', 'ancestry', 'listing'):
        invoke(main, '--json', extra={'INVENTORY_FAIL': failure}, helper_shell='bash', success=False)
    unavailable = json.loads(run(main, '--json', extra={'INVENTORY_FAIL': 'merge-tree'}, helper_shell='bash'))
    assert next(r for r in unavailable if r['path'] == str(squash))['merged'] == 'unknown'
    assert next(r for r in unavailable if r['path'] == str(ff))['merged'] == 'yes'
    for key in ('extensions.partialClone', 'remote.origin.promisor'):
        git(main, 'config', key, 'origin' if key.startswith('extensions') else 'true')
        partial_rows = json.loads(run(main, '--json'))
        assert next(r for r in partial_rows if r['path'] == str(squash))['merged'] == 'unknown'
        assert next(r for r in partial_rows if r['path'] == str(ff))['merged'] == 'yes'
        git(main, 'config', '--unset', key)
    # An actual blob-filtered clone with offline promisor must not fetch missing base blobs.
    git(origin, 'config', 'uploadpack.allowFilter', 'true')
    git(opened, 'push', 'origin', 'open')
    partial = root / 'partial'
    git(root, 'clone', '--filter=blob:none', '--branch', 'open', origin.as_uri(), str(partial))
    git(partial, 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/main')
    git(partial, 'remote', 'set-url', 'origin', (root / 'offline').as_uri())
    repositories.append(partial)
    inspected_paths.append(partial)
    missing_before = readonly_git(partial, 'rev-list', '--objects', '--all', '--missing=print')
    assert any(line.startswith(b'?') for line in missing_before.splitlines()), 'fixture must lack blobs'
    trace = root / 'partial.trace'
    partial_rows = json.loads(run(root, '--json', partial, extra={'GIT_TRACE': str(trace)}))
    assert partial_rows[0]['merged'] == 'unknown', partial_rows
    assert ' fetch ' not in trace.read_text(), trace.read_text()
    assert readonly_git(partial, 'rev-list', '--objects', '--all', '--missing=print') == missing_before
    assert (root / 'merge-directories').is_file(), 'content check must actually run'
    assert all(not Path(p).exists() for p in (root / 'merge-directories').read_text().splitlines())
    driver_rows = {r['path']: r for r in json.loads(run(root, '--json', driver_main,
                    extra={'INVENTORY_DRIVER_KEYS': json.dumps(expected_driver_keys)}))}
    for label, path in driver_paths.items():
        expected = 'no' if label == 'union' else 'unknown'
        assert driver_rows[str(path)]['merged'] == expected, (label, driver_rows[str(path)])
        assert driver_rows[str(path)]['state'] == 'open', (label, driver_rows[str(path)])
    assert not driver_marker.exists(), 'a custom merge command ran'
    print('merge-driver cases: info/attributes, merge.default, committed attributes, union, spaced/equals config key; mergiraf ' + ('passed' if mergiraf else 'SKIP (not on PATH)'))
    # Linked REPO and common-dir main activate different conditional includes.
    driver_include = driver_main / '.git/keep.inc'
    git(driver_main, 'config', '--file', str(driver_include), 'merge.keep.driver', keep)
    git(driver_main, 'config', '--unset', 'merge.keep.driver')
    git(driver_main, 'config', 'includeIf.onbranch:main.path', 'keep.inc')
    assert 'merge.keep.driver' in git(driver_main, 'config', '--name-only', '--get-regexp', '^merge\\..*\\.driver$').decode().splitlines()
    assert 'merge.keep.driver' not in git(driver_paths['info'], 'config', '--name-only', '--get-regexp', '^merge\\..*\\.driver$').decode().splitlines()
    conditional_rows = {r['path']: r for r in json.loads(run(root, '--json', driver_paths['info']))}
    assert conditional_rows[str(driver_paths['info'])]['merged'] == 'unknown', conditional_rows
    assert not driver_marker.exists(), 'a conditionally included merge command ran'
    print('conditional include driver: linked REPO/common main -> merged=unknown')
    # A remote ref does not reveal the default branch when origin/HEAD is absent.
    git(main, 'symbolic-ref', '--delete', 'refs/remotes/origin/HEAD')
    unknown = json.loads(run(main, '--json'))
    assert all(r['ahead'] == r['behind'] == r['merged'] == 'unknown' for r in unknown)
    assert next(r for r in unknown if r['path'] == str(opened))['state'] == 'open'
    bad = subprocess.run([shell, str(helper), str(root)], cwd=root, env=run_env,
                         text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert bad.returncode != 0 and 'not a repository' in bad.stderr
    print('herdr-worktrees-test: ok (' + shell + '; content checks, edge cases, fail-closed Git, refs/worktrees/status/index/objects unchanged)')
PY
