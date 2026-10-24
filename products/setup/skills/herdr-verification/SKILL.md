---
name: herdr-verification
description: Author or maintain a project verification map using its existing driver, real user paths, and durable evidence.
---

# Project verification maps

Use when asked to create or maintain a project's verification instructions.
This authors project-owned instructions; it does not install a runtime or driver.
Read the [feature-map contract](references/feature-map.md) before mapping journeys.
Generated output goes to a repository path only when that repository's instructions
allow it. Otherwise keep the proposed content in the task's authorized report path.
Preserve existing verification ownership and stay inside the granted edit scope.

For task-local proof profiles or a covered formal boundary, use
[task proof and code sync](references/task-proof.md). Select checks by risk;
trivial edits need no formal model. A map routes to owned checks, not extra agents.

## Author from the project

Inspect documented start commands, existing tests and harnesses, entry points,
fixtures, supported surfaces, and isolation. Reuse the project's driver first.
Identify the primary surface: UI, terminal, service API, mobile, or library API.
Record other surfaces and missing capabilities rather than silently excluding them.
Do not generate untested startup instructions against a broken checkout.
Report a startup block; fix product code only if the task separately permits it.

Write a project skill with valid name/description frontmatter and concrete steps:

- Launch: exact build/start command, revision, fixture setup, owned process/session,
  and a readiness predicate that means usable from fresh disposable state.
- Doctor: read-only app identity, build, instance ownership, readiness, and required
  access checks. Run before driving, on each fresh short-lived session, and after
  surprises. Reset or relaunch when a healthy process conceals a wedged user state.
- Drive: real user actions through the supported surface with stable handles.
  Use an existing browser, PTY, CLI, HTTP, or library harness as appropriate.
- Observe: action trace, discriminating result, and read-only side-effect checks.
  Never inject internal state to manufacture the behavior being verified.
- Evidence: exact durable output location, capture format, redaction, and retention.
  Evidence must survive cleanup of the disposable instance and fixture state.
- Cleanup: stop only owned processes/sessions and remove only owned scratch state.
  Preserve user work and evidence. Document executable helpers and their invocation.

Seed an index and a few concrete feature journeys from real source or documented
surfaces, using the linked contract. Leave no placeholder commands or selectors.
Run launch, doctor, one mapped journey, observe, and cleanup end to end.
Confirm the named evidence still exists after cleanup. Until that succeeds,
label the generated skill a draft and state the missing proof.
Report every other mapped feature as unrun; one exercised journey is not full coverage.
If a stand-in environment was used, list setup and target behavior it did not prove.

## Maintain the map

Read the index and every feature file; reconcile missing, dead, and duplicate entries.
Review source for every mapped feature and inspect changed user surfaces for omissions.
Record source entry points, suspected drift, and a bounded recipe for each feature.
One owner drives the live instance serially. Short-lived commands may use a fresh
isolated session per journey. Other readers never drive that same instance.
Exercise every mapped feature, or report it unrun with prerequisite, attempted path,
and exact reason. An unavailable feature is not a passing live check.
Doctor again after a failed drive, clean failed-attempt residue, and preserve evidence.
Distinguish documentation drift, a harness gap, and a product regression.
Do not rewrite the map to bless broken product behavior. Report product regressions.
Edit only authorized verification-owned files; re-drive every harness correction.
End with clean, changed, or blocked, plus per-feature coverage and evidence paths.
Clean requires source and live coverage of every feature. No automatic cadence applies.

Work directly within bounded scope. Only an authorized parent may assign a
HERDR-BRIEF through taskr and `herdr agent start` in a separate task-workspace tab
with `FLEET_ROLE=worker --no-focus`; use the existing root tab for the first helper
in a new task workspace. No in-process agents, worker-created workspaces/worktrees,
or splits of the owner's pane. This skill grants no publishing, merge, watcher,
service, memory, ticket, commit, or pull-request authority.

Adapted from pstack by Lauren Tan (2026); license and provenance: [third_party/pstack](../../../../third_party/pstack/).
