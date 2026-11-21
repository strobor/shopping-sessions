# Experiment log

`HISTORY.md` records the earlier experiments and decisions, including incomplete measurements. New benchmark jobs append one JSON record to `runs.jsonl` when they finish, whether they succeed or fail. Each record includes the binary hash, commit and working-tree status, machine and dataset inventory, limits, individual results, stage timings, and correctness outcome. Raw samples, captured logs, and the executable snapshot live in the ignored `.benchmarks/` job directory; archive those separately if long-term retention is needed.

Record the hypothesis, comparison, and decision in `HISTORY.md` for substantive changes. Keep unfavorable or inconclusive results. Use `--no-journal` for harness smoke tests rather than performance experiments. The inventory hash covers file names and sizes, not file contents; retain the generator seed and command in notes, and use a content checksum when exact dataset identity matters.

Treat local and leaderboard measurements separately. Compare repeated runs on the same machine and dataset, under the same limits, with a quiet machine and a correctness check. The harness serializes its own jobs, but other processes can still interfere. A successful run without a reference is marked `not_checked`, not correct.
