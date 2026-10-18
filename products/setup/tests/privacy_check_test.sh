#!/bin/sh
# Regression check for tests/privacy_check.sh. Every fixture is generated in a
# fresh temporary tree; private-looking values are assembled at run time so this
# file itself stays clean.
checker=$(cd "${0%/*}" && pwd)/privacy_check.sh
failures=0
token="gh""p_$(printf 'A%.0s' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30)"
tail_host="node.tail0f.ts"".net"

# case_run NAME EXPECTED_RC [EXPECTED_OUTPUT]: runs the checker in $tree.
case_run() {
    out=$(sh "$tree/tests/privacy_check.sh" 2>&1)
    rc=$?
    problem=
    [ "$rc" -eq "$2" ] || problem="rc $rc, want $2"
    case $out in *"$token"* | *"$tail_host"*) problem="$problem; matched value printed" ;; esac
    if [ -n "${3:-}" ]; then
        case $out in *"$3"*) ;; *) problem="$problem; missing '$3'" ;; esac
    fi
    if [ -n "$problem" ]; then
        printf 'FAIL %s: %s\n%s\n' "$1" "$problem" "$out"
        failures=$((failures + 1))
    else
        printf 'ok   %s\n' "$1"
    fi
}

new_tree() {
    tree=$(mktemp -d) || exit 2
    mkdir -p "$tree/tests" "$tree/skills/herdr-fleet" && cp "$checker" "$tree/tests/" || exit 2
    printf '%s\n' 'client http://hub.example.ts.net:7788 or http://100.64.0.1:7788 and 100.64.0.2' >"$tree/README.md"
}

new_tree
case_run 'allowed examples pass' 0 'privacy check passed'

new_tree
printf 'token=%s\n' "$token" >"$tree/new.txt"
case_run 'credential reported without its value (F1)' 1 'new.txt:1: embedded credential'

new_tree
printf 'x\000token=%s\n' "$token" >"$tree/new.txt"
case_run 'NUL-containing file rejected (F2)' 1 'new.txt: contains NUL bytes'

new_tree
ln -s missing "$tree/dangling"
case_run 'dangling symlink rejected (F2)' 1 'dangling: symlink is not allowed'

new_tree
ln -s README.md "$tree/link.md"
case_run 'symlink to a clean file rejected' 1 'link.md: symlink is not allowed'

new_tree
mkdir -p "$tree/local" "$tree/.git"
printf '%s %s\n' "$tail_host" "$token" >"$tree/local/notes.md"
printf '%s %s\n' "$tail_host" "$token" >"$tree/skills/herdr-fleet/local.md"
printf '%s\n' "$tail_host" >"$tree/.git/config"
case_run 'private notes and .git excluded (F4)' 0 'privacy check passed'

new_tree
printf '%s\n' "$tail_host" >"$tree/skills/herdr-fleet/notes.md"
case_run 'ordinary new file still scanned' 1 'skills/herdr-fleet/notes.md:1: tailnet DNS name'

new_tree
printf 'hub.example.ts.net %s\n' "$tail_host" >"$tree/dns.txt"
case_run 'allowed DNS does not hide a real one' 1 'dns.txt:1: tailnet DNS name'

new_tree
printf '100.64.0.1 100.64.0.1%s\n' 0 >"$tree/ip.txt"
case_run 'allowed IP does not hide a real one' 1 'ip.txt:1: Tailscale IPv4'

if [ "$(id -u)" -ne 0 ]; then
    new_tree
    mkdir "$tree/sealed" && chmod 000 "$tree/sealed"
    case_run 'unreadable directory fails closed (F3)' 2 'ERROR: scan failed'
    chmod 755 "$tree/sealed"

    new_tree
    printf 'x\n' >"$tree/sealed.txt" && chmod 000 "$tree/sealed.txt"
    case_run 'unreadable file fails closed (F3)' 2 'ERROR: scan failed'
    chmod 644 "$tree/sealed.txt"
fi

new_tree
out=$(PATH=/nonexistent /bin/sh "$tree/tests/privacy_check.sh" 2>&1)
if [ $? -eq 2 ]; then echo 'ok   missing python3 fails closed'; else echo "FAIL missing python3: $out"; failures=$((failures + 1)); fi

printf '%s failures\n' "$failures"
[ "$failures" -eq 0 ]
