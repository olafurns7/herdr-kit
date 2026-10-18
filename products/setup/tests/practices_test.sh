#!/bin/sh
# Content contract only; the installer owns placement and client discovery.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
exec python3 - "$root" <<'PY'
import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

root = Path(sys.argv[1])
errors = []
orchestration = root / "products/setup/skills/herdr-orchestration"
continuity = {
    "SKILL.md": (
        "Intent: <optional: the orchestrator's own words for what done looks like for the owner>",
        "Non-goals: <explicitly excluded outcomes>",
        "Validation: <exact commands, one per acceptance item>",
    ),
    "workflow.md": (
        "Sequence independently verifiable slices",
        "name each slice's check",
        "disjoint lane coverage with no gaps",
        "Before reporting done, audit each claim against its evidence",
        "commit, check output, review verdict",
        "name what was not verified",
    ),
    "recovery.md": (
        "Before a planned stop, handover, or context compaction",
        "[campaign plan](workflow.md#goal-plan-and-documents)",
        "taskr handover --as <me> --note",
        "evidence for done work, each in-flight lane's expected next event, and proof gaps",
        "On resume, read the campaign plan, ledger, and latest reports before re-deriving work",
    ),
    "owner.md": (
        "reversible execution choices within the authorized scope",
        "take the reasonable default, record it with `taskr decide`, and keep going",
        "Ask for destructive, irreversible, outward-facing, product-direction, or authority decisions",
    ),
}
for name, criteria in continuity.items():
    body = (orchestration / name).read_text()
    for criterion in criteria:
        if criterion not in body:
            errors.append(f"orchestration {name} is missing continuity rule: {criterion}")
# Slice C allows about 25 lines above the 382-line core baseline.
if sum(len((orchestration / name).read_text().splitlines()) for name in continuity) > 407:
    errors.append("orchestration continuity core exceeds its line budget")
review = (root / "products/setup/skills/herdr-practices/references/review.md").read_text()
if len(review.splitlines()) > 80:
    errors.append("review reference exceeds 80 lines")
for criterion in (
    "author's stated intent", "[Runbooks](runbooks.md#frame-the-work)",
    "[root-cause path](runbooks.md#bug-fix)",
    "Validate external data at entry boundaries", "boundary enforces their assumptions",
    "layers to trace and hidden mutable state", "when worth the cost",
    "owning schema", "Test observable results, errors, and external effects",
    "catches broken behavior and survives an internal-only refactor",
    "Do not count votes or use reviewer agreement as proof or approval.",
    "When dismissing a finding, record its claim, the checked path, and the reason.",
    "An empty findings list is valid; there is no minimum quota of criticism.",
    "unresolved high-risk claims open",
    "open Act finding makes the verdict not ready",
    "supplies no authority to modify code", "[Writing](writing.md)",
):
    if criterion not in review:
        errors.append(f"review reference is missing criterion: {criterion}")
for disposition in ("Act", "Consider", "Noted", "Dismissed"):
    if not re.search(rf"^- {disposition}: .+", review, re.M):
        errors.append(f"review reference is missing disposition: {disposition}")
intake = root / "products/setup/skills/herdr-practices/references/incident-intake.md"
if not intake.is_file():
    errors.append("incident intake reference is missing")
elif len(intake.read_text().splitlines()) > 120:
    errors.append("incident intake reference exceeds 120 lines")
writing = root / "products/setup/skills/herdr-practices/references/writing.md"
if not writing.is_file() or not writing.read_text().strip():
    errors.append("writing reference is missing or empty")
elif len(writing.read_text().splitlines()) > 80:
    errors.append("writing reference exceeds 80 lines")
index = root / "products/setup/skills/herdr-practices/SKILL.md"
if "[Writing](references/writing.md)" not in index.read_text():
    errors.append("writing reference is not indexed")
runbooks = root / "products/setup/skills/herdr-practices/references/runbooks.md"
if not runbooks.is_file() or not runbooks.read_text().strip():
    errors.append("runbooks reference is missing or empty")
else:
    body = runbooks.read_text()
    if len(body.splitlines()) > 150:
        errors.append("runbooks reference exceeds 150 lines")
    for recipe in ("Bug fix", "Feature", "Refactoring", "Runtime forensics", "Performance"):
        if f"## {recipe}\n" not in body:
            errors.append(f"runbooks recipe is missing: {recipe}")
if "[Runbooks](references/runbooks.md)" not in index.read_text():
    errors.append("runbooks reference is not indexed")
design = root / "products/setup/skills/herdr-practices/references/design.md"
if not design.is_file() or not design.read_text().strip():
    errors.append("design reference is missing or empty")
else:
    body = design.read_text()
    if len(body.splitlines()) > 90:
        errors.append("design reference exceeds 90 lines")
    for section in ("Shape before code", "Red flags", "Re-check during implementation",
                    "Arena for a flagged decision", "Throwaway prototypes"):
        if f"## {section}\n" not in body:
            errors.append(f"design section is missing: {section}")
if "[Design](references/design.md)" not in index.read_text():
    errors.append("design reference is not indexed")
# SHA-256 of pstack/LICENSE at cursor/plugins
# f713f14203cd6420cec13cd7c55c32d598dbab68; no network or source checkout needed.
license_sha = "bc957ca6bee02792566a1a028d105e02e247c6e77cf057061674273da77b200e"
license_path = root / "third_party/pstack/LICENSE"
if not license_path.is_file() or hashlib.sha256(license_path.read_bytes()).hexdigest() != license_sha:
    errors.append("third_party/pstack/LICENSE: missing or differs from pinned upstream bytes")

for name in ("herdr-practices", "herdr-verification"):
    skill = root / "products/setup/skills" / name
    entry = skill / "SKILL.md"
    text = entry.read_text() if entry.is_file() else ""
    header = re.match(r"\A---\n(.*?)\n---\n", text, re.S)
    fields = {}
    # The pack uses two single-line YAML scalars; reject other shapes fail-closed.
    valid = header is not None
    for line in header[1].splitlines() if header else []:
        field = re.fullmatch(r"(name|description): (.+)", line)
        if not field or field[1] in fields:
            valid = False
            continue
        key, value = field.groups()
        if value.startswith('"'):
            try:
                value = json.loads(value)
            except ValueError:
                valid = False
        elif (not re.match(r"[A-Za-z]", value) or ": " in value or " #" in value
              or value.lower() in {"null", "true", "false", "yes", "no", "on", "off"}):
            valid = False
        fields[key] = value
    description = fields.get("description")
    if (not valid or fields.get("name") != name or not isinstance(description, str)
            or not description.strip()):
        errors.append(f"{entry.relative_to(root)}: invalid name/description frontmatter")

    for path in sorted(skill.rglob("*")):
        if not path.is_file():
            continue
        body = path.read_text()
        if re.search(r"subagent|Task tool|cursor|\.mdc|spawn_agent", body, re.I):
            errors.append(f"{path.relative_to(root)}: forbidden tooling string")
        # Inline links (including images) and reference-style link definitions.
        links = re.findall(r"\[[^\]\n]*\]\(\s*(<[^>\n]+>|[^\s)]+)", body)
        links += re.findall(r"^\s*\[[^\]\n]+\]:\s*(<[^>\n]+>|\S+)", body, re.M)
        for target in links:
            url = urlsplit(target.strip("<>"))
            if url.scheme or url.netloc or not url.path:
                continue
            dest = path.parent / unquote(url.path)
            if url.path.startswith("/") or not dest.exists():
                errors.append(f"{path.relative_to(root)}: broken relative link {target}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print("practices content check passed")
PY
