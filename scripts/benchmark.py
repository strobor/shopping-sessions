#!/usr/bin/env python3
"""Run repeatable, resource-limited benchmarks and retain machine-readable results."""
import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
TIME = Path('/usr/bin/time')
FORMAT = '\n'.join(f'{k}=%{v}' for k, v in [
    ('elapsed_seconds','e'), ('user_seconds','U'), ('system_seconds','S'),
    ('max_rss_kib','M'), ('major_faults','F'), ('minor_faults','R'),
    ('block_inputs','I'), ('block_outputs','O'), ('voluntary_switches','w'),
    ('involuntary_switches','c'), ('exit_status','x')])
STAGE = re.compile(r'^([^:]+): ([0-9]+\.[0-9]+)s$', re.MULTILINE)


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def write_json(path, value):
    tmp = path.with_suffix('.tmp')
    tmp.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    tmp.replace(path)


def git(*args):
    p = subprocess.run(['git', '-C', str(ROOT), *args], capture_output=True, text=True)
    return p.stdout.strip() if p.returncode == 0 else None


def file_sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def dataset_info(path):
    manifest = path / 'manifest.json'
    files = sorted(p for p in path.rglob('*') if p.is_file())
    h = hashlib.sha256()
    for p in files:
        stat = p.stat()
        h.update(f'{p.relative_to(path)}\0{stat.st_size}\n'.encode())
    return {'path': str(path), 'files': len(files), 'bytes': sum(p.stat().st_size for p in files),
            'inventory_sha256': h.hexdigest(),
            'manifest_sha256': file_sha(manifest) if manifest.is_file() else None,
            'manifest': json.loads(manifest.read_text()) if manifest.is_file() else None}


def cpu_model():
    try:
        for line in Path('/proc/cpuinfo').read_text().splitlines():
            if line.startswith('model name'):
                return line.split(':', 1)[1].strip()
    except OSError:
        pass
    return platform.processor()


