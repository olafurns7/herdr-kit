# Design

Open on demand for a decision that is costly to undo: a new module, data model,
schema, or protocol. This is not a mode. Default to shaping the code before
implementing it; work directly unless the owner or orchestrator flags an arena.
The owner's variant skill covers UI variants; this reference covers code and
architecture. It grants no publication or merge authority.

## Shape before code

1. State the requirement, constraints, and behavior that must remain. Read the
   owning code and callers; use [research](research.md) for unclear mechanisms.
2. Model the domain and choose core types before logic. Name valid states,
   transitions, invariants, and dominant reads and writes. Prefer a state model
   over booleans that must stay in sync; keep a clear local shape when it fits.
3. Write one realistic caller example. Derive types, signatures, and module
   boundaries from it; show how input reaches output without implementing bodies.
   Name each state owner, validation boundary, and concurrent writer. Explain how
   conflicting writes are prevented or reconciled; do not assume shared state safe.
4. Keep the public surface small and hide storage and protocol details behind
   the owner. Group related rules by domain knowledge, not execution phase.
5. Record a short rationale in the taskr-linked report: problem and constraints,
   caller example, proposed shape, accepted tradeoffs, alternatives and rejection
   reasons, open questions, and first implementation step. Identify affected
   callers, tests, fixtures, exports, and configuration. Ask through taskr for a
   missing product or ownership decision; stop dependent work until answered.

## Red flags

- A repeated shape assumption across files: put the representation and invariant
  with one owner; derive other views instead of keeping copies in sync.
- Branching on the same condition in several places: check whether a domain
  type or state transition can own the rule once.
- Unclear ownership of shared state: name who writes it and what happens when
  writers overlap before choosing locks or another coordination mechanism.
- Callers must know internal stages or import internals: move that coordination
  behind the boundary. Remove forwarding layers that add no policy or adaptation.
- Casts, escape hatches, or repeated special cases hide valid domain states:
  revisit the model instead of weakening validation.

## Re-check during implementation

Compare each implemented slice with the caller example, types, and rationale.
When a new parameter, branch, or state owner appears, explain whether the sketch
missed a requirement or the implementation exceeds scope. Update the shape and
all affected references together. Use [runbooks](runbooks.md#feature) for slice
proof and delivery; use [review](review.md) for independent assessment.

If a new requirement does not fit, redesign from first principles: sketch what
you would build if that requirement had been present at the start. Compare with
the current design, remove obsolete workarounds, and deliver the smallest coherent
change in verified steps. Repeated red flags are evidence to revisit the shape;
they do not authorize an unbounded rewrite or mandatory fan-out.

## Arena for a flagged decision

Run only when the owner or orchestrator flags the decision. If its lane budget
or judge is unavailable, report the gap through taskr; do not improvise agents.

1. Freeze the decision, constraints, candidate output, and rubric before work.
   Use concrete criteria such as invariant coverage, caller simplicity, state
   ownership, migration cost, and proof. State the lane and time/attempt budgets.
2. Request 2 or 3 structurally distinct candidates with the same task and inputs.
   Only the authorized parent starts them as Herdr tabs in the task workspace,
   within that lane budget, with disjoint output paths. Never use in-process agents.
   Candidates do not read each other's outputs.
3. After candidates finish, choose a read-only judge from a lane allowed for review
   by the operator's [lane policy](../../herdr-orchestration/lanes.md) and
   [private overlay](../../herdr-fleet/SKILL.md), on a model different from every
   candidate's. If no such lane is allowed, report the gap through taskr. Score
   against the frozen rubric with neutral labels and author/model identities
   omitted; retain missing outputs as gaps.
4. Read every candidate and the judge's reasons. Pick a base against the rubric,
   then graft useful parts by hand into one coherent shape. Record the base,
   grafts, rejected parts, disagreements, and gaps in the taskr-linked report.
5. Verify the combined result under the task's acceptance checks. Candidate votes
   and a judge's preference do not establish correctness or approval.

## Throwaway prototypes

Use a prototype only to answer a named design question within the task's budget.
Keep it in owned scratch space outside production source. Record the inputs,
observed behavior, tradeoff, and scratch path; follow [measurement](measurement.md)
for comparative timing claims. Prototypes are throwaway and never count as
production acceptance. Build the chosen shape through the feature runbook and
prove it on the required surface; retain [incident intake](incident-intake.md)
as the owner of intake, deduplication, and reproduction rules.
