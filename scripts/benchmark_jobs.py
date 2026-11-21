#!/usr/bin/env python3
"""Launch benchmark.py in a persistent tmux session and inspect its status."""
import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT/'.benchmarks'
LAUNCHES = BASE/'launches'
SOCKET = BASE/'tmux.sock'


def start(args):
    if args and args[0] == '--':
        args = args[1:]
    if not args:
        raise SystemExit('Usage: benchmark_jobs.py start -- <benchmark.py arguments>')
    LAUNCHES.mkdir(parents=True, exist_ok=True)
    name = 'bench-' + uuid.uuid4().hex[:12]
    directory = LAUNCHES/name
    directory.mkdir()
    log = directory/'launch.log'
    command = [sys.executable, str(ROOT/'scripts/benchmark.py'), *args]
    (directory/'command.json').write_text(json.dumps(command, indent=2) + '\n')
    shell = 'exec ' + shlex.join(command) + ' > ' + shlex.quote(str(log)) + ' 2>&1'
    with (directory/'tmux-error.log').open('w') as errors:
        result = subprocess.run(['tmux', '-S', str(SOCKET), 'new-session', '-d', '-s', name, shell],
                                stdout=subprocess.DEVNULL, stderr=errors)
    if result.returncode:
        raise SystemExit('Could not launch tmux: ' + (directory/'tmux-error.log').read_text().strip())
    print('Launch ID:', name, flush=True)
    # Wait until the executable is snapshotted, or a setup error is visible.
    for _ in range(300):
        text = log.read_text(errors='replace') if log.exists() else ''
        if 'Benchmark: ' in text:
            print(text.splitlines()[0], flush=True)
            return 0
        alive = subprocess.run(['tmux', '-S', str(SOCKET), 'has-session', '-t', name],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
        if not alive:
            print(text, file=sys.stderr)
            return 1
        time.sleep(0.1)
    print('Still starting; inspect the launch log before replacing the binary.', flush=True)
    return 0


def status(name):
    if not re.fullmatch(r'bench-[0-9a-f]{12}', name):
        raise SystemExit('invalid launch ID')
    directory = LAUNCHES/name
    if not directory.is_dir():
        raise SystemExit('unknown launch ID')
    log = directory/'launch.log'
    text = log.read_text(errors='replace') if log.exists() else ''
    print(text or '(waiting for log)', end='' if text.endswith('\n') else '\n')
    match = re.search(r'^Benchmark: (.+)$', text, re.MULTILINE)
    if match:
        path = Path(match.group(1))/'status.json'
        if path.exists():
            print('Status:', path.read_text().strip())
            print('Status file:', path)
    return 0


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['start', 'status', 'list'])
    p.add_argument('args', nargs=argparse.REMAINDER)
    a = p.parse_args()
    if a.action == 'start': return start(a.args)
    if a.action == 'status':
        if len(a.args) != 1: raise SystemExit('status requires one launch ID')
        return status(a.args[0])
    for d in sorted(LAUNCHES.glob('bench-*')):
        print(d.name)
    return 0

if __name__ == '__main__':
    sys.exit(main())
