#!/usr/bin/env bash
# Claude Code status line.
# Base mirrors the bashrc PS1 (bold green user@host, bold blue cwd), then adds
# git branch, active model + effort, remaining usage quotas, and context left.

input=$(cat)

eval "$(printf '%s' "$input" | jq -r '
  @sh "cwd=\(.workspace.current_dir // .cwd // "")",
  @sh "model=\(.model.display_name // .model.id // "")",
  @sh "effort=\(.effort.level // "")",
  @sh "fast=\(.fast_mode // false | tostring)",
  @sh "h5=\(.rate_limits.five_hour.used_percentage // -1)",
  @sh "h5r=\(.rate_limits.five_hour.resets_at // 0)",
  @sh "d7=\(.rate_limits.seven_day.used_percentage // -1)",
  @sh "ctx=\(.context_window.remaining_percentage // -1)"
')"

[ -n "$cwd" ] || cwd=$PWD

R=$'\033[0m'; GRN=$'\033[01;32m'; BLU=$'\033[01;34m'; DIM=$'\033[02m'
MAG=$'\033[35m'; CYN=$'\033[36m'; YEL=$'\033[33m'; RED=$'\033[31m'
SEP="${DIM} │ ${R}"

# --- user@host:cwd (PS1) ---
disp=$cwd
case "$disp" in
  "$HOME") disp='~' ;;
  "$HOME"/*) disp="~${disp#"$HOME"}" ;;
esac
out="${GRN}${USER:-$(id -un)}@$(hostname -s)${R}:${BLU}${disp}${R}"

# --- git branch, dirty flag, upstream divergence ---
if git -C "$cwd" --no-optional-locks rev-parse --git-dir >/dev/null 2>&1; then
  branch=$(git -C "$cwd" --no-optional-locks branch --show-current 2>/dev/null)
  [ -n "$branch" ] || branch="@$(git -C "$cwd" --no-optional-locks rev-parse --short HEAD 2>/dev/null)"
  mark=""
  [ -n "$(git -C "$cwd" --no-optional-locks status --porcelain 2>/dev/null | head -1)" ] && mark="${YEL}*${R}"
  track=""
  if counts=$(git -C "$cwd" --no-optional-locks rev-list --left-right --count '@{u}...HEAD' 2>/dev/null); then
    behind=${counts%%	*}; ahead=${counts##*	}
    [ "$ahead" -gt 0 ] 2>/dev/null && track="${track}${GRN}↑${ahead}${R}"
    [ "$behind" -gt 0 ] 2>/dev/null && track="${track}${RED}↓${behind}${R}"
  fi
  out="${out} ${DIM}(${R}${MAG}${branch}${R}${mark}${track}${DIM})${R}"
fi

# --- model and effort ---
model=${model/ (1M context)/ 1M}
seg=""
[ -n "$model" ] && seg="${CYN}${model}${R}"
[ -n "$effort" ] && { [ -n "$seg" ] && seg="${seg}${DIM}·${R}"; seg="${seg}${effort}"; }
[ "$fast" = "true" ] && seg="${seg}${DIM}·${R}${YEL}fast${R}"
[ -n "$seg" ] && out="${out}${SEP}${seg}"

# color by remaining headroom: >50 green, >20 yellow, else red
hue() { if [ "$1" -gt 50 ]; then printf '%s' "$GRN"; elif [ "$1" -gt 20 ]; then printf '%s' "$YEL"; else printf '%s' "$RED"; fi; }

# --- remaining usage quotas ---
quota=""
if [ "$h5" -ge 0 ] 2>/dev/null; then
  left=$((100 - h5)); reset=""
  [ "$h5r" -gt 0 ] 2>/dev/null && reset="${DIM}→$(date -d "@$h5r" +%H:%M)${R}"
  quota="${DIM}5h${R} $(hue $left)${left}%${R}${reset}"
fi
if [ "$d7" -ge 0 ] 2>/dev/null; then
  left=$((100 - d7))
  [ -n "$quota" ] && quota="${quota} "
  quota="${quota}${DIM}7d${R} $(hue $left)${left}%${R}"
fi
[ -n "$quota" ] && out="${out}${SEP}${quota}"

# --- context window left ---
[ "$ctx" -ge 0 ] 2>/dev/null && out="${out}${SEP}${DIM}ctx${R} $(hue "$ctx")${ctx}%${R}"

printf '%s' "$out"
