# Shared placement contract; sourced by products/taskr/install.sh.
# Accepts a path relative to HOME whose components are all non-empty and
# none is . or .., so that it can never name a path outside HOME.
valid_keep_path() {
    case $1 in
        '' | /* | '~'* | *'
'*) return 1 ;;
    esac
    keep_rest=$1/
    while [ -n "$keep_rest" ]; do
        keep_part=${keep_rest%%/*}
        keep_rest=${keep_rest#*/}
        case $keep_part in
            '' | . | ..) return 1 ;;
        esac
    done
}

# Succeeds when every existing component of a path relative to HOME,
# symlinks included, resolves physically inside HOME. The walk stops at the
# first missing component or regular file.
resolves_inside_home() {
    inside_dir=$home_physical
    inside_rest=$1/
    inside_hops=0
    while [ -n "$inside_rest" ]; do
        inside_part=${inside_rest%%/*}
        inside_rest=${inside_rest#*/}
        inside_path=$inside_dir/$inside_part
        if [ -d "$inside_path" ]; then
            inside_dir=$(CDPATH= cd -P "$inside_path" && pwd -P) || return 1
        elif [ -L "$inside_path" ]; then
            # A symlink to a file, or a dangling one: follow one hop.
            inside_hops=$((inside_hops + 1))
            [ "$inside_hops" -le 40 ] || return 1
            inside_target=$(readlink "$inside_path") || return 1
            case $inside_target in
                /*) ;;
                *) inside_target=$inside_dir/$inside_target ;;
            esac
            inside_parent=$(dirname "$inside_target")
            [ -d "$inside_parent" ] || return 1
            inside_dir=$(CDPATH= cd -P "$inside_parent" && pwd -P) || return 1
            case ${inside_target##*/} in
                '' | . | ..) return 1 ;;
            esac
            inside_rest=${inside_target##*/}/$inside_rest
        else
            return 0
        fi
        case $inside_dir in
            "$home_physical" | "$home_physical"/*) ;;
            *) return 1 ;;
        esac
    done
    return 0
}

add_keep_arg() {
    if ! valid_keep_path "$1"; then
        printf 'ERROR: --keep needs a path relative to HOME without . or .. components (for example .codex/config.toml), got: %s\n' "$1" >&2
        exit 2
    fi
    if ! resolves_inside_home "$1"; then
        printf 'ERROR: --keep path %s leaves HOME through a symlink\n' "$1" >&2
        exit 2
    fi
    keep_args="${keep_args}$1
"
}

# Atomically replaces DEST with SRC's content through a temporary file in
# DEST's directory. An existing DEST keeps its mode; a new one gets MODE. The
# caller has checked that DEST is not a symlink.
replace_file() {
    replace_src=$1
    replace_dest=$2
    replace_tmp=$(mktemp "$(dirname "$replace_dest")/.herdr-setup.XXXXXX") || return 1
    if ! { [ -f "$replace_dest" ] && cp -p "$replace_dest" "$replace_tmp" 2>/dev/null; }; then
        chmod "${3:-0644}" "$replace_tmp"
    fi
    if cat "$replace_src" >"$replace_tmp" && mv -f "$replace_tmp" "$replace_dest"; then
        return 0
    fi
    rm -f "$replace_tmp"
    return 1
}

hash_tool=
if command -v sha256sum >/dev/null 2>&1; then
    hash_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
    hash_tool=shasum
elif command -v openssl >/dev/null 2>&1; then
    hash_tool=openssl
fi

# Prints the SHA-256 of a file as 64 lowercase hex digits, or nothing.
file_hash() {
    case $hash_tool in
        sha256sum) hash_value=$(sha256sum <"$1" | cut -d ' ' -f 1) ;;
        shasum) hash_value=$(shasum -a 256 <"$1" | cut -d ' ' -f 1) ;;
        openssl) hash_value=$(openssl dgst -sha256 <"$1" | sed 's/.*= *//') ;;
        *) hash_value= ;;
    esac
    case $hash_value in
        *[!0-9a-f]*) ;;
        *) [ "${#hash_value}" -eq 64 ] && printf '%s\n' "$hash_value" ;;
    esac
    return 0
}

# The keep list: destinations, relative to HOME, that differ on purpose on
# this machine. The installer never writes them.
keep_file=$home_dir/.config/herdr-setup/keep
keep_list=$work_dir/keep
keep_usable=1
if ! resolves_inside_home .config/herdr-setup/keep; then
    printf 'WARN: %s leaves HOME through a symlink; ignored, and nothing is kept or updated this run\n' "$keep_file" >&2
    keep_usable=0
