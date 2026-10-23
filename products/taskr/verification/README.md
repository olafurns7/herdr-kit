# Taskr verification map

Use [task-local proof guidance](../../setup/skills/herdr-verification/references/task-proof.md)
to choose a command/runtime check or a bounded formal check by risk. Follow the
brief's execution scope; workers do not gain build or full-suite authority here.
The parent runs approved checks at a pinned source and retains durable evidence.

| Boundary | Driver and evidence | Coverage status |
| --- | --- | --- |
| Inbox consumer fencing under `ASK_ONLY` | [Package and invocation](inbox/README.md), [source/tool/receipt map](inbox/manifest.json) | Historical source `71d4146` verified: 25 cases and six self-checks; package `d4c08a6`, frozen checkout `69f3ff9`, independent execution review READY. Current v5 source drift is BLOCKED pending final maintenance. |
| Durable work domain | [Command/contract docs](../docs/work.md) | Source/API guide; this index records no fresh integration or runtime proof. |
| Local controller and deterministic gate | [Controller docs](../docs/work-controller.md) | Source/runner guide; full local-loop, actual stop/recovery, Linux admission and host rollout evidence remain separate lead-owned gates. |
| Native jobs and optional owner input | [Jobs docs](../docs/jobs.md) | Synthetic/API behavior and actual client transport, restart/fallback and owner interaction require distinct evidence. No integrated real-client proof is asserted here. |

The inbox manifest retains exact covered source hashes, properties, assumptions,
bounds, tool identities and portable historical receipt hashes. Its model checks
ordering safety and its fixed CLI replays exercise selected real transactions.
Those are different proof surfaces. It does not certify v5 work/controller code,
dispatch, filesystem writers, native MCP or cross-host delivery. No covered hash
or model is updated by this documentation slice.

Before reusing evidence, compare all relevant source, profile, dependency,
configuration, tool/environment and expected case inputs. Name unrun and blocked
checks. On source drift, inspect and maintain the code/action map, then rerun the
correct check and negative controls; refreshing hashes alone cannot make it pass.
Installation, release and fleet enablement retain their own authorization/gates.
