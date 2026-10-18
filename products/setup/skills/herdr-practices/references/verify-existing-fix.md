# Verify an existing fix

Use when a concrete existing patch plausibly addresses a reported symptom.
The existing artifact owns the fix; verify it without editing or competing with it.
A branch name, tracker status, or discussion claim alone is not a qualifying patch.
Choose a linked change or commit with matching code and state why it qualifies.

## Prepare comparable runs

Record the report, artifact URL, baseline revision, patched revision, and symptom.
For an open change, choose its base; for a merged change, use the preceding revision
when it builds and represents the broken behavior. Explain any different baseline.
Use a clean checkout or supported isolation without overwriting user changes.
Record the shared build, data, account, flags, environment, and driver inputs.
Use the project's existing launch, doctor, drive, observe, and cleanup instructions.
Confirm the exact app, build, and test environment before each attempt.
Never drive a production or someone else's instance under an assumed test identity.

## Establish the baseline

1. Launch the baseline and check it is usable, not merely listening.
2. Drive the reported user path with real inputs through the supported surface.
3. Observe the discriminating broken state and capture actions plus resulting state.
4. Cross-check the relevant state read-only, such as output, logs, or stored values.
5. Reset to a known starting state and repeat the symptom a second time.

If the symptom is not seen twice, the baseline is missing and the result is inconclusive.
A passing test or a plausible cause does not substitute for the broken baseline.
Label environment translations and explain which part of the defect they exercise.
If the missing platform or hardware is part of the defect, keep that proof unrun.

## Exercise the patch

Build and launch the specified patch with the same inputs and fixture state.
Drive the same path twice, resetting between attempts.
Confirm both the absence of the broken state and presence of the expected state.
Capture the same action trace and read-only cross-check as for the baseline.
Compilation and unit tests can support the result but cannot replace these runs.

## Outcomes and cleanup

- Confirmed: the baseline breaks twice and the patch resolves it twice.
- Insufficient fix: the established symptom remains on the patched build.
- Inconclusive: either build cannot run, the baseline is missing, or observations
  cannot distinguish broken behavior from the expected state.

Report revisions, both attempts per side, evidence paths, outcome, and gaps.
Retain artifacts outside disposable instance state and confirm they survive cleanup.
Stop only processes created for this run; remove only owned temporary state.
Restore disposable fixtures without discarding user work.
Do not author a competing fix by default. Further implementation requires task scope.
This verification does not authorize comments, tracker writes, commits, or publication.
