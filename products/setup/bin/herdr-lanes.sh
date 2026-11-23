# Shared by herdr-launch, herdr-lead and herdr-hub; data never becomes shell code.
lane_table_load() {
    command -v jq >/dev/null 2>&1 || { printf '%s\n' 'jq is required to read lanes' >&2; return 1; }
    lane_table=$HOME/.config/herdr-fleet/lanes.json
    if [ ! -e "$lane_table" ] && [ ! -L "$lane_table" ]; then
        if [ -f "$herdr_bin_dir/../lib/placement.sh" ]; then
            lane_table=$herdr_bin_dir/../lanes.json
        else
            lane_table=$HOME/.local/share/herdr-setup/lanes.json
        fi
    fi
    lane_data=$(jq -ecs '
        if length == 1 then .[0] else error("expected one lane table") end
        | def patterns: type == "array" and all(.[];
            type == "string" and length > 0 and
            (. as $pattern | try ("" | test($pattern) | type == "boolean") catch false));
        def prefix: IN("impl", "mech", "rs", "rev", "arch", "glm", "orch", "hub", "gather");
        def fields: type == "object" and all(to_entries[];
            (.key | IN("model", "provider", "kind")) and (.value | patterns));
        def denied($rules): . as $lane
            | ["model", "provider", "kind"] | map(. as $field
                | select(any($rules[$field][]?; . as $pattern | $lane[$field] | test($pattern)))
                | select(all($lane.prefixes[]; . as $prefix
                    | any($rules.except[$prefix][$field][]?;
                        . as $pattern | $lane[$field] | test($pattern))) | not));
        if (type == "object" and
            ((if has("deny") then .deny else {} end) | type == "object" and
                (del(.except) | fields) and
                (if has("except") then .except | type == "object" and all(to_entries[];
                    (.key | prefix) and (.value | fields)) else true end)) and
            (if has("pins") then .pins | type == "object" and all(to_entries[];
                (.key | test("^[a-z][a-z0-9_-]{0,31}$")) and (.value | type == "string")) else true end) and
            (del(.deny, .pins) as $lanes | all(.pins // {} | .[]; . as $lane | $lanes | has($lane))) and
            (del(.deny, .pins) | length > 0 and all(to_entries[];
            (.key | test("^[a-z][a-z0-9-]*$")) and
            (.value | type == "object" and
                (.kind | IN("claude", "codex", "pi", "opencode")) and
                (if .kind == "opencode" then
                    .model | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._-]*$")
                 else .model | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._-]*$") end) and
                (.effort | IN("low", "medium", "high", "xhigh", "max", "ultra")) and
                ((.provider // (if .kind == "pi" then "opencode" else .kind end)) |
                    type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._-]*$")) and
                (.prefixes | type == "array" and length > 0 and
                    all(.[]; prefix))))
        )) then . else error("invalid lane table") end
        | (.deny // {}) as $rules | (.pins // {}) as $pins | del(.deny, .pins)
        | {pins: $pins, lanes: map_values((if .kind == "opencode" then
                (.model | split("/")[0]) as $provider
                | if has("provider") and .provider != $provider then
                    error("opencode provider must match model prefix: " + $provider)
                  else .provider = $provider end
            else .provider //= (if .kind == "pi" then "opencode" else .kind end) end)
            | .denied = denied($rules))}
    ' "$lane_table") || { printf '%s\n' 'invalid lane table' >&2; return 1; }
    # lane_pins maps a task-name prefix to the one lane that may start it (lanes.md "Lane pins").
    lane_pins=$(printf '%s\n' "$lane_data" | jq -c '.pins') &&
        lane_data=$(printf '%s\n' "$lane_data" | jq -c '.lanes') || { printf '%s\n' 'invalid lane table' >&2; return 1; }
}

lane_list() {
    lane_table_load || return 1
    printf 'Lane table: %s\n' "$lane_table"
    printf '%s\n' "$lane_data" | jq -r '
        to_entries | sort_by(.key)[]
        | [.key, .value.kind, .value.model, .value.effort, (.value.prefixes | join(" "))] +
            (if (.value.denied | length) > 0 then ["denied: " + (.value.denied | join(", "))] else [] end) | @tsv
    '
    printf '%s\n' "$lane_pins" | jq -r 'to_entries | sort_by(.key)[] | ["pin", .key, .value] | @tsv'
}

# lane_pin_mismatch NAME LANE: the longest pinned prefix of NAME decides; print "PREFIX LANE" when its lane is not LANE.
lane_pin_mismatch() {
    printf '%s\n' "$lane_pins" | jq -r --arg name "$1" --arg lane "$2" '
        [to_entries[] | select(.key as $prefix | $name | startswith($prefix))]
        | sort_by(.key | length) | last // empty | select(.value != $lane) | .key + " " + .value
    '
}

lane_info() {
    lane_table_load || return 1
    lane_row=$(printf '%s\n' "$lane_data" | jq -er --arg lane "$1" '
        .[$lane] // error("unknown lane")
        | if (.denied | length) > 0 then error("lane " + $lane + " denied: " + (.denied | join(", "))) else . end
        | [.kind, .model, .effort, (.prefixes | join(" ")), .provider] | join("\n")
    ') || { printf '%s\n' 'invalid lane table' >&2; return 1; }
    kind=$(printf '%s\n' "$lane_row" | sed -n '1p')
    model=$(printf '%s\n' "$lane_row" | sed -n '2p')
    effort=$(printf '%s\n' "$lane_row" | sed -n '3p')
    prefixes=$(printf '%s\n' "$lane_row" | sed -n '4p')
    provider=$(printf '%s\n' "$lane_row" | sed -n '5p')
}

# agent_tmp_command prints one shell command line that creates the per-user
# disk folder for agent temp files, checks it, and exports TMPDIR to it
# (README.md "Temp files"). herdr-launch and herdr-lead append it to the pane
# setup, so it runs on the agent's host as that user; herdr-hub evals it.
# HERDR_TEST_AGENT_TMP_BASE replaces /var/tmp for the stub tests only.
# Codex binds TMPDIR/codex-daemon-UID/<64 hex>; AF_UNIX allows 107 bytes, so
# the folder may have at most 24.
agent_tmp_command() {
    agent_tmp_base=${HERDR_TEST_AGENT_TMP_BASE:-/var/tmp}
    case $agent_tmp_base in
        /*[[:cntrl:]]* | [!/]*) printf '%s\n' 'agent temp base must be an absolute single-line path' >&2; return 1 ;;
    esac
    agent_tmp_base=$(printf '%s\n' "$agent_tmp_base" | sed "s/'/'\\\\''/g")
    printf '%s' "herdr_agent_tmp='$agent_tmp_base'"'/agent-$(id -u) && { [ "$(($(printf %s "$herdr_agent_tmp" | wc -c)))" -le 24 ] || { printf "herdr-agent-tmp: %s is over 24 bytes (Codex socket path limit); not starting\n" "$herdr_agent_tmp" >&2; false; }; } && { (umask 077; mkdir -p -- "$herdr_agent_tmp") 2>/dev/null; [ -d "$herdr_agent_tmp" ] && [ ! -L "$herdr_agent_tmp" ] && [ -O "$herdr_agent_tmp" ] && chmod 0700 -- "$herdr_agent_tmp" || { printf "herdr-agent-tmp: %s must be a directory owned by you, not a symlink; not starting\n" "$herdr_agent_tmp" >&2; false; }; } && export TMPDIR="$herdr_agent_tmp"'
}
