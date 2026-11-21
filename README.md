# Shopping Sessions in Rust

This is an initial correctness-focused pipeline. It has not yet been optimized or validated at full scoring scale.

Rust 1.92 or newer and Cargo are required. Run Cargo through `./cargo-local` to keep downloaded dependencies and Cargo state in this directory rather than your home directory. Build artifacts also stay here:

```sh
./cargo-local build
./cargo-local test
./cargo-local build --release
```

The Dockerfile builds a Linux executable and runs it as UID 1000. From this directory, build an amd64 image with `docker build --platform linux/amd64 -t shopping-sessions .`. The input, output, and scratch directories are supplied at runtime as described in the challenge brief.

This directory is intended to be the root of the submission repository, where the challenge requires the Dockerfile. Do not commit `target` or `.cargo-home`.

## Correctness fixtures

The `fixtures/` directory and `scripts/run_fixtures.py` come from the challenge starter. Once the pipeline writes its Parquet outputs, run all 36 cases with Python 3 and Polars installed:

```sh
./cargo-local build --release
python3 scripts/run_fixtures.py fixtures -- ./target/release/shopping-sessions
```

The fixture runner supplies `DATA_DIR` and `OUTPUT_DIR` and compares both tables to the expected CSVs, including their Parquet schemas. Rust unit tests cover the input primitives independently with `./cargo-local test`. GitHub Actions also runs the fixture suite and builds the container image.

To compare a generated public development run with the supplied answer:

```sh
DATA_DIR=/path/to/dev OUTPUT_DIR=/path/to/output ./target/release/shopping-sessions
python3 scripts/compare.py /path/to/output /path/to/dev-answer
```

The initial implementation holds events in memory and scans each session's qualifying clicks for assists. It is a correctness baseline and may exceed time or memory limits at scoring scale.

Set `SHOPPING_TIMING=1` to print per-stage timings to stderr for local profiling. Timings are disabled by default.

## Benchmarks

The benchmark harness snapshots the release executable, pins it to available CPUs, applies an address-space limit, records stage timings and resource use, and can compare every output to a reference. Build first, and install Polars in the Python environment used to run the harness if comparing outputs:

```sh
python3 -m venv .venv
.venv/bin/pip install polars==1.44.2
./cargo-local build --release --locked
.venv/bin/python scripts/benchmark.py \
  --name baseline --data /path/to/data --reference /path/to/reference \
  --threads 8 --memory-gib 24 --timeout 180 --runs 3 \
  --notes 'Dataset seed, size, and hypothesis'
```

The reference can be the supplied CSV answer directory or another pipeline's output directory. A run without `--reference` is explicitly marked **not checked**. Output is removed after comparison unless `--keep-output` is specified. Jobs write metadata, logs, resource measurements, sampled process data, and a report under `.benchmarks/`; completed jobs also append to `experiments/runs.jsonl`. See [the experiment log](experiments/README.md) for the history and measurement caveats.

For a benchmark that continues independently, use the installed `tmux` session manager:

```sh
.venv/bin/python scripts/benchmark_jobs.py start -- \
  --name baseline --data /path/to/data --reference /path/to/reference --runs 3
.venv/bin/python scripts/benchmark_jobs.py list
.venv/bin/python scripts/benchmark_jobs.py status bench-xxxxxxxxxxxx
```

The launcher waits for the executable snapshot before returning. In this environment launching a session may need permission to run outside the restricted command environment; inspecting saved status and reports does not. Jobs sharing the same results directory queue behind one lock. They can continue while other work is done, but CPU- or disk-intensive work can distort their measurements. The session manager does not protect jobs from host restart or termination. If a job stops unexpectedly, its last status may remain `running` and no completed report should be assumed.

The harness uses `/usr/bin/time` for CPU time, peak RSS, faults, and context switches, and samples Linux `/proc` for process RSS, CPU time, and physical I/O counters. The sampling interval and the address-space limit are approximations, not a reproduction of the scorer's container or cgroup limits. Source commit, working-tree status, binary SHA-256, CPU, affinity, limits, dataset manifest, and a file-name/size inventory hash are saved with every run; the inventory hash does not prove file contents are identical.