fi

# Prints the entries of a keep file, one per line, without blank lines,
# comments, or surrounding whitespace.
keep_entries() {
    sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' "$1" | grep -v -e '^#' -e '^$' || true
}

# Every destination place() can write, relative to HOME.
is_managed() {
    case $1 in
        .agents/skills/herdr-orchestration/SKILL.md | \
            .agents/skills/herdr-orchestration/model-selection.md | \
            .agents/skills/herdr-orchestration/lanes.md | \
            .agents/skills/herdr-orchestration/launch.md | \
            .agents/skills/herdr-orchestration/workflow.md | \
            .agents/skills/herdr-orchestration/owner.md | \
            .agents/skills/herdr-orchestration/teardown.md | \
            .agents/skills/herdr-orchestration/recovery.md | \
            .agents/skills/herdr-fleet/SKILL.md | \
            .agents/skills/herdr-practices/* | .agents/skills/herdr-verification/* | \
            .local/share/herdr-setup/* | .local/share/taskr/plugin/* | \
            .local/bin/herdr-lanes.sh | \
            .agents/skills/herdr/SKILL.md | \
            .agents/skills/taskr/SKILL.md | \
            .agents/skills/taskr/references/* | \
            .local/state/taskr/dashboard.addr | \
            .local/state/taskr/hub.url | \
            .local/state/taskr/server.url | \
            .config/systemd/user/taskr.service | \
            .claude/CLAUDE.md | .claude/settings.json | .claude/statusline.sh | \
            .codex/AGENTS.md | .codex/config.toml | .codex/hooks.json | \
            .local/bin/cdx | .local/bin/cdl | .local/bin/herdr-launch | .local/bin/herdr-loop | .local/bin/herdr-worktrees | .local/bin/herdr-lead | .local/bin/herdr-hub | \
            .config/opencode/plugins/taskr-hook.js | \
            .pi/agent/extensions/taskr-hook.ts | \
            .config/herdr/config.toml)
            return 0
            ;;
    esac
    return 1
}

# Adds the --keep paths to the keep file through one atomic replace. A
# symlink or a non-regular file there is never written.
append_keep_paths() {
    if [ -z "$keep_args" ]; then
        return 0
    fi
    if [ "$keep_usable" -ne 1 ]; then
        printf 'ERROR: %s leaves HOME through a symlink; nothing written.\n' "$keep_file" >&2
        exit 1
    fi
    if [ -L "$keep_file" ] || { [ -e "$keep_file" ] && [ ! -f "$keep_file" ]; }; then
        printf 'ERROR: %s is a symlink or not a regular file; add the --keep paths to it by hand. Nothing written.\n' \
            "$keep_file" >&2
        exit 1
    fi
    keep_new=$work_dir/keep.new
    if [ -f "$keep_file" ]; then
        if ! cat "$keep_file" >"$keep_new"; then
            printf 'ERROR: cannot read %s. Nothing written.\n' "$keep_file" >&2
            exit 1
        fi
        if [ -s "$keep_new" ] && [ -n "$(tail -c 1 "$keep_new")" ]; then
            printf '\n' >>"$keep_new"
        fi
    else
        printf '%s\n' '# herdr-setup keep-local list: one path per line, relative to HOME.' >"$keep_new"
    fi
    keep_added=0
    keep_ifs=$IFS
    IFS='
'
    set -f
    for keep_path in $keep_args; do
        if keep_entries "$keep_new" | grep -F -x -e "$keep_path" >/dev/null; then
            printf 'SKIP %s (already in %s)\n' "$keep_path" "$keep_file"
        else
            printf '%s\n' "$keep_path" >>"$keep_new"
            printf 'ADD %s to %s\n' "$keep_path" "$keep_file"
            keep_added=1
        fi
    done
    set +f
    IFS=$keep_ifs
    if [ "$keep_added" -eq 1 ]; then
        if ! mkdir -p "$(dirname "$keep_file")" || ! replace_file "$keep_new" "$keep_file" 0644; then
            printf 'ERROR: cannot write %s\n' "$keep_file" >&2
            exit 1
        fi
    fi
}

# Loads the keep list; an unreadable list stops the run, because a kept
# file that is unchanged since the last install would otherwise be updated.
load_keep_list() {
    : >"$keep_list"
    if [ "$keep_usable" -ne 1 ]; then
        # Without the keep list, an unedited kept file must not be updated.
        record_usable=0
        return 0
    fi
    if [ ! -e "$keep_file" ]; then
        return 0
    fi
    if [ ! -f "$keep_file" ] || [ ! -r "$keep_file" ]; then
        printf 'ERROR: cannot read the keep list %s; fix or remove it and re-run.\n' "$keep_file" >&2
        exit 1
    fi
    keep_entries "$keep_file" >"$work_dir/keep.raw"
    while IFS= read -r keep_path; do
        if ! valid_keep_path "$keep_path"; then
            printf 'WARN: ignoring %s in %s; entries are paths relative to HOME\n' "$keep_path" "$keep_file" >&2
            continue
        fi
        if ! is_managed "$keep_path"; then
            printf 'WARN: %s in %s is not a file herdr-setup places; nothing to keep\n' "$keep_path" "$keep_file" >&2
        fi
        printf '%s\n' "$keep_path" >>"$keep_list"
    done <"$work_dir/keep.raw"
}

is_kept() {
    kept_rel=${1#"$home_dir"/}
    [ "$kept_rel" != "$1" ] && grep -F -x -e "$kept_rel" "$keep_list" >/dev/null
}

# The last-installed record: one "<sha256> <dest relative to HOME>" line for
# each destination whose content herdr-setup last wrote or found identical.
# A symlink, a non-regular file, or an unreadable file there is never read
# or written: the run has no history, as before the record existed.
record_file=$home_dir/.local/state/herdr-setup/installed
record_work=$work_dir/installed
record_usable=1
record_changed=0
: >"$record_work"
if ! resolves_inside_home .local/state/herdr-setup/installed; then
    printf 'WARN: %s leaves HOME through a symlink; ignored, and nothing is recorded this run\n' "$record_file" >&2
    record_usable=0
elif [ -z "$hash_tool" ]; then
    printf '%s\n' 'WARN: no sha256sum, shasum, or openssl; herdr-setup keeps no install record this run' >&2
    record_usable=0
elif [ -L "$record_file" ] || { [ -e "$record_file" ] && [ ! -f "$record_file" ]; }; then
    printf 'WARN: %s is a symlink or not a regular file; ignored, and nothing is recorded this run\n' "$record_file" >&2
    record_usable=0
elif [ -e "$record_file" ] && [ ! -r "$record_file" ]; then
    printf 'WARN: %s is unreadable; ignored, and nothing is recorded this run\n' "$record_file" >&2
    record_usable=0
elif [ -f "$record_file" ]; then
    grep -E '^[0-9a-f]{64} [^ ]' "$record_file" >"$record_work" || true
fi

# Prints the recorded hash for a destination relative to HOME, or nothing.
record_get() {
    if [ "$record_usable" -eq 1 ]; then
        REL=$1 awk 'substr($0, 66) == ENVIRON["REL"] { h = substr($0, 1, 64) } END { if (h != "") print h }' \
            "$record_work"
    fi
}

record_set() {
    if [ "$record_usable" -ne 1 ] || [ -z "$2" ] || [ "$(record_get "$1")" = "$2" ]; then
        return 0
    fi
    case $1 in
        /*) return 0 ;;
    esac
    REL=$1 awk 'substr($0, 66) != ENVIRON["REL"]' "$record_work" >"$record_work.new"
    printf '%s %s\n' "$2" "$1" >>"$record_work.new"
    mv -f "$record_work.new" "$record_work"
    record_changed=1
}

write_record() {
    if [ "$record_usable" -ne 1 ] || [ "$record_changed" -ne 1 ]; then
        return 0
    fi
    if ! mkdir -p "$(dirname "$record_file")" || [ -L "$record_file" ] \
        || ! replace_file "$record_work" "$record_file" 0644; then
        printf 'WARN: could not write %s\n' "$record_file" >&2
    fi
}

place_result=
# The one placement helper for every file this script writes. It never
# writes a keep-listed destination, never follows a symlink at the
# destination or at the sidecar path, replaces a different file only when
# the record shows it is unchanged since the last install, never replaces a
# directory, and chmods only what it creates.
place() {
    place_src=$1
    place_dest=$2
    place_mode=${3:-0644}
    place_sidecar=${place_dest}.herdr-setup
    place_rel=${place_dest#"$home_dir"/}
    place_result=MERGE

    if is_kept "$place_dest"; then
        printf 'KEEP %s (listed in ~/.config/herdr-setup/keep)\n' "$place_dest"
        place_result=KEEP
        return 0
    fi
    if [ -L "$place_dest" ]; then
        printf 'MERGE %s is a symlink; left in place, nothing written\n' "$place_dest"
        return 0
    fi
    if [ ! -e "$place_dest" ]; then
        if ! mkdir -p "$(dirname "$place_dest")"; then
            printf 'MERGE %s: cannot create its directory; nothing written\n' "$place_dest"
            return 0
        fi
        cp "$place_src" "$place_dest"
        chmod "$place_mode" "$place_dest"
        printf 'INSTALL %s\n' "$place_dest"
        place_result=INSTALL
        record_set "$place_rel" "$(file_hash "$place_src")"
        return 0
    fi
    if [ -f "$place_dest" ] && cmp -s "$place_src" "$place_dest"; then
        printf 'SKIP %s (identical)\n' "$place_dest"
        place_result=SKIP
        record_set "$place_rel" "$(file_hash "$place_src")"
        return 0
    fi

    if [ -f "$place_dest" ]; then
        place_recorded=$(record_get "$place_rel")
        if [ -n "$place_recorded" ] && [ "$(file_hash "$place_dest")" = "$place_recorded" ]; then
            if replace_file "$place_src" "$place_dest" "$place_mode"; then
                printf 'UPDATE %s (unchanged since last install)\n' "$place_dest"
                place_result=UPDATE
                record_set "$place_rel" "$(file_hash "$place_src")"
                return 0
            fi
            printf 'WARN: could not update %s in place\n' "$place_dest" >&2
        fi
        place_state=differs
    else
        place_state='is not a regular file'
    fi
    # ponytail: preserving a different user file and staging a sidecar is the
    # deliberate safety ceiling; merging user configuration needs judgment.
    if [ -L "$place_sidecar" ]; then
        printf 'MERGE %s %s; sidecar %s is a symlink, nothing written\n' \
            "$place_dest" "$place_state" "$place_sidecar"
    elif [ ! -e "$place_sidecar" ]; then
        cp "$place_src" "$place_sidecar"
        chmod "$place_mode" "$place_sidecar"
        printf 'MERGE %s %s; staged %s\n' "$place_dest" "$place_state" "$place_sidecar"
    elif [ -f "$place_sidecar" ] && cmp -s "$place_src" "$place_sidecar"; then
        printf 'MERGE %s %s; %s already staged\n' "$place_dest" "$place_state" "$place_sidecar"
    else
        printf 'MERGE %s %s; %s holds other content, nothing written\n' \
            "$place_dest" "$place_state" "$place_sidecar"
    fi
}

# Places the staged skill tree without following source or destination
# symlinks. Only new directories get 755; files use the usual placement rule.
place_taskr_skill_dir() {
    if [ -L "$1" ]; then
        printf 'WARN: staged taskr skill directory %s is a symlink; nothing placed\n' "$1" >&2
        return 0
    fi
    if [ -L "$2" ] || { [ -e "$2" ] && [ ! -d "$2" ]; }; then
        printf 'MERGE %s is a symlink or not a directory; nothing placed\n' "$2"
        return 0
    fi
    if [ ! -d "$2" ] && ! mkdir -m 0755 "$2"; then
        printf 'MERGE %s: cannot create its directory; nothing placed\n' "$2"
        return 0
    fi
    for taskr_entry in "$1"/* "$1"/.[!.]* "$1"/..?*; do
        if [ -L "$taskr_entry" ]; then
            printf 'WARN: staged taskr skill file %s is a symlink; nothing placed\n' "$taskr_entry" >&2
        elif [ -d "$taskr_entry" ]; then
            place_taskr_skill_dir "$taskr_entry" "$2/${taskr_entry##*/}"
        elif [ -f "$taskr_entry" ]; then
            place "$taskr_entry" "$2/${taskr_entry##*/}" 0644
        fi
    done
}

