#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd -P)
root=$(mktemp -d "${TMPDIR:-/tmp}/statusline-test.XXXXXX")
trap 'rm -rf "$root"' 0
trap 'exit 1' 1 2 3 15
export HOME="$root/home" XDG_CACHE_HOME="$root/cache" TZ=UTC
export CLAUDE_CONFIG_DIR="$HOME/codator/claude/claude3/native"
export FAKE_LOG="$root/codator.log"
mkdir -p "$HOME" "$root/bin" "$CLAUDE_CONFIG_DIR"
cat > "$root/bin/codator" <<'STUB'
#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_LOG"
exit 1
STUB
chmod +x "$root/bin/codator"
export PATH="$root/bin:$PATH"

git init -q -b fixture "$root/work"
printf 'base\n' > "$root/work/file"
git -C "$root/work" add file
git -C "$root/work" -c user.name=Fixture -c user.email=fixture@example.com commit -qm base
printf 'base\nstaged\n' > "$root/work/file"
git -C "$root/work" add file
printf 'updated\nstaged\n' > "$root/work/file"
jq --arg cwd "$root/work" '.workspace.current_dir = $cwd' \
    "$repo_dir/tests/fixtures/statusline/input.json" > "$root/input.json"
usage="${CLAUDE_CONFIG_DIR%/*}/.claude-usage.json"
fixture="$repo_dir/tests/fixtures/statusline/usage.json"
base='fixture* │ Claude 1M·high·fast │ +2/-1 │ 5h 75%→22:13 7d 60%'
tail=' │ ctx 80%'

render() {
    bash "$repo_dir/claude/statusline.sh" < "$root/input.json" > "$root/raw"
    if grep -F 'example.com' "$root/raw"; then
        echo 'FAIL: account detail leaked' >&2; exit 1
    fi
    sed "s/$(printf '\033')\[[0-9;]*m//g" "$root/raw"
}
expect() {
    [ "$1" = "$2" ] || { printf 'FAIL: expected <%s>, got <%s>\n' "$2" "$1"; exit 1; }
}
fresh_usage() {
    jq '.observed_at = (now | strftime("%Y-%m-%dT%H:%M:%S.547322389Z"))' "$fixture" > "$usage"
}

fresh_usage
expect "$(render)" "$base fbl 42%$tail"
printf 'fresh Fable snapshot, remaining percent and installed layout: ok\n'

output=$(unset CLAUDE_CONFIG_DIR; render)
expect "$output" "$base$tail"
printf 'unset config directory: ok\n'
rm "$usage"
expect "$(render)" "$base$tail"
printf 'missing usage file: ok\n'
jq '.observed_at = (now | strftime("%Y-%m-%dT%H:%M:%S.547322389Z"))' \
    "$repo_dir/tests/fixtures/statusline/usage-no-fable.json" > "$usage"
expect "$(render)" "$base$tail"
printf 'missing Fable entry: ok\n'
jq '.observed_at = (now - 1860 | strftime("%Y-%m-%dT%H:%M:%S.547322389Z"))' "$fixture" > "$usage"
expect "$(render)" "$base$tail"
printf '31-minute-old snapshot: ok\n'
jq 'del(.observed_at)' "$fixture" > "$usage"
expect "$(render)" "$base$tail"
printf 'missing observation time: ok\n'
jq '.observed_at = "invalid"' "$fixture" > "$usage"
expect "$(render)" "$base$tail"
printf 'invalid observation time: ok\n'
printf '{invalid JSON\n' > "$usage"
expect "$(render)" "$base$tail"
printf 'malformed usage JSON: ok\n'
fresh_usage
jq '.snapshot.rate_limits.model_scoped[1].utilization = 100' "$usage" > "$root/zero.json"
mv "$root/zero.json" "$usage"
expect "$(render)" "$base fbl 0%$tail"
printf 'zero remaining quota: ok\n'
[ ! -e "$FAKE_LOG" ] || { echo 'FAIL: codator was called'; exit 1; }
[ ! -e "$XDG_CACHE_HOME" ] || { echo 'FAIL: statusline created a cache'; exit 1; }
printf 'no codator call or statusline-owned cache: ok\n'
printf 'statusline: ok\n'
