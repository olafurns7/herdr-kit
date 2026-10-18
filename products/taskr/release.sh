#!/bin/sh
# Build the Rust taskr release (Linux here, darwin on the Mac over ssh) and
# publish it as a stable GitHub release. See docs/release.md.
set -eu
# Tmp permission fixtures require a deterministic, non-group-writable default.
umask 022

repo=olafurns7/herdr-kit
# The darwin half's remote command; tests point it at a fake, never at the Mac.
remote=${TASKR_RELEASE_SSH:-}
# The GitHub CLI; tests point it at a fake, never at the real repository.
gh=${TASKR_RELEASE_GH:-gh}
work=.scratch/release

usage() {
	printf '%s\n' 'usage: ./release.sh <tag> [--dry-run] [--darwin-from DIR]' \
		'       ./release.sh <tag> --darwin-only [--dry-run]' \
		'       ./release.sh <tag> --promote' >&2
	exit 2
}

die() {
	printf 'release.sh: %s\n' "$*" >&2
	exit 1
}

# Run one command on the Mac through $remote (word-split on purpose).
on_mac() {
	# shellcheck disable=SC2086
	$remote "$@"
}

[ "$#" -ge 1 ] || usage
tag=$1
shift
mode=release
dry_run=0
darwin_from=
while [ "$#" -gt 0 ]; do
	case $1 in
	--dry-run) dry_run=1 ;;
	--darwin-from) [ "$#" -ge 2 ] || usage; darwin_from=$2; shift ;;
	--darwin-only) [ "$mode" = release ] || usage; mode=darwin ;;
	--promote) [ "$mode" = release ] || usage; mode=promote ;;
	*) usage ;;
	esac
	shift
done
[ "$mode" != promote ] || { [ "$dry_run" -eq 0 ] && [ -z "$darwin_from" ]; } || usage
[ "$mode" != darwin ] || [ -z "$darwin_from" ] || usage
# Repository tags and runtime/plugin versions are distinct.
version=${tag#taskr/}
printf '%s\n' "$version" | grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || usage
case $tag in taskr/v*|v*) ;; *) usage ;; esac
tag=taskr/v${version#v}

if [ "$mode" != promote ] && [ -z "$darwin_from" ]; then
	case $remote in
	*[![:space:]]*) ;;
	*)
		printf '%s\n' "release.sh: TASKR_RELEASE_SSH is required; example: 'ssh -o BatchMode=yes -o ConnectTimeout=10 user@mac-host'" >&2
		exit 2
		;;
	esac
fi

cd "$(dirname -- "$0")"
export PATH="$HOME/.cargo/bin:$HOME/.cache/taskr-tools/cargo/bin:$HOME/.cache/taskr-tools/bin:$PATH"

check_release_version() {
    # Compare stable taskr releases only; other products share this repo.
    filter='.[] | select(.draft == false and .prerelease == false) | .tag_name | select(test("^taskr/v(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$")) | ltrimstr("taskr/v")'
    versions=$("$gh" api --paginate "repos/$repo/releases?per_page=100" --jq "$filter") || die 'could not list taskr releases'
    newest=$(printf '%s\n%s\n' "$versions" "${version#v}" | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)
    [ "$newest" = "${version#v}" ] || die "refusing to $mode $tag: it is older than the latest taskr release v$newest"
}

if [ "$mode" = promote ]; then
    check_release_version
    "$gh" release edit "$tag" --repo "$repo" --prerelease=false --latest=false
	exit 0
fi

# --- Preflight ---------------------------------------------------------------

tools='git file strings sha256sum'
[ -n "$darwin_from" ] || tools="$tools ${remote%% *}"
if [ "$mode" = release ]; then
	[ "$(uname -s)" = Linux ] || die 'the release builds on Linux; the darwin half runs on the Mac over ssh'
	tools="$tools cargo rustup zig cargo-zigbuild python3 tar sed awk"
	[ "$dry_run" -eq 1 ] || tools="$tools $gh"
fi
missing=
for tool in $tools; do
	command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
if [ "$mode" = release ] && command -v rustup >/dev/null 2>&1; then
	installed=$(rustup target list --installed)
	for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
		printf '%s\n' "$installed" | grep -qx "$target" || missing="$missing rustup-target:$target"
	done
