#!/usr/bin/env python3
"""Read-only per-class cgroup peaks and overlap replay (stdlib only)."""
import argparse
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
import heapq
import json
import math
from pathlib import Path
import re
import subprocess
import tomllib


def timestamp(value):
    if isinstance(value, (int, float)):
        return float(value)
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('timestamps must include a timezone')
    return parsed.timestamp()


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


@dataclass
class Run:
    run: str
    start: float
    arrival: float
    cost: int
    kind: str | None
    campaign: str
    weight: int
    pool: str
    end: float | None = None
    peak: float | None = None

    @property
    def admission_class(self):
        workload = self.kind if self.kind is not None else f'cost:{self.cost}'
        return f'{self.pool}:{workload}'


def read_runs(path, pool):
    runs, latest = [], {}
    with Path(path).open() as stream:
        for number, line in enumerate(stream, 1):
            try:
                event = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f'event line {number}: {error}') from error
            if event.get('event') == 'admit' and event.get('pool') == pool:
                start = timestamp(event['at'])
                cost = event['cost_mib']
                run = Run(event['run'], start, start - event.get('waited_ms', 0) / 1000,
                          cost,
                          event.get('kind'), event.get('campaign', ''),
                          event.get('weight', event.get('level', 0)), pool)
                runs.append(run)
                latest[run.run] = run
            elif event.get('event') == 'end' and event.get('run') in latest:
                run = latest[event['run']]
                end = timestamp(event['at'])
                if end >= run.start and run.end is None:
                    run.end = end
                    peak = event.get('memory_peak_bytes')
                    run.peak = peak / 1048576 if peak is not None else None
    return runs


def peak_groups(runs):
    groups = defaultdict(list)
    for run in runs:
        if run.peak is not None:
            groups[run.admission_class].append(run.peak)
    return groups


def replay(runs, budget, slots, fair_wait, campaign_cap=0, charge="cost"):
    """Static-budget model: recorded weights/caps, aged fair turns, 8 backfills."""
    charges = [run.cost if charge == "cost" else max(run.cost, run.peak) for run in runs]
    arrivals = sorted(range(len(runs)), key=lambda i: (runs[i].arrival, i))
    pending, active, schedule = [], [], []
    backfills = Counter()
    held = Counter()
    position, charged = 0, 0
    now = min((run.arrival for run in runs), default=0)
    while position < len(arrivals) or pending or active:
        while active and active[0][0] <= now:
            _, i = heapq.heappop(active)
            charged -= charges[i]
            held[runs[i].campaign] -= 1
        while position < len(arrivals) and runs[arrivals[position]].arrival <= now:
            pending.append(arrivals[position])
            position += 1

        def capped(i):
            run = runs[i]
            if run.weight > 0:
                return held[run.campaign] >= (min(slots, campaign_cap) if campaign_cap else slots)
            return campaign_cap > 0 and held[run.campaign] >= campaign_cap and any(
                runs[j].campaign != run.campaign and held[runs[j].campaign] < campaign_cap
                for j in pending)

        while len(active) < slots:
            eligible = sorted((i for i in pending if not capped(i)), key=lambda i: (
                now - runs[i].arrival < fair_wait,
                0 if now - runs[i].arrival >= fair_wait else -runs[i].weight,
                runs[i].arrival, i))
            if not eligible:
                break
            first = eligible[0]
            chosen = first if charged + charges[first] <= budget else None
            if chosen is None and now - runs[first].arrival >= fair_wait and backfills[first] < 8:
                chosen = next((i for i in eligible if runs[i].cost < runs[first].cost
                               and charged + charges[i] <= budget), None)
                if chosen is not None:
                    backfills[first] += 1
            if chosen is None:
                break
            duration = max(.001, runs[chosen].end - runs[chosen].start)
            schedule.append((now, now + duration, chosen))
            heapq.heappush(active, (now + duration, chosen))
            charged += charges[chosen]
            held[runs[chosen].campaign] += 1
            pending.remove(chosen)
        next_times = [finish for finish, _ in active]
        if position < len(arrivals):
            next_times.append(runs[arrivals[position]].arrival)
        next_times.extend(runs[i].arrival + fair_wait for i in pending
                          if runs[i].arrival + fair_wait > now)
        if not next_times:
            if pending:
                raise ValueError('replay cannot admit a request; exclude admission charges above the budget')
            break
        now = min(next_times)
    return schedule


