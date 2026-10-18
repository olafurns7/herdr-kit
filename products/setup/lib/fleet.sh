# Explicit public asset allowlist. Never traverse a fleet skill tree or local overlay.
for asset in \
    skills/herdr-orchestration/SKILL.md \
    skills/herdr-orchestration/lanes.md skills/herdr-orchestration/launch.md \
    skills/herdr-orchestration/workflow.md skills/herdr-orchestration/owner.md \
    skills/herdr-orchestration/teardown.md skills/herdr-orchestration/recovery.md \
    skills/herdr-fleet/SKILL.md; do
    place "$setup_dir/$asset" "$home_dir/.agents/$asset"
done
place "$setup_dir/docs/model-selection.md" "$home_dir/.agents/skills/herdr-orchestration/model-selection.md"
if [ -e "$home_dir/.agents/skills/herdr-fleet/local.md" ] || [ -L "$home_dir/.agents/skills/herdr-fleet/local.md" ]; then
    printf '%s\n' 'PRESERVE ~/.agents/skills/herdr-fleet/local.md (private overlay)'
fi
if command -v herdr >/dev/null 2>&1; then
    if herdr --skill >"$work_dir/herdr-skill.md" && [ -s "$work_dir/herdr-skill.md" ]; then
        place "$work_dir/herdr-skill.md" "$home_dir/.agents/skills/herdr/SKILL.md"
    else
        printf '%s\n' 'WARN: herdr --skill produced nothing; skill not refreshed' >&2
    fi
fi
for asset in herdr-launch herdr-loop herdr-worktrees herdr-lead herdr-hub herdr-lanes.sh; do
    place "$setup_dir/bin/$asset" "$home_dir/.local/bin/$asset" 0755
done
place "$setup_dir/lanes.json" "$home_dir/.local/share/herdr-setup/lanes.json"
place "$setup_dir/intake.example.yaml" "$home_dir/.local/share/herdr-setup/intake.example.yaml"
instructions_src=$setup_dir/claude/CLAUDE.md
instructions_overlay=$home_dir/.config/herdr-fleet/instructions.local.md
if ! resolves_inside_home .config/herdr-fleet/instructions.local.md; then
    printf '%s\n' 'WARN: instructions.local.md leaves HOME through a symlink; ignored' >&2
elif [ -f "$instructions_overlay" ]; then
    instructions_src=$work_dir/instructions.md
    cat "$setup_dir/claude/CLAUDE.md" >"$instructions_src"
    printf '\n' >>"$instructions_src"
    cat "$instructions_overlay" >>"$instructions_src"
elif [ -e "$instructions_overlay" ] || [ -L "$instructions_overlay" ]; then
    printf '%s\n' 'WARN: instructions.local.md is not a regular file; ignored' >&2
fi
[ "$have_claude" -eq 0 ] || place "$instructions_src" "$home_dir/.claude/CLAUDE.md"
[ "$have_codex" -eq 0 ] || place "$instructions_src" "$home_dir/.codex/AGENTS.md"
if [ "$notifications" -eq 0 ]; then
    place "$setup_dir/claude/settings.json" "$home_dir/.claude/settings.json"
    place "$setup_dir/codex/hooks.json" "$home_dir/.codex/hooks.json"
fi
[ "$have_opencode" -eq 0 ] || place "$setup_dir/opencode/plugins/taskr-hook.js" "$home_dir/.config/opencode/plugins/taskr-hook.js"
[ "$have_pi" -eq 0 ] || place "$setup_dir/pi/extensions/taskr-hook.ts" "$home_dir/.pi/agent/extensions/taskr-hook.ts"
taskr_state_dir=$home_dir/.local/state/taskr
taskr_addr_file=$taskr_state_dir/dashboard.addr
taskr_hub_file=$taskr_state_dir/hub.url
taskr_server_file=$taskr_state_dir/server.url
taskr_db_file=$taskr_state_dir/taskr.db
taskr_addr_is_hub=0
if [ -f "$taskr_addr_file" ]; then
    case $(sed -n 1p "$taskr_addr_file" | tr -d '[:space:]') in
        tailnet | tailnet:*) taskr_addr_is_hub=1 ;;
    esac
fi
if [ "$taskr_role" = hub ]; then
    if [ -e "$taskr_hub_file" ] || [ -L "$taskr_hub_file" ]; then
        printf 'MERGE %s exists from a legacy taskr peer; the hub role needs tailnet in %s and no hub.url. Remove %s by hand, then re-run with --taskr-hub. Nothing written.\n' \
            "$taskr_hub_file" "$taskr_addr_file" "$taskr_hub_file"
    else
        printf '%s\n' tailnet >"$work_dir/dashboard.addr"
        place "$work_dir/dashboard.addr" "$taskr_addr_file"
        if [ "$place_result" = INSTALL ] || [ "$place_result" = UPDATE ]; then
            add_next_step 'Apply the taskr hub role:'
            add_next_step 'taskr daemon --restart'
        fi
    fi
elif [ "$taskr_role" = client ]; then
    if [ "$taskr_addr_is_hub" -eq 1 ]; then
        printf 'MERGE %s says tailnet, so this machine is the taskr hub and server; it cannot be a client. Nothing written.\n' \
            "$taskr_addr_file"
    elif [ -e "$taskr_db_file" ] || [ -L "$taskr_db_file" ]; then
        printf 'MERGE %s exists; a client keeps no ledger. First write the server URL to ~/.local/state/taskr/server.url, then stop the taskr daemon (the Herdr plugin restarts it in client mode). Archive the ledger with sqlite3 .backup, set pragma journal_mode=delete on the copy, run integrity_check and chmod 0444, then remove the ledger with its -wal/-shm files and re-run with --taskr-client. Nothing written.\n' \
            "$taskr_db_file"
    else
        printf '%s\n' "$taskr_server_url" >"$work_dir/server.url"
        place "$work_dir/server.url" "$taskr_server_file"
        if [ "$place_result" = INSTALL ] || [ "$place_result" = UPDATE ]; then
            add_next_step 'Start the client daemon (later: taskr daemon --restart):'
            add_next_step 'sh ~/.local/share/taskr/plugin/run.sh'
        fi
    fi
fi

if [ -f "$taskr_addr_file" ]; then
    case $(sed -n 1p "$taskr_addr_file" | tr -d '[:space:]') in
        tailnet | tailnet:*)
            place "$setup_dir/systemd/taskr.service" "$home_dir/.config/systemd/user/taskr.service"
            case $place_result in
                INSTALL)
                    add_next_step 'Run once, unless loginctl show-user "$USER" -p Linger already says yes:'
                    add_next_step 'loginctl enable-linger'
                    add_next_step 'Start the ledger daemon at boot and keep it up without Herdr:'
                    add_next_step 'systemctl --user daemon-reload && systemctl --user enable --now taskr.service'
                    add_next_step 'End the unsupervised daemon so the unit daemon takes over:'
                    add_next_step 'kill "$(taskr --json daemon --status | jq -er '\''select(.supervised|not).pid'\'')"'
                    ;;
                UPDATE)
                    add_next_step 'Reload and restart the updated unit:'
                    add_next_step 'systemctl --user daemon-reload && systemctl --user restart taskr.service'
                    ;;
            esac
            ;;
    esac
fi
