# pstack provenance

Source: [poteto's pstack in cursor/plugins](https://github.com/cursor/plugins/tree/f713f14203cd6420cec13cd7c55c32d598dbab68/pstack).
Full pin: `f713f14203cd6420cec13cd7c55c32d598dbab68`.
Copyright (c) 2026 Lauren Tan; MIT. The complete [LICENSE](LICENSE) is copied unchanged.
Pinned `pstack/LICENSE` SHA-256: `bc957ca6bee02792566a1a028d105e02e247c6e77cf057061674273da77b200e`.

Paths below are relative to upstream `pstack/` and this repository respectively.
These are rewritten references, not an imported plugin or automation runtime.
Include this directory's notices with distributions containing the adapted text;
installer placement and client discovery are owned by the installer slice.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `LICENSE` | `third_party/pstack/LICENSE` | verbatim: complete upstream license |
| `skills/how/SKILL.md`, `skills/reflect/SKILL.md` | `products/setup/skills/herdr-practices/SKILL.md` | adapted: short local reference index and authorized Herdr/taskr assignment instead of mandatory fan-out |
| `skills/why/references/epistemics.md` | `products/setup/skills/herdr-practices/references/research.md` | adapted: four evidence labels, scoped source coverage, explicit unknowns |
| `skills/how/SKILL.md` | `products/setup/skills/herdr-practices/references/research.md` | adapted: direct bounded mechanism walkthrough; removed model/tool fan-out |
| `skills/teach/SKILL.md` | `products/setup/skills/herdr-practices/references/research.md` | adapted: layered explanation and concrete examples; no required images or extra skills |
| `skills/interrogate/references/lead-judgment.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: reachable paths and explicit dismissals; no vote-based acceptance |
| `skills/blast-radius/SKILL.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: decisive safety assumption and proof ladder within authorized checks |
| `skills/benchmark-checklist/SKILL.md` | `products/setup/skills/herdr-practices/references/measurement.md` | adapted: fixed comparison, work/error counts, alternating samples and inconclusive verdicts |
| `skills/principle-explain-the-number/SKILL.md` | `products/setup/skills/herdr-practices/references/measurement.md` | adapted: measured limiter and retained evidence; no transitive skill dependencies |
| `automations/benny/skills/reproduce-and-fix-issues/references/verify-existing-fix.md` | `products/setup/skills/herdr-practices/references/verify-existing-fix.md` | adapted: repeated baseline/patch observations; removed operational status and source posting |
| `skills/reflect/SKILL.md` | `products/setup/skills/herdr-practices/references/retrospective.md` | adapted: bounded evidence and proposals; no transcript discovery or automatic writes |
| `skills/reflect/references/synthesizer.md` | `products/setup/skills/herdr-practices/references/retrospective.md` | adapted: durability and existing-owner filtering; no automatic backlog filing |
| `skills/correct/SKILL.md` | `products/setup/skills/herdr-practices/references/retrospective.md` | adapted: enforcement-before-prose ladder; removed implicit commits and instruction edits |
| `skills/create-verification-skill/SKILL.md` | `products/setup/skills/herdr-verification/SKILL.md` | adapted: repository-authorized output, existing drivers, one proved journey, surviving evidence |
| `skills/maintain-verification-skill/SKILL.md` | `products/setup/skills/herdr-verification/SKILL.md` | adapted: bounded source reading, one live driver, every feature accounted for; no automatic PR |
| `automations/benny/skills/reproduce-and-fix-issues/references/feature-map.example.md` | `products/setup/skills/herdr-verification/references/feature-map.md` | adapted: compact surface-neutral feature contract without fictional app examples |
| `automations/benny/skills/reproduce-and-fix-issues/references/control-adapter.md` | `products/setup/skills/herdr-verification/references/feature-map.md` | adapted: existing owned driver, explicit capability gaps, evidence and cleanup; no automation activation |

`herdr-practices/SKILL.md` is a local index of the adapted references. Both skill
entrypoints replace upstream delegation with direct work or authorized parent
Herdr/taskr assignment. No model policy, publication, or watcher authority is imported.
`products/setup/tests/practices_test.sh` is a local content check, not upstream code.

## Incident intake adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`, not the earlier `f713f14` pin.
The full `f713f14203cd6420cec13cd7c55c32d598dbab68` pin and all rows above
remain unchanged. The inspected snapshot's LICENSE has the same SHA-256 above.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `automations/benny/skills/triage-issue-reports/SKILL.md` | `products/setup/skills/herdr-practices/references/incident-intake.md` | adapted: immutable source identity, evidence review, classification and dedupe; no tracker writes, Slack verdicts or follow-up watcher |
| `automations/benny/skills/reproduce-and-fix-issues/SKILL.md` | `products/setup/skills/herdr-practices/references/incident-intake.md` | adapted: existing-owner and patch gates, existing feature-map driver, two bounded reproduction attempts; no fixes, PRs or external posts |
| `automations/benny/skills/setup-benny/SKILL.md` | `products/setup/skills/herdr-practices/references/incident-intake.md` | adapted: user-owned configuration outside the pack and capability gaps; no pack setup, credential provisioning or automation activation |
| `automations/benny/templates/configuration.example.yaml` | `products/setup/intake.example.yaml` | adapted: placeholder per-repo source and verification config supplements existing taskr health/repo scope; no tokens, auth, posting, polling or model settings |

## Writing adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and rows remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/unslop/SKILL.md` | `products/setup/skills/herdr-practices/references/writing.md` | adapted: optional editing checklist and before/after pairs for filler, vague claims, and repetition; no always-on vocabulary or punctuation rules |
| `skills/technical-writing/SKILL.md` | `products/setup/skills/herdr-practices/references/writing.md` | adapted: reader/task structure, precise instructions, consistent terms, and evidence boundaries for fleet writing; no recursive skill triggers or universal formatting mandates |
| `skills/no-comments/SKILL.md` | `products/setup/skills/herdr-practices/references/writing.md` | adapted: comments explain non-obvious reasons and constraints; preserve safety context, with no mandatory delegation, deletion campaign, or automatic code changes (mapping: rejected the deletion mechanism; kept only the why-comment rule) |
| `skills/principle-minimize-reader-load/SKILL.md` | `products/setup/skills/herdr-practices/references/writing.md` | adapted: keep evidence beside claims and make conditions explicit; no code restructuring mandate |

## Runbooks adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and rows remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/tdd/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: red/green/refactor at the owning behavior boundary, with executable reproduction when a practical test is unavailable |
| `skills/poteto-mode/playbooks/bug-fix.md`, `skills/principle-fix-root-causes/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: reproduce, trace the first wrong state, fix its owner, and verify on the same surface; no mandatory delegation or publication |
| `skills/poteto-mode/playbooks/feature.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: explicit behavior contract, bounded slices, preserved paths and taskr evidence; no fan-out requirement |
| `skills/poteto-mode/playbooks/refactoring.md`, `skills/principle-migrate-callers-then-delete-legacy-apis/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: pin behavior, migrate internal callers and delete the old API; retain external compatibility obligations |
| `skills/poteto-mode/playbooks/runtime-forensics.md`, `skills/poteto-mode/playbooks/trace-forensics.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: immutable capture identity, reduced evidence and source attribution; no implicit live instrumentation or hot-patching |
| `skills/poteto-mode/playbooks/perf-issue.md`, `skills/poteto-mode/playbooks/hillclimb.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: measured limiter, frozen comparison and kept/reverted attempt trail; link existing measurement proof and budget, omit iteration floors and background loops |
| `skills/principle-attack-the-premise/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: after two same-gate failures, test the shared premise before another fix |
| `skills/principle-subtract-before-you-add/SKILL.md`, `skills/principle-build-the-lever/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: remove proved-unused work, reuse checks and make repeated work rerunnable; preserve live validation and error handling |
| `skills/figure-it-out/SKILL.md`, `skills/principle-outcome-oriented-execution/SKILL.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: observable finish predicate, phase outputs and proof, taskr audit trail; no competing filesystem ledger or implicit authority for intermediate breakage |
| `docs/guide/05-build-and-clean.md`, `skills/poteto-help/references/recipes.md` | `products/setup/skills/herdr-practices/references/runbooks.md` | adapted: short task recipes and cleanup within the brief; link existing research, intake, review and measurement owners |

## Design adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and rows remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/architect/SKILL.md`, `skills/architect/references/rationale-template.md`, `skills/architect/references/runner-prompt.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: caller example, types, signatures, module boundaries, short rationale and implementation re-checks; no mandatory arena or scaffold breakage |
| `skills/architect/references/design-red-flags.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: repeated representation and conditions, shared-state ownership, exposed internals and escape hatches prompt a shape review |
| `skills/principle-foundational-thinking/SKILL.md`, `skills/principle-model-the-domain/SKILL.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: choose domain types, state transitions, access patterns and write ownership before logic; no forced abstraction |
| `skills/principle-redesign-from-first-principles/SKILL.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: reconsider the whole shape when a requirement does not fit, then deliver bounded verified steps |
| `skills/arena/SKILL.md`, `skills/principle-exhaust-the-design-space/SKILL.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: owner/orchestrator-flagged decision only, 2 or 3 candidates, frozen rubric, blind different-model judge, base and graft record; parent-started Herdr tabs within a lane budget, no in-process agents |
| `skills/poteto-mode/playbooks/prototype.md`, `docs/guide/04-design.md` | `products/setup/skills/herdr-practices/references/design.md` | adapted: throwaway scratch evidence for a named code/architecture question; UI variants stay with the owner's variant skill, production acceptance stays with implementation checks |

## Review adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and rows remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/interrogate/SKILL.md`, `skills/interrogate/references/reviewer-prompt.md` | `products/setup/skills/herdr-practices/references/review.md`, `products/setup/skills/herdr-orchestration/workflow.md` | adapted: stated-intent check and Act / Consider / Noted / Dismissed findings; no mandatory fan-out, voting or publication authority |
| `skills/interrogate/references/code-quality-review.md`, `skills/interrogate/references/rubric.md`, `skills/poteto-mode/references/bugbot-triage.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: scoped complexity costs, verification quality and evidence-based human/bot triage; unresolved high-risk claims go to the parent, with no automatic fixes or thread writes |
| `skills/principle-boundary-discipline/SKILL.md`, `skills/principle-laziness-protocol/SKILL.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: enforced entry boundaries and preserved error handling; link runbooks for smallest complete change and root-cause checks |
| `skills/principle-minimize-reader-load/SKILL.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: fewer layers and less hidden mutable state; link to the existing writing reference for prose |
| `skills/principle-test-behavior-not-implementation/SKILL.md`, `skills/principle-type-system-discipline/SKILL.md` | `products/setup/skills/herdr-practices/references/review.md` | adapted: observable behavior checks that survive internal refactors and types justified by concrete invalid states; no blanket branding or new schema layer |

Existing proof-ladder, no-vote and dismissal guidance remains attributed in the
earlier table; this slice adds no replacement for those landed adaptations.

## Brief and continuity adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and all preceding adaptation sections remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/in-your-own-words/SKILL.md`, `skills/sharpen/SKILL.md`, `skills/principle-experience-first/SKILL.md`, `skills/poteto-help/references/prompting.md` | `products/setup/skills/herdr-orchestration/SKILL.md` | adapted: optional orchestrator intent restatement, explicit non-goals and per-slice proof; preserve the owner's recorded goal and existing brief compatibility |
| `skills/principle-sequence-verifiable-units/SKILL.md`, `skills/poteto-mode/playbooks/multi-phase-plan.md`, `skills/swarm/SKILL.md` | `products/setup/skills/herdr-orchestration/workflow.md` | adapted: independently verifiable slices and disjoint complete coverage; taskr remains the plan owner, no mandatory fan-out or imported launch policy |
| `skills/poteto-mode/playbooks/pause-safely.md`, `skills/principle-guard-the-context-window/SKILL.md` | `products/setup/skills/herdr-orchestration/recovery.md` | adapted: safe atomic boundary and evidence, lane, ask, next-action and proof-gap checkpoint through existing taskr plan/handover; no automatic commit or new file format |
| `skills/recall/SKILL.md` | `products/setup/skills/herdr-orchestration/recovery.md` | adapted: read campaign plan, ledger and latest reports before re-deriving work; replaces the deferred Recall disposition, no transcript mining or new reference |
| `skills/principle-never-block-on-the-human/SKILL.md` | `products/setup/skills/herdr-orchestration/owner.md` | adapted: record reversible execution defaults within existing authority and continue; preserve product, external-action and authority asks |
| `skills/show-me-your-work/SKILL.md` | `products/setup/skills/herdr-orchestration/workflow.md` | adapted: final claim/evidence audit and named proof gaps; existing taskr decisions and reports replace the upstream TSV and transcript audit, no extra reviewer or publication authority |

Writing, runbooks, design, review and recurring watches keep their existing owners;
see the adaptation sections above and `products/setup/skills/herdr-loop/SKILL.md`.

## Worktree inventory adaptation

The rows below use the separately inspected snapshot
`cdb3c41c9f2f6d41f050e3dc25e15e325493d499`. Its LICENSE has the same SHA-256
above; the earlier `f713f14` pin and rows remain unchanged.
Paths are relative to upstream `pstack/` and this repository respectively.

| Upstream path | Destination path | Adaptation |
|---|---|---|
| `skills/poteto-mode/scripts/worktree-audit.sh` | `products/setup/bin/herdr-worktrees` | adapted: local worktree inventory, rebuilt in POSIX sh with byte-exact merge-tree content checks and isolated temporary objects; no fetch, PR lookup, transcript scan, size/age ranking, deletion advice or writes |
| `skills/poteto-mode/playbooks/worktree-cleanup.md` | `products/setup/skills/herdr-orchestration/teardown.md` | adapted: inventory before cleanup and owner decisions for unfinished work; task closure checked by hand, existing ownership rules retained, no forced removal |
