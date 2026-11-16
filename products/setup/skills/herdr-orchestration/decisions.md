# Decisions

Load when (the hub and campaign leads): before you decide anything a brief or an owner answer did not
settle, before you ask the owner, and before you send another root a message.

This table says who decides what, the check that must pass first, and the record the decision leaves.
Workers decide only inside their brief. Operators move a row down a layer only by an owner-approved
rule in their private overlay; this file never names an operator's classes.

## Checks

- **G**: the repository's proof rule is green at the exact head (CI, or the local gate receipt).
- **R**: an independent review seat is READY at that exact head, with no open P0 or P1.
- **E**: the record names its evidence: a measurement, a report, or a review.
- **O**: the owner approved the exact text of the rule that allows it: an answered owner ask.

## Table

| Decision | Who decides | Check first | Record |
| --- | --- | --- | --- |
| How to do the work inside the brief's owned paths | Worker | the brief's validation passes | commit; `ready --kv head=` with report |
| A gap in a brief that the approved scope, design and acceptance already settle | Lead; the worker asks | the answer changes none of them | worker `ask --blocking`; lead `answer` |
| A gap that would change product, scope or acceptance | Whoever approved them; for product, the owner | none | lead asks up; the worker stays blocked |
| Plan, slices, tickets, briefs, lanes and models within lane policy | Lead | the lane table allows the lane | plan doc; `work add`; launch record |
| A reversible choice inside approved scope (a technical option inside an approved design, a tool, slice order) | Lead | E | `taskr decide` naming the evidence |
| Accept a lane's result | Lead | the ticket's proof: G and R at a code head, an accepted report, or the work loop's typed acceptance | the work loop's acceptance, or `work done` where it applies; `close --outcome` |
| Another review round, up to round 5 | Lead | the last round left a P0 or P1 | the review ticket's rounds |
| A review round after round 5 | Owner | the six numbers (`owner.md`) | owner ask |
| Open a PR | Lead, under the campaign's publish authorization | G and R at the head | `taskr set pr=` |
| Merge a PR | Owner, unless an O rule gives its class to the hub | the rule's checks; at least G, R, no open review threads, the diff read by the merger, and the head is still the one the ask or rule names | the owner's answer on the merge ask that names the repository, PR and head; or the hub's `taskr decide` citing the rule's ask id and the head |
| Order between campaigns inside the owner's product priorities, resource weights, starting and stopping leads, which lead owns a ticket | Hub | none | `taskr decide` on the hub root |
| Destructive, irreversible or outward-facing actions; money; production data; credentials; security | Owner | none | owner ask |
| Product direction, words users see, schedule and priority between products | Owner | none | owner ask |
| Rules: model and lane policy, any layer's authority, this table | Owner, on exact text | E | owner ask on the text, then a PR or an overlay change |
| A fact that needs no decision | Nobody | none | owner note or Board card, never an ask |
| An action only the owner can do | Owner | none | owner ask, cleared with `answer ID done` |

1. A decision the table does not name goes up one layer.
2. When two rows fit, the higher layer's row wins: owner over hub, hub over lead, lead over worker.
3. A delegated decision whose check fails goes up one layer, with the failed check in the ask.
4. Write the record before the act it allows.
5. A merge whose head changed after the ask or the review goes back to its lead; an earlier yes does not
   cover the new head.
6. Only O text moves a row down a layer. It names the class, the checks and the record. The owner
   revokes it with one answer.

## Messages between layers

Reporting is by exception. The ledger is the status: a lead keeps `taskr next` and its Board card
current, and the hub reads `taskr glance --brief --since CURSOR`.

- A lead files its own owner asks on its root, merge asks included, with the merge's fact lines (repository,
  PR, head, each review verdict at that head). The hub passes them to the owner and records the answer
  on that ask, where the lead receives it.
- An answer on an ask is the message. When the asker is not waiting, deliver it with taskr's transport
  (`answer --prompt`, taskr's orchestrator reference); that is delivery, not a copy. Do not restate an ask
  or its answer, and do not announce an ask you filed: the hub already has its `owner_ask` event.
- An owner yes on a merge ask is the hub's merge trigger, through its `owner_closed` event. Under an O
  rule, the lead sends the hub the merge as a decision the table gives the hub, with the same fact lines.
- A lead sends the hub a message only for: a production fault (marked not yet checked); a decision
  this table gives the hub; a read the hub asked for; a fact the hub relayed to the owner that turned
  out wrong.
- The hub sends a lead a message only for an instruction of its own, or to deliver an answer as above.
- Non-blocking owner asks that are not merges go to the owner in rounds, at most one per root per
  hour. Blocking asks go at once; merge asks follow the operator's merge rule.
