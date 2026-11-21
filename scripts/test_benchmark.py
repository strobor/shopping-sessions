#!/usr/bin/env python3
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class HarnessTest(unittest.TestCase):
    def run_harness(self, executable, directory, timeout=5):
        cmd = [sys.executable, str(ROOT/'scripts/benchmark.py'), '--name', 'test',
               '--data', str(ROOT/'fixtures/A1/input'), '--binary', str(executable),
               '--results', str(directory), '--runs', '1', '--threads', '1',
               '--timeout', str(timeout), '--sample-seconds', '0.02', '--no-journal']
        result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        job = next(d for d in directory.iterdir() if d.is_dir())
        return result, json.loads((job/'report.json').read_text()), json.loads((job/'status.json').read_text())

    def test_success_is_not_correctness(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, report, status = self.run_harness(Path('/bin/true'), Path(tmp))
            self.assertEqual(result.returncode, 0)
            self.assertEqual(report['correctness'], 'not_checked')
            self.assertEqual(status['state'], 'completed')

    def test_failure_is_recorded(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, report, status = self.run_harness(Path('/bin/false'), Path(tmp))
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(report['success'])
            self.assertEqual(status['state'], 'failed')

    def test_timeout_is_recorded(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            binary = directory/'slow'
            binary.write_text('#!/bin/sh\nsleep 10\n')
            binary.chmod(0o755)
            results = directory/'results'
            result, report, status = self.run_harness(binary, results, timeout=0.15)
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(report['runs'][0]['timed_out'])
            self.assertEqual(status['state'], 'failed')

if __name__ == '__main__':
    unittest.main()