def metrics(runs, schedule, limit):
    changes = []
    for start, end, i in schedule:
        changes.extend(((start, 1, i), (end, -1, i)))
    changes.sort()  # Releases precede admissions at equal timestamps.
    active = 0
    peaks = area = over_seconds = maximum = 0.0
    max_concurrency = over_starts = over_intervals = 0
    previous = min((run.arrival for run in runs), default=0)
    was_over = False
    for at, change, i in changes:
        area += active * (at - previous)
        if peaks > limit:
            over_seconds += at - previous
        active += change
        peaks += change * runs[i].peak
        maximum = max(maximum, peaks)
        max_concurrency = max(max_concurrency, active)
        if change > 0 and peaks > limit:
            over_starts += 1
        if peaks > limit and not was_over:
            over_intervals += 1
        was_over = peaks > limit
        previous = at
    span = previous - min((run.arrival for run in runs), default=0)
    return dict(n=len(schedule), maximum_concurrency=max_concurrency,
                mean_concurrency=area / span if span else 0,
                peak_sum_mib=maximum, overrun_admissions=over_starts,
                overrun_intervals=over_intervals, overrun_seconds=over_seconds,
                makespan_seconds=span,
                queue_hours=sum(start - runs[i].arrival for start, _, i in schedule) / 3600)


