#!/usr/bin/env python3
"""Observe an already-running Mac app and its children; never start capture or upload data."""
import argparse
import datetime as dt
import json
import math
from pathlib import Path
import platform
import re
import subprocess
import time


def process_tree(text, root):
    rows = {}
    for line in text.splitlines():
        pid, parent, rss, cpu = line.split()
        row = (int(parent), int(rss), float(cpu))
        if row[1] < 0 or not math.isfinite(row[2]) or row[2] < 0:
            raise ValueError('invalid process measurement')
        rows[int(pid)] = row
    if root not in rows:
        return None
    chosen = {root}
    while True:
        more = {pid for pid, (parent, _, _) in rows.items() if parent in chosen} - chosen
        if not more:
            break
        chosen.update(more)
    return {'processes': len(chosen), 'rss_kib_sum': sum(rows[p][1] for p in chosen),
            'cpu_percent_sum': sum(rows[p][2] for p in chosen)}


def command(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()


def battery():
    try:
        text = command('pmset', '-g', 'batt')
    except subprocess.CalledProcessError:
        return None
    value = re.search(r'(\d+)%;\s*([^;]+)', text)
    return None if value is None else {'percent': int(value[1]), 'state': value[2].strip()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pid', type=int, required=True, help='PID of boardcast, not its launcher')
    parser.add_argument('--minutes', type=float, default=30)
    parser.add_argument('--output', type=Path, default=Path('eval/reports/macos'))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        parser.error('run this on the physical Mac being measured')
    if args.pid <= 0 or not math.isfinite(args.minutes) or args.minutes <= 0:
        parser.error('PID and minutes must be positive')
    identity = command('ps', '-p', str(args.pid), '-o', 'lstart=')
    if not identity:
        parser.error('that process is not running')
    args.output.mkdir(parents=True, exist_ok=True)
    path = args.output / f'mac-{time.time_ns()}.jsonl'
    info = {'type': 'environment', 'utc': dt.datetime.now(dt.timezone.utc).isoformat(),
            'os': platform.mac_ver()[0], 'arch': platform.machine(),
            'hardware_model': command('sysctl', '-n', 'hw.model'),
            'memory_bytes': int(command('sysctl', '-n', 'hw.memsize')), 'requested_minutes': args.minutes,
            'rss_definition': 'sum of app and descendant RSS; shared pages may be counted more than once',
            'temperature_celsius': None}
    started = time.monotonic()
    count, peak, reason = 0, 0, 'duration-complete'
    with path.open('x') as out:
        def write(row):
            out.write(json.dumps(row, ensure_ascii=False) + '\n')
            out.flush()
        write(info)
        try:
            while True:
                elapsed = time.monotonic() - started
                try:
                    same = command('ps', '-p', str(args.pid), '-o', 'lstart=') == identity
                    measured = process_tree(command('ps', '-axo', 'pid=,ppid=,rss=,%cpu='), args.pid) if same else None
                except subprocess.CalledProcessError:
                    measured = None
                if measured is None:
                    reason = 'app-exited'
                    break
                write({'type': 'sample', 'elapsed_seconds': elapsed, **measured, 'battery': battery()})
                count += 1
                peak = max(peak, measured['rss_kib_sum'])
                remaining = args.minutes * 60 - (time.monotonic() - started)
                if remaining <= 0:
                    break
                time.sleep(min(5, remaining))
        except KeyboardInterrupt:
            reason = 'interrupted'
        write({'type': 'end', 'reason': reason, 'samples': count, 'elapsed_seconds': time.monotonic() - started,
               'peak_rss_kib_sum': peak, 'completed_requested_duration': reason == 'duration-complete'})
    print(path)
    if reason != 'duration-complete':
        raise SystemExit(f'Partial measurement: {reason}')


if __name__ == '__main__':
    main()
