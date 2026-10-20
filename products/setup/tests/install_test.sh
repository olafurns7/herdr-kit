#!/bin/sh
# Real placement and archives; scratch HOME, fake downloads/control plane.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
exec python3 - "$repo_dir" "${INSTALL_SH:-sh}" <<'PY'
import hashlib, json, os, re, shutil, subprocess, sys, tarfile, tempfile
from pathlib import Path
from urllib.parse import unquote, urlsplit

repo = Path(sys.argv[1])
shell = shutil.which(sys.argv[2])
installer = repo / 'products/taskr/install.sh'
setup = repo / 'products/setup'
count = 0

def executable(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(0o755)

def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

with tempfile.TemporaryDirectory(prefix='kit-install-test.') as scratch:
    scratch = Path(scratch)
    stubs = scratch / 'bin'
    stubs.mkdir()
    executable(stubs / 'herdr', '''#!/bin/sh
printf '%s\\n' "$*" >>"$HOME/control.log"
[ "$*" = --skill ] || exit 99
printf '%s\\n' 'upstream synthetic herdr skill'
''')
    assets = scratch / 'assets'
    assets.mkdir()
    for os_name in ('linux', 'darwin'):
        for arch in ('amd64', 'arm64'):
            for binary in ('taskr', 'taskr-tui'):
                executable(assets / f'{binary}-{os_name}-{arch}', '#!/bin/sh\nprintf \'j1 {"version":"0.0.1"}\\n\'\n')
    skill = scratch / 'skill'
    write(skill / 'SKILL.md', 'release taskr skill\n')
    write(skill / 'references/format.md', 'release reference\n')
    write(skill / 'references/nested/recovery.md', 'nested release reference\n')
    plugin = scratch / 'plugin'
    write(plugin / 'herdr-plugin.toml', 'id = "synthetic.taskr"\n')
    for name, source in (('skill.tar.gz', skill), ('plugin.tar.gz', plugin)):
        with tarfile.open(assets / name, 'w:gz') as archive:
            for f in source.rglob('*'):
                archive.add(f, arcname=str(f.relative_to(source)), recursive=False)
    write(assets / 'SHA256SUMS', ''.join(f'{sha(f)}  {f.name}\n' for f in assets.iterdir()))
    # Standalone extraction must exclude the private neighbours in this source archive.
    with tarfile.open(assets / 'source.tar.gz', 'w:gz') as archive:
        archive.add(setup, arcname='source/products/setup')
        archive.add(installer, arcname='source/products/taskr/install.sh')
        archive.add(repo / 'third_party', arcname='source/third_party')
        import io
        data = b'SYNTH_PRIVATE_SOURCE\n'
        entry = tarfile.TarInfo('source/products/setup/skills/herdr-fleet/local.md')
        entry.size = len(data)
        archive.addfile(entry, io.BytesIO(data))
    executable(stubs / 'gh', '''#!/bin/sh
set -eu
[ -z "${FIXTURE_LOG:-}" ] || printf '%s\\n' "$*" >>"$FIXTURE_LOG"
case "$1" in
    auth) exit 0 ;;
    api)
        if [ "$2" = --paginate ]; then printf '0.0.1\\n';
        else cat "$FIXTURE_ASSETS/source.tar.gz"; fi
        ;;
    release)
        while [ "$1" != --dir ]; do shift; done
        shift
        cp "$FIXTURE_ASSETS/"* "$1/"
        ;;
    *) exit 99 ;;
esac
''')

    def case(name):
        global count
        count += 1
        home = scratch / f'case{count}' / 'home'
        home.mkdir(parents=True)
        print(f'case {count}: {name}', flush=True)
        return home

    def run(home, *args, success=True, remote=False, binary=False, extra=None, unpinned=False, bootstrap=False):
        env = dict(HOME=str(home), PATH=f'{stubs}:/usr/bin:/bin',
                   TMPDIR=str(home.parent), FIXTURE_ASSETS=str(assets), TASKR_VERSION='taskr/v0.0.1')
        if unpinned: env.pop('TASKR_VERSION')
        env.update(extra or {})
        script = setup / 'bootstrap.sh' if bootstrap else installer
        if remote:
            script = home.parent / 'install.sh'
            shutil.copyfile(installer, script)
        result = subprocess.run([shell, str(script), *([] if binary else ['--files-only']), *args],
                                env=env, text=True, capture_output=True)
        if (result.returncode == 0) != success:
            raise AssertionError(f'exit {result.returncode}\n{result.stdout}\n{result.stderr}')
        if (home / 'control.log').exists():
            if '--radar' not in args:
                assert all(line == '--skill' for line in (home / 'control.log').read_text().splitlines()), 'activation attempted'
        return result.stdout + result.stderr

    def record(home, relative, digest):
        write(home / '.local/state/herdr-setup/installed', f'{digest} {relative}\n')

    h = case('default release CLI/TUI + skill and staged plugin; rerun')
    run(h, binary=True, unpinned=True)
    for name in ('taskr', 'taskr-tui'):
        assert (h / '.local/bin' / name).stat().st_mode & 0o777 == 0o755
    assert (h / '.agents/skills/taskr/SKILL.md').read_text() == 'release taskr skill\n'
    assert (h / '.local/share/taskr/plugin/herdr-plugin.toml').is_file()
    assert not (h / '.agents/skills/herdr-fleet').exists()
    assert 'SKIP' in run(h, binary=True)

    for installed, compact in (('99.0.0', False), ('0.8.0-capacity.0a5a6cf', False),
                               ('0.8.0+local', False), ('99.0.0', True)):
        h = case(f'unpinned keeps newer/local taskr {installed}, compact={compact}; refreshes skill/plugin')
        cli, tui = (h / '.local/bin' / name for name in ('taskr', 'taskr-tui'))
        version_text = ('j1 ' if compact else '') + json.dumps({'version': 'v' + installed})
        executable(cli, "#!/bin/sh\nprintf '%s\\n' '" + version_text + "'\n")
        executable(tui, '#!/bin/sh\nexit 77\n')
        before = {f: f.read_bytes() for f in (cli, tui)}
        out = run(h, binary=True, unpinned=True)
        reason = ('newer than the pinned taskr/v0.0.1' if installed == '99.0.0'
                  else 'a local or pre-release build')
        assert f'KEEP taskr {installed} ({reason}; run with TASKR_VERSION=<tag> to replace it)' in out
        assert all(f.read_bytes() == content for f, content in before.items())
        assert (h / '.agents/skills/taskr/SKILL.md').read_text() == 'release taskr skill\n'
        assert (h / '.local/share/taskr/plugin/herdr-plugin.toml').is_file()

    for installed in ('99.0.0', '0.8.0-capacity.0a5a6cf'):
        h = case(f'explicit TASKR_VERSION replaces {installed}')
        cli = h / '.local/bin/taskr'
        version_text = json.dumps({'version': installed})
        executable(cli, "#!/bin/sh\nprintf '%s\\n' '" + version_text + "'\n")
        run(h, binary=True)
        assert cli.read_bytes() == (assets / 'taskr-linux-amd64').read_bytes()

    h = case('unpinned upgrades older taskr with compact version output')
    cli = h / '.local/bin/taskr'
    executable(cli, "#!/bin/sh\nprintf '%s\\n' 'j1 {\"version\":\"v0.0.0\"}'\n")
    run(h, binary=True, unpinned=True)
    assert cli.read_bytes() == (assets / 'taskr-linux-amd64').read_bytes()

    h = case('unpinned custom install directory preserves local binary with skills/plugin disabled')
    cli = h / 'custom/bin/taskr'
    executable(cli, "#!/bin/sh\nprintf '%s\\n' 'taskr 0.8.0+local'\n")
    before = cli.read_bytes()
    out = run(h, binary=True, unpinned=True,
              extra={'TASKR_INSTALL_DIR': str(cli.parent), 'TASKR_NO_SKILL': '1', 'TASKR_NO_PLUGIN': '1'})
    assert 'KEEP taskr 0.8.0+local' in out
    assert cli.read_bytes() == before

    for installed in ('99.0.0', '0.8.0+local'):
        for explicit in (False, True):
            h = case(f'bootstrap caller TASKR_VERSION explicit={explicit}, installed={installed}')
            cli, tui = (h / '.local/bin' / name for name in ('taskr', 'taskr-tui'))
            version_text = json.dumps({'version': installed})
            executable(cli, "#!/bin/sh\nprintf '%s\\n' '" + version_text + "'\n")
            executable(tui, '#!/bin/sh\nexit 77\n')
            before = {f: f.read_bytes() for f in (cli, tui)}
            log = h / 'downloads.log'
            out = run(h, binary=True, bootstrap=True, unpinned=not explicit,
                      extra={'FIXTURE_LOG': str(log)})
            if explicit:
                for f in (cli, tui):
                    assert f.read_bytes() == (assets / f'{f.name}-linux-amd64').read_bytes()
            else:
                assert f'KEEP taskr {installed}' in out
                assert all(f.read_bytes() == content for f, content in before.items())
            calls = log.read_text()
            tag = 'taskr/v0.0.1' if explicit else 'taskr/v0.25.1'
            assert f'tarball/{tag}' in calls and f'release download {tag} ' in calls
            assert '--paginate' not in calls
            assert (h / '.agents/skills/taskr/SKILL.md').is_file()
            assert (h / '.local/share/taskr/plugin/herdr-plugin.toml').is_file()

    for harness in ('.claude', '.codex', '.config/opencode'):
        h = case(f'selected harness skills directory refuses escape: {harness}')
        outside = h.parent / 'outside'
        outside.mkdir()
        write(outside / 'sentinel', 'untouched\n')
        skills_dir = h / harness / 'skills'
        skills_dir.parent.mkdir(parents=True)
        skills_dir.symlink_to(outside, target_is_directory=True)
        out = run(h, '--fleet', success=False)
        assert 'skills directory leaves HOME' in out
        assert list(outside.iterdir()) == [outside / 'sentinel']
        assert (outside / 'sentinel').read_text() == 'untouched\n'

    instruction_paths = ('.claude/CLAUDE.md', '.codex/AGENTS.md')
    instruction_base = (setup / 'claude/CLAUDE.md').read_bytes()
    assert instruction_base == (setup / 'codex/AGENTS.md').read_bytes()
    for remote in (False, True):
        for with_overlay in (False, True):
            h = case(f'global instructions: standalone={remote}, overlay={with_overlay}')
            for dest in instruction_paths: (h / dest).parent.mkdir()
            overlay = h / '.config/herdr-fleet/instructions.local.md'
            overlay_text = b'SYNTH_OPERATOR_POLICY\n'
            if with_overlay:
                write(overlay, overlay_text.decode())
                overlay.chmod(0o600)
            out = run(h, '--fleet', remote=remote, binary=remote)
            expected = instruction_base + (b'\n' + overlay_text if with_overlay else b'')
            for dest in instruction_paths: assert (h / dest).read_bytes() == expected
            assert overlay_text.decode().strip() not in out
            if with_overlay:
                assert overlay.read_bytes() == overlay_text
                assert overlay.stat().st_mode & 0o777 == 0o600
            rerun = run(h, '--fleet', remote=remote, binary=remote)
            for dest in instruction_paths: assert f'SKIP {h / dest}' in rerun

    for selected in (False, True):
        h = case(f'global instructions need fleet and existing directories: fleet={selected}')
        if not selected:
            for dest in instruction_paths: (h / dest).parent.mkdir()
            write(h / '.config/herdr-fleet/instructions.local.md', 'SYNTH_OPERATOR_POLICY\n')
        run(h, *(['--fleet'] if selected else []))
        for dest in instruction_paths: assert not (h / dest).exists()

    for kind in ('inside-link', 'outside-link', 'outside-parent', 'directory', 'dangling'):
        h = case(f'global instruction overlay: {kind}')
        for dest in instruction_paths: (h / dest).parent.mkdir()
        overlay = h / '.config/herdr-fleet/instructions.local.md'
        target = (h if kind == 'inside-link' else h.parent) / 'policy'
        write(target / 'instructions.local.md', 'SYNTH_OPERATOR_POLICY\n')
        overlay.parent.mkdir(parents=True)
        if kind == 'outside-parent':
            overlay.parent.rmdir()
            overlay.parent.symlink_to(target, target_is_directory=True)
        elif kind == 'directory': overlay.mkdir()
        else: overlay.symlink_to(target / ('missing' if kind == 'dangling' else 'instructions.local.md'))
        out = run(h, '--fleet')
        expected = instruction_base + (b'\nSYNTH_OPERATOR_POLICY\n' if kind == 'inside-link' else b'')
        for dest in instruction_paths: assert (h / dest).read_bytes() == expected
        if kind != 'inside-link': assert 'WARN: instructions.local.md' in out and 'ignored' in out
        assert 'SYNTH_OPERATOR_POLICY' not in out
        assert (target / 'instructions.local.md').read_text() == 'SYNTH_OPERATOR_POLICY\n'

    for outcome in ('UPDATE', 'MERGE', 'KEEP'):
        h = case(f'merged global instructions retain placement contract: {outcome}')
        overlay = h / '.config/herdr-fleet/instructions.local.md'
        write(overlay, 'SYNTH_OPERATOR_POLICY\n')
        for dest in instruction_paths: write(h / dest, 'previous\n')
        write(h / '.local/state/herdr-setup/installed', ''.join(f'{sha(h / dest)} {dest}\n' for dest in instruction_paths))
        if outcome == 'MERGE':
            for dest in instruction_paths: write(h / dest, 'edited\n')
        if outcome == 'KEEP': write(h / '.config/herdr-setup/keep', '\n'.join(instruction_paths) + '\n')
        out = run(h, '--fleet')
        for dest in instruction_paths:
            assert f'{outcome} {h / dest}' in out
            sidecar = h / (dest + '.herdr-setup')
            if outcome == 'UPDATE': assert (h / dest).read_bytes() == instruction_base + b'\nSYNTH_OPERATOR_POLICY\n'
            elif outcome == 'MERGE':
                assert (h / dest).read_text() == 'edited\n'
                assert sidecar.read_bytes() == instruction_base + b'\nSYNTH_OPERATOR_POLICY\n'
            else:
                assert (h / dest).read_text() == 'previous\n'
                assert not sidecar.exists()
        assert overlay.read_text() == 'SYNTH_OPERATOR_POLICY\n'

    h = case('fleet staging preserves overlay and private table byte-for-byte')
    overlay = h / '.agents/skills/herdr-fleet/local.md'
    private = h / '.config/herdr-fleet/lanes.json'
    private_intake = h / '.config/herdr-fleet/intake.yaml'
    write(overlay, 'SYNTH_PRIVATE_OVERLAY\n')
    write(private, 'SYNTH_PRIVATE_INVALID_TABLE\n')
    write(private_intake, 'SYNTH_PRIVATE_INTAKE\n')
    overlay.chmod(0o600)
    before = {f: f.read_bytes() for f in (overlay, private, private_intake)}
    (h / '.codex').mkdir()
    out = run(h, '--fleet', '--taskr-hub')
    assert 'PRESERVE' in out
    for f, content in before.items(): assert f.read_bytes() == content
    assert overlay.stat().st_mode & 0o777 == 0o600
    assert (h / '.codex/skills/herdr-fleet').is_symlink()
    assert (h / '.config/systemd/user/taskr.service').is_file()
    assert not (h / '.local/bin/cdx').exists()
    assert not (h / '.agents/skills/herdr-practices').exists()
    for f in h.rglob('*'):
        if f.is_file() and f not in before:
            assert b'SYNTH_PRIVATE' not in f.read_bytes(), str(f)
    rerun = run(h, '--fleet')
    assert 'systemctl' not in rerun and 'loginctl' not in rerun

    h = case('standalone installer extracts only public allowlist')
    run(h, '--fleet', '--practices', '--verification', remote=True, binary=True)
    assert not (h / '.agents/skills/herdr-fleet/local.md').exists()
    for f in h.rglob('*'):
        if f.is_file(): assert b'SYNTH_PRIVATE' not in f.read_bytes(), str(f)
    for optional in ('herdr-practices', 'herdr-verification'):
        entry = h / '.agents/skills' / optional / 'SKILL.md'
        assert entry.is_file()
        links = re.findall(r'\]\(([^)]+)\)', entry.read_text())
        for target in links:
            if '://' not in target: assert (entry.parent / target).exists(), target
    assert (h / '.local/share/herdr-setup/third_party/pstack/LICENSE').read_bytes() == (repo / 'third_party/pstack/LICENSE').read_bytes()
    for reference in ('incident-intake.md', 'writing.md', 'runbooks.md', 'design.md'):
        relative = Path('skills/herdr-practices/references') / reference
        assert (h / '.agents' / relative).read_bytes() == (setup / relative).read_bytes()
    assert (h / '.local/share/herdr-setup/intake.example.yaml').read_bytes() == (setup / 'intake.example.yaml').read_bytes()
    assert not (h / '.config/herdr-fleet/intake.yaml').exists()

    h = case('one explicit optional skill does not select the other or fleet')
    run(h, '--verification')
    assert (h / '.agents/skills/herdr-verification/SKILL.md').exists()
    assert not (h / '.agents/skills/herdr-practices').exists()
    assert not (h / '.agents/skills/herdr-fleet').exists()

    relative = '.codex/hooks.json'
    source = setup / 'codex/hooks.json'
    h = case('equal SKIP and recorded UPDATE retain existing mode')
    dest = h / relative
    write(dest, 'old\n'); dest.chmod(0o640)
    record(h, relative, sha(dest))
    assert f'UPDATE {dest}' in run(h, '--fleet')
    assert dest.read_bytes() == source.read_bytes()
    assert dest.stat().st_mode & 0o777 == 0o640
    assert f'SKIP {dest}' in run(h, '--fleet')

    for sidecar in ('missing', 'equal', 'different', 'directory', 'symlink', 'dangling'):
        h = case(f'edited MERGE with {sidecar} sidecar')
        dest = h / relative
        write(dest, 'edited\n'); dest.chmod(0o640)
        record(h, relative, '0' * 64)
        side = dest.with_name(dest.name + '.herdr-setup')
        victim = h / 'victim'
        if sidecar == 'equal': shutil.copyfile(source, side)
        if sidecar == 'different': write(side, 'other\n')
        if sidecar == 'directory': side.mkdir()
        if sidecar in ('symlink', 'dangling'):
            if sidecar == 'symlink': write(victim, 'victim\n')
            side.symlink_to(victim)
        assert f'MERGE {dest}' in run(h, '--fleet')
        assert dest.read_text() == 'edited\n'
        assert dest.stat().st_mode & 0o777 == 0o640
        assert f'{"0" * 64} {relative}' in (h / '.local/state/herdr-setup/installed').read_text()
        if sidecar in ('missing', 'equal'): assert side.read_bytes() == source.read_bytes()
        if sidecar == 'different': assert side.read_text() == 'other\n'
        if victim.exists(): assert victim.read_text() == 'victim\n'

    for kind in ('directory', 'symlink', 'dangling'):
        h = case(f'destination {kind} retained')
        dest = h / relative
        dest.parent.mkdir()
        if kind == 'directory': dest.mkdir(); dest.chmod(0o750)
        else:
            if kind == 'symlink': write(h / 'victim', 'victim\n')
            dest.symlink_to(h / 'victim')
        run(h, '--fleet')
        if kind == 'directory': assert dest.stat().st_mode & 0o777 == 0o750
        else: assert dest.is_symlink()
        if (h / 'victim').exists(): assert (h / 'victim').read_text() == 'victim\n'

    h = case('KEEP beats recorded UPDATE; appended paths are deduplicated')
    dest = h / relative
    write(dest, 'kept\n'); record(h, relative, sha(dest))
    write(h / '.config/herdr-setup/keep', '# comment\n\n ' + relative + ' ')
    assert f'KEEP {dest}' in run(h, '--fleet', '--keep', '.local/bin/herdr-lead')
    run(h, '--fleet', '--keep', '.local/bin/herdr-lead')
    assert dest.read_text() == 'kept\n'
    assert not dest.with_name(dest.name + '.herdr-setup').exists()
    assert not (h / '.local/bin/herdr-lead').exists()
    assert (h / '.config/herdr-setup/keep').read_text().count('.local/bin/herdr-lead') == 1

    for bad in ('', '/etc/passwd', '../outside', './x', 'a/../x', 'a//x', 'a/', '~/x'):
        h = case(f'invalid keep path: {bad!r}')
        run(h, '--keep', bad, success=False)
        assert not (h / '.agents').exists()

    for kind in ('symlink', 'directory'):
        h = case(f'keep {kind} refuses append')
        keep = h / '.config/herdr-setup/keep'
        keep.parent.mkdir(parents=True)
        if kind == 'symlink':
            write(h / 'victim', 'victim\n'); keep.symlink_to(h / 'victim')
        else: keep.mkdir()
        run(h, '--keep', relative, success=False)
        assert not (h / '.agents').exists()

    for kind in ('symlink', 'directory', 'malformed', 'unreadable'):
        if kind == 'unreadable' and os.geteuid() == 0: continue
        h = case(f'{kind} record never authorizes UPDATE')
        dest = h / relative
        write(dest, 'mine\n')
        state = h / '.local/state/herdr-setup/installed'
        state.parent.mkdir(parents=True)
        if kind == 'symlink':
            write(h / 'victim', f'{sha(dest)} {relative}\n'); state.symlink_to(h / 'victim')
        elif kind == 'directory': state.mkdir()
        elif kind == 'malformed': write(state, f'{sha(dest)[:-1]} {relative}\n{sha(dest)}  {relative}\n')
        else: write(state, f'{sha(dest)} {relative}\n'); state.chmod(0)
        assert f'MERGE {dest}' in run(h, '--fleet')
        assert dest.read_text() == 'mine\n'
        if kind == 'unreadable': state.chmod(0o600)

    for metadata in ('.config/herdr-setup', '.local/state/herdr-setup'):
        h = case(f'escaped metadata parent: {metadata}')
        outside = h.parent / 'outside'; outside.mkdir()
        link = h / metadata; link.parent.mkdir(parents=True); link.symlink_to(outside)
        assert 'leaves HOME' in run(h, '--fleet')
        assert not list(outside.iterdir())

    h = case('escaped keep argument and asset parent refused')
    outside = h.parent / 'outside'; outside.mkdir()
    (h / '.codex').symlink_to(outside)
    run(h, '--fleet', '--keep', '.codex/hooks.json', success=False)
    run(h, '--fleet', success=False)
    assert not list(outside.iterdir())

    for ref in ('taskr', 'taskr/references'):
        h = case(f'skill directory symlink refused: {ref}')
        outside = h / 'other'; outside.mkdir()
        link = h / '.agents/skills' / ref; link.parent.mkdir(parents=True); link.symlink_to(outside)
        run(h)
        assert not list(outside.iterdir())

    h = case('interrupted install reruns from equal files without prior record')
    # A real failed cp after one successful placement leaves a partial install.
    failbin = h.parent / 'bin'; failbin.mkdir()
    executable(failbin / 'cp', '''#!/bin/sh
case "$*" in *herdr-fleet/SKILL.md*) kill -TERM "$PPID"; exit 91 ;; esac
exec /bin/cp "$@"
''')
    run(h, '--fleet', success=False, extra={'PATH': f'{failbin}:{stubs}:/usr/bin:/bin'})
    assert (h / '.agents/skills/taskr/SKILL.md').is_file()
    assert not (h / '.local/state/herdr-setup/installed').exists()
    assert 'SKIP' in run(h, '--fleet')
    assert (h / '.agents/skills/herdr-fleet/SKILL.md').is_file()
    assert (h / '.local/state/herdr-setup/installed').is_file()

    h = case('custom taskr skill directory retains loader link support')
    (h / '.codex').mkdir()
    custom = h / 'custom/taskr'
    run(h, extra={'TASKR_SKILL_DIR': str(custom), 'TASKR_LINK_SKILLS': '1'})
    assert (h / '.codex/skills/taskr/SKILL.md').resolve() == custom / 'SKILL.md'
    assert not (h / '.agents/skills/herdr-fleet').exists()

    if os.geteuid() != 0:
        h = case('unreadable keep list stops before placement')
        keep = h / '.config/herdr-setup/keep'
        write(keep, relative + '\n'); keep.chmod(0)
        run(h, '--fleet', success=False)
        keep.chmod(0o600)
        assert not (h / '.agents').exists()

    for copy in ('equal', 'edited', 'symlink-core', 'wrong-link'):
        h = case('skill loader preserves/refuses existing copies: ' + copy)
        dest = h / '.codex/skills/herdr-fleet'
        dest.parent.mkdir(parents=True)
        if copy == 'wrong-link': dest.symlink_to('missing')
        elif copy == 'symlink-core':
            dest.mkdir()
            write(h / 'private-core', 'private core\n')
            (dest / 'SKILL.md').symlink_to(h / 'private-core')
        else:
            dest.mkdir()
            shutil.copyfile(setup / 'skills/herdr-fleet/SKILL.md', dest / 'SKILL.md')
            if copy == 'edited': write(dest / 'notes.md', 'keep notes\n')
        run(h, '--fleet')
        if copy in ('equal', 'wrong-link'):
            assert dest.is_symlink()
            assert (dest / 'SKILL.md').is_file()
        else:
            assert not dest.is_symlink()
            if copy == 'edited': assert (dest / 'notes.md').read_text() == 'keep notes\n'
            else: assert (dest / 'SKILL.md').is_symlink()

    h = case('invalid keep refuses a binary install before replacing anything')
    run(h, '--keep', '../outside', binary=True, success=False)
    assert not (h / '.local/bin/taskr').exists()
    assert not (h / '.agents').exists()

    for bad in ('relative', 'http://host:0', 'http://host:65536', 'https://host:7788', 'http://host:7788/path'):
        h = case('invalid client URL refused')
        run(h, '--taskr-client', bad, success=False)
        assert not (h / '.agents').exists()
    h = case('mutually exclusive roles refused')
    run(h, '--taskr-hub', '--taskr-client', 'http://100.64.0.1:7788', success=False)
    assert not (h / '.agents').exists()

    for role in ('ledger', 'hub', 'legacy'):
        h = case('conflicting taskr role retained: ' + role)
        if role == 'ledger': write(h / '.local/state/taskr/taskr.db', 'ledger')
        if role == 'hub': write(h / '.local/state/taskr/dashboard.addr', 'tailnet\n')
        if role == 'legacy': write(h / '.local/state/taskr/hub.url', 'legacy')
        args = ('--taskr-hub',) if role == 'legacy' else ('--taskr-client', 'http://100.64.0.1:7788')
        assert 'MERGE' in run(h, *args)
        assert not (h / '.local/state/taskr/server.url').exists()

    # Reject archive links, traversal, duplicate entries and unknown skill files
    # before modifying a destination. Checksums match these synthetic bad archives.
    good_skill = (assets / 'skill.tar.gz').read_bytes()
    good_sums = (assets / 'SHA256SUMS').read_bytes()
    for bad in ('symlink', 'hardlink', 'traversal', 'absolute', 'option', 'duplicate', 'unexpected'):
        h = case('unsafe skill archive: ' + bad)
        with tarfile.open(assets / 'skill.tar.gz', 'w:gz') as archive:
            name = {'traversal': '../outside', 'absolute': '/outside', 'option': '--option',
                    'unexpected': 'private.md'}.get(bad, 'SKILL.md')
            entry = tarfile.TarInfo(name)
            if bad in ('symlink', 'hardlink'):
                entry.type = tarfile.SYMTYPE if bad == 'symlink' else tarfile.LNKTYPE
                entry.linkname = 'victim'
            archive.addfile(entry)
            if bad == 'duplicate': archive.addfile(entry)
        sums = good_sums.decode()
        sums = re.sub(r'^[0-9a-f]+  skill.tar.gz$', sha(assets / 'skill.tar.gz') + '  skill.tar.gz', sums, flags=re.M)
        write(assets / 'SHA256SUMS', sums)
        run(h, binary=True, success=False)
        assert not (h / '.agents').exists()
        assert not (h / '.local/bin/taskr').exists()
    (assets / 'skill.tar.gz').write_bytes(good_skill)
    (assets / 'SHA256SUMS').write_bytes(good_sums)

    for ref in ('references','references/nested'):
        h = case('nested release reference directory symlink is not followed: ' + ref)
        other = h / 'other'; other.mkdir()
        link = h / '.agents/skills/taskr' / ref; link.parent.mkdir(parents=True,exist_ok=True)
        link.symlink_to(other)
        run(h, binary=True)
        assert link.is_symlink()
        assert not list(other.iterdir())
    for suffix in ('','.herdr-setup'):
        h = case('reference file/sidecar symlink is not followed: ' + suffix)
        dest = h / '.agents/skills/taskr/references/format.md'
        write(dest, 'edited\n')
        victim = h / 'victim'; write(victim, 'victim\n')
        link = h / (str(dest.relative_to(h)) + suffix)
        if not suffix: link.unlink()
        link.symlink_to(victim)
        run(h)
        assert link.is_symlink()
        assert victim.read_text() == 'victim\n'

    h = case('absolute inside-HOME and multiline keep arguments are refused')
    run(h, '--keep', str(h / '.codex/hooks.json'), success=False)
    run(h, '--keep', 'one\ntwo', success=False)
    assert not (h / '.agents').exists()

    h = case('only Codex is linked; bare pi config gets no extension')
    (h / '.codex').mkdir(); (h / '.pi').mkdir()
    run(h, '--fleet')
    assert (h / '.codex/skills/herdr-fleet').is_symlink()
    assert not (h / '.claude/skills').exists()
    assert not (h / '.pi/agent/extensions/taskr-hook.ts').exists()

    h = case('Codator wrappers are independent opt-ins; Codator itself is never installed')
    run(h, '--codator')
    for wrapper in ('cdx', 'cdl'):
        assert (h / '.local/bin' / wrapper).read_bytes() == (setup / 'bin' / wrapper).read_bytes()
        assert (h / '.local/bin' / wrapper).stat().st_mode & 0o777 == 0o755
    assert not (h / '.local/bin/codator').exists()
    assert not (h / '.agents/skills/herdr-fleet').exists()

    h = case('notification fragments are explicit, idempotent, and preserve unrelated keys')
    for dest in ('.claude/settings.json', '.codex/hooks.json'):
        write(h / dest, '{"unrelated":{"keep":42},"hooks":{}}\n')
        record(h, dest, sha(h / dest))
        # Add both entries to the record rather than replacing the prior line.
    write(h / '.local/state/herdr-setup/installed', ''.join(f'{sha(h / dest)} {dest}\n' for dest in ('.claude/settings.json', '.codex/hooks.json')))
    run(h, '--notifications')
    for dest in ('.claude/settings.json', '.codex/hooks.json'):
        current = json.loads((h / dest).read_text())
        assert current['unrelated'] == {'keep':42}
        assert 'moshi-hook' in (h / dest).read_text()
        assert 'TASKR_LAUNCH' in (h / dest).read_text()
    before = {dest: (h / dest).read_bytes() for dest in ('.claude/settings.json', '.codex/hooks.json')}
    run(h, '--notifications')
    assert all((h / dest).read_bytes() == data for dest, data in before.items())

    h = case('edited notification settings merge into a sidecar without losing private keys')
    write(h / '.claude/settings.json', '{"private_setting":"synthetic","hooks":{}}\n')
    run(h, '--notifications')
    assert 'moshi-hook' not in (h / '.claude/settings.json').read_text()
    side = json.loads((h / '.claude/settings.json.herdr-setup').read_text())
    assert side['private_setting'] == 'synthetic'
    assert (h / '.claude/settings.json.herdr-setup').stat().st_mode & 0o777 == 0o600
    assert 'moshi-hook' in json.dumps(side)

    h = case('radar flag alone is inert in files-only mode')
    run(h, '--radar')
    assert not (h / 'control.log').exists()
    h = case('radar flag alone uses manual steps outside Herdr')
    assert 'MANUAL: install the patched radar' in run(h, '--radar', binary=True)
    assert not (h / 'control.log').exists()
    h = case('radar flag reaches the original helper and contains an optional failure')
    out = run(h, '--radar', binary=True, extra={'HERDR_ENV':'1'})
    assert 'WARN: herdr plugin list' in out
    assert (h / 'control.log').read_text().splitlines() == ['plugin list --plugin hhdebb.herdr-radar --json']
    assert (h / '.local/bin/taskr').is_file()

    if os.uname().sysname == 'Linux' and os.uname().machine == 'x86_64':
        slotr_release = scratch / 'slotr-release'; slotr_release.mkdir()
        slotr_archive = slotr_release / 'slotr-0.2.0-x86_64-unknown-linux-musl.tar.gz'
        slotr_sum = Path(str(slotr_archive)+'.sha256')
        slotr_bytes = b'#!/bin/sh\necho slotr 0.2.0\n'
        def slotr_pack(kind):
            with tarfile.open(slotr_archive,'w:gz') as archive:
                entry = tarfile.TarInfo('slotr'); entry.mode=0o755
                if kind in ('symlink','hardlink'):
                    entry.type=tarfile.SYMTYPE if kind == 'symlink' else tarfile.LNKTYPE
                    entry.linkname='LICENSE'; archive.addfile(entry)
                else:
                    entry.size=len(slotr_bytes); archive.addfile(entry,io.BytesIO(slotr_bytes))
                if kind == 'duplicate': archive.addfile(entry,io.BytesIO(slotr_bytes))
                if kind in ('unknown','extra-symlink','duplicate-license'):
                    notice=tarfile.TarInfo('LICENSE' if kind=='duplicate-license' else 'unexpected')
                    if kind=='extra-symlink': notice.type=tarfile.SYMTYPE; notice.linkname='slotr'
                    archive.addfile(notice)
                    if kind=='duplicate-license': archive.addfile(notice)
            write(slotr_sum,sha(slotr_archive)+'  '+slotr_archive.name+'\n')
        slotr_env={'SLOTR_INSTALL_BASE_URL':'file://'+str(slotr_release),'SLOTR_VERSION':'slotr/v0.2.0'}
        for bad in ('checksum','wrong-checksum-name','extra-checksum-line','symlink','hardlink','duplicate','unknown','extra-symlink','duplicate-license','missing-download'):
            h = case('slotr opt-in refusal preserves installed binary and core continues: '+bad)
            slotr_pack(bad)
            if bad=='checksum': write(slotr_sum,'0'*64+'  '+slotr_archive.name+'\n')
            if bad=='wrong-checksum-name': write(slotr_sum,sha(slotr_archive)+'  wrong.tar.gz\n')
            if bad=='extra-checksum-line': write(slotr_sum,slotr_sum.read_text()*2)
            if bad=='missing-download': slotr_archive.unlink()
            write(h / '.local/bin/slotr','old binary\n')
            out=run(h,'--slotr',binary=True,extra=slotr_env)
            assert 'WARN: optional slotr installation failed' in out
            assert (h / '.local/bin/slotr').read_text()=='old binary\n'
            assert (h / '.local/bin/taskr').is_file()
        h=case('slotr opt-in accepts a single regular binary; version override remains explicit')
        slotr_pack('single')
        run(h,'--slotr',binary=True,extra=slotr_env)
        assert (h / '.local/bin/slotr').read_bytes()==slotr_bytes
        executable(h / '.local/bin/slotr', '#!/bin/sh\necho slotr 0.2.0\nexit 1\n')
        run(h,'--slotr',binary=True,extra=slotr_env)
        assert (h / '.local/bin/slotr').read_bytes()==slotr_bytes
        h=case('slotr files-only is inert')
        run(h,'--slotr',extra=slotr_env)
        assert not (h / '.local/bin/slotr').exists()

    for platform_os,platform_machine in (('Darwin','x86_64'),('Linux','aarch64')):
        h=case('unsupported slotr platform is contained: '+platform_os+'/'+platform_machine)
        platform_bin=h.parent / 'platform-bin';platform_bin.mkdir()
        executable(platform_bin / 'uname', '#!/bin/sh\ncase "$1" in -s) echo "$PLATFORM_OS" ;; -m) echo "$PLATFORM_MACHINE" ;; esac\n')
        run(h,'--slotr',binary=True,extra={'PATH':f'{platform_bin}:{stubs}:/usr/bin:/bin','PLATFORM_OS':platform_os,'PLATFORM_MACHINE':platform_machine})
        assert not (h / '.local/bin/slotr').exists()
        assert (h / '.local/bin/taskr').is_file()

    h = case('fleet installs executable hub launcher from public archive')
    run(h, '--fleet', remote=True, binary=True)
    hub = h / '.local/bin/herdr-hub'
    assert hub.read_bytes() == (setup / 'bin/herdr-hub').read_bytes()
    assert hub.stat().st_mode & 0o777 == 0o755

    # The same placer protects every retained file family, not only one JSON file.
    retained = {
        '.claude/CLAUDE.md': setup / 'claude/CLAUDE.md',
        '.codex/AGENTS.md': setup / 'codex/AGENTS.md',
        '.agents/skills/taskr/SKILL.md': repo / 'products/taskr/SKILL.md',
        '.agents/skills/taskr/references/format.md': repo / 'products/taskr/references/format.md',
        '.agents/skills/herdr/SKILL.md': None,
        '.agents/skills/herdr-orchestration/launch.md': setup / 'skills/herdr-orchestration/launch.md',
        '.agents/skills/herdr-orchestration/model-selection.md': setup / 'docs/model-selection.md',
        '.agents/skills/herdr-fleet/SKILL.md': setup / 'skills/herdr-fleet/SKILL.md',
        '.config/opencode/plugins/taskr-hook.js': setup / 'opencode/plugins/taskr-hook.js',
        '.pi/agent/extensions/taskr-hook.ts': setup / 'pi/extensions/taskr-hook.ts',
        '.local/bin/cdx': setup / 'bin/cdx',
        '.local/bin/cdl': setup / 'bin/cdl',
        '.local/bin/herdr-launch': setup / 'bin/herdr-launch',
        '.local/bin/herdr-loop': setup / 'bin/herdr-loop',
        '.local/bin/herdr-worktrees': setup / 'bin/herdr-worktrees',
        '.local/bin/herdr-lead': setup / 'bin/herdr-lead',
        '.local/bin/herdr-hub': setup / 'bin/herdr-hub',
        '.config/systemd/user/taskr.service': setup / 'systemd/taskr.service',
    }
    for dest, source in retained.items():
        for outcome in ('UPDATE', 'MERGE', 'KEEP'):
            h = case(f'placement matrix {dest}: {outcome}')
            (h / '.config/opencode').mkdir(parents=True)
            (h / '.pi/agent').mkdir(parents=True)
            write(h / dest, 'previous installer content\n')
            (h / dest).chmod(0o640)
            recorded = sha(h / dest)
            record(h, dest, recorded)
            if outcome == 'MERGE': write(h / dest, 'edited content\n')
            if outcome == 'KEEP': write(h / '.config/herdr-setup/keep', dest + '\n')
            out = run(h, '--fleet', '--codator', '--taskr-hub')
            assert f'{outcome} {h / dest}' in out
            assert (h / dest).stat().st_mode & 0o777 == 0o640
            expected = source.read_bytes() if source else b'upstream synthetic herdr skill\n'
            if outcome == 'UPDATE': assert (h / dest).read_bytes() == expected
            if outcome == 'MERGE':
                assert (h / dest).read_text() == 'edited content\n'
                assert (h / (dest + '.herdr-setup')).read_bytes() == expected
                assert f'{recorded} {dest}' in (h / '.local/state/herdr-setup/installed').read_text()
            if dest == '.agents/skills/taskr/SKILL.md':
                assert (h / '.agents/skills/taskr/references/format.md').is_file()
            if outcome == 'KEEP':
                assert (h / dest).read_text() == 'previous installer content\n'
                assert not (h / (dest + '.herdr-setup')).exists()

    h = case('fresh fleet links all existing harnesses; new references have public modes')
    for agent in ('.claude', '.codex', '.config/opencode', '.pi/agent'):
        (h / agent).mkdir(parents=True)
    run(h, '--fleet', '--practices', '--verification')
    for agent in ('.claude', '.codex', '.config/opencode'):
        for skill_name in ('taskr','herdr','herdr-orchestration','herdr-fleet'):
            assert (h / agent / 'skills' / skill_name).is_symlink()
            assert (h / agent / 'skills' / skill_name / 'SKILL.md').is_file()
    assert not (h / '.config/systemd/user/taskr.service').exists()
    assert (h / '.agents/skills/taskr/references').stat().st_mode & 0o777 == 0o755
    assert (h / '.agents/skills/taskr/references/format.md').stat().st_mode & 0o777 == 0o644

    # Check prose pointers after real placement; fenced commands contain example paths.
    # Local Markdown pointers must resolve to files and, when named, heading anchors.
    def prose(path):
        return re.sub(r'^(```|~~~)[^\n]*\n.*?^\1[^\n]*(?:\n|$)', '', path.read_text(), flags=re.M | re.S)

    def heading_slugs(body):
        slugs = set()
        for heading in re.findall(r'^ {0,3}#{1,6}[ \t]+(.+)$', body, re.M):
            heading = re.sub(r'<[^>]*>', '', heading.strip().rstrip('#').rstrip())
            heading = re.sub(r'\[([^]\n]+)\]\([^)]*\)', r'\1', heading)
            base = re.sub(r'[^\w -]', '', heading.lower()).replace(' ', '-')
            slug, suffix = base, 0
            while slug in slugs:
                suffix += 1
                slug = f'{base}-{suffix}'
            slugs.add(slug)
        return slugs

    assert heading_slugs('# Hello, `World`!\n## Repeat\n## Repeat\n') == {'hello-world', 'repeat', 'repeat-1'}

    def pointer_gaps(home):
        skills = home / '.agents/skills'
        taskr = skills / 'taskr'
        paths = [*sorted((skills / 'herdr-orchestration').glob('*.md')),
                 *sorted((skills / 'herdr-practices').rglob('*.md')),
                 *sorted((skills / 'herdr-verification').rglob('*.md')),
                 taskr / 'SKILL.md', *sorted((taskr / 'references').rglob('*.md'))]
        gaps = set()
        checked = {'herdr-orchestration': 0, 'taskr': 0,
                   'herdr-practices': 0, 'herdr-verification': 0}
        for path in paths:
            body = prose(path)
            targets = [s for s in re.findall(r'`([^`\n]+)`', body)
                       if re.fullmatch(r'[\w~./-]+\.md(?:#[\w-]+)?', s)]
            targets += re.findall(r'\[[^\]\n]*\]\(\s*(<[^>\n]+>|[^\s)]+)', body)
            targets += re.findall(r'^\s*\[[^\]\n]+\]:\s*(<[^>\n]+>|\S+)', body, re.M)
            for target in set(targets):
                url = urlsplit(target.strip('<>'))
                if url.scheme or url.netloc or (url.path and not url.path.endswith('.md')) or not (url.path or url.fragment):
                    continue
                checked[path.relative_to(skills).parts[0]] += 1
                relative = unquote(url.path)
                candidates = ([home / relative[2:]] if relative.startswith('~/') else
                              [path.parent / relative, taskr / relative] if relative else [path])
                if not any(dest.is_file() and dest.resolve().is_relative_to(home.resolve()) and
                           (not url.fragment or unquote(url.fragment) in heading_slugs(prose(dest)))
                           for dest in candidates):
                    gaps.add((str(path.relative_to(home)), target))
        return gaps, checked

    known_gaps = {
        ('.agents/skills/herdr-orchestration/SKILL.md', 'local.md'),
        ('.agents/skills/herdr-orchestration/model-selection.md', '~/.agents/skills/herdr-fleet/local.md'),
    }
    gaps, checked = pointer_gaps(h)
    # Current unique local targets: orchestration 34, taskr 24; allow a small margin.
    assert checked['herdr-orchestration'] >= 30 and checked['taskr'] >= 22, ('pointer coverage too low', checked)
    assert checked['herdr-practices'] >= 8 and checked['herdr-verification'] >= 1, ('optional pointer coverage too low', checked)
    assert gaps == known_gaps, ('installed Markdown pointers: unexpected missing targets',
                                sorted(gaps - known_gaps), 'resolved known gaps', sorted(known_gaps - gaps))
    home_alias = h.parent / 'home-alias'
    home_alias.symlink_to(h, target_is_directory=True)
    assert pointer_gaps(home_alias) == (known_gaps, checked), 'symlinked scratch HOME changes pointer resolution'

    recovery = h / '.agents/skills/taskr/references/recovery.md'
    original = recovery.read_text()
    recovery.write_text(original.replace('(recovery.md#agent-hooks)', '(recovery.md#agent-hookz)') +
                        '\n[local hooks](#agent-hooks)\n[broken local hooks](#agent-hookz)\n')
    assert pointer_gaps(h)[0] == {
        ('.agents/skills/herdr-orchestration/SKILL.md', 'local.md'),
        ('.agents/skills/herdr-orchestration/model-selection.md', '~/.agents/skills/herdr-fleet/local.md'),
        ('.agents/skills/taskr/references/recovery.md', 'recovery.md#agent-hookz'),
        ('.agents/skills/taskr/references/recovery.md', '#agent-hookz'),
    }, 'broken file or same-file anchors must fail; valid same-file anchors must pass'
    recovery.write_text(original)
    assert pointer_gaps(h) == (known_gaps, checked)

    h = case('missing kept skill source never replaces an existing loader link')
    write(h / '.config/herdr-setup/keep', '.agents/skills/taskr/SKILL.md\n')
    dest = h / '.codex/skills/taskr'; dest.parent.mkdir(parents=True); dest.symlink_to('missing')
    assert 'MISSING' in run(h, '--fleet')
    assert os.readlink(dest) == 'missing'

    h = case('inside-HOME relocated harness gets absolute loader links')
    (h / 'dotfiles/codex').mkdir(parents=True)
    (h / '.codex').symlink_to('dotfiles/codex')
    run(h, '--fleet', '--keep', '.codex/hooks.json')
    assert os.readlink(h / '.codex/skills/herdr-fleet') == str(h / '.agents/skills/herdr-fleet')

    h = case('client URL UPDATE and keep do not touch a local ledger')
    run(h, '--taskr-client', 'http://100.64.0.1:7788')
    assert 'UPDATE' in run(h, '--taskr-client', 'http://100.64.0.2:7788')
    assert (h / '.local/state/taskr/server.url').read_text() == 'http://100.64.0.2:7788\n'
    assert not (h / '.config/systemd/user/taskr.service').exists()
    write(h / '.local/state/taskr/taskr.db', 'ledger\n')
    run(h, '--taskr-client', 'http://100.64.0.1:7788')
    assert (h / '.local/state/taskr/server.url').read_text() == 'http://100.64.0.2:7788\n'
    assert (h / '.local/state/taskr/taskr.db').read_text() == 'ledger\n'

    for peer_args in (('--taskr-peer',),('--taskr-peer=http://100.64.0.1:7788',),('--taskr-hub','--taskr-peer')):
        h=case('removed peer flags write nothing: '+str(peer_args))
        run(h,*peer_args,success=False)
        assert not (h / '.agents').exists()
    h=case('an edited dashboard address MERGEs and retains its record')
    write(h / '.local/state/taskr/dashboard.addr','custom\n')
    record(h,'.local/state/taskr/dashboard.addr','0'*64)
    out=run(h,'--taskr-hub')
    assert 'MERGE' in out
    assert (h / '.local/state/taskr/dashboard.addr').read_text()=='custom\n'
    assert (h / '.local/state/taskr/dashboard.addr.herdr-setup').read_text()=='tailnet\n'
    assert '0'*64+' .local/state/taskr/dashboard.addr' in (h / '.local/state/herdr-setup/installed').read_text()
    h=case('hub conflict preserves an existing client URL')
    write(h / '.local/state/taskr/dashboard.addr','tailnet\n')
    write(h / '.local/state/taskr/server.url','existing\n')
    run(h,'--taskr-client','http://100.64.0.1:7788')
    assert (h / '.local/state/taskr/server.url').read_text()=='existing\n'
    h=case('a role-less rerun preserves legacy hub URL')
    write(h / '.local/state/taskr/hub.url','legacy\n')
    run(h,'--fleet')
    assert (h / '.local/state/taskr/hub.url').read_text()=='legacy\n'
    assert not (h / '.local/state/taskr/dashboard.addr').exists()
    h=case('a directory at generated Herdr SKILL.md retains its mode')
    dest=h / '.agents/skills/herdr/SKILL.md';dest.mkdir(parents=True);dest.chmod(0o750)
    run(h,'--fleet')
    assert dest.is_dir() and dest.stat().st_mode&0o777==0o750
    for invalid in ('[]','invalid synthetic JSON'):
        h=case('notification merge refuses invalid settings without replacing them')
        write(h / '.claude/settings.json',invalid)
        run(h,'--notifications',success=False)
        assert (h / '.claude/settings.json').read_text()==invalid

    h = case('12698ba host cutover: exact UPDATE set and removed settings/hooks')
    # Synthetic stand-ins except where case 152 requires real 12698ba bytes.
    fixture = setup / 'tests/fixtures/cutover-12698ba'
    def baseline(path):
        return (fixture / path).read_bytes()
    old_files = {
        '.claude/CLAUDE.md':'claude/CLAUDE.md', '.claude/settings.json':'claude/settings.json',
        '.claude/statusline.sh':'claude/statusline.sh', '.codex/AGENTS.md':'codex/AGENTS.md',
        '.codex/config.toml':'codex/config.toml', '.codex/hooks.json':'codex/hooks.json',
        '.local/bin/cdx':'bin/cdx', '.local/bin/cdl':'bin/cdl',
        '.local/bin/herdr-launch':'bin/herdr-launch', '.local/bin/herdr-lead':'bin/herdr-lead',
        '.config/opencode/plugins/taskr-hook.js':'opencode/plugins/taskr-hook.js',
        '.pi/agent/extensions/taskr-hook.ts':'pi/extensions/taskr-hook.ts',
        '.config/systemd/user/taskr.service':'systemd/taskr.service',
        '.agents/skills/herdr-fleet/SKILL.md':'skills/herdr-fleet/SKILL.md',
        '.agents/skills/herdr-orchestration/model-selection.md':'docs/model-selection.md',
    }
    for ref in ('SKILL','lanes','launch','workflow','owner','teardown','recovery'):
        name = ref + '.md'
        old_files['.agents/skills/herdr-orchestration/'+name] = 'skills/herdr-orchestration/'+name
    for dest, source in old_files.items():
        target = h / dest; target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes(baseline('products/setup/'+source)); target.chmod(0o644)
    for reference in sorted((fixture / 'products/taskr/references').rglob('*')):
        if not reference.is_file():
            continue
        path = reference.relative_to(fixture).as_posix()
        dest = '.agents/skills/taskr/'+path.removeprefix('products/taskr/')
        target = h / dest; target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(baseline(path))
    write(h / '.agents/skills/taskr/SKILL.md', baseline('products/taskr/SKILL.md').decode())
    write(h / '.agents/skills/herdr/SKILL.md', 'upstream synthetic herdr skill\n')
    write(h / '.local/state/taskr/dashboard.addr', 'tailnet\n')
    seeded = {str(f.relative_to(h)):f.read_bytes() for f in h.rglob('*') if f.is_file()}
    write(h / '.local/state/herdr-setup/installed', ''.join(f'{hashlib.sha256(data).hexdigest()} {dest}\n' for dest,data in seeded.items()))
    out = run(h, '--fleet')
    updates = set(re.findall(r'^UPDATE '+re.escape(str(h))+r'/(.*?) \(unchanged since last install\)$',out,re.M))
    expected_updates = {
        '.claude/CLAUDE.md','.codex/AGENTS.md',
        '.claude/settings.json','.codex/hooks.json','.local/bin/herdr-launch','.local/bin/herdr-lead',
        '.agents/skills/herdr-fleet/SKILL.md',
    }
    for dest, source in old_files.items():
        if dest.startswith('.agents/skills/herdr-orchestration/') and baseline('products/setup/'+source) != (setup / source).read_bytes():
            expected_updates.add(dest)
    for reference in [fixture / 'products/taskr/SKILL.md', *(fixture / 'products/taskr/references').rglob('*')]:
        path = reference.relative_to(fixture).as_posix()
        source = repo / path
        if reference.is_file() and source.is_file() and reference.read_bytes() != source.read_bytes():
            expected_updates.add('.agents/skills/taskr/'+path.removeprefix('products/taskr/'))
    assert updates == expected_updates, (sorted(updates),sorted(expected_updates))
    lost_keys = {}
    lost_hooks = {}
    for harness,dest in (('claude','.claude/settings.json'),('codex','.codex/hooks.json')):
        before = json.loads(seeded[dest]); after = json.loads((h / dest).read_text())
        lost_keys[harness] = sorted(set(before)-set(after))
        lost_hooks[harness] = {event:sum('moshi-hook' in hook.get('command','') for group in groups for hook in group.get('hooks',[])) for event,groups in before['hooks'].items()}
        lost_hooks[harness] = {event:n for event,n in lost_hooks[harness].items() if n}
        assert 'moshi-hook' not in (h / dest).read_text()
    assert lost_keys == {'claude':['agentPushNotifEnabled','alwaysThinkingEnabled','autoCompactWindow','enabledPlugins','extraKnownMarketplaces','model','statusLine'],'codex':[]}
    assert json.loads((h / '.claude/settings.json').read_text())['permissions']['defaultMode'] == 'bypassPermissions'
    assert lost_hooks == {'claude':{'Notification':1,'PermissionRequest':1,'PostToolUse':2,'PreToolUse':2,'SessionEnd':1,'SessionStart':1,'Stop':1,'UserPromptSubmit':1},'codex':{'PermissionRequest':1,'SessionStart':1,'Stop':1,'UserPromptSubmit':1}}
    for dest in instruction_paths:
        assert (h / dest).read_bytes() == instruction_base, dest
    for dest in ('.codex/config.toml','.claude/statusline.sh','.local/bin/cdx','.local/bin/cdl'):
        assert (h / dest).read_bytes() == seeded[dest], dest
    print('cutover UPDATE: '+json.dumps(sorted(updates)),flush=True)
    print('cutover removed keys: '+json.dumps(lost_keys,sort_keys=True),flush=True)
    print('cutover removed notification hooks: '+json.dumps(lost_hooks,sort_keys=True),flush=True)

    h = case('compatibility entry delegates without a second placer')
    result = subprocess.run([shell, str(setup / 'install.sh'), '--files-only'],
                            env={'HOME': str(h), 'PATH': '/usr/bin:/bin'}, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr
    assert (h / '.agents/skills/herdr-fleet/SKILL.md').is_file()

print(f'PASS: {count} installer cases ({sys.argv[2]})')
PY
