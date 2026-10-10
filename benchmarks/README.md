# ModelBox Rust reproducible performance baselines

These baselines document `modelbox-rs` results. They are not a benchmark against the original C++ ModelBox and do not imply production-grade image inference performance.

## Benchmark modes

- `bench`: repeated one-frame `Runtime::run_plan` calls. Reports min, mean, p50 and p95 **latency**. The historical `throughput_items_per_sec` field is a *reciprocal-of-mean estimate*, not sustained throughput. A machine-readable `throughput_is_estimated_from_mean_latency: true` field makes this explicit.
- `bench-batch`: **one** complete `run_plan` call with N preconstructed input frames and captured terminal outputs. Reports measured wall-clock items/sec for a finite batch. It does not measure live input acquisition, stream ingestion, network I/O or image decoding.

Both modes exclude ONNX session load and Graph compilation, but include thread creation, channel overhead, inference and output capture. `bench-batch` also excludes constructing the input vector, is limited to 64 MiB of input tensors, and retains all outputs in memory. Use a realistic model and fixed shapes before comparing alternative runtimes.

## Running the baselines

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench examples/relu.onnx examples/relu-input.json 100 10

cargo run --release -p modelbox-cli --features onnx -- \
  bench-batch examples/relu.onnx examples/relu-input.json 1000 10
```

Record, with every result: git commit, Rust version, operating system version, CPU/SoC, number of threads, power state, model hash, input shapes, Release profile, warmup count, number of items, and output checksum. Repeat several times on the same hardware. Avoid comparisons across different workload definitions or hardware.

The first captured result was supplied from an Apple Silicon Mac mini on 2026-10-11. Its device subtype and OS patch version were not supplied, and its items/sec figure must be interpreted as an estimate, not throughput.

## Tensor-only graph overhead control

For a control group with the **same three-node graph topology** but no ONNX Runtime model execution, use:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-overhead examples/relu-input.json 1000 10
```

This substitutes a Rust tensor identity FlowUnit for the ONNX inference node. All graph/worker/channel and capture mechanics remain, including tensor cloning. The value is measured finite-batch overhead for this particular trivial graph shape, **not** the platform's overall scheduling overhead or a stable estimate of ONNX kernel time. Measure both commands in the same Release binary and environment; repeat each command several times, interleaving the order to reduce drift. Do not assume that subtracting these two timings isolates pure inference cost: ONNX Runtime adds its own marshaling, mutex and memory allocations.

## User-reported 1,000-item Relu batch baseline

[2026-10-11-macos-arm64-relu-batch.json](baselines/2026-10-11-macos-arm64-relu-batch.json) records the Apple Silicon Mac mini result: 1,000 items in 2.334583 ms, 428,342.02 items/s, aggregate output sum 5,000.0. This is a **finite batch** throughput measurement, not a long-running sustained stream or a statistically robust throughput estimate. Exact SoC, OS version and repeated-run variance remain unmeasured.
