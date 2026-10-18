#!/bin/sh
# Fail when a shared file carries private host, path, credential, or usage data.
# Scans every file under products/setup (a clone or an extracted ZIP) except
# .git, local/, and skills/herdr-fleet/local.md. Prints only path, line, and
# category, never the matched text. Exit 0 clean, 1 findings, 2 scan error.
# Allowed examples: hub.example.ts.net, 100.64.0.1, 100.64.0.2.
# ponytail: pattern denylist, not proof of privacy; review diffs before sharing.
command -v python3 >/dev/null 2>&1 || { echo 'ERROR: python3 is required' >&2; exit 2; }
case $0 in */*) root=${0%/*}/.. ;; *) root=.. ;; esac
exec python3 - "$root" <<'EOF'
import os, re, stat, sys

EXCLUDED = {".git", "local", "skills/herdr-fleet/local.md"}
CHECKS = [
    ("tailnet DNS name",
     re.compile(r"(?<![A-Za-z0-9.-])[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net(?![A-Za-z0-9-])"),
     {"hub.example.ts.net"}),
    ("Tailscale IPv4",
     re.compile(r"(?<![0-9.])100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}(?![0-9])"),
     {"100.64.0.1", "100.64.0.2"}),
    ("home directory path", re.compile(r"/(?:home|Users)/[A-Za-z0-9_]"), set()),
    ("embedded credential", re.compile(
        r"gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}"
        r"|xox[abpr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY"
        r"|://[^/\s:@]+:[^/\s@]+@"), set()),
    ("usage audit", re.compile(
        r"Usage\ audit|weekly-%\ points|at\ API\ prices\ \(|Logs\ from\ both\ machines"), set()),
]


def fail(err):
    raise err


def main(root):
    findings = 0
    for top, dirs, files in os.walk(root, onerror=fail):
        rel_top = os.path.relpath(top, root)
        for name in sorted(dirs + files):
            rel = name if rel_top == "." else rel_top + "/" + name
            if rel in EXCLUDED:
                if name in dirs:
                    dirs.remove(name)
                continue
            mode = os.lstat(os.path.join(top, name)).st_mode
            if stat.S_ISDIR(mode):
                continue
            if not stat.S_ISREG(mode):
                kind = "symlink" if stat.S_ISLNK(mode) else "special file"
                print("%s: %s is not allowed in shared files" % (rel, kind))
                findings += 1
                if name in dirs:
                    dirs.remove(name)
                continue
            with open(os.path.join(top, name), "rb") as f:
                text = f.read().decode("latin-1")
            if "\0" in text:
                print("%s: contains NUL bytes; binary files are not supported" % rel)
                findings += 1
                continue
            for number, line in enumerate(text.split("\n"), 1):
                for label, pattern, allowed in CHECKS:
                    if any(m.group(0) not in allowed for m in pattern.finditer(line)):
                        print("%s:%d: %s" % (rel, number, label))
                        findings += 1
    return findings


try:
    found = main(os.path.realpath(sys.argv[1]))
except Exception as err:
    # Name the failure without echoing any file content.
    path = getattr(err, "filename", None) or ""
    print("ERROR: scan failed (%s) %s" % (type(err).__name__, path), file=sys.stderr)
    sys.exit(2)
if found:
    print("%d privacy finding(s); matched text is never printed" % found)
    sys.exit(1)
print("privacy check passed")
EOF
