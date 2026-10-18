# Write for the next reader

Open this checklist when drafting or editing briefs, worker reports, review
findings, PR bodies, owner asks, docs, or code comments. It is on-demand,
not a mode. Apply the checks that help the reader; there is no banned-word list.
Product UI copy follows its existing owner's guidance.

## Put the reader's task first

- Lead with the result, requested decision, or action the reader needs to take.
- In a brief, name the scope, constraints, and proof needed to finish.
- In a report or PR body, explain what changed, why, and what the checks proved.
  Link the detailed evidence instead of pasting the entire command log.
  Reviewers and workers still list each command and its exit code.
- In a finding, give the location, trigger, consequence, and smallest fix.
- In an owner ask, state the decision, options, recommendation, and tradeoff.
  Record the ask through taskr; prose does not grant approval.
- In docs, separate steps to perform from reference facts and explanations.
  Put conditions before the actions they guard; number steps that require order.
- Keep the evidence beside the claim. Do not make the reader reconstruct the
  result through several summaries or hold unstated conditions in memory.

## Make each claim checkable

- Name the actor, action, and affected thing. Prefer concrete verbs and nouns.
- Give a source or measured result for claims about safety, speed, or coverage.
  If evidence is missing, say what is unknown and which check would resolve it.
- Distinguish source inspection, local checks, saved receipts, and live behavior.
  Preserve the scope and uncertainty of the evidence when shortening a sentence.
- Use one name for each thing. Replace an ambiguous "it" or "this" with the noun.
- Split a sentence when the reader must backtrack. Keep articles, verbs, and
  conditions that prevent a second reading; brevity must not hide meaning.

The following examples use fictional paths and results:

| Context | Before | After |
|---|---|---|
| Brief | Improve installer robustness. | Change only the file allowlist. Prove that the new reference installs in a scratch HOME. |
| Worker report | Everything looks good. | The content check passed. Installation was not tested. |
| Review finding | This could be problematic. | `loader.py:42` truncates the destination before validating the input. Invalid input erases the previous file. Validate before opening the destination. |
| PR body | This change enhances the overall developer experience. | Add the writing reference to the optional practices pack so installed agents can open it. |
| Owner ask | Should we proceed? | Replace the legacy config now, or keep it until migration passes (recommended)? Replacing now discards local edits. |
| Docs | The configuration should then be updated accordingly. | If the check passes, set `enabled` to `true` in the config file. |
| Measurement | The check is significantly faster. | In five local runs, the median fell from 12 seconds to 8 seconds. Hosted timing is unknown. |

## Cut patterns that add no information

- Filler openings: "It is important to note that", "In order to", "Of course".
- Cut hedging ("could potentially"), superficial -ing tails ("ensuring…"),
  inflated "is" ("serves as"), and chatbot openers ("Great question", "Certainly!").
  Prefer plain words ("use" rather than "leverage") and literal names over
  abstract metaphor nouns ("north star").
- Unsupported praise or certainty: "robust", "seamless", "fully verified".
  Replace the claim with behavior and evidence, or delete it.
- Vague attribution: "experts say" or "best practices suggest" without a source.
- Repeated conclusions, forced groups of three, and synonym changes for one thing.
- Contrast slogans such as "not just X, but Y" and generic closing offers.
- Decorative headings and bold labels that merely repeat the sentence after them.

## Comment on the reason

Prefer names and straightforward code for behavior the code can show.
Keep comments for a non-obvious reason, external constraint, or safety invariant.
Do not delete useful safety context just to reduce comment count.

- Before: `// Increment the retry count.` After: omit it above `retries += 1`.
- Before: `// Do not remove this delay.` After: `// The device rejects commands
  sent less than 10 ms apart.` Write the reason only when you can cite it;
  otherwise keep the warning and note that the reason is unknown.
- If a comment reveals a missing check, propose the smallest check within scope.
  Do not claim that the comment enforces the constraint or widen the task to fix it.
