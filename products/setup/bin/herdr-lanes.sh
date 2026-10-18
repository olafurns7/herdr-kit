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
        def prefix: IN("impl", "mech", "rs", "rev", "arch", "glm", "orch", "gather");
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
            (del(.deny) | length > 0 and all(to_entries[];
            (.key | test("^[a-z][a-z0-9-]*$")) and
            (.value | type == "object" and
                (.kind | IN("claude", "codex", "pi")) and
                (.model | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._-]*$")) and
                (.effort | IN("low", "medium", "high", "xhigh", "max", "ultra")) and
                ((.provider // (if .kind == "pi" then "opencode" else .kind end)) |
                    type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._-]*$")) and
                (.prefixes | type == "array" and length > 0 and
                    all(.[]; prefix))))
        )) then . else error("invalid lane table") end
        | (.deny // {}) as $rules | del(.deny)
        | map_values(.provider //= (if .kind == "pi" then "opencode" else .kind end)
            | .denied = denied($rules))
    ' "$lane_table") || { printf '%s\n' 'invalid lane table' >&2; return 1; }
}

lane_list() {
    lane_table_load || return 1
    printf 'Lane table: %s\n' "$lane_table"
    printf '%s\n' "$lane_data" | jq -r '
        to_entries | sort_by(.key)[]
        | [.key, .value.kind, .value.model, .value.effort, (.value.prefixes | join(" "))] +
            (if (.value.denied | length) > 0 then ["denied: " + (.value.denied | join(", "))] else [] end) | @tsv
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
