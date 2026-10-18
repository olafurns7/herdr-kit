---
name: herdr-practices
description: An on-demand reference index for research, review, measurement, fix verification, and retrospectives; not always-on and not a mode.
---

# Herdr practices

Open only the reference needed for the current task:

- [Research](references/research.md): explain behavior or historical rationale, with evidence and gaps.
- [Design](references/design.md): shape costly code and architecture decisions before implementation; compare candidates only for a flagged decision.
- [Runbooks](references/runbooks.md): fix bugs, build features, refactor, diagnose captures, or improve measured performance.
- [Review](references/review.md): assess a change, its decisive safety assumption, and disputed findings.
- [Measurement](references/measurement.md): produce or assess a performance number.
- [Verify an existing fix](references/verify-existing-fix.md): compare a reported symptom before and after an existing patch.
- [Incident intake](references/incident-intake.md): triage a bounded source batch and reproduce read-only, with access gaps and existing owners.
- [Retrospective](references/retrospective.md): turn demonstrated mistakes into proposals for an existing owner.
- [Writing](references/writing.md): edit briefs, reports, findings, PR bodies, owner asks, docs, and comments for clear claims and less filler.

Work directly within a bounded task. Only an authorized parent may assign a
HERDR-BRIEF through taskr and `herdr agent start`, using a separate tab in the
task workspace with `FLEET_ROLE=worker --no-focus`. The first helper in a new
task workspace uses its existing root tab. Helpers do not create their own
workspaces or worktrees, split the owner's pane, or start in-process agents.
These references grant no permission to publish, merge, operate watchers,
change services, or edit memory. Task authorization and host policy govern.

Adapted from pstack by Lauren Tan (2026); license and provenance: [third_party/pstack](../../../../third_party/pstack/).
