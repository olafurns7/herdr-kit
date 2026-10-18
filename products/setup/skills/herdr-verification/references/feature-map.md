# Feature-map contract

A project owns its verification map outside the installed reference pack.
Use the project's allowed verification path and preserve its existing layout.
Keep an index linking every feature file and a file for each mapped user purpose.
Identify the source entry point that establishes each feature exists.
During authoring, a small initial map is acceptable if omitted surfaces are named.
During maintenance, account for every mapped feature and newly discovered surface.

## Each feature records

- Purpose: the user-visible outcome and which sub-features belong to it.
- Entry path: screen/menu/control, terminal command, endpoint, or public API.
- Preconditions: disposable data, access, permissions, flags, and required services.
- Driver: existing harness, exact actions and inputs, and stable selectors or handles.
- States: relevant normal, focus, disabled, loading, empty, error, selected, or open
  states. Mark inapplicable states rather than simulating an irrelevant UI.
- Reset: a known starting state for an independent second attempt.
- Observation: the expected visible result and a read-only underlying cross-check.
- Evidence: action trace and final discriminating state at named durable paths.
- Gotchas: wrong surfaces, known dead ends, environment translations, and gaps.

Prefer accessible roles/names, labels, stable purpose attributes, prompt strings,
or public route paths. Avoid generated classes, child indexes, and stale coordinates.
If coordinates are necessary, take a fresh capture first and record the target.
Fixture setup may arrange preconditions; it must not inject the symptom itself.
Never replace a user path with hidden methods, direct storage edits, or DOM mutations.
Mocks belong only at an existing isolated external boundary and must be disclosed.

## One owned driver

Before driving, report capabilities and prove the requested app/build/environment.
Record the session, process or target identity, and what the run owns for cleanup.
Refuse an ambiguous shared or production target rather than guessing ownership.
For UIs, capture actions and the resulting state, with enough app identity visible.
For terminals, capture input, output, exit status, and relevant filesystem effects.
For services or libraries, use their real public entry point and inspect results.
Use read-only state checks to support observations; classify state-changing queries
as actions and require that they represent the supported user path.
Bound retries. Doctor after failure; reset state before repeating a journey.
Inspect what a dry-run actually changes before relying on that name for safety.

## Coverage and durable proof

For every mapped feature, record source coverage, live status, and evidence paths.
Live status is exercised with its result, or unrun with the exact blocking reason.
Record missing auth, data, hardware, platform, or service prerequisites explicitly.
Do not weaken a journey because its required fixture is missing.
Label translated environments and the part of the behavior they leave unproven.
Store evidence apart from disposable profiles and avoid secrets in captures or logs.
After each failed attempt and final teardown, verify captured evidence remains.
Cleanup stops only what this run started and removes only owned disposable state.
List anything left for the operator, why it remains, and how to identify it safely.
