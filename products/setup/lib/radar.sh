# Explicit --radar opt-in; original pin, helpers, and refusal behavior.
RADAR_REPO=olafurns7/herdr-radar
RADAR_COMMIT=f0dcd825267b78671c29c0c9dd89e71ab5b42042
RADAR_PLUGIN=hhdebb.herdr-radar

manual_radar() {
    printf '%s\n' 'MANUAL: if a radar is already installed, stop it first and wait up to 10 seconds for animator.pid to name no live process:'
    printf 'herdr plugin action invoke %s.state-stop\n' "$RADAR_PLUGIN"
    printf '%s\n' 'MANUAL: install the patched radar inside Herdr with jq on PATH:'
    printf 'herdr plugin install %s --ref %s -y\n' "$RADAR_REPO" "$RADAR_COMMIT"
    printf '%s\n' 'MANUAL: find the radar settings file:'
    printf 'herdr plugin config-dir %s\n' "$RADAR_PLUGIN"
    printf '%s\n' 'MANUAL: add missing top-level parent_token = "taskr_parent", parent_label_token = "taskr_campaign", space_owner = false, attention_token = "taskr_owner_ask", space_logo_names = false; preserve existing values. On Linux, add [colors] and active_row_bg_light = "" only if both are absent.'
    printf '%s\n' 'MANUAL: back up Herdr config once before applying:'
    printf '%s\n' 'test -e "$HOME/.config/herdr/config.toml.pre-radar-$(date -u +%F)" || cp "$HOME/.config/herdr/config.toml" "$HOME/.config/herdr/config.toml.pre-radar-$(date -u +%F)"'
    printf '%s\n' 'MANUAL: start radar and wait up to 10 seconds for a live animator.pid; repeat once if needed:'
    printf 'herdr plugin action invoke %s.state-start\n' "$RADAR_PLUGIN"
    printf '%s\n' 'MANUAL: apply the radar settings:'
    printf 'herdr plugin action invoke %s.configure\n' "$RADAR_PLUGIN"
}


radar_file_warn() {
    radar_file_failed=1
    printf 'WARN: herdr-radar could not access %s; skipping radar setup\n' "$1"
    if [ "$radar_state_stopped" -eq 1 ]; then
        printf '%s\n' "WARN: herdr-radar's state process was stopped; start it with:"
        printf 'herdr plugin action invoke %s.state-start\n' "$RADAR_PLUGIN"
    fi
}

radar_file_contains() {
    radar_match_rc=0
    grep "$1" "$2" "$3" >/dev/null || radar_match_rc=$?
    if [ "$radar_match_rc" -gt 1 ]; then
        radar_file_warn "$3"
    fi
    return "$radar_match_rc"
}

