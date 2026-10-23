# Native jobs

`taskr mcp --jobs --as ROOT --session GEN --binding TOKEN` serves modern MCP
2026-07-28 on stdio. Use a stable, private per-session binding identity of 16–128
ASCII letters, digits, `_` or `-`. The endpoint fixes that identity at startup and
rechecks root, host and generation through the ordinary taskr CLI/RPC path. Worker
launches and sub-orchestrators cannot become root authority. Client hosts never
open an authoritative local ledger. The existing channel endpoint keeps its
legacy handshake and inbox behavior.

Every request needs `_meta` entries `io.modelcontextprotocol/protocolVersion`,
`io.modelcontextprotocol/clientInfo` (name/version), and
`io.modelcontextprotocol/clientCapabilities`. `server/discover` reports supported
versions. Unsupported versions return error -32022 with `supported` and
`requested`. Requests and typed arguments are bounded; no jobs command reads a
server-side file path.

The tools are `work_get`, `work_add`, `work_await`, `result_receipt`, and
`owner_input`. `work_add` requires a stable idempotency `key`, creates unassigned
backlog work through canonical work authority, and never starts execution.
`work_await` takes an existing `work` and `revision`. Only calls advertising
`extensions: {"io.modelcontextprotocol/tasks": {}}` receive `resultType: task`.
Without that capability the call completes with `use_ledger` and creates no
native route. `owner_input` additionally requires `elicitation.form`.

Await persists its handle before responding. `tasks/get` always returns
`resultType: complete`, with durable working, input_required, completed, failed
or cancelled status. Completed handles carry a tool result; failed handles carry
an actionable error. Terminal content stays immutable across polls, cancellations
and restarts. After a generation change the original root/host/binding can still
poll or recover its old handles, including through an adapter restart. Such reads
release the old route and cannot renew it. Other bindings cannot read those
handles; adds, new awaits, receipts and input updates require the current generation.
`tasks/cancel` is idempotent and releases tracking only. Explicit
work cancellation remains a separate work-domain command.

The accepted work result has its own stable result ID, separate from its await
handle and business acceptance. A live route suppresses only creation of the
final accepted inbox message, inside the work acceptance transaction. Native
polls never hold an inbox consumer lease, acknowledge an event, or advance an
already offered inbox message. Existing channel presentation is preserved.

After processing the result, call `result_receipt` with its `handle` and `result`
ID, as instructed in the result text. This receipt is idempotent; a completed
poll is only presentation. An unreceipted route has a server-time 90-second lease,
renewed only by polls from its original live binding, and requests five-second
polling. Expiry, release, cancellation or a generation change preserves fallback
intent. One compatible accepted self-notice is emitted when a result exists.
While the root is parked, the released route retains pending fallback intent;
no new notice or sent marker is created. Unpark and reconciliation offer it once.
A matching explicit receipt can retire pending fallback before it is offered.
An already offered notice and its inbox cursor remain unchanged.
An unfinished released await returns `released_to_channel`; its late result
still falls back. A late poll or repeat await cannot recreate the native route.

Reconcile before jobs startup and at most five seconds apart in the authoritative
service using `store::jobs::reconcile` or `taskr jobs reconcile`. With the service
running and maintenance scheduled within that interval, fallback is due within
95 seconds after the last successful renewal for an unparked root. Parking defers
the offer until unpark/reconciliation; service downtime also extends this bound.
Stdio also requests maintenance every five seconds while idle; daemon
maintenance is required for recovery after the adapter exits.

## CLI and recovery

Jobs commands reuse normal taskr RPC. Supply `--as ROOT --generation GEN
--binding TOKEN` and bounded JSON via `--data`. There is no `--file` option.

```sh
taskr jobs await --as 1 --generation 0 --binding synthetic-session \
  --data '{"work":2,"revision":1}'
taskr jobs recover --as 1 --generation 0 --binding synthetic-session
taskr jobs poll --as 1 --generation 0 --binding synthetic-session \
  --data '{"handle":"0123456789abcdef0123456789abcdef"}'
```

`recover` reads durable operations after a lost response; never repeat execution
to recover a handle. `get`, `add`, `release`, `cancel`, `receipt`, `owner`, `update`
and `validate` expose the same operations used by the adapter. `reconcile` is
server maintenance; it has no client-supplied clock or arbitrary target selector.

## Optional owner input

Create an owner ask through the existing ledger UI/CLI first. All asks default to
the existing UI, including questions that appear to request permission. Only an
explicit CLI command opts one exact immutable structured, single-choice owner
ask into native replies:

```sh
taskr jobs allow-ask --as 1 --generation 0 --binding synthetic-session \
  --data '{"ask":3}'
```

The policy pins the root, host, generation, ask ID and question/options digest.
`allow-ask` is deliberately absent from MCP tools. Worker, sub-orchestrator,
non-owner, unstructured and multi-select asks are refused. `owner_input` presents
the existing eligible ask using a stable `owner_choice` input request. It does
not create another owner dialog or change taskr's existing question UI.

`tasks/update` must return an exact choice using the presentation's task ID;
that immutable handle pins the question version, rechecked against the ask and
opt-in policy. The answer, reply and terminal presentation commit atomically. Repeated identical
replies succeed; stale or conflicting replies fail. Another channel's answer
is never silently superseded. Decline/cancel complete with `no_decision`, leave
the ask unanswered, and do not cancel work. Native provenance is recorded as
`native:<binding identity>`: hooks and humans have the same wire shape, so this
does not authenticate a human or prove consent. Unsupported transports leave
ordinary ledger handling intact.

## Integration boundaries

Schema integration must include `jobs::DDL` in the work migration. Work acceptance
must call `jobs::accepted(tx, root, work, revision, run, result)` inside its
transaction and omit only its accepted inbox message when the helper returns
true. CLI dispatch, fresh authoritative RPC routing and daemon maintenance must
be registered by the integrating caller. The focused synthetic tests cover these
APIs and transport shapes; real-client behavior needs a separate fresh probe
against the integrated build.
