#!/bin/sh
# All GitHub traffic is replaced by local fixtures; installers get scratch HOME.
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec python3 - <<'PY'
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path.cwd()
with tempfile.TemporaryDirectory(prefix='kit-release-test.') as tmp:
    work = Path(tmp)
    fake = work / 'bin'
    fake.mkdir()
    fixture = work / 'fixture'
    fixture.mkdir()
    # One local implementation serves both curl and gh without opening a socket.
    driver = fake / 'driver'
    driver.write_text('''#!/usr/bin/env python3
import json, os, shutil, subprocess, sys
from pathlib import Path
from urllib.parse import urlparse, parse_qs, unquote
root = Path(os.environ['FIXTURE'])
args = sys.argv[1:]
with (root / 'calls').open('a') as f: f.write(json.dumps([Path(sys.argv[0]).name, *args]) + '\\n')
def releases(): return json.loads((root / 'releases.json').read_text())
def asset(tag, name, output):
    shutil.copyfile(root / 'assets' / tag.replace('/', '_') / name, output)
if Path(sys.argv[0]).name == 'gh':
    if args[:2] == ['auth', 'status']: sys.exit(0 if os.environ.get('USE_GH') == '1' else 1)
    if args[0] == 'api':
        query = args[args.index('--jq') + 1]
        # gh --paginate runs --jq on each page; exercise the actual jq expression.
        for page in json.loads((root / 'pages.json').read_text()):
            subprocess.run(['jq', '-r', query], input=json.dumps(page), text=True, check=True)
        sys.exit(0)
    if args[:2] == ['release', 'download']:
        tag = args[2]
        dest = Path(args[args.index('--dir') + 1])
        for i, arg in enumerate(args):
            if arg == '--pattern': asset(tag, args[i + 1], dest / args[i + 1])
        sys.exit(0)
    raise SystemExit('unexpected gh call')
url = next(a for a in args if a.startswith(('https://', 'file://')))
out_flag = '--output' if '--output' in args else '-o'
output = Path(args[args.index(out_flag) + 1])
parsed = urlparse(url)
path = parsed.path
if '/releases/latest' in path: raise SystemExit('repository-wide latest is forbidden')
if path.endswith('/releases'):
    page = int(parse_qs(parsed.query)['page'][0])
    output.write_text(json.dumps(json.loads((root / 'pages.json').read_text())[page - 1]))
elif '/releases/tags/' in path:
    raw = path.split('/releases/tags/')[1]
    assert '%2F' in raw, 'tag slash must be URL encoded'
    output.write_text(json.dumps(next(r for r in releases() if r['tag_name'] == unquote(raw))))
elif '/releases/assets/' in path:
    wanted = int(path.rsplit('/', 1)[1])
    for r in releases():
        for a in r['assets']:
            if a['id'] == wanted: asset(r['tag_name'], a['name'], output)
elif '/releases/download/' in path:
    tag, name = path.split('/releases/download/')[1].rsplit('/', 1)
    asset(tag, name, output)
else: raise SystemExit('unexpected URL: ' + url)
''')
    driver.chmod(0o755)
    for tool in ('curl', 'gh'):
        (fake / tool).symlink_to(driver)
    releases = []
    next_id = 100
    def release(product, version, *, draft=False, prerelease=False, missing=False):
        global next_id
        tag = f'{product}/v{version}'
        dest = fixture / 'assets' / tag.replace('/', '_')
        dest.mkdir(parents=True)
        names = []
        if product == 'taskr':
            kernel = {'Linux': 'linux', 'Darwin': 'darwin'}[os.uname().sysname]
            arch = 'arm64' if os.uname().machine in ('arm64', 'aarch64') else 'amd64'
            binary = f'taskr-{kernel}-{arch}'
            (dest / binary).write_text(f'#!/bin/sh\necho v{version}\n')
            (dest / f'taskr-tui-{kernel}-{arch}').write_text('#!/bin/sh\nexit 0\n')
            names = [binary, f'taskr-tui-{kernel}-{arch}']
            sums = ''.join(f'{hashlib.sha256((dest / n).read_bytes()).hexdigest()}  {n}\n' for n in names)
            (dest / 'SHA256SUMS').write_text(sums)
            names += ['SHA256SUMS']
        else:
            name = f'slotr-{version}-x86_64-unknown-linux-musl.tar.gz'
            with tarfile.open(dest / name, 'w:gz') as tar:
                data = f'#!/bin/sh\necho slotr {version}\n'.encode()
                member = tarfile.TarInfo('slotr')
                member.mode = 0o755
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
            (dest / f'{name}.sha256').write_text(f'{hashlib.sha256((dest / name).read_bytes()).hexdigest()}  {name}\n')
            names = [name, f'{name}.sha256']
            if version != '3.10.0':
                (dest / 'config.example.toml').write_text('[pools.default]\nslots = 1\n')
                names += ['config.example.toml']
        if missing:
            names.remove("SHA256SUMS" if product == "taskr" else f"{name}.sha256")
        assets = []
        for name in names:
            next_id += 1
            assets.append({'id': next_id, 'name': name})
        row = {'tag_name': tag, 'draft': draft, 'prerelease': prerelease, 'assets': assets}
        releases.append(row)
        return row

    # Deliberately interleave products and versions, independent of API order.
    release('taskr', '2.9.0')
    release('slotr', '3.9.0')
    release('taskr', '2.10.0')
    release('slotr', '3.10.0')
    release('taskr', '9.0.0', prerelease=True)
    release('slotr', '9.0.0', prerelease=True)
    release('taskr', '8.0.0', draft=True)
    release('slotr', '8.0.0', draft=True)
    release('taskr', '7.0.0', missing=True)
    release('slotr', '7.0.0', missing=True)
    release('taskr', '99.0.0-rc.1')
    release('slotr', '99.0.0-rc.1')
    release('taskr', '099.0.0')
    release('slotr', '099.0.0')
    # Put the highest complete stable versions on page two, past 100 records.
    second = [releases.pop(2), releases.pop(2)]
    pages = [releases + [{'tag_name': 'other/v1.0.0', 'draft': False, 'prerelease': False, 'assets': []}] * (100 - len(releases)), second]
    releases += second
    (fixture / 'releases.json').write_text(json.dumps(releases))
    (fixture / 'pages.json').write_text(json.dumps(pages))

    def run(product, expected, *, version=None, transport='public', succeeds=True, config=False):
        home = work / f'home-{product}-{transport}-{version or "auto"}'
        shutil.rmtree(home, ignore_errors=True)
        home.mkdir(exist_ok=True, parents=True)
        env = {k: v for k, v in os.environ.items() if not k.startswith(('TASKR_', 'SLOTR_', 'GH_', 'GITHUB_', 'HERDR_'))}
        env.update(HOME=str(home), PATH=f'{fake}:{os.environ["PATH"]}', FIXTURE=str(fixture), TASKR_NO_SKILL='1', TASKR_NO_PLUGIN='1', USE_GH='1' if transport == 'gh' else '0')
        if version is not None:
            env[f'{product.upper()}_VERSION'] = version
        if transport == 'api':
            env['GH_TOKEN'] = 'fixture-only'
        (fixture / 'calls').write_text('')
        result = subprocess.run(['sh', str(ROOT / f'products/{product}/install.sh'), *(['--config'] if config else [])], env=env, text=True, capture_output=True)
        if not succeeds:
            assert result.returncode != 0, result.stdout
            assert not (home / f'.local/bin/{product}').exists()
            return
        assert result.returncode == 0, (product, transport, result.stdout, result.stderr)
        got = subprocess.check_output([str(home / f'.local/bin/{product}'), '--version'], text=True).strip()
        assert got == (f'v{expected}' if product == 'taskr' else f'slotr {expected}'), got
        calls = (fixture / 'calls').read_text()
        assert f'{product}/v{expected}' in calls or f'{product}%2Fv{expected}' in calls
        if version is None and transport != 'gh':
            assert 'page=2' in calls, calls
        print(f'ok {product} {transport} {version or "highest complete stable across pages"}')

    for transport in ('public', 'api', 'gh'):
        run('taskr', '2.9.0', version='v2.9.0', transport=transport)
        run('taskr', '2.9.0', version='taskr/v2.9.0', transport=transport)
        run('taskr', '2.10.0', transport=transport)
    if os.uname().sysname == 'Linux' and os.uname().machine == 'x86_64':
        run('slotr', '3.9.0', version='3.9.0')
        run('slotr', '3.9.0', version='slotr/v3.9.0')
        run('slotr', '3.10.0')
        run('slotr', '3.9.0', version='v3.9.0', transport='gh')
        run('slotr', '3.10.0', transport='gh')
        run('slotr', '3.9.0', config=True)
        run('slotr', '3.9.0', config=True, transport='gh')
    run('taskr', '', version='slotr/v3.9.0', succeeds=False)
    run('slotr', '', version='taskr/v2.9.0', succeeds=False)
    (fixture / 'pages.json').write_text('[[]]')
    run('taskr', '', succeeds=False)
    run('slotr', '', succeeds=False)
    print('ok invalid product and absent product release fail closed')
PY
