# Task-local proof and code sync

Load for an approved work proof profile or a change to formally covered code.
Use the project's existing verification index and driver. This reference neither
installs tools nor authorizes runtime, mutations, builds or publication. The brief
still determines whether the worker inspects or the parent executes the proof.

## Select the smallest discriminating check

Use parser/leaf checks for trivial edits. Use an observable command or runtime
journey for behavior, errors, cleanup and real-client claims. For ordering,
authority or retry races that examples cover poorly, add a bounded formal check
at one useful boundary. Keep real transaction boundaries; a composite client
loop is not one atomic action. A formal pass establishes its stated model and
assumptions; runtime traces establish the exercised implementation path.

## Pin the acceptance inputs

Approve work/revision/run and checker launch, source commit/tree and clean state,
profile version/digest, dependencies and acceptance receipts, exact argv,
tool/environment identity, expected case IDs, deadlines and admission. Preserve
commands/exits, observed cases and artifact hashes outside disposable state.
Use the command docs for typed submission and canonical digest algorithms.

The gate is an assigned deterministic writer. Missing, duplicate, extra, skipped,
failed or unparsed required cases cannot pass. Command-only success establishes
no case set. A string saying PASS, a receipt count, result settlement or an idle
process does not establish all acceptance conditions or actual writer stop.
Independent review handles judgment; an executor cannot self-approve.

Reuse a receipt only when every relevant source, profile, dependency, tool,
configuration, environment and case input matches, with the dependency basis
recorded. Do not treat patch equality as runtime identity or rerun unrelated
checks without a changed relevant input. Name pass, fail, unrun, blocked, stale
or inconclusive explicitly; no missing capability becomes a passing check.

## Formal checks that can fail

Record named properties, harm, bounds, assumptions, omissions, tool identity,
code/action mapping and the last successful source revision. Bound the actual
state space and describe any fairness assumption separately from safety.
For each protected behavioral promise, require a named mutation and counterexample;
parse errors, tool crashes, timeouts and vacuous exploration are not negative
controls. Mutate real code only in a separately authorized disposable copy.
Replay supported traces through the real API/CLI and record any unsupported step.
Explain each trace as a short story in code terms; keep formal and runtime
observations separate. The optional `/statechart` guidance can help presentation;
it adds no required install, delegated agent or second proof engine.

## Maintain from source, not from a green result

On covered-source drift, block proof use and inspect changes since the sync pin,
including moved code and new writers of protected state. Classify no-effect
changes, model drift, an explicitly changed promise, or a possible code bug.
Update actions and their mapping to match real atomicity before judging results.
Keep an unexplained invariant failure as a finding; change a property only with
evidence that the intended promise changed. Report any reduced bound or omission.

Prepare a reviewed candidate code/action/hash map for the exact new source, with
current proof unrun. Rerun the correct model, named negative controls and supported
implementation replays. Record the candidate as verified and advance the last
successful source revision only after these pass. A hash refresh alone never clears
stale proof. Keep the historical success on blocked runs; label current proof
blocked until maintenance completes. Preserve evidence and distinguish drift from
implementation bugs; authorized product fixes remain a separate change.

The repository's
[verification index](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/verification/README.md)
records the inbox package's historical pre-v5 proof and current source-sync block.
It supplies no proof for the whole controller or native/cross-host paths.

Adapted from pstack by Lauren Tan (2026), pinned source and MIT notices:
[third_party/pstack](../../../../../third_party/pstack/).