# A real directory may become a link only when it holds nothing but a
# SKILL.md identical to the source (a .DS_Store is ignored).
is_plain_skill_copy() {
    copy_dir=$1
    copy_source=$2
    if [ -L "$copy_dir/SKILL.md" ] || [ ! -f "$copy_dir/SKILL.md" ]; then
        return 1
    fi
    if ! cmp -s "$copy_dir/SKILL.md" "$copy_source"; then
        return 1
    fi
    for copy_entry in "$copy_dir"/* "$copy_dir"/.[!.]* "$copy_dir"/..?*; do
        if [ ! -e "$copy_entry" ] && [ ! -L "$copy_entry" ]; then
            continue
        fi
        case ${copy_entry##*/} in
            SKILL.md | .DS_Store) ;;
            *) return 1 ;;
        esac
    done
    return 0
}

# Prints the link target for a skill: the relative path when the agent's
# directory and its skills directory sit where HOME says, otherwise the
# absolute source path, because a relative path would resolve from the
# relocated physical directory.
skill_link_target() {
    target_agent_dir=$1
    target_relative=$2
    target_name=$3
    target_physical=$(CDPATH= cd "$target_agent_dir" && pwd -P)
    if [ "$target_physical" = "$home_physical${target_agent_dir#"$home_dir"}" ] \
        && [ ! -L "$target_agent_dir/skills" ]; then
        printf '%s\n' "$target_relative"
    else
        printf '%s\n' "$home_dir/.agents/skills/$target_name"
    fi
}