def journal_ooms(records, runs):
    episodes = defaultdict(list)
    for run in runs:
        episodes[run.run + '.service'].append(run.start)
    found = set()
    unattributed = []
    for record in records:
        message = record.get('MESSAGE', '')
        if not isinstance(message, str) or not re.search(r'oom-kill|OOM killer', message, re.I):
            continue
        names = [record.get(key, '') for key in ('USER_UNIT', 'UNIT', '_SYSTEMD_USER_UNIT')]
        names += re.findall(r'slotr-[A-Za-z0-9_-]+\.service', message)
        unit = next((name for name in names if isinstance(name, str) and
                     re.fullmatch(r'slotr-[A-Za-z0-9_-]+\.service', name)), None)
        if unit is None:
            slices = names + re.findall(r'slotr(?:-tests)?\.slice', message)
            if any(name in ('slotr-tests.slice', 'slotr.slice') for name in slices):
                unattributed.append(record)
            continue
        at = int(record.get('__REALTIME_TIMESTAMP', '0')) / 1000000
        invocation = record.get('USER_INVOCATION_ID') or record.get('INVOCATION_ID')
        # Manager messages and payload messages have different journald origins;
        # admission episode is their shared deduplication key when available.
        starts = [start for start in episodes[unit] if start <= at + 2]
        key = max(starts) if starts else invocation or 'unmatched'
        found.add((unit, key))
    return found, unattributed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('events', type=Path)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--pool', default='heavy')
    parser.add_argument('--since', required=True, help='inclusive ISO timestamp with timezone')
    parser.add_argument('--until', required=True, help='exclusive ISO timestamp with timezone')
    parser.add_argument('--slice-limit-mib', required=True, type=int)
    parser.add_argument('--available-mib', type=float,
                        help='assumed MemAvailable before modeled runs; default is the slice limit')
    parser.add_argument('--no-journal', action='store_true', help='skip journal query for offline fixtures')
    args = parser.parse_args()
    since, until = timestamp(args.since), timestamp(args.until)
    if until <= since or args.slice_limit_mib <= 0:
        parser.error('until must follow since and slice limit must be positive')
    cfg = tomllib.loads(args.config.read_text())
    pool = cfg['pools'][args.pool]
    reserve = cfg.get('admission', {}).get('reserve_mib', 5120)
    available = args.slice_limit_mib if args.available_mib is None else args.available_mib
    budget = available - reserve
    if budget <= 0:
        parser.error('assumed available memory must exceed reserve_mib')
    all_runs = read_runs(args.events, args.pool)
    window = [run for run in all_runs if since <= run.start < until]
    groups = peak_groups(window)
    print(f'Window: {args.since} <= admission < {args.until}; pool={args.pool}')
    print(f'Slice limit={args.slice_limit_mib} MiB; assumed available={available} MiB; '
          f'reserve={reserve} MiB; static admission budget={budget} MiB')
    print('Peaks are cgroup totals, nearest-rank percentiles, all measured outcomes including censored OOMs.')
    print('| class | admitted | measured n | p50 MiB | p95 MiB | max MiB | current cost MiB |')
    print('| --- | ---: | ---: | ---: | ---: | ---: | ---: |')
    counts = Counter(run.admission_class for run in window)
    costs = {run.admission_class: run.cost for run in window if run.kind is None}
    costs.update({f'{args.pool}:{kind}': entry.get('cost_mib', '-')
                  for kind, entry in pool.get('kinds', {}).items()})
    for admission_class in sorted(set(counts) | set(costs)):
        peaks = groups.get(admission_class, [])
        cost = costs.get(admission_class, '-')
        stats = ([f'{percentile(peaks, p):.1f}' for p in (.5, .95)] + [f'{max(peaks):.1f}']
                 if peaks else ['-', '-', '-'])
        print(f'| {admission_class} | {counts[admission_class]} | {len(peaks)} | {" | ".join(stats)} | {cost} |')
    cohort = [run for run in all_runs if run.arrival < until and
              ((run.end is not None and run.end > since) or
               (run.end is None and run.start >= since))]
    unknown_earlier = sum(run.end is None and run.start < since for run in all_runs)
    complete = [run for run in cohort if run.end is not None and run.peak is not None]
    print(f'Overlap cohort={len(cohort)}; missing end/peak excluded={len(cohort)-len(complete)}')
    print(f'Earlier admissions with no recorded end={unknown_earlier}; their overlap is unknown, '
          'not assumed to still be running. Journal-only peaks are not imported.')
    observed = [(run.start, run.end, i) for i, run in enumerate(complete)]
    results = {'observed': metrics(complete, observed, args.slice_limit_mib)}
    for charge in ('cost', 'max_cost_peak'):
        jobs = [run for run in complete if
                (run.cost if charge == 'cost' else max(run.cost, run.peak)) <= budget]
        schedule = replay(jobs, budget, pool['slots'],
                          cfg.get('priority', {}).get('fair_wait_seconds', 300),
                          pool.get('campaign_cap', 1), charge)
        results[charge] = metrics(jobs, schedule, args.slice_limit_mib)
        results[charge]['excluded_above_budget'] = len(complete) - len(jobs)
    print('Each overlap sum uses measured whole-run peaks for every included run throughout its modeled lifetime.')
    for name, result in results.items():
        print(name + ': ' + json.dumps(result, sort_keys=True))
    print('cost charges the declared admit-time cost; max_cost_peak charges max(cost, future whole-run peak). '
          'The latter uses information unavailable at admission and is a comparison model, not a safety proof.')
    print('Neither the slice-sized nor the host-total static scenario reproduces observed queue time; '
          'these are not before/after forecasts.')
    print('Replay assumptions: fixed recorded arrivals/runtimes/weights, campaign caps and aged '
          'fair turns with eight backfills; assumed static available memory (historical MemAvailable '
          'is not logged); no PSI/load/quiet tails, cancellations, cross-pool jobs, dependency feedback '
          'or time-varying memory profiles. An overrun is modeled peak overlap, not a predicted OOM; '
          'missing/censored peaks limit conclusions. No configuration proposal is generated.')
    if args.no_journal:
        print('Real journal OOM unit episodes / unattributed OOM records: not queried (--no-journal)')
    else:
        bounds = [datetime.fromtimestamp(at, timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')
                  for at in (since, until)]
        try:
            journal = subprocess.run(['journalctl', '--user', '--since', bounds[0], '--until', bounds[1],
                                      '--output=json', '--no-pager'], capture_output=True, text=True,
                                     timeout=120, check=True)
            records = [json.loads(line) for line in journal.stdout.splitlines()]
            records = [record for record in records if since <=
                       int(record.get('__REALTIME_TIMESTAMP', '0')) / 1000000 < until]
            ooms, unattributed = journal_ooms(records, all_runs)
            print(f'Real journal OOM count: {len(ooms)} slotr unit episodes; '
                  f'{sum(unit.startswith("slotr-" + args.pool + "-") for unit, _ in ooms)} in {args.pool}')
            print(f'Unattributed OOM records: {len(unattributed)} (slotr-tests.slice / slotr.slice; not assigned to a run)')
        except (OSError, subprocess.SubprocessError, ValueError) as error:
            print(f'Real journal OOM unit episodes / unattributed OOM records: unavailable ({error}); not zero')



if __name__ == '__main__':
    main()
