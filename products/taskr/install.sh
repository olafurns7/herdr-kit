#!/bin/sh
set -eu

REPO=olafurns7/herdr-kit

fail() {
	printf 'install.sh: %s\n' "$*" >&2
	exit 1
}

manual_herdr_sidebar() {
	printf '%s\n' 'MANUAL: find the herdr-radar settings directory: herdr plugin config-dir hhdebb.herdr-radar'
	printf '%s\n' 'MANUAL: add missing top-level keys to its config.toml; preserve existing values:' \
		'parent_token = "taskr_parent"' 'parent_label_token = "taskr_campaign"' \
		'attention_token = "taskr_owner_ask"' 'space_owner = false'
}

# Isolate fail's exit and clean staging files so sidebar refusal cannot abort installation.
configure_herdr_sidebar() (
	install_tmp=
	trap 'if [ -n "$install_tmp" ]; then rm -f "$install_tmp"; fi' 0
	if [ "${TASKR_HERDR_CONFIG:-}" != 1 ] || [ "${HERDR_ENV:-}" != 1 ] ||
		! radar_dir=$(herdr plugin config-dir hhdebb.herdr-radar 2>/dev/null) || [ -z "$radar_dir" ]; then
		manual_herdr_sidebar
		return
	fi
	mkdir -p "$radar_dir" || fail 'could not create herdr-radar settings directory'
	radar_config=$radar_dir/config.toml
	[ ! -L "$radar_config" ] || fail 'herdr-radar config.toml is a symlink; configure it manually'
	if [ -e "$radar_config" ] && [ ! -f "$radar_config" ]; then
		fail 'herdr-radar config.toml is not a regular file'
	fi
	install_tmp=$(mktemp "$radar_dir/.taskr-config.XXXXXX") || fail 'could not stage herdr-radar settings'
	radar_input=/dev/null
	if [ -f "$radar_config" ]; then
		radar_input=$radar_config
		cp -p "$radar_config" "$install_tmp" || fail 'could not preserve herdr-radar settings'
	fi
	# Missing root keys precede tables; leave complex TOML for manual setup.
	awk '
		function missing( i) {
			for (i = 1; i <= 4; i++) if (!seen[key[i]]) print key[i] " = " value[i]
		}
		BEGIN {
			key[1] = "parent_token"; value[1] = "\"taskr_parent\""
			key[2] = "parent_label_token"; value[2] = "\"taskr_campaign\""
			key[3] = "attention_token"; value[3] = "\"taskr_owner_ask\""
			key[4] = "space_owner"; value[4] = "false"
		}
		/"""|\047\047\047/ { exit 1 }
		!table && /=[[:space:]]*\[/ && !/\][[:space:]]*(#.*)?$/ { exit 1 }
		!table && /^[[:space:]]*\[/ { missing(); table = 1 }
		!table && /^[[:space:]]*[^#[:space:]][^=]*=/ {
			name = $0
			sub(/=.*/, "", name)
			gsub(/^[[:space:]]+|[[:space:]]+$/, "", name)
			if (name ~ /^[^"\047]*\./) exit 1
			if (name ~ /\\/) exit 1
			gsub(/^["\047]|["\047]$/, "", name)
			seen[name] = 1
		}
		{ print }
		END { if (!table) missing() }
	' "$radar_input" > "$install_tmp" || fail 'could not stage herdr-radar settings; configure complex TOML manually'
	if [ -f "$radar_config" ] && cmp -s "$radar_config" "$install_tmp"; then
		rm -f "$install_tmp"
	else
		mv -f "$install_tmp" "$radar_config" || fail 'could not save herdr-radar settings'
	fi
	install_tmp=
	printf 'sidebar: herdr-radar settings: %s\n' "$radar_config"
)

taskr_bin() {
    if [ -x "$DIR/taskr" ]; then
        printf '%s\n' "$DIR/taskr"
    elif command -v taskr >/dev/null 2>&1; then
        command -v taskr
    fi
}

# Prints the installed taskr version without a leading v, keeping any
# pre-release or build suffix (0.8.0-capacity.0a5a6cf). Prints nothing when
# taskr is missing or its version cannot be read.
taskr_installed_version() {
    installed_bin=$(taskr_bin)
    if [ -z "$installed_bin" ]; then
        return 0
    fi
    version_output=$("$installed_bin" version 2>/dev/null || true)
    # Current releases print compact `j1 {...}`; older ones print plain JSON.
    installed_version=$(printf '%s\n' "${version_output#j1 }" | jq -r '.version // empty' 2>/dev/null || true)
    if [ -z "$installed_version" ]; then
        installed_version=$(printf '%s\n' "$version_output" \
            | grep -o -E 'v?[0-9]+\.[0-9]+\.[0-9]+[-+0-9A-Za-z.]*' | head -n 1 || true)
    fi
    printf '%s\n' "${installed_version#v}"
}

# Succeeds when $1 is a plain release version: three dot-separated numbers.
taskr_plain_version() {
    case $1 in
        '' | *[!0-9.]* | .* | *. | *..* | *.*.*.*) return 1 ;;
        *.*.*) return 0 ;;
    esac
    return 1
}

# Compares an installed taskr version ($1) with the pinned one ($2), both
# without a leading v, and prints same, older, newer, or local. Any other
# version with a pre-release or build suffix is a local build.
taskr_version_relation() {
    if [ "$1" = "$2" ]; then
        printf '%s\n' same
        return 0
    fi
    if ! taskr_plain_version "$1"; then
        printf '%s\n' local
        return 0
    fi
    if ! taskr_plain_version "$2"; then
        printf '%s\n' older
        return 0
    fi
    cur_major=${1%%.*}
    cur_rest=${1#*.}
    cur_minor=${cur_rest%%.*}
    cur_patch=${cur_rest#*.}
    pin_major=${2%%.*}
    pin_rest=${2#*.}
    pin_minor=${pin_rest%%.*}
    pin_patch=${pin_rest#*.}
    for pair in "$cur_major $pin_major" "$cur_minor $pin_minor" "$cur_patch $pin_patch"; do
        set -- $pair
        if [ "$1" -gt "$2" ]; then
            printf '%s\n' newer
            return 0
        fi
        if [ "$1" -lt "$2" ]; then
            printf '%s\n' older
            return 0
        fi
    done
    printf '%s\n' same
}

# Only regular files and directories with relative, non-escaping paths.
validate_archive() {
    tar -tzf "$1" >"$tmp/archive.names" || fail 'could not list archive'
    tar -tvzf "$1" >"$tmp/archive.types" || fail 'could not inspect archive'
    if grep -v '^[d-]' "$tmp/archive.types" >/dev/null; then fail 'archive contains a link or special file'; fi
    while IFS= read -r member; do
        case $member in -* | .. | /* | ../* | */../* | */.. | *[[:cntrl:]]*) fail 'unsafe archive path' ;; esac
    done <"$tmp/archive.names"
    [ "$(sort "$tmp/archive.names" | uniq -d | wc -l)" -eq 0 ] || fail 'duplicate archive path'
}

public_assets='
products/setup/lib/placement.sh
products/setup/lib/fleet.sh
products/setup/lib/radar.sh
products/setup/lib/notifications.sh
products/setup/bin/cdx
products/setup/bin/cdl
products/setup/claude/notifications.json
products/setup/codex/notifications.json
products/setup/bin/herdr-launch
products/setup/bin/herdr-codex-identity
products/setup/bin/herdr-loop
products/setup/bin/herdr-worktrees
products/setup/bin/herdr-lead
products/setup/bin/herdr-hub
products/setup/bin/herdr-lanes.sh
products/setup/lanes.json
products/setup/intake.example.yaml
products/setup/docs/model-selection.md
products/setup/claude/CLAUDE.md
products/setup/claude/settings.json
products/setup/codex/AGENTS.md
products/setup/codex/hooks.json
products/setup/systemd/taskr.service
products/setup/opencode/plugins/taskr-hook.js
products/setup/pi/extensions/taskr-hook.ts
products/setup/skills/herdr-fleet/SKILL.md
products/setup/skills/herdr-orchestration/SKILL.md
products/setup/skills/herdr-orchestration/lanes.md
products/setup/skills/herdr-orchestration/launch.md
products/setup/skills/herdr-orchestration/owner.md
products/setup/skills/herdr-orchestration/recovery.md
products/setup/skills/herdr-orchestration/teardown.md
products/setup/skills/herdr-orchestration/workflow.md
products/setup/skills/herdr-practices/SKILL.md
products/setup/skills/herdr-practices/references/design.md
products/setup/skills/herdr-practices/references/incident-intake.md
products/setup/skills/herdr-practices/references/measurement.md
products/setup/skills/herdr-practices/references/research.md
products/setup/skills/herdr-practices/references/retrospective.md
products/setup/skills/herdr-practices/references/review.md
products/setup/skills/herdr-practices/references/runbooks.md
products/setup/skills/herdr-practices/references/verify-existing-fix.md
products/setup/skills/herdr-practices/references/writing.md
products/setup/skills/herdr-verification/SKILL.md
products/setup/skills/herdr-verification/references/feature-map.md
products/setup/skills/herdr-verification/references/task-proof.md
third_party/pstack/LICENSE
third_party/pstack/PROVENANCE.md
'

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
fleet=0 files_only=0 practices=0 verification=0 slotr=0 radar=0 codator=0 notifications=0
keep_args= taskr_role= taskr_server_url= next_steps=
home_dir=${HOME:?HOME must be set}
home_physical=$(CDPATH= cd "$home_dir" && pwd -P)
setup_dir=$script_dir/../setup
usage() {
    printf '%s\n' 'Usage: install.sh [--fleet] [--practices] [--verification] [--slotr] [--radar] [--codator] [--notifications] [--files-only] [--keep PATH] [--taskr-hub | --taskr-client http://HOST:PORT]'
}
# Accept only http://HOST:PORT with an optional trailing slash; taskr itself
# checks that HOST belongs to the tailnet.
valid_hub_url() {
    hub_rest=${1#http://}
    if [ "$hub_rest" = "$1" ]; then
        return 1
    fi
    case $hub_rest in
        '' | *[!A-Za-z0-9.:/-]*) return 1 ;;
    esac
    hub_host=${hub_rest%%:*}
    hub_port=${hub_rest#*:}
    if [ "$hub_host" = "$hub_rest" ]; then
        return 1
    fi
    hub_port=${hub_port%/}
    case $hub_host in
        '' | *[!A-Za-z0-9.-]*) return 1 ;;
    esac
    case $hub_port in
        '' | *[!0-9]*) return 1 ;;
    esac
    [ "${#hub_port}" -le 5 ] && [ "$hub_port" -ge 1 ] && [ "$hub_port" -le 65535 ]
}

set_taskr_role() {
    if [ -n "$taskr_role" ] && [ "$taskr_role" != "$1" ]; then
        printf '%s\n' 'ERROR: --taskr-hub and --taskr-client are mutually exclusive.' >&2
        exit 2
    fi
    taskr_role=$1
}

set_taskr_client() {
    if ! valid_hub_url "$1"; then
        printf 'ERROR: --taskr-client needs http://HOST:PORT, got: %s\n' "$1" >&2
        exit 2
    fi
    set_taskr_role client
    taskr_server_url=$1
}

# Reject empty/multiline arguments before storing the newline-delimited list.
queue_keep() {
    case $1 in
        '' | *'
'*) printf '%s\n' 'ERROR: --keep needs a nonempty single-line path' >&2; exit 2 ;;
    esac
    keep_args="${keep_args}$1
"
}
# Validate keep paths once the shared placement library is loaded.
while [ "$#" -gt 0 ]; do
    case $1 in
        --fleet) fleet=1 ;;
        --practices) practices=1 ;;
        --verification) verification=1 ;;
        --slotr) slotr=1 ;;
        --radar) radar=1 ;;
        --codator) codator=1 ;;
        --notifications) notifications=1 ;;
        --files-only) files_only=1 ;;
        --taskr-hub) fleet=1; set_taskr_role hub ;;
        --taskr-client)
            [ "$#" -ge 2 ] || fail '--taskr-client requires a URL'
            shift; fleet=1; set_taskr_client "$1" ;;
        --taskr-client=*) fleet=1; set_taskr_client "${1#*=}" ;;
        --taskr-peer | --taskr-peer=*) fail '--taskr-peer was removed; use --taskr-client http://HOST:PORT' ;;
        --keep)
            [ "$#" -ge 2 ] || fail '--keep requires a path'
            shift; queue_keep "$1" ;;
        --keep=*) queue_keep "${1#*=}" ;;
        -h | --help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
    shift
done

kernel=$(uname -s)
case $kernel in
	Darwin) OS=darwin ;;
	Linux) OS=linux ;;
	*) fail "unsupported operating system: $kernel" ;;
esac

machine=$(uname -m)
case $machine in
	arm64|aarch64) ARCH=arm64 ;;
	x86_64|amd64) ARCH=amd64 ;;
	*) fail "unsupported architecture: $machine" ;;
esac

ASSET="taskr-$OS-$ARCH"
TUI_ASSET="taskr-tui-$OS-$ARCH"
TAG=${TASKR_VERSION:-}
taskr_version_override=0
[ -z "$TAG" ] || taskr_version_override=1
TAG=${TAG:-${TASKR_SOURCE_TAG:-}}
if [ -n "$TAG" ]; then
    TAG=${TAG#taskr/}
    TAG=${TAG#v}
    printf '%s\n' "$TAG" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || fail 'invalid TASKR_VERSION'
    TAG=taskr/v$TAG
fi
taskr_home=${HOME:-}
[ -n "$taskr_home" ] || fail 'HOME is not set'
DIR=${TASKR_INSTALL_DIR:-$taskr_home/.local/bin}
SKILL_DIR=${TASKR_SKILL_DIR:-$taskr_home/.agents/skills/taskr}
PLUGIN_DIR=$taskr_home/.local/share/taskr/plugin
with_skill=1
[ "${TASKR_NO_SKILL:-}" != 1 ] || with_skill=0
with_plugin=1
[ "${TASKR_NO_PLUGIN:-}" != 1 ] || with_plugin=0
with_tui=1
[ "${TASKR_NO_TUI:-}" != 1 ] || with_tui=0
token=${GH_TOKEN:-${GITHUB_TOKEN:-}}

umask 077
tmp=$(mktemp -d) || fail 'could not create temporary directory'
install_tmp=
trap 'if [ -n "$install_tmp" ]; then rm -f -- "$install_tmp"; fi; rm -rf "$tmp"' 0
trap 'exit 1' HUP INT TERM

download_with_gh() {
	set -- --repo "$REPO" --pattern "$ASSET" --pattern SHA256SUMS --dir "$tmp" --clobber
	[ "$with_skill" -eq 0 ] || set -- "$@" --pattern skill.tar.gz
	[ "$with_plugin" -eq 0 ] || set -- "$@" --pattern plugin.tar.gz
	gh release download "$TAG" "$@"
}

curl_api() {
	accept_type=$1
	url=$2
	output=$3
	printf 'header = "Authorization: Bearer %s"\nheader = "Accept: %s"\n' "$token" "$accept_type" > "$tmp/curl.conf"
	curl -q -f -L -sS --config "$tmp/curl.conf" --output "$output" "$url"
}

# asset_id prints the id of the named asset in $tmp/release.json. GitHub may
# send compact or pretty JSON, so the layout is normalized first: newlines are
# dropped, the "assets" array is cut out, URL templates such as {/other_user}
# are dropped so that each asset's nested "uploader" object (which has its own
# "id") can be removed whole, and "}" ends each asset's line.
asset_id() {
	name=$1
	pairs=$(tr -d '\n\r' < "$tmp/release.json" | sed -n '
		/"assets"[[:space:]]*:[[:space:]]*\[/ {
			s/.*"assets"[[:space:]]*:[[:space:]]*\[//
			s/{\/[^}]*}//g
			s/"uploader"[[:space:]]*:[[:space:]]*{[^}]*}//g
			s/\].*//
			p
		}
	' | tr '}' '\n' | sed -n '
		s/.*"id"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1 \2/p
		t
		s/.*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*"id"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\2 \1/p
	')
	id=$(printf '%s\n' "$pairs" | while read -r pair_id pair_name; do
		if [ "$pair_name" = "$name" ]; then
			printf '%s\n' "$pair_id"
			break
		fi
	done)
	[ -n "$id" ] || fail "release asset not found: $name"
	printf '%s\n' "$id"
}

if [ "$files_only" -eq 0 ]; then
names="$ASSET SHA256SUMS"
[ "$with_skill" -eq 0 ] || names="$names skill.tar.gz"
[ "$with_plugin" -eq 0 ] || names="$names plugin.tar.gz"
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    transport=gh
elif [ -n "$token" ]; then
    transport=api
    command -v curl >/dev/null 2>&1 || fail 'curl is required when using GH_TOKEN or GITHUB_TOKEN'
else
    transport=public
    command -v curl >/dev/null 2>&1 || fail 'curl or gh is required'
fi

if [ -z "$TAG" ]; then
    # Select by semver and asset completeness, never by repository-wide latest.
    required=$names
    [ "$with_tui" -eq 0 ] || required="$required $TUI_ASSET"
    filter='.[]
        | select(.draft == false and .prerelease == false)
        | select(.tag_name | test("^taskr/v(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$"))
        | select(([.assets[].name] as $assets | ("'"$required"'" | split(" ")) - $assets | length) == 0)
        | .tag_name | ltrimstr("taskr/v")'
    if [ "$transport" = gh ]; then
        gh api --paginate "repos/$REPO/releases?per_page=100" --jq "$filter" > "$tmp/versions" || fail 'could not list releases'
    else
        command -v jq >/dev/null 2>&1 || fail 'jq is required for unpinned installs; set TASKR_VERSION to pin a release'
        page=1
        : > "$tmp/versions"
        while :; do
            url="https://api.github.com/repos/$REPO/releases?per_page=100&page=$page"
            if [ "$transport" = api ]; then
                curl_api application/vnd.github+json "$url" "$tmp/page.json" || fail 'could not list releases'
            else
                curl -q -f -L -sS --output "$tmp/page.json" "$url" || fail 'could not list releases'
            fi
            jq -r "$filter" "$tmp/page.json" >> "$tmp/versions" || fail 'invalid release metadata'
            count=$(jq 'length' "$tmp/page.json") || fail 'invalid release metadata'
            [ "$count" -eq 100 ] || break
            page=$((page + 1))
        done
    fi
    version=$(sort -t. -k1,1n -k2,2n -k3,3n "$tmp/versions" | tail -n 1)
    [ -n "$version" ] || fail 'no stable taskr release has the required assets'
    TAG=taskr/v$version
fi

case $transport in
    gh) download_with_gh || fail 'could not download release assets with gh' ;;
    api)
        encoded_tag=$(printf '%s' "$TAG" | sed 's|/|%2F|g')
        curl_api application/vnd.github+json "https://api.github.com/repos/$REPO/releases/tags/$encoded_tag" "$tmp/release.json" || fail 'could not download release metadata'
        for name in $names; do
            id=$(asset_id "$name")
            curl_api application/octet-stream "https://api.github.com/repos/$REPO/releases/assets/$id" "$tmp/$name" || fail "could not download release asset: $name"
        done
        ;;
    public)
        for name in $names; do
            curl -q -f -L -sS --output "$tmp/$name" "https://github.com/$REPO/releases/download/$TAG/$name" || fail "could not download release asset: $name"
        done
        ;;
esac

# Older releases have no TUI checksum entry; leave an installed TUI alone.
if [ "$with_tui" -eq 1 ]; then
	if awk -v n="$TUI_ASSET" '$2 == n { found = 1 } END { exit !found }' "$tmp/SHA256SUMS"; then
		case $transport in
		gh)
			set -- --repo "$REPO" --pattern "$TUI_ASSET" --dir "$tmp" --clobber
			gh release download "$TAG" "$@" || fail "could not download release asset: $TUI_ASSET"
			;;
		api)
			id=$(asset_id "$TUI_ASSET")
			curl_api application/octet-stream "https://api.github.com/repos/$REPO/releases/assets/$id" "$tmp/$TUI_ASSET" || fail "could not download release asset: $TUI_ASSET"
			;;
		public)
			url="https://github.com/$REPO/releases/download/$TAG/$TUI_ASSET"
			curl -q -f -L -sS --output "$tmp/$TUI_ASSET" "$url" || fail "could not download release asset: $TUI_ASSET"
			;;
		esac
	else
		with_tui=0
		printf '%s\n' 'taskr-tui is not in that release; leaving any existing taskr-tui unchanged'
	fi
fi

verify() {
	grep -F "  $1" "$tmp/SHA256SUMS" > "$tmp/asset.checksum" || fail "checksum entry not found: $1"
	if command -v shasum >/dev/null 2>&1; then
		if ! (cd "$tmp" && shasum -a 256 -c "$tmp/asset.checksum"); then
			fail "checksum verification failed: $1"
		fi
	elif command -v sha256sum >/dev/null 2>&1; then
		if ! (cd "$tmp" && sha256sum -c "$tmp/asset.checksum"); then
			fail "checksum verification failed: $1"
		fi
	else
		fail 'shasum or sha256sum is required to verify downloads'
	fi
}
verify "$ASSET"
[ "$with_tui" -eq 0 ] || verify "$TUI_ASSET"
if [ "$with_skill" -eq 1 ]; then
	verify skill.tar.gz
	validate_archive "$tmp/skill.tar.gz"
    while IFS= read -r member; do
        case $member in SKILL.md | references | references/ | references/*) ;;
            *) fail 'skill archive contains an unexpected member' ;;
        esac
    done <"$tmp/archive.names"
	mkdir "$tmp/skill"
	tar -xzf "$tmp/skill.tar.gz" -C "$tmp/skill" || fail 'could not extract skill.tar.gz'
	[ -f "$tmp/skill/SKILL.md" ] && [ -d "$tmp/skill/references" ] || fail 'skill.tar.gz is missing core or references'
fi
if [ "$with_plugin" -eq 1 ]; then
    verify plugin.tar.gz
    validate_archive "$tmp/plugin.tar.gz"
    mkdir "$tmp/plugin"
    tar -xzf "$tmp/plugin.tar.gz" -C "$tmp/plugin" || fail 'could not extract plugin.tar.gz'
fi

fi

# Source installs use the checkout. Standalone installs fetch only allowlisted
# public assets from the selected product tag, never main or a private overlay.
if [ "$radar" -eq 1 ] || [ "$codator" -eq 1 ] || [ "$notifications" -eq 1 ] || [ -n "$keep_args" ] || [ "$with_skill" -eq 1 ] || [ "$fleet" -eq 1 ] || [ "$practices" -eq 1 ] || [ "$verification" -eq 1 ] || [ "$with_plugin" -eq 1 ]; then
    if [ ! -f "$setup_dir/lib/placement.sh" ]; then
        [ "$files_only" -eq 0 ] || fail '--files-only requires a source checkout'
        encoded_tag=$(printf '%s' "$TAG" | sed 's|/|%2F|g')
        source_url="https://api.github.com/repos/$REPO/tarball/$encoded_tag"
        case $transport in
            gh) gh api "repos/$REPO/tarball/$TAG" >"$tmp/source.tar.gz" || fail 'could not download setup assets' ;;
            api) curl_api application/vnd.github+json "$source_url" "$tmp/source.tar.gz" || fail 'could not download setup assets' ;;
            public) curl -q -f -L -sS --output "$tmp/source.tar.gz" "$source_url" || fail 'could not download setup assets' ;;
        esac
        validate_archive "$tmp/source.tar.gz"
        source_prefix=$(tar -tzf "$tmp/source.tar.gz" | sed -n '1s|/.*||p')
        [ -n "$source_prefix" ] || fail 'source archive has no root'
        mkdir "$tmp/source"
        # Extract only the public source subtree; overlays/backups are not selected.
        for asset in $public_assets; do
            tar -xzf "$tmp/source.tar.gz" -C "$tmp/source" -- "$source_prefix/$asset" || fail "missing public asset: $asset"
        done
        setup_dir=$tmp/source/$source_prefix/products/setup
    fi
    work_dir=$tmp/placement
    mkdir "$work_dir"
    umask 022
    . "$setup_dir/lib/placement.sh"
    if [ "$radar" -eq 1 ] && [ "$files_only" -eq 0 ] && ! command -v jq >/dev/null 2>&1; then
        . "$setup_dir/lib/radar.sh"
        manual_radar
        printf '%s\n' 'ERROR: jq is required; install it and retry.' >&2
        exit 1
    fi
    # Existing helper bodies own the keep/record rules.
    saved_keep_args=$keep_args
    keep_args=
    old_ifs=$IFS; IFS='
'
    set -f
    for keep_arg in $saved_keep_args; do add_keep_arg "$keep_arg"; done
    set +f; IFS=$old_ifs
    append_keep_paths
    load_keep_list
    for parent in .agents .agents/skills .local/bin; do
        resolves_inside_home "$parent" || fail "destination parent $parent leaves HOME"
    done
    if [ "$with_plugin" -eq 1 ]; then
        resolves_inside_home .local/share/taskr || fail 'plugin parent leaves HOME'
    fi
    if [ "$notifications" -eq 1 ]; then
        for parent in .claude .codex; do
            resolves_inside_home "$parent" || fail "notification parent $parent leaves HOME"
        done
    fi
    if [ "$fleet" -eq 1 ]; then
        for parent in .local/share/herdr-setup .claude .codex .config/opencode/plugins .pi/agent/extensions .config/systemd/user .local/state/taskr; do
            resolves_inside_home "$parent" || fail "destination parent $parent leaves HOME"
        done
        for skill in herdr herdr-orchestration herdr-fleet; do
            resolves_inside_home ".agents/skills/$skill" || fail "skill parent $skill leaves HOME"
        done
    fi
    have_claude=0 have_codex=0 have_opencode=0 have_pi=0
    [ ! -d "$home_dir/.claude" ] || have_claude=1
    [ ! -d "$home_dir/.codex" ] || have_codex=1
    [ ! -d "$home_dir/.config/opencode" ] || have_opencode=1
    [ ! -d "$home_dir/.pi/agent" ] || have_pi=1
    add_next_step() { next_steps="${next_steps}  $1
"; }
    if [ "$with_skill" -eq 1 ]; then
        case $SKILL_DIR in "$home_dir"/*) ;; *) fail 'TASKR_SKILL_DIR must be inside HOME' ;; esac
        resolves_inside_home "${SKILL_DIR#"$home_dir"/}" || fail 'TASKR_SKILL_DIR leaves HOME'
        if [ "$files_only" -eq 1 ]; then
            mkdir "$tmp/skill"
            cp "$script_dir/SKILL.md" "$tmp/skill/SKILL.md"
            cp -R "$script_dir/references" "$tmp/skill/references"
        fi
        mkdir -p "$(dirname "$SKILL_DIR")"
        place_taskr_skill_dir "$tmp/skill" "$SKILL_DIR"
    fi
    [ "$fleet" -eq 0 ] || . "$setup_dir/lib/fleet.sh"
    [ "$notifications" -eq 0 ] || . "$setup_dir/lib/notifications.sh"
    if [ "$codator" -eq 1 ]; then
        for wrapper in cdx cdl; do
            place "$setup_dir/bin/$wrapper" "$home_dir/.local/bin/$wrapper" 0755
        done
    fi
    for optional in herdr-practices herdr-verification; do
        case $optional in
            herdr-practices) selected=$practices ;;
            herdr-verification) selected=$verification ;;
        esac
        [ "$selected" -eq 1 ] || continue
        # Source and installed attribution links have different relative roots.
        for asset in $public_assets; do
            case $asset in
                products/setup/skills/"$optional"/*)
                    rel=${asset#products/setup/skills/}
                    optional_src=$setup_dir/${asset#products/setup/}
                    if [ "${asset##*/}" = SKILL.md ]; then
                        sed 's|../../../../third_party/pstack/|../../../.local/share/herdr-setup/third_party/pstack/|g' "$optional_src" >"$work_dir/$optional.md"
                        optional_src=$work_dir/$optional.md
                    fi
                    resolves_inside_home "$(dirname ".agents/skills/$rel")" || fail 'optional skill parent leaves HOME'
                    place "$optional_src" "$home_dir/.agents/skills/$rel" ;;
            esac
        done
    done
    if [ "$practices" -eq 1 ] || [ "$verification" -eq 1 ]; then
        for notice in LICENSE PROVENANCE.md; do
            resolves_inside_home .local/share/herdr-setup/third_party/pstack || fail 'notice parent leaves HOME'
            place "$setup_dir/../../third_party/pstack/$notice" "$home_dir/.local/share/herdr-setup/third_party/pstack/$notice"
        done
    fi
    if [ "$fleet" -eq 1 ] || [ "${TASKR_LINK_SKILLS:-}" = 1 ]; then
        [ "$have_claude" -eq 0 ] || resolves_inside_home .claude/skills || fail 'Claude skills directory leaves HOME'
        [ "$have_codex" -eq 0 ] || resolves_inside_home .codex/skills || fail 'Codex skills directory leaves HOME'
        [ "$have_opencode" -eq 0 ] || resolves_inside_home .config/opencode/skills || fail 'OpenCode skills directory leaves HOME'
        link_all_skills
    fi
    write_record
fi

keep_taskr=0
if [ "$files_only" -eq 0 ] && [ "$taskr_version_override" -eq 0 ]; then
    current_version=$(taskr_installed_version)
    if [ -n "$current_version" ]; then
        case $(taskr_version_relation "$current_version" "${TAG#taskr/v}") in
            newer)
                printf 'KEEP taskr %s (newer than the pinned %s; run with TASKR_VERSION=<tag> to replace it)\n' \
                    "$current_version" "$TAG"
                keep_taskr=1
                ;;
            local)
                printf 'KEEP taskr %s (a local or pre-release build; run with TASKR_VERSION=<tag> to replace it)\n' \
                    "$current_version"
                keep_taskr=1
                ;;
        esac
    fi
fi

if [ "$files_only" -eq 0 ] && [ "$keep_taskr" -eq 0 ]; then
mkdir -p "$DIR"
install_binary() {
	install_tmp=$(mktemp "$DIR/.$2.XXXXXX") || fail "could not stage $2"
	cp "$tmp/$1" "$install_tmp"
	chmod 755 "$install_tmp"
	mv -f "$install_tmp" "$DIR/$2"
	install_tmp=
}
install_binary "$ASSET" taskr
if [ "$with_tui" -eq 1 ]; then
	install_binary "$TUI_ASSET" taskr-tui
	printf 'installed %s/taskr-tui\n' "$DIR"
fi
fi

if [ "$files_only" -eq 0 ] && [ "$keep_taskr" -eq 0 ]; then
    version_output=$("$DIR/taskr" version)
    printf 'installed %s/taskr (%s)\n' "$DIR" "$version_output"
fi
# Stage the plugin through the same placement rules; activation is manual.
if [ "$with_plugin" -eq 1 ] && [ "$files_only" -eq 0 ]; then
    mkdir -p "$(dirname "$PLUGIN_DIR")"
    place_taskr_skill_dir "$tmp/plugin" "$PLUGIN_DIR"
    write_record
    printf 'plugin staged: %s (activate with herdr plugin link)\n' "$PLUGIN_DIR"
fi
if [ "$slotr" -eq 1 ]; then
    if [ "$files_only" -eq 1 ]; then
        printf '%s\n' 'MANUAL: run products/slotr/install.sh to install slotr'
    elif [ -f "$setup_dir/../slotr/install.sh" ]; then
        if ! sh "$setup_dir/../slotr/install.sh"; then
            printf '%s\n' 'WARN: optional slotr installation failed; taskr is installed' >&2
        fi
    else
        printf '%s\n' 'MANUAL: install slotr from its pinned product release'
    fi
fi
if [ "$radar" -eq 1 ]; then
    . "$setup_dir/lib/radar.sh"
    herdr_config_dest=$home_dir/.config/herdr/config.toml
    if [ "$files_only" -eq 0 ]; then
        if command -v herdr >/dev/null 2>&1 && [ "${HERDR_ENV:-}" = 1 ] && command -v jq >/dev/null 2>&1; then
            install_radar
        else
            manual_radar
        fi
    fi
fi
[ -z "$next_steps" ] || printf '\nNext steps (run yourself):\n%s' "$next_steps"
# Legacy explicit config opt-in; never imports or starts a sidebar.
if ! configure_herdr_sidebar; then
	manual_herdr_sidebar
fi
case ":${PATH:-}:" in
	*":$DIR:"*) ;;
	*) printf 'install.sh: add %s to PATH\n' "$DIR" >&2 ;;
esac
