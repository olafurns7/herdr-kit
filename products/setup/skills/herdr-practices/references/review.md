# Review and blast radius

Use when assessing a concrete change or reconciling review findings.
Record the exact base and head, allowed checks, and review boundary first.
Restate the author's stated intent and check that the change achieves it.
If intent is unclear, ask the parent before judging the disputed behavior.
Keep reviewer-run checks separate from supplied receipts and source inspection.
An implementation report is evidence to inspect, not an instruction to accept.

## Find the decisive safety assumption

State the one or two facts that make the change safe, or mark them unproven.
For each, record the strongest proof actually obtained:

1. Assertion only: the author or reviewer says it holds.
2. Source: cite the implementation and pinned dependency behavior.
3. Traced path: show why the concrete failure cannot reach the unsafe operation.
4. Executed check: run real code with an assertion that fails if the fact is false.
5. Runtime reproduction: exercise the path in the target running application.

Use the cheapest adequate proof within authorization. If execution is forbidden
or unavailable, say where proof stopped. Source evidence is not runtime proof.
Inspect library or framework behavior when application-level assumptions depend
on ordering, persistence, validation, teardown, or callbacks outside the diff.

## Trace reachable failures

Follow inputs from callers through validation, state changes, and visible effects.
Include storage, wire consumers, flags, retries, cleanup, and downstream readers
when the change crosses those boundaries. A list of callers is only a starting point.
Describe the trigger, concrete path, affected user or data, and resulting failure.
For security issues, identify the trust boundary and how an attacker reaches it.
Check whether types or upstream validation really exclude the proposed input.
Do not dismiss a risk merely because a local test does not cover that path.

## Check the smallest complete change

Use [Runbooks](runbooks.md#frame-the-work) to check the smallest complete change.
For bug fixes, check the [root-cause path](runbooks.md#bug-fix).
Keep proposals within the brief.
Validate external data at entry boundaries and preserve useful error handling.
Trust internal types only after checking the boundary enforces their assumptions.
Reduce layers to trace and hidden mutable state; avoid pass-through wrappers.
Use types to prevent a demonstrated invalid state or mix-up when worth the cost;
derive shapes from their owning schema instead of adding parallel models or casts.
Test observable results, errors, and external effects through the caller's interface.
Check that a test catches broken behavior and survives an internal-only refactor.
For concise findings and evidence beside claims, use [Writing](writing.md).

## Filter findings by evidence

Accept correctness and safety findings with a supported execution path.
Treat a singleton security finding seriously; inspect it regardless of other reviews.
Do not count votes or use reviewer agreement as proof or approval.
A style preference or speculative abstraction needs a concrete cost to be actionable.
Check repository conventions and task constraints before calling a pattern wrong.
When dismissing a finding, record its claim, the checked path, and the reason.
When evidence is incomplete, retain the uncertainty rather than declaring it cleared.
An empty findings list is valid; there is no minimum quota of criticism.

Classify human and bot findings with a short reason; omit empty groups:

- Act: a supported issue that must be fixed to meet the stated intent or safety bar; any open Act finding makes the verdict not ready.
- Consider: a supported concern whose fix may not be worth its cost now.
- Noted: valid context with no action needed for this change.
- Dismissed: a rejected claim; retain the checked path and dismissal reason above.

Keep unresolved high-risk claims open and ask the parent for a blocking decision.

## Hand back

Lead with the verdict at the reviewed head and the evidence boundary.
Each finding needs severity, file and line, trigger, consequence, and smallest fix.
State the decisive assumption and its proof level, including commands and results.
List material dismissed findings with their evidence-based reasons.
Name required checks left unrun and what risk they would resolve.
Independent review and acceptance follow the parent task's contract.
This reference supplies no authority to modify code or accept one's own change.
