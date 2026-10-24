# Taskr verification map

Use [task-local proof guidance](../../setup/skills/herdr-verification/references/task-proof.md)
to choose a command/runtime check or a bounded formal check by risk. Follow the
brief's execution scope; workers do not gain build or full-suite authority here.
The parent runs approved checks at a pinned source and retains durable evidence.

| Boundary | Driver and evidence | Coverage status |
| --- | --- | --- |
| Inbox consumer fencing under `ASK_ONLY` | [Package and invocation](inbox/README.md), [source/tool/receipt map](inbox/manifest.json) | Source map `a21f83a`, package/frozen checkout `9af0ea5`: all 25 exact cases and six self-checks PASS; retained inventories/hashes checked. Independent correspondence/execution review READY for those pins; metadata `6da0a9a` consistent. Historical `71d4146` result and READY review preserved separately. |
| Durable work domain | [Command/contract docs](../docs/work.md) | Source/API guide; this index records no fresh integration or runtime proof. |
| Local controller and deterministic gate | [Controller docs](../docs/work-controller.md) | Source/runner guide; full local-loop, actual stop/recovery, Linux admission and host rollout evidence remain separate lead-owned gates. |
| Native jobs and optional owner input | [Jobs docs](../docs/jobs.md) | Synthetic/API behavior and actual client transport, restart/fallback and owner interaction require distinct evidence. No integrated real-client proof is asserted here. |

The inbox manifest retains exact covered source hashes, properties, assumptions,
bounds, tool identities and portable current/historical receipt hashes. Its model checks
ordering safety and its fixed CLI replays exercise selected real transactions.
Those are different proof surfaces. It does not certify v5 work/controller code,
dispatch, filesystem writers, native MCP or cross-host delivery. The covered source
pin is the executed `a21f83a` map; model logic is unchanged and independent
correspondence/execution review is READY at those pins. Its nineteen covered blobs match `0e99765`,
without proving later uncovered changes. See the [correspondence note](inbox/SYNC.md).

Before reusing evidence, compare all relevant source, profile, dependency,
configuration, tool/environment and expected case inputs. Name unrun and blocked
checks. On source drift, inspect and maintain the code/action map, then rerun the
correct check and negative controls; refreshing hashes alone cannot make it pass.
Installation, release and fleet enablement retain their own authorization/gates.