fi
if [ -n "$missing" ]; then
	{
		printf 'release.sh: missing:%s\n\n' "$missing"
		printf '%s\n\n' 'Install the build tools in user space (tools/contract/README.md):'
		awk '/^## Static musl builds/ { s = 1 } s && /^```/ { if (p) exit; p = 1; next } p' tools/contract/README.md
		printf '\n%s\n%s\n' 'rust-toolchain.toml pins 1.99.0, so add the targets to that toolchain:' \
			'rustup target add --toolchain 1.99.0 x86_64-unknown-linux-musl aarch64-unknown-linux-musl'
	} >&2
	exit 1
fi

if [ "$dry_run" -eq 0 ]; then
	dirty=$(git status --porcelain) || die 'could not inspect git status'
	[ -z "$dirty" ] || die 'refusing to release with a dirty worktree'
fi
if [ "$mode" = release ] && [ "$dry_run" -eq 0 ]; then
	! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "tag $tag already exists"
	check_release_version
	git fetch -q origin main
	git merge-base --is-ancestor HEAD origin/main || die 'HEAD is not in origin/main; push it to main first'
fi
if [ -z "$darwin_from" ]; then
	on_mac true || die "cannot reach the Mac ($remote); build darwin by hand and pass --darwin-from DIR"
fi

rm -rf .scratch/release
mkdir -p .scratch/release/home

# file(1) wording, the tag compiled into the binary, and no test-only hook
# (the same strings tools/release-rust.sh refuses).
static_check() {
	file -b "$1" | grep -E -q "$2" || die "$1 is not $2: $(file -b "$1")"
	# taskr-tui has no embedded release version (its package version is 0.0.0).
	case ${1##*/} in
	taskr-tui-*) ;;
	*) strings -a "$1" | grep -F -q "$version" || die "$1 does not contain $version" ;;
	esac
	! strings -a "$1" | grep -E -q 'TASKR_FROZEN_NOW|TASKR_CONTRACT_|--contract-' || die "$1 contains a test-only hook"
	! strings -a "$1" | grep -E -q '/home/|/Users/' || die "$1 contains a builder path"
}

# The JSON version of a binary that runs here, in an empty scratch home.
run_version() {
	out=$(env -i HOME="$work/home" TASKR_DB="$work/home/version.db" PATH=/usr/bin:/bin "$@" --json version) ||
		die "$* --json version failed"
	got=$(printf '%s\n' "$out" | python3 -c 'import json, sys; print(json.load(sys.stdin)["version"])')
	[ "$got" = "$version" ] || die "$* reports version $got, wanted $version"
	printf '%s: %s\n' "$*" "$out"
}

# --- Darwin half: the Mac builds HEAD; we keep only checksummed copies ---------

darwin_build() {
	rm -rf dist-darwin
	mkdir dist-darwin
	macdir=$(on_mac 'mktemp -d /tmp/taskr-release.XXXXXX')
	printf '%s\n' "$macdir" | grep -E -qx '/tmp/taskr-release\.[A-Za-z0-9]{6}' || die "unexpected Mac scratch dir: $macdir"
	trap 'on_mac "rm -rf -- $macdir"' 0
	git -C ../.. archive --format=tar HEAD | on_mac "tar -xf - -C $macdir"
	# release-rust.sh smokes the native arm64 binary there (version, help, status).
	on_mac "cd $macdir/products/taskr && TASKR_VERSION=$version sh tools/release-rust.sh"
	for name in SHA256SUMS taskr-rust-aarch64-apple-darwin taskr-rust-x86_64-apple-darwin taskr-tui-rust-aarch64-apple-darwin taskr-tui-rust-x86_64-apple-darwin; do
		on_mac "cat $macdir/products/taskr/dist-rust/$name" > "dist-darwin/$name"
	done
	on_mac "rm -rf -- $macdir"
	trap - 0
}

# All darwin files must match the Mac's SHA256SUMS and be the right Mach-O.
darwin_check() {
	for pair in aarch64:arm64 x86_64:x86_64; do
		for binary in taskr taskr-tui; do
			name=$binary-rust-${pair%:*}-apple-darwin
			want=$(awk -v n="$name" '$2 == n || $2 == "*" n { print $1 }' "$1/SHA256SUMS")
			got=$(sha256sum < "$1/$name")
			[ -n "$want" ] && [ "$want" = "${got%% *}" ] || die "$1/$name does not match $1/SHA256SUMS"
			static_check "$1/$name" "^Mach-O 64-bit .*${pair#*:}"
		done
	done
}

# A hand-built pair is checked now, before the long gate.
[ -z "$darwin_from" ] || darwin_check "$darwin_from"

if [ "$mode" = darwin ]; then
	darwin_build
	darwin_check dist-darwin
	cat dist-darwin/SHA256SUMS
	printf '%s\n' "release.sh: dist-darwin is ready: ./release.sh $tag --darwin-from dist-darwin"
	exit 0
