# Durable local task loops

Load for `taskr work` or a work-bound assignment. The lead or designated hub adds
unassigned work to the existing ledger; backlog is not authority to execute.
Before queueing, approve the complete typed contract: intent, acceptance, scope,
checkout/host, source/input identity, dependency revisions, local authority,
proof profile and permitted independent lanes. Missing contract fields keep work
in backlog. The installed local lane table and private overlay govern models;
a stored provider/model/effort snapshot cannot bypass them.

[Work commands and contract](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/work.md)
own the schema and canonical hashes;
[controller commands](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/work-controller.md)
own runtime setup and teardown. Do not duplicate their full schemas in briefs.

## From an approved contract to dispatch

This example uses synthetic root `42` and workspace `w1`. Substitute the actual
registered root/workspace; run in authorized root context, without a worker
launch. First prepare `approved-work.json` using the linked Add/Contract schema:
its campaign is 42, its title is scoped, its stable creation key is unique to this
intent, and its contract contains approved real source/profile/input digests and
local lane choices. The canonical `work hash` helper computes profile/input
digests; arbitrary digest labels and the command docs' illustrative hashes are
not executable proof. The new work starts at revision 1. Use a clean, canonical
checkout on the campaign's authenticated host and its configured task workspace.

```sh
set -eu
campaign=42
generation=$(taskr --json work list "$campaign" | jq -er '.generation')
work_id=$(taskr --json work add --as "$campaign" --generation "$generation" \
  --file approved-work.json | jq -er '.work_id')
taskr work queue "$work_id" --as "$campaign" --generation "$generation" \
  --data '{"expected_revision":1}'
# Explicit opt-in: a compatible resident daemon can now claim and launch locally.
taskr work configure "$campaign" --as "$campaign" --generation "$generation" \
  --enabled on --workspace w1 --max-active 1 --max-review-pending 1
taskr work list "$campaign"
taskr work show "$work_id"
```

Enablement is off by default; this is an authorized configuration example, not
an installation or rollout claim. A compatible running Herdr/taskr environment,
approved runtime gates and same-host ownership are prerequisites. Do not start,
restart or install services implicitly. Client hosts keep using CLI/RPC to the
authoritative ledger. Checkouts are reserved by authenticated host/canonical path;
aliases and unrelated legacy writers still need explicit reconciliation.
Disabled or parked campaigns cannot claim; lower capacity limits do not cancel
existing assignments. Enable only after the required rollout proof is accepted.

## Identity, stages, and quiet reporting

Work ID/revision identify intent; run ID groups a local loop and its rework rounds.
Legacy task/lane IDs and launch IDs identify stage writers/attempts. Use returned
IDs, never infer one from another or from a native task handle. Root mutations
pin `--as ROOT --generation GEN`; stale generation exits 6 and stops that work.
Workers submit typed immutable results under their assigned task/launch identity.

The controller claims and reserves before launch: executor → actual executor stop
→ deterministic gate → actual gate stop → independent reviewer → actual review
stop → conditional independent adjudicator → required stops → acceptance.
There is one checkout writer. A settled result, `ready`, idle pane, or cancellation
does not prove process exit. Unknown start/stop retains the reservation until
native identity and shell foreground establish the actual stop receipt.

Approve exact source/tree, profile/version/digest, dependency acceptance receipts,
argv/tool/environment identity and expected case IDs. Failed, missing, duplicate,
extra or skipped cases cannot pass; command-only checks claim no case coverage.
A deterministic gate needs no always-on third model. The executor cannot approve
itself or dismiss review findings; a disputed finding needs its permitted
adjudicator, otherwise escalation. Check and review failures share the contract's
bounded rework budget. Scope/authority changes, exhausted rework and uncertain
external effects go to the lead; acceptance grants no publication authority.

Routine submitted/ready/done/rework stay audit data under an enabled work loop;
do not relay them repeatedly through lead and hub. Final acceptance and real
exceptions get short notices with work/revision/run/result IDs, reason and report
pointer. Inspect proof and review, not a prose PASS. Manual/legacy lanes retain
their existing reports and ask/failure routes.

## Observe and recover

`taskr work list ROOT` is the configuration/generation/detail snapshot;
`taskr work show WORK` reads immutable revisions, runs, results and reasons.
`work campaigns` and `work dispatch ROOT` are bounded thin controller scans;
respect explicit truncation, never treat a returned count as the whole backlog.
After interruption, use `taskr work recover ROOT --as ROOT --generation GEN`
and inspect `show` before any effect. The controller's `work drive ROOT --once`
is an explicit effectful reconciliation pass, not a read or proof of safe rollout.
Unknown dispatch must reconcile its existing launch/prompt; never blind-restart,
resend, release a checkout, or forge a stopped observation. Disabling new claims
does not stop running agents. Follow the linked controller recovery protocol.

[Native jobs](https://github.com/olafurns7/herdr-kit/blob/main/products/taskr/docs/jobs.md)
use a separate modern endpoint over the same ledger. Recover a lost handle rather
than rerun work. Native polling is presentation, not semantic handling or owner
consent: process the stable result ID and send its explicit `result_receipt`.
Lease release/expiry or unsupported clients use compatible channel fallback;
existing offered inbox events remain consumable under the normal fenced rules.
Owner elicitation is optional for one existing structured owner ask explicitly
opted in by CLI. Native provenance does not authenticate a human; decline/cancel
leaves no decision, and another channel's answer is not silently superseded.
