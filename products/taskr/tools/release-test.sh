#!/bin/sh
# Release privacy, packaging and refusal tests, including stable-version guards.
# Usage: sh tools/release-test.sh (needs release.sh's Linux build tools, for its preflight).
# TASKR_RELEASE_SSH and TASKR_RELEASE_GH are fakes: no test reaches a real host or GitHub.
set -eu
cd "$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
t=$(mktemp -d /tmp/taskr-release-test.XXXXXX)
trap 'case "$t" in /tmp/taskr-release-test.??????) rm -rf -- "$t" ;; *) echo "release-test: refusing cleanup of $t" >&2 ;; esac' 0
pass=0
ok() { pass=$((pass + 1)); printf 'ok %s\n' "$1"; }
no() { printf 'FAIL %s\n' "$1"; exit 1; }

# The fake remote fails every call; the fake gh handles release API and publish calls.
printf '#!/bin/sh\necho "fake-ssh: unexpected: $*" >&2\nexit 9\n' > "$t/ssh"
cat > "$t/gh" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_GH_LOG"
case $* in
'api --paginate '*) printf '%s\n' "${FAKE_LATEST#taskr/v}" | sed 's/^v//' ;;
'release edit '*|'release create '*) ;;
*) echo "fake-gh: unexpected: $*" >&2; exit 9 ;;
esac
EOF
printf '#!/bin/sh\necho "fake-cargo: unexpected: $*" >&2\nexit 9\n' > "$t/cargo"
cp "$t/cargo" "$t/git"
cp "$t/cargo" "$t/rustup"
chmod +x "$t/ssh" "$t/gh" "$t/cargo" "$t/git" "$t/rustup"
mkdir -p "$t/home"
export TASKR_RELEASE_SSH="$t/ssh" TASKR_RELEASE_GH="$t/gh" FAKE_GH_LOG="$t/gh.log" FAKE_LATEST=v0.0.0

# A regressed guard must fail before any real build, SSH or GitHub call.
for mode in release dry-run darwin-only; do
	for ssh_value in unset empty whitespace; do
		set -- taskr/v0.17.0
		case $mode in
		dry-run) set -- "$@" --dry-run ;;
		darwin-only) set -- "$@" --darwin-only ;;
		esac
		code=0
		(
			export HOME="$t/home" PATH="$t:$PATH"
			case $ssh_value in
			unset) unset TASKR_RELEASE_SSH ;;
			empty) export TASKR_RELEASE_SSH= ;;
			whitespace) TASKR_RELEASE_SSH=$(printf ' \t\n '); export TASKR_RELEASE_SSH ;;
			esac
			./release.sh "$@"
		) > "$t/out" 2>&1 || code=$?
		[ "$code" -eq 2 ] || { cat "$t/out"; no "$mode with $ssh_value TASKR_RELEASE_SSH: expected exit 2"; }
		grep -Fq "TASKR_RELEASE_SSH is required; example: 'ssh -o BatchMode=yes -o ConnectTimeout=10 user@mac-host'" "$t/out" || { cat "$t/out"; no "$mode: missing SSH guidance"; }
		ok "$mode refuses $ssh_value TASKR_RELEASE_SSH before building"
	done
done

# Run the real build script with fake tools in a disposable workspace.
mkdir -p "$t/rust/products/taskr/tools" "$t/rust-bin"
cp tools/release-rust.sh "$t/rust/products/taskr/tools/"
cat > "$t/rust-bin/cargo" <<'EOF'
#!/bin/sh
set -eu
[ "$RUSTFLAGS" = "$FAKE_RUSTFLAGS" ] || { echo 'fake-cargo: wrong path remapping' >&2; exit 9; }
while [ "$1" != --target ]; do shift; done
target=$2
mkdir -p "$CARGO_TARGET_DIR/$target/release"
for binary in taskr taskr-tui; do
    asset=$CARGO_TARGET_DIR/$target/release/$binary
    printf '#!/bin/sh\nprintf '\''{"version":"%s"}\n'\''\n' "$TASKR_VERSION" > "$asset"
    [ "$binary" != "$FAKE_BINARY" ] || printf '\n# %s\n' "$FAKE_BINARY_TEXT" >> "$asset"
    chmod +x "$asset"
