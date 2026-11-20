# Shopping Sessions in Rust

This is a starting scaffold, not a working pipeline. The executable exits with an error until the processing and Parquet output are implemented.

Rust 1.92 or newer and Cargo are required. Run Cargo through `./cargo-local` to keep downloaded dependencies and Cargo state in this directory rather than your home directory. Build artifacts also stay here:

```sh
./cargo-local build
./cargo-local test
./cargo-local build --release
```

The Dockerfile builds a Linux executable and runs it as UID 1000. From this directory, build an amd64 image with `docker build --platform linux/amd64 -t shopping-sessions .`. The image still needs the pipeline before it can satisfy the challenge; the input, output, and scratch directories are supplied at runtime as described in the challenge brief.

This directory is intended to be the root of the submission repository, where the challenge requires the Dockerfile. Do not commit `target` or `.cargo-home`.