fi

# --- Gate (decision 1A, the interim gate) --------------------------------------

cargo fmt --all --check
(
	# Tests get a scratch HOME and ledger and no task identity; the build caches stay put.
	export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
	export HOME="$PWD/$work/home" TASKR_DB="$PWD/$work/home/gate.db"
	unset TASKR_TASK TASKR_LAUNCH TASKR_BIN TASKR_FROZEN_NOW
	# Clear inherited contract fixture switches before running the gate.
	for var in $(env | sed -n 's/^\(TASKR_CONTRACT_[A-Za-z0-9_]*\)=.*/\1/p'); do
		unset "$var"
	done
	for features in '' '--features taskr/contract'; do
		# shellcheck disable=SC2086 # $features is empty or two words
		cargo clippy --locked --workspace --all-targets $features -- -D warnings
		# shellcheck disable=SC2086
		cargo test --locked --workspace $features
	done
)

# --- Build ---------------------------------------------------------------------

# Linux half: static musl, the contract-hook check and the native version smoke.
TASKR_VERSION=$version sh tools/release-rust.sh
if [ -z "$darwin_from" ]; then
	darwin_build
	darwin_from=dist-darwin
	darwin_check dist-darwin
fi

rm -rf dist && mkdir dist
for binary in taskr taskr-tui; do
	cp "dist-rust/$binary-rust-x86_64-unknown-linux-musl" "dist/$binary-linux-amd64"
	cp "dist-rust/$binary-rust-aarch64-unknown-linux-musl" "dist/$binary-linux-arm64"
	cp "$darwin_from/$binary-rust-aarch64-apple-darwin" "dist/$binary-darwin-arm64"
	cp "$darwin_from/$binary-rust-x86_64-apple-darwin" "dist/$binary-darwin-amd64"
done
chmod 755 dist/taskr-*

run_version dist/taskr-linux-amd64
if command -v qemu-aarch64 >/dev/null 2>&1; then
	run_version qemu-aarch64 dist/taskr-linux-arm64
else
	printf '%s\n' 'release.sh: no qemu-aarch64: linux/arm64 has the static check only'
fi
# darwin/arm64 ran on the Mac (release-rust.sh); darwin/amd64 has the static check only (decision 2A).

cp SKILL.md install.sh dist/
tar --owner=0 --group=0 --numeric-owner --sort=name -czf dist/skill.tar.gz SKILL.md references
# plugin.tar.gz holds plugin/ flat, with the manifest version set from the tag.
mkdir "$work/plugin"
cp plugin/run.sh "$work/plugin/run.sh"
sed "s/^version = \".*\"$/version = \"${version#v}\"/" plugin/herdr-plugin.toml > "$work/plugin/herdr-plugin.toml"
grep -q "^version = \"${version#v}\"$" "$work/plugin/herdr-plugin.toml" || die 'could not set the plugin version'
tar --owner=0 --group=0 --numeric-owner --sort=name -czf dist/plugin.tar.gz -C "$work/plugin" run.sh herdr-plugin.toml
# Every shipped binary, imported or built here, gets the same final check.
for binary in taskr taskr-tui; do
	static_check "dist/$binary-linux-amd64" '^ELF 64-bit LSB .*x86-64.*statically linked'
	static_check "dist/$binary-linux-arm64" '^ELF 64-bit LSB .*ARM aarch64.*statically linked'
	static_check "dist/$binary-darwin-arm64" '^Mach-O 64-bit .*arm64'
	static_check "dist/$binary-darwin-amd64" '^Mach-O 64-bit .*x86_64'
done
(
	cd dist
	sha256sum taskr-darwin-arm64 taskr-darwin-amd64 taskr-linux-amd64 taskr-linux-arm64 taskr-tui-darwin-arm64 taskr-tui-darwin-amd64 taskr-tui-linux-amd64 taskr-tui-linux-arm64 SKILL.md install.sh skill.tar.gz plugin.tar.gz > SHA256SUMS
	cat SHA256SUMS
)

if [ "$dry_run" -eq 1 ]; then
	printf '%s\n' "release.sh: dry run: dist/ is ready; nothing tagged, pushed or uploaded"
	exit 0
fi

# --- Publish ------------------------------------------------------------------

git tag -a "$tag" -m "$tag"
git push origin "$tag"
"$gh" release create "$tag" dist/* --repo "$repo" --title "$tag" --notes "Prebuilt taskr binaries for macOS and Linux (arm64 and amd64), checksums, install script, skill, and plugin. See https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/install.md for agent-guided installation."