# Read only top-level keys; new keys must precede the first TOML table.
ensure_radar_setting() {
    if ! radar_value=$(awk -v key="$1" '
        /^[[:space:]]*\[/ { exit }
        $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
            sub(/^[^=]*=[[:space:]]*/, "")
            sub(/[[:space:]]+#.*$/, "")
            sub(/[[:space:]]*$/, "")
            print; exit
        }' "$radar_config"); then
        radar_file_warn "$radar_config"
        return 1
    fi
    if awk -v key="$1" '
        /^[[:space:]]*\[/ { exit }
        $0 ~ "^[[:space:]]*" key "[[:space:]]*=" { found=1; exit }
        END { exit !found }' "$radar_config"; then
        if [ "$radar_value" != "$2" ]; then
            printf 'KEEP herdr-radar %s (existing value differs)\n' "$1"
        fi
    else
        radar_key_rc=$?
        if [ "$radar_key_rc" -ne 1 ]; then
            radar_file_warn "$radar_config"
            return 1
        fi
        if ! awk -v setting="$1 = $2" '
            /^[[:space:]]*\[/ && !inserted { print setting; inserted=1 }
            { print }
            END { if (!inserted) print setting }' "$radar_config" >"$work_dir/radar-config.toml"; then
            radar_file_warn "$radar_config (staging: $work_dir/radar-config.toml)"
            return 1
        fi
        if ! cat "$work_dir/radar-config.toml" >"$radar_config"; then
            radar_file_warn "$radar_config"
            return 1
        fi
        radar_value=$2
        radar_changed=1
    fi
}

radar_pid_alive() {
    [ -f "$radar_pid_file" ] || return 1
    radar_pid=$(cat "$radar_pid_file" 2>/dev/null) || return 1
    case $radar_pid in
        '' | *[!0-9]* | 0) return 1 ;;
    esac
    kill -0 "$radar_pid" 2>/dev/null
}

wait_radar_pid() {
    radar_wait=0
    while [ "$radar_wait" -lt 10 ]; do
        if radar_pid_alive; then
            [ "$1" = live ] && return 0
        else
            [ "$radar_file_failed" -eq 0 ] || return 1
            [ "$1" = stopped ] && return 0
        fi
        sleep 1
        radar_wait=$((radar_wait + 1))
    done
    if [ "$1" = live ]; then
        radar_pid_alive
    else
        if radar_pid_alive; then
            return 1
        fi
        [ "$radar_file_failed" -eq 0 ]
    fi
}

install_radar() {
    radar_changed=0
    radar_file_failed=0
    radar_state_stopped=0
    if ! radar_json=$(herdr plugin list --plugin "$RADAR_PLUGIN" --json) ||
        ! radar_source=$(printf '%s\n' "$radar_json" | jq -er '.result.plugins | if length == 0 then "absent" else .[0].source | [.kind, .owner, .repo, .resolved_commit] | join("/") end'); then
        printf 'WARN: herdr plugin list --plugin %s --json failed; skipping radar setup\n' "$RADAR_PLUGIN"
        return
    fi
    if [ "$radar_source" != "github/$RADAR_REPO/$RADAR_COMMIT" ]; then
        if [ "$radar_source" != absent ]; then
            radar_state_stopped=1
            herdr plugin action invoke "$RADAR_PLUGIN.state-stop" ||
                printf 'WARN: herdr plugin action invoke %s.state-stop failed\n' "$RADAR_PLUGIN"
        fi
        if ! herdr plugin install "$RADAR_REPO" --ref "$RADAR_COMMIT" -y; then
            printf 'WARN: herdr plugin install %s --ref %s -y failed; skipping radar setup\n' "$RADAR_REPO" "$RADAR_COMMIT"
            return
        fi
        radar_changed=1
    fi
    if ! radar_dir=$(herdr plugin config-dir "$RADAR_PLUGIN") || [ -z "$radar_dir" ]; then
        printf 'WARN: herdr plugin config-dir %s failed; skipping radar setup\n' "$RADAR_PLUGIN"
        return
    fi
    if ! mkdir -p "$radar_dir"; then
        radar_file_warn "$radar_dir"
        return
    fi
    radar_config=$radar_dir/config.toml
    if [ ! -f "$radar_config" ]; then
        if ! touch "$radar_config" 2>/dev/null; then
            radar_file_warn "$radar_config"
            return
        fi
        radar_changed=1
    fi
    ensure_radar_setting parent_token '"taskr_parent"' || return 0
    ensure_radar_setting parent_label_token '"taskr_campaign"' || return 0
    ensure_radar_setting space_owner false || return 0
    radar_space_owner=$radar_value
    ensure_radar_setting attention_token '"taskr_owner_ask"' || return 0
    ensure_radar_setting space_logo_names false || return 0
    # Linux desktop light/dark says nothing about the attached terminal and would add a light selected-row fill.
    if [ "$(uname -s)" = Linux ] &&
        ! radar_file_contains -Eq '^[[:space:]]*\[[[:space:]]*colors[[:space:]]*\]([[:space:]]|#|$)' "$radar_config" &&
        [ "$radar_file_failed" -eq 0 ] &&
        ! radar_file_contains -Eq '^[[:space:]]*active_row_bg_light[[:space:]]*=' "$radar_config"; then
        [ "$radar_file_failed" -eq 0 ] || return 0
        if ! printf '\n%s\n%s\n' '[colors]' 'active_row_bg_light = ""' >>"$radar_config"; then
            radar_file_warn "$radar_config"
            return
        fi
        radar_changed=1
    fi
    [ "$radar_file_failed" -eq 0 ] || return 0
    if [ "$radar_space_owner" = true ] && [ -f "$herdr_config_dest" ] &&
        ! radar_file_contains -F '$space_owner' "$herdr_config_dest"; then
        [ "$radar_file_failed" -eq 0 ] || return 0
        radar_changed=1
    fi
    if [ "$radar_changed" -eq 0 ]; then
        printf '%s\n' 'SKIP herdr-radar (already configured)'
        return
    fi
    radar_backup=$herdr_config_dest.pre-radar-$(date -u +%F)
    if [ -f "$herdr_config_dest" ] && [ ! -e "$radar_backup" ]; then
        if ! cp "$herdr_config_dest" "$radar_backup"; then
            radar_file_warn "$radar_backup"
            return
        fi
    fi
    radar_pid_file=$home_dir/.local/state/herdr/plugins/$RADAR_PLUGIN/animator.pid
    radar_state_stopped=1
    herdr plugin action invoke "$RADAR_PLUGIN.state-stop" ||
        printf 'WARN: herdr plugin action invoke %s.state-stop failed\n' "$RADAR_PLUGIN"
    wait_radar_pid stopped || {
        [ "$radar_file_failed" -eq 0 ] || return 0
        printf 'WARN: herdr plugin action invoke %s.state-stop left a live pid after 10s\n' "$RADAR_PLUGIN"
    }
    herdr plugin action invoke "$RADAR_PLUGIN.state-start" ||
        printf 'WARN: herdr plugin action invoke %s.state-start failed\n' "$RADAR_PLUGIN"
    if ! wait_radar_pid live; then
        [ "$radar_file_failed" -eq 0 ] || return 0
        herdr plugin action invoke "$RADAR_PLUGIN.state-start" ||
            printf 'WARN: herdr plugin action invoke %s.state-start failed\n' "$RADAR_PLUGIN"
        wait_radar_pid live || {
            [ "$radar_file_failed" -eq 0 ] || return 0
            printf 'WARN: herdr-radar state process is not running; start it by hand: herdr plugin action invoke %s.state-start\n' "$RADAR_PLUGIN"
        }
    fi
    herdr plugin action invoke "$RADAR_PLUGIN.configure" ||
        printf 'WARN: herdr plugin action invoke %s.configure failed\n' "$RADAR_PLUGIN"
}
