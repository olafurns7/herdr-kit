#!/usr/bin/env python3
"""Read-only reservation proposals and conservative overlap replay (stdlib only)."""
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
    reservation: int
    kind: str
    campaign: str
    weight: int
    end: float | None = None
    peak: float | None = None


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
                reservation = event.get('reservation_mib')
                run = Run(event['run'], start, start - event.get('waited_ms', 0) / 1000,
                          cost, cost if reservation is None else reservation,
                          event.get('kind') or '<untyped>', event.get('campaign', ''),
                          event.get('weight', event.get('level', 0)))
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


def proposals(runs, kinds):
    groups = defaultdict(list)
    for run in runs:
        if run.peak is not None:
            groups[run.kind].append(run.peak)
    proposed = {}
    for kind, peaks in groups.items():
        if kind == '<untyped>':
            continue
        cost = kinds[kind]['cost_mib'] if kind in kinds else min(
            run.cost for run in runs if run.kind == kind)
        proposed[kind] = min(cost, math.ceil(percentile(peaks, .95) / 256) * 256)
    return groups, proposed


def replay(runs, reservations, budget, slots, fair_wait, campaign_cap=0):
    """Static-budget model: recorded weights/caps, aged fair turns, 8 backfills."""
    arrivals = sorted(range(len(runs)), key=lambda i: (runs[i].arrival, i))
    pending, active, schedule = [], [], []
    backfills = Counter()
    held = Counter()
    position, reserved = 0, 0
    now = min((run.arrival for run in runs), default=0)
    while position < len(arrivals) or pending or active:
        while active and active[0][0] <= now:
            _, i = heapq.heappop(active)
            reserved -= max(reservations[i], runs[i].peak)
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
            chosen = first if reserved + reservations[first] <= budget else None
            if chosen is None and now - runs[first].arrival >= fair_wait and backfills[first] < 8:
                chosen = next((i for i in eligible if reservations[i] < reservations[first]
                               and reserved + reservations[i] <= budget), None)
                if chosen is not None:
                    backfills[first] += 1
            if chosen is None:
                break
            duration = max(.001, runs[chosen].end - runs[chosen].start)
            schedule.append((now, now + duration, chosen))
            heapq.heappush(active, (now + duration, chosen))
            # Treat each held whole-run peak as anonymous memory already used:
            # available + outstanding accounting then charges max(peak, reservation).
            reserved += max(reservations[chosen], runs[chosen].peak)
            held[runs[chosen].campaign] += 1
            pending.remove(chosen)
        next_times = [finish for finish, _ in active]
        if position < len(arrivals):
            next_times.append(runs[arrivals[position]].arrival)
        next_times.extend(runs[i].arrival + fair_wait for i in pending
                          if runs[i].arrival + fair_wait > now)
        if not next_times:
            if pending:
                raise ValueError('replay cannot admit a request; exclude reservations above the budget')
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
    for record in records:
        message = record.get('MESSAGE', '')
        if not isinstance(message, str) or not re.search(r'oom-kill|OOM killer', message, re.I):
            continue
        names = [record.get(key, '') for key in ('USER_UNIT', 'UNIT', '_SYSTEMD_USER_UNIT')]
        names += re.findall(r'slotr-[A-Za-z0-9_-]+\.service', message)
        unit = next((name for name in names if isinstance(name, str) and
                     re.fullmatch(r'slotr-[A-Za-z0-9_-]+\.service', name)), None)
        if unit is None:
            continue
        at = int(record.get('__REALTIME_TIMESTAMP', '0')) / 1000000
        invocation = record.get('USER_INVOCATION_ID') or record.get('INVOCATION_ID')
        # Manager messages and payload messages have different journald origins;
        # admission episode is their shared deduplication key when available.
        starts = [start for start in episodes[unit] if start <= at + 2]
        key = max(starts) if starts else invocation or 'unmatched'
        found.add((unit, key))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('events', type=Path)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--pool', default='heavy')
    parser.add_argument('--since', required=True, help='inclusive ISO timestamp with timezone')
    parser.add_argument('--until', required=True, help='exclusive ISO timestamp with timezone')
    parser.add_argument('--slice-limit-mib', required=True, type=int)
    parser.add_argument('--no-journal', action='store_true', help='skip journal query for offline fixtures')
    args = parser.parse_args()
    since, until = timestamp(args.since), timestamp(args.until)
    if until <= since or args.slice_limit_mib <= 0:
        parser.error('until must follow since and slice limit must be positive')
    cfg = tomllib.loads(args.config.read_text())
    pool = cfg['pools'][args.pool]
    reserve = cfg.get('admission', {}).get('reserve_mib', 5120)
    budget = args.slice_limit_mib - reserve
    if budget <= 0:
        parser.error('slice limit must exceed reserve_mib')
    all_runs = read_runs(args.events, args.pool)
    window = [run for run in all_runs if since <= run.start < until]
    groups, proposed = proposals(window, pool.get('kinds', {}))
    print(f'Window: {args.since} <= admission < {args.until}; pool={args.pool}')
    print(f'Slice limit={args.slice_limit_mib} MiB; reserve={reserve} MiB; static admission budget={budget} MiB')
    print('Peaks are cgroup totals, nearest-rank percentiles, all measured outcomes including censored OOMs.')
    print('| kind | admitted | measured n | p50 MiB | p95 MiB | max MiB | current cost MiB | proposed reservation MiB |')
    print('| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |')
    counts = Counter(run.kind for run in window)
    for kind in sorted(set(counts) | set(pool.get('kinds', {}))):
        peaks = groups.get(kind, [])
        cost = pool.get('kinds', {}).get(kind, {}).get('cost_mib', '-')
        stats = ([f'{percentile(peaks, p):.1f}' for p in (.5, .95)] + [f'{max(peaks):.1f}']
                 if peaks else ['-', '-', '-'])
        print(f'| {kind} | {counts[kind]} | {len(peaks)} | {" | ".join(stats)} | {cost} | {proposed.get(kind, "unset")} |')
    cohort = [run for run in all_runs if run.arrival < until and
              ((run.end is not None and run.end > since) or
               (run.end is None and run.start >= since))]
    unknown_earlier = sum(run.end is None and run.start < since for run in all_runs)
    complete = [run for run in cohort if run.end is not None and run.peak is not None]
    def candidate_reservation(run):
        return min(run.cost, proposed[run.kind]) if run.kind in proposed else run.reservation

    jobs = [run for run in complete if max(run.reservation, candidate_reservation(run)) <= budget]
    print(f'Overlap cohort={len(cohort)}; missing end/peak excluded={len(cohort)-len(complete)}; '
          f'baseline/proposed reservation above static budget excluded={len(complete)-len(jobs)}')
    print(f'Earlier admissions with no recorded end={unknown_earlier}; their overlap is unknown, '
          'not assumed to still be running. Journal-only peaks are not imported.')
    baseline = [run.reservation for run in jobs]
    candidate = [candidate_reservation(run) for run in jobs]
    observed = [(run.start, run.end, i) for i, run in enumerate(jobs)]
    results = {'observed': metrics(jobs, observed, args.slice_limit_mib)}
    for name, reservations in (('baseline', baseline), ('proposed', candidate)):
        schedule = replay(jobs, reservations, budget, pool['slots'],
                          cfg.get('priority', {}).get('fair_wait_seconds', 300), pool.get('campaign_cap', 1))
        results[name] = metrics(jobs, schedule, args.slice_limit_mib)
    print('Worst case: each included run consumes its measured whole-run peak for its entire lifetime.')
    for name, result in results.items():
        print(name + ': ' + json.dumps(result, sort_keys=True))
    base, new = results['baseline'], results['proposed']
    print('Concurrency gain: maximum=%+d; mean=%+.3f; makespan_seconds=%+.3f' % (
        new['maximum_concurrency'] - base['maximum_concurrency'],
        new['mean_concurrency'] - base['mean_concurrency'],
        new['makespan_seconds'] - base['makespan_seconds']))
    print('Replay is an approximation: fixed recorded arrivals/runtimes/weights, campaign caps and aged '
          'fair turns with eight backfills; static slice budget, held charge=max(reservation, peak), '
          'peak treated as anonymous memory; no PSI/load/quiet tails, '
          'cancellations, cross-pool jobs, dependency feedback or time-varying memory profiles. '
          'An overrun is modeled peak overlap, not a predicted OOM; missing/censored peaks limit conclusions.')
    if args.no_journal:
        print('Real journal OOM count: not queried (--no-journal)')
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
            ooms = journal_ooms(records, all_runs)
            print(f'Real journal OOM count: {len(ooms)} slotr unit episodes; '
                  f'{sum(unit.startswith("slotr-" + args.pool + "-") for unit, _ in ooms)} in {args.pool}')
        except (OSError, subprocess.SubprocessError, ValueError) as error:
            print(f'Real journal OOM count: unavailable ({error}); not zero')
    print('Proposed additions only (no file is changed; no samples means leave unset):')
    for kind in sorted(pool.get('kinds', {})):
        if kind in proposed:
            print(f'[pools.{args.pool}.kinds.{kind}]\nreservation_mib = {proposed[kind]}\n')


if __name__ == '__main__':
    main()
