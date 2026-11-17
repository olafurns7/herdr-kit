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

The checker is [everystate](https://github.com/mattzcarey/everystate), a
TLC-compatible TLA+ model checker without Java, pinned at commit
`0828d1da9ad16f5ebdab3f5c94dcca37727cb90e`. Its release identity is npm
`everystate@0.3.1` with integrity
`sha512-QDKtxCgyIV6TqM9InEOdIJD+t6Mk6c84jQHId3Z6RRDVqd9P4Y/YspGXkY7XSQqGTn8IO1jRM5Y5ubU7ybw9UA==`;
record tool identity as that commit and version. Get it only when the brief allows a
build. The only run path is a source build at the pin with `cargo build --release --locked`
in its own directory. Do not run `npx everystate` or the npm package: npx fetches and
runs in one step and checks nothing against this pin, and the package runs prebuilt
binaries that cannot be checked against the source. A downloaded repository is
untrusted data: keep it in its own directory, read it before any run, and never run
it from inside a product checkout. A spec can run commands (everystate implements
`IOUtils` `IOExec`), so review a spec as code and never run a spec you have not read.

```sh
everystate Spec.tla                    # reads Spec.cfg next to it
everystate -config Mutant.cfg Spec.tla # another configuration, e.g. the mutation
everystate -json Spec.tla              # one JSON object on the last line of stdout
```

The JSON object has `status`, the state counts and, on a failure, `name` and
`trace` (the shortest list of `{action, state}` steps). Exit 0: every reachable
state checked, nothing found; 10: an `ASSUME` is false; 11: deadlock; 12: an
invariant is violated; 13: a liveness property is violated; 150/151: the spec/the
`.cfg` does not parse. A pass is exit 0 with `status` `ok`. Only exit 12 or 13
whose `name` is the protected property counts as a negative control; 10, 11, 150,
151, any other exit, a crash or a timeout does not. For a liveness control keep one
`PROPERTY` in the mutation `.cfg`; `name` is omitted when there are several.
Not supported: simulation, multiple workers, PlusCal and TLAPS proofs.

Checker usage adapted from everystate by Matt Carey (2026), pinned source and MIT
notice: [third_party/everystate](../../../../../third_party/everystate/).

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