done
EOF
cat > "$t/rust-bin/uname" <<'EOF'
#!/bin/sh
case $1 in
-s) printf '%s\n' "$FAKE_OS" ;;
-m) if [ "$FAKE_OS" = Darwin ]; then echo arm64; else echo x86_64; fi ;;
*) exit 9 ;;
esac
EOF
printf '#!/bin/sh\necho fixture-builder\n' > "$t/rust-bin/id"
chmod +x "$t/rust-bin/"*
for fault in defaults overrides darwin home users login tui-home darwin-users; do
    code=0
    (
        export HOME="$t/home" PATH="$t/rust-bin:$PATH" CARGO_TARGET_DIR="$t/rust-target" TASKR_VERSION=v0.17.0
        unset CARGO_HOME RUSTUP_HOME
        export FAKE_OS=Linux FAKE_BINARY=taskr FAKE_BINARY_TEXT=clean
        case $fault in
        overrides) export CARGO_HOME="$t/cargo-home" RUSTUP_HOME="$t/rustup-home" ;;
        darwin) export FAKE_OS=Darwin ;;
        home|tui-home) export FAKE_BINARY_TEXT=/home/example/.cargo/registry/source.rs ;;
        users|darwin-users) export FAKE_BINARY_TEXT=/Users/example/.cargo/registry/source.rs ;;
        login) export FAKE_BINARY_TEXT=fixture-builder ;;
        esac
        [ "$fault" != tui-home ] || export FAKE_BINARY=taskr-tui
        [ "$fault" != darwin-users ] || export FAKE_OS=Darwin
        export RUSTFLAGS='--cfg caller_flag'
        export FAKE_RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup --remap-path-prefix=$t/rust=/herdr-kit"
        sh "$t/rust/products/taskr/tools/release-rust.sh"
    ) > "$t/out" 2>&1 || code=$?
    case $fault in
    defaults|overrides|darwin)
        [ "$code" -eq 0 ] || { cat "$t/out"; no "release-rust $fault"; }
        ok "release-rust preserves caller flags and remaps paths: $fault" ;;
    *)
        [ "$code" -eq 1 ] && grep -q 'release-rust: builder path or login name found' "$t/out" || { cat "$t/out"; no "release-rust $fault: wrong refusal"; }
        ok "release-rust refuses builder data: $fault" ;;
    esac
done

# A Mach-O-headed stub for one arch (cputype bytes as octal) carrying the given text.
macho() {
	# shellcheck disable=SC2059 # $2 is octal escapes for the format
	{ printf '\317\372\355\376'"$2"'\002\000\000\000'; printf '\000%.0s' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; printf '\n%s\n' "$3"; } > "$1"
}
pair() { # DIR ARM64_TEXT
	mkdir -p "$1"
	macho "$1/taskr-rust-aarch64-apple-darwin" '\014\000\000\001\000\000\000\000' "$2"
	macho "$1/taskr-rust-x86_64-apple-darwin" '\007\000\000\001\003\000\000\000' 'stub v0.17.0'
	macho "$1/taskr-tui-rust-aarch64-apple-darwin" '\014\000\000\001\000\000\000\000' 'tui stub'
	macho "$1/taskr-tui-rust-x86_64-apple-darwin" '\007\000\000\001\003\000\000\000' 'tui stub'
	(cd "$1" && sha256sum taskr-rust-* taskr-tui-rust-* > SHA256SUMS)
}