link_skill() {
    link_name=$1
    link_agent_dir=$2
    link_target=$(skill_link_target "$link_agent_dir" "$3" "$link_name")
    link_source=$home_dir/.agents/skills/$link_name
    if [ "$link_name" = taskr ] && [ "$SKILL_DIR" != "$link_source" ]; then
        link_source=$SKILL_DIR
        link_target=$SKILL_DIR
    fi
    link_dest=$link_agent_dir/skills/$link_name

    if [ -L "$link_dest" ]; then
        link_current=$(readlink "$link_dest")
        if [ ! -f "$link_source/SKILL.md" ]; then
            printf 'MISSING %s -> %s (no source at %s)\n' "$link_dest" "$link_current" "$link_source"
        elif [ "$link_current" = "$link_target" ]; then
            printf 'SKIP %s (linked)\n' "$link_dest"
        else
            rm -f "$link_dest"
            ln -s "$link_target" "$link_dest"
            printf 'INSTALL %s -> %s (replaced link to %s)\n' "$link_dest" "$link_target" "$link_current"
        fi
    elif [ ! -f "$link_source/SKILL.md" ]; then
        printf 'SKIP %s (no source at %s)\n' "$link_dest" "$link_source"
    elif [ -d "$link_dest" ]; then
        link_dest_physical=$(CDPATH= cd "$link_dest" && pwd -P)
        case $link_dest_physical in
            "$home_physical"/*) ;;
            *)
                printf 'MERGE %s is outside HOME (%s); left in place\n' "$link_dest" "$link_dest_physical"
                return 0
                ;;
        esac
        if is_plain_skill_copy "$link_dest" "$link_source/SKILL.md"; then
            rm -r "$link_dest"
            ln -s "$link_target" "$link_dest"
            printf 'INSTALL %s -> %s (replaced identical copy)\n' "$link_dest" "$link_target"
        else
            # ponytail: a different real skill directory is never overwritten.
            printf 'MERGE %s differs from %s; left in place\n' "$link_dest" "$link_source"
        fi
    elif [ -e "$link_dest" ]; then
        printf 'MERGE %s is not a directory; left in place (source %s)\n' "$link_dest" "$link_source"
    elif ! mkdir -p "$link_agent_dir/skills"; then
        printf 'MERGE %s: cannot create %s; nothing linked\n' "$link_dest" "$link_agent_dir/skills"
    else
        ln -s "$link_target" "$link_dest"
        printf 'INSTALL %s -> %s\n' "$link_dest" "$link_target"
    fi
}

link_all_skills() {
    for skill_name in herdr herdr-orchestration herdr-fleet taskr herdr-practices herdr-verification; do
        case $skill_name in
            herdr | herdr-orchestration | herdr-fleet) [ "$fleet" -eq 1 ] || continue ;;
            taskr) [ "$with_skill" -eq 1 ] || continue ;;
            herdr-practices) [ "$practices" -eq 1 ] || continue ;;
            herdr-verification) [ "$verification" -eq 1 ] || continue ;;
        esac
        if [ "$have_claude" -eq 1 ]; then
            link_skill "$skill_name" "$home_dir/.claude" "../../.agents/skills/$skill_name"
        fi
        if [ "$have_codex" -eq 1 ]; then
            link_skill "$skill_name" "$home_dir/.codex" "../../.agents/skills/$skill_name"
        fi
        if [ "$have_opencode" -eq 1 ]; then
            link_skill "$skill_name" "$home_dir/.config/opencode" "../../../.agents/skills/$skill_name"
        fi
    done
}
