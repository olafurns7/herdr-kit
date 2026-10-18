# Explicit hook opt-in; preserve existing keys and stage edits through place().
command -v jq >/dev/null 2>&1 || fail 'jq is required for --notifications'
for harness in claude codex; do
    case $harness in
        claude) hook_dest=$home_dir/.claude/settings.json; hook_base=$setup_dir/claude/settings.json ;;
        codex) hook_dest=$home_dir/.codex/hooks.json; hook_base=$setup_dir/codex/hooks.json ;;
    esac
    if [ -f "$hook_dest" ] && [ ! -L "$hook_dest" ]; then
        hook_base=$hook_dest
    fi
    case $harness in claude) hook_minimal=$setup_dir/claude/settings.json ;; codex) hook_minimal=$setup_dir/codex/hooks.json ;; esac
    jq -s '
        if (.[0] | type) != "object" then error("settings must be an object") else . end |
        .[0] as $base |
        reduce .[1:][] as $fragment ($base;
            reduce ($fragment.hooks | to_entries[]) as $event (.;
                .hooks[$event.key] = ((.hooks[$event.key] // []) + $event.value | unique)))
    ' "$hook_base" "$hook_minimal" "$setup_dir/$harness/notifications.json" >"$work_dir/$harness-notifications.json" || fail "invalid $harness settings for --notifications"
    place "$work_dir/$harness-notifications.json" "$hook_dest" 0600
done