# A checksummed imported pair with a private path or test hook is refused before the gate.
for marker in TASKR_FROZEN_NOW TASKR_CONTRACT_FROZEN_NOW --contract-seed /home/example/source.rs /Users/example/source.rs; do
	pair "$t/import" "stub v0.17.0 $marker"
	case $marker in /home/*|/Users/*) want='contains a builder path' ;; *) want='contains a test-only hook' ;; esac
	if ./release.sh v0.17.0 --dry-run --darwin-from "$t/import" > "$t/out" 2>&1; then
		no "imported pair with $marker accepted"
	fi
	grep -q "$want" "$t/out" || { cat "$t/out"; no "imported pair with $marker: wrong refusal"; }
	ok "refuses an imported pair with $marker"
done

# The imported TUI needs its own checksum, architecture and hook checks.
for fault in checksum architecture hook builder-path; do
	pair "$t/tui-$fault" 'stub v0.17.0'
	asset="$t/tui-$fault/taskr-tui-rust-aarch64-apple-darwin"
	case $fault in
	checksum) printf corrupt >> "$asset"; want='does not match' ;;
	architecture) macho "$asset" '\007\000\000\001\003\000\000\000' 'tui stub'; want='is not' ;;
	hook) printf '\nTASKR_CONTRACT_BAD\n' >> "$asset"; want='contains a test-only hook' ;;
	builder-path) printf '\n/home/example/source.rs\n' >> "$asset"; want='contains a builder path' ;;
	esac
	if [ "$fault" != checksum ]; then
		(cd "$t/tui-$fault" && sha256sum taskr-rust-* taskr-tui-rust-* > SHA256SUMS)
	fi
	if ./release.sh v0.17.0 --dry-run --darwin-from "$t/tui-$fault" > "$t/out" 2>&1; then
		no "TUI $fault accepted"
	fi
	grep -q "$want" "$t/out" || { cat "$t/out"; no "TUI $fault: wrong refusal"; }
	ok "refuses TUI $fault"
done

# --promote reads the latest release and refuses an older tag.
promote() { # INPUT LATEST WANT(edit|refuse) [EXPECTED_TAG]
    expected_tag=${4:-$1}
	: > "$t/gh.log"
	if TASKR_RELEASE_SSH= FAKE_LATEST=$2 ./release.sh "$1" --promote > "$t/out" 2>&1; then got=0; else got=$?; fi
	if [ "$3" = edit ]; then
		if [ "$got" -ne 0 ] || ! grep -qx "release edit $expected_tag --repo olafurns7/herdr-kit --prerelease=false --latest=false" "$t/gh.log"; then
			cat "$t/out" "$t/gh.log"
			no "promote $1 over $2 should edit"
		fi
	elif [ "$got" -eq 0 ] || grep -q 'release edit' "$t/gh.log" || ! grep -q 'older than the latest taskr release' "$t/out"; then
		cat "$t/out" "$t/gh.log"
		no "promote $1 over $2 should refuse"
	fi
	ok "promote $1 over latest $2: $3"
}
promote taskr/v0.16.0 taskr/v0.16.1 refuse
promote taskr/v0.9.9 taskr/v0.16.1 refuse
promote taskr/v0.17.0 taskr/v0.16.1 edit
promote taskr/v0.16.10 taskr/v0.16.9 edit
promote taskr/v0.16.1 taskr/v0.16.1 edit
promote v0.17.0 taskr/v0.16.1 edit taskr/v0.17.0
# Exercise complete packaging and both darwin modes in a disposable fake tree.
mkdir -p "$t/full/tools" "$t/full/plugin" "$t/full/references" "$t/fake-bin" "$t/home"
cp release.sh install.sh SKILL.md "$t/full/"
cp plugin/run.sh plugin/herdr-plugin.toml "$t/full/plugin/"
cp references/format.md "$t/full/references/"
pair "$t/mac" 'stub v0.17.0'
cat > "$t/fake-bin/cargo" <<'EOF'
#!/bin/sh
exit 0
EOF
cp "$t/fake-bin/cargo" "$t/fake-bin/zig"
cp "$t/fake-bin/cargo" "$t/fake-bin/cargo-zigbuild"
cat > "$t/fake-bin/git" <<'EOF'
#!/bin/sh
# A Mac build needs the root workspace, not git archive's current subdirectory.
printf '%s\n' "$*" >> "$FAKE_GIT_LOG"
case $* in
'-C ../.. archive --format=tar HEAD'|'status --porcelain'|'fetch -q origin main'|'merge-base --is-ancestor HEAD origin/main'|'tag -a '*|'push origin '*) ;;
'rev-parse -q --verify refs/tags/'*) exit 1 ;;
*) exit 9 ;;
esac
EOF
cat > "$t/fake-bin/rustup" <<'EOF'
#!/bin/sh
printf '%s\n' x86_64-unknown-linux-musl aarch64-unknown-linux-musl
EOF
cat > "$t/fake-bin/file" <<'EOF'
#!/bin/sh
printf '%s\n' "$2" >> "$FAKE_FILE_LOG"
case $2 in
*linux-amd64)
  printf '%s\n' 'ELF 64-bit LSB executable, x86-64, statically linked' ;;
*linux-arm64)
  link='statically linked'
  if [ "${FAKE_DYNAMIC_TUI:-}" = 1 ] && [ "${2##*/}" = taskr-tui-linux-arm64 ]; then link='dynamically linked'; fi
  printf 'ELF 64-bit LSB executable, ARM aarch64, %s\n' "$link" ;;
*) exec /usr/bin/file "$@" ;;
esac
EOF
cat > "$t/fake-bin/remote" <<'EOF'
#!/bin/sh
case $1 in
'mktemp -d '*) printf '%s\n' /tmp/taskr-release.ABC123 ;;
'cat '*) cat "$FAKE_MAC/${1##*/}" ;;
'cd '*|'rm -rf '*|true) ;;
' tar '*|'tar '*) cat >/dev/null ;;
*) printf 'unexpected remote: %s\n' "$*" >&2; exit 9 ;;
esac
EOF
cat > "$t/full/tools/release-rust.sh" <<'EOF'
#!/bin/sh
set -eu
mkdir -p dist-rust
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  printf '#!/bin/sh\nprintf '\''{"version":"%s"}\n'\''\n' "$TASKR_VERSION" > "dist-rust/taskr-rust-$target"
  printf '#!/bin/sh\nexit 0\n' > "dist-rust/taskr-tui-rust-$target"
