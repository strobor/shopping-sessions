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