def proc_sample(parent):
    """Sample the pipeline child launched by GNU time; values are cumulative."""
    try:
        children = Path(f'/proc/{parent}/task/{parent}/children').read_text().split()
        if not children:
            return None
        pid = int(children[0])
        stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        io = dict(line.split(':', 1) for line in Path(f'/proc/{pid}/io').read_text().splitlines())
        return {'cpu_seconds': (int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK'),
                'rss_bytes': int(stat[21]) * os.sysconf('SC_PAGE_SIZE'),
                'read_bytes': int(io['read_bytes']), 'write_bytes': int(io['write_bytes'])}
    except (OSError, ValueError, IndexError, KeyError):
        return None


def parse_time(path):
    if not path.exists():
        return {}
    values = {}
    for line in path.read_text().splitlines():
        if '=' in line:
            k, v = line.split('=', 1)
            try:
                values[k] = float(v) if '.' in v else int(v)
            except ValueError:
                pass
    return values


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--data', type=Path, required=True, help='input dataset directory')
    p.add_argument('--name', required=True, help='experiment name')
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/shopping-sessions')
    p.add_argument('--reference', type=Path, help='expected CSV directory or reference output directory')
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--threads', type=int, default=8)
    p.add_argument('--memory-gib', type=float, default=24)
    p.add_argument('--timeout', type=float, default=180)
    p.add_argument('--sample-seconds', type=float, default=0.5)
    p.add_argument('--results', type=Path, default=ROOT/'.benchmarks')
    p.add_argument('--keep-output', action='store_true')
    p.add_argument('--notes', default='')
    p.add_argument('--no-journal', action='store_true', help='do not add this run to experiments/runs.jsonl')
    return p


def main():
    a = parser().parse_args()
    if min(a.runs, a.threads) < 1 or min(a.timeout, a.memory_gib, a.sample_seconds) <= 0:
        raise SystemExit('runs, threads, memory, timeout and sample interval must be positive')
    data, binary = a.data.resolve(), a.binary.resolve()
    reference = a.reference.resolve() if a.reference else None
    if not data.is_dir() or not binary.is_file() or not TIME.is_file():
        raise SystemExit('dataset, binary, or /usr/bin/time is missing')
    if reference and not reference.is_dir():
        raise SystemExit('reference directory is missing')
    allowed = sorted(os.sched_getaffinity(0))
    if len(allowed) < a.threads:
        raise SystemExit(f'requested {a.threads} CPUs but only {len(allowed)} are available')
    cpus = allowed[:a.threads]
    base = a.results.resolve()
    base.mkdir(parents=True, exist_ok=True)
    job = base / (dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8])
    job.mkdir()
    # Snapshot before queueing so later builds cannot change the executable under test.
    snapshot = job/'pipeline'
    shutil.copy2(binary, snapshot)
    diff = subprocess.run(['git', '-C', str(ROOT), 'diff', '--binary', 'HEAD'], capture_output=True).stdout
    metadata = {'name': a.name, 'notes': a.notes, 'created_utc': now(),
        'git_commit': git('rev-parse', 'HEAD'), 'git_status': git('status', '--porcelain'),
        'tracked_diff_sha256': hashlib.sha256(diff).hexdigest(),
        'binary_sha256': file_sha(snapshot), 'binary_source': str(binary),
        'dataset': dataset_info(data), 'reference': str(reference) if reference else None,
        'machine': {'host': platform.node(), 'cpu': cpu_model(), 'kernel': platform.release(),
                    'architecture': platform.machine(), 'allowed_cpus': allowed, 'selected_cpus': cpus},
        'settings': {'runs': a.runs, 'threads': a.threads, 'memory_gib': a.memory_gib,
                     'timeout_seconds': a.timeout, 'sample_seconds': a.sample_seconds,
                     'keep_output': a.keep_output}}
    write_json(job/'metadata.json', metadata)
    def status(state, **extra):
        write_json(job/'status.json', {'state': state, 'updated_utc': now(), 'pid': os.getpid(), **extra})
    status('queued')
    print(f'Benchmark: {job}', flush=True)
    print('Waiting for the benchmark lock (other experiments may run, but benchmark timing should be isolated).', flush=True)
    all_results = []
    with (base/'.benchmark.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        status('running', iteration=1)
        for index in range(1, a.runs + 1):
            status('running', iteration=index)
            run_dir = job/f'run-{index:03d}'
            run_dir.mkdir()
            out = run_dir/'output'
            out.mkdir()
            scratch = run_dir/'scratch'
            scratch.mkdir()
            time_file = run_dir/'time.txt'
            env = os.environ.copy()
            env.update(DATA_DIR=str(data), OUTPUT_DIR=str(out), SCRATCH_DIR=str(scratch), HOME=str(scratch),
                       NCPU=str(a.threads), MEM_GB=str(a.memory_gib), SHOPPING_TIMING='1', LC_ALL='C')
            def limits():
                os.setsid()
                os.sched_setaffinity(0, cpus)
                limit = int(a.memory_gib * (1024 ** 3))
                resource.setrlimit(resource.RLIMIT_AS, (limit, limit))
            start = time.monotonic()
            with (run_dir/'stdout.txt').open('w') as stdout, (run_dir/'stderr.txt').open('w') as stderr, (run_dir/'samples.jsonl').open('w') as samples:
                p = subprocess.Popen([str(TIME), '-o', str(time_file), '-f', FORMAT, str(snapshot)],
                                     cwd=ROOT, env=env, stdout=stdout, stderr=stderr, preexec_fn=limits)
                timed_out = False
                while p.poll() is None:
                    elapsed = time.monotonic() - start
                    sample = proc_sample(p.pid)
                    if sample:
                        samples.write(json.dumps({'elapsed_seconds': round(elapsed, 3), **sample}) + '\n')
                    if elapsed >= a.timeout:
                        timed_out = True
                        os.killpg(p.pid, signal.SIGKILL)
                        break
                    try:
                        p.wait(timeout=min(a.sample_seconds, max(0.01, a.timeout - elapsed)))
                    except subprocess.TimeoutExpired:
                        pass
                p.wait()
            elapsed = time.monotonic() - start
            stderr_text = (run_dir/'stderr.txt').read_text(errors='replace')
            resources = parse_time(time_file)
            if 'user_seconds' in resources and 'system_seconds' in resources:
                resources['average_cpu_percent'] = 100 * (resources['user_seconds'] + resources['system_seconds']) / elapsed
            samples_data = [json.loads(line) for line in (run_dir/'samples.jsonl').read_text().splitlines()]
            sampled = {'count': len(samples_data)}
            if samples_data:
                for field in ('rss_bytes', 'read_bytes', 'write_bytes'):
                    sampled['max_' + field] = max(sample[field] for sample in samples_data)
            result = {'iteration': index, 'wall_seconds': elapsed, 'exit_code': p.returncode,
                      'timed_out': timed_out, 'resources': resources, 'sampled': sampled,
                      'stages_seconds': {k: float(v) for k, v in STAGE.findall(stderr_text)},
                      'correctness': 'not_checked'}
            if p.returncode == 0 and reference:
                comparison = subprocess.run([sys.executable, str(ROOT/'scripts/compare.py'), str(out), str(reference)],
                                            cwd=ROOT, capture_output=True, text=True)
                (run_dir/'comparison.txt').write_text(comparison.stdout + comparison.stderr)
                result['correctness'] = 'pass' if comparison.returncode == 0 else 'fail'
            if not a.keep_output:
                shutil.rmtree(out)
            shutil.rmtree(scratch)
            write_json(run_dir/'result.json', result)
            all_results.append(result)
            print(f"run {index}: {elapsed:.2f}s, exit {p.returncode}, correctness {result['correctness']}", flush=True)
            if p.returncode != 0 or result['correctness'] == 'fail':
                break
        ok = len(all_results) == a.runs and all(r['exit_code'] == 0 and r['correctness'] != 'fail' for r in all_results)
        summary = {'runs': all_results, 'completed_utc': now(), 'success': ok,
                   'median_wall_seconds': statistics.median(r['wall_seconds'] for r in all_results) if all_results else None,
                   'correctness': 'not_checked' if not reference else ('pass' if ok else 'fail')}
        stage_names = sorted({name for r in all_results for name in r['stages_seconds']})
        summary['median_stages_seconds'] = {name: statistics.median(r['stages_seconds'][name] for r in all_results if name in r['stages_seconds']) for name in stage_names}
        summary['max_rss_kib'] = max((r['resources'].get('max_rss_kib', 0) for r in all_results), default=0) or None
        write_json(job/'report.json', summary)
        if not a.no_journal:
            journal = ROOT/'experiments'/'runs.jsonl'
            journal.parent.mkdir(exist_ok=True)
            with journal.open('a') as f:
                fcntl.flock(f, fcntl.LOCK_EX)
                f.write(json.dumps({'job': str(job), 'metadata': metadata, 'summary': summary}, sort_keys=True) + '\n')
                f.flush()
                fcntl.flock(f, fcntl.LOCK_UN)
        status('completed' if ok else 'failed', completed_runs=len(all_results))
    print(f"Result: {job/'report.json'}", flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