done
EOF
chmod +x "$t/fake-bin/"*
export FAKE_MAC="$t/mac" FAKE_FILE_LOG="$t/file.log" FAKE_GIT_LOG="$t/git.log"
: > "$t/gh.log"
: > "$t/git.log"
if (cd "$t/full" && HOME="$t/home" PATH="$t/fake-bin:$PATH" FAKE_LATEST=taskr/v0.24.0 ./release.sh taskr/v0.17.0 --darwin-from "$t/mac") > "$t/out" 2>&1; then
	no 'publishing an older stable release accepted'
fi
grep -q 'older than the latest taskr release v0.24.0' "$t/out" || { cat "$t/out"; no 'older stable publish: wrong refusal'; }
if grep -Eq '^release create ' "$t/gh.log" || grep -Eq '^tag -a |^push origin ' "$t/git.log"; then
	no 'older stable release tagged, pushed or published'
fi
ok 'refuses to publish an older stable release'

for mode in imported remote darwin-only publish; do
	: > "$t/file.log"
    : > "$t/git.log"
	set -- --dry-run
	release_remote=$t/fake-bin/remote
	case $mode in
	imported) set -- "$@" --darwin-from "$t/mac"; release_remote= ;;
	darwin-only) set -- "$@" --darwin-only ;;
	publish) set -- --darwin-from "$t/mac"; release_remote=; : > "$t/gh.log" ;;
	esac
	if ! (cd "$t/full" && HOME="$t/home" PATH="$t/fake-bin:$PATH" FAKE_LATEST=taskr/v0.16.1 TASKR_RELEASE_SSH="$release_remote" ./release.sh taskr/v0.17.0 "$@") > "$t/out" 2>&1; then
		cat "$t/out"; no "packaging $mode"
	fi
	if [ "$mode" = publish ]; then
		grep -q '^release create taskr/v0.17.0 ' "$t/gh.log" || no 'stable release was not created'
		if grep -Eq -- '--prerelease|24 h|24-hour' "$t/gh.log" "$t/out"; then
			no 'new release uses prerelease or watch flow'
		fi
		ok 'publishes a newer stable release immediately'
	fi
    if [ "$mode" = remote ] || [ "$mode" = darwin-only ]; then
        grep -qx -- '-C ../.. archive --format=tar HEAD' "$t/git.log" || no 'Mac archive omitted the root workspace'
    fi
	if [ "$mode" = darwin-only ]; then
		(cd "$t/full/dist-darwin" && sha256sum -c SHA256SUMS) > "$t/out" 2>&1 || { cat "$t/out"; no 'darwin-only checksums'; }
		[ "$(wc -l < "$t/full/dist-darwin/SHA256SUMS")" -eq 4 ] || no 'darwin-only asset count'
	else
		(cd "$t/full/dist" && sha256sum -c SHA256SUMS) > "$t/out" 2>&1 || { cat "$t/out"; no "$mode checksums"; }
		[ "$(wc -l < "$t/full/dist/SHA256SUMS")" -eq 12 ] || no 'release checksum count'
        tar -xOf "$t/full/dist/plugin.tar.gz" herdr-plugin.toml | grep -qx 'version = "0.17.0"' || no 'plugin version includes tag prefix'
        for archive in skill plugin; do
            tar -tvzf "$t/full/dist/$archive.tar.gz" > "$t/archive-list"
            awk '$2 != "0/0" { exit 1 }' "$t/archive-list" || no "$archive archive owner is not 0/0"
        done
        "$t/full/dist/taskr-linux-amd64" --json version | grep -qx '{"version":"v0.17.0"}' || no 'binary version includes tag prefix'
		for binary in taskr taskr-tui; do
			for platform in linux-amd64 linux-arm64 darwin-amd64 darwin-arm64; do
				[ -x "$t/full/dist/$binary-$platform" ] || no "missing executable $binary-$platform"
				grep -qx "dist/$binary-$platform" "$t/file.log" || no "missing final file check $binary-$platform"
			done
		done
	fi
	ok "packaging $mode includes checksummed TUI assets"
done
if (cd "$t/full" && HOME="$t/home" PATH="$t/fake-bin:$PATH" FAKE_DYNAMIC_TUI=1 ./release.sh v0.17.0 --dry-run --darwin-from "$t/mac") > "$t/out" 2>&1; then
	no 'dynamic Linux TUI accepted'
fi
grep -q 'taskr-tui-linux-arm64 is not' "$t/out" || { cat "$t/out"; no 'dynamic Linux TUI: wrong refusal'; }
ok 'refuses dynamically linked Linux TUI'
printf 'release-test: all %s passed\n' "$pass"
