# Measurement

Use before reporting a speedup, regression, throughput, latency, or evaluation score.
Write the claim, workload, unit, and pass predicate before collecting samples.
Read the harness: identify the timed region, counted work, outputs, and omissions.

## Fix the comparison

Record revisions, build mode, dependency versions, data size, concurrency, and flags.
Use realistic production settings on both sides, including batching and pools.
Declare whether caches are warm or cold and apply the same policy to both sides.
Keep input data and expected output fixed; record any unavoidable differences.
If a side is untuned and cannot be corrected, do not use it to pick a winner.
Record host capacity and competing load without stopping other people's processes.
Separate fixture setup and warmup from measured work unless they are in the claim.

## Count work and failures

For every sample, count attempted, completed, skipped, failed, and retried operations.
Check output correctness, not merely that an output file or response exists.
Confirm the real operation occurred inside the timed region and consumed its result.
Await asynchronous work and consume lazy results before stopping the clock.
Inspect fast failures, timeouts, cache hits, and no-ops as possible false improvements.
Keep errors visible alongside timing rather than silently excluding failed samples.
Missing work or error counts makes a comparative verdict inconclusive.

Before retrying, name the symptom, attempt budget, and stop predicate. A flaky
check gets one authorized rerun, then an isolated reproduction; never retry
silently. Retain each attempt and its result, including an inconclusive stop.

## Repeat and explain

Run at least five samples per side for a comparison; alternate A and B to reduce drift.
Retain individual samples and report the median, range, and run count.
If the harness uses another statistic, name it and retain the underlying samples.
A gap within run-to-run spread is no measurable difference, not a win.
A requested one-run ballpark must be labeled as such and cannot select a winner.
Profile a separate diagnostic run so instrumentation does not skew reported samples.
Name the measured limiter: CPU path, lock, I/O, bandwidth, or load generator.
Tie it to counters or a profile and then to the responsible source or configuration.
Source inspection alone is a hypothesis about the limiter.

Check physical and arithmetic bounds: throughput versus available resources,
and time saved versus the original share of the changed operation.
Measure the end-to-end user path alongside a microbenchmark when making a user claim.
If the claimed gain exceeds the work removed, investigate the harness first.
For evaluations, retain task completion, trial count, scenario mix, and score spread.

## Hand back

Choose faster, slower, no measurable difference, or inconclusive.
Put the unit, workload, revisions, sample count, spread, and limiter beside the number.
Link durable raw samples, correctness checks, work counts, and error counts.
If you cannot name the limiter or establish a fair comparison, say inconclusive
and identify the missing evidence. Do not replace a missing run with a source claim.
