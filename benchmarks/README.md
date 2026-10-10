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

## Paired, interleaved comparison

The `bench-compare` command samples both controls **within one process**, with their sessions and graphs compiled only once. Each pair alternates execution order (Identity→ONNX, then ONNX→Identity) to reduce systematic ordering bias. Statistics are based on repeated **wall-clock finite-batch durations** rather than the reciprocal of single-item latency:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare examples/relu.onnx examples/relu-input.json 1000 9 10
```

The arguments are items per batch, number of paired rounds (3–31) and warmup single-frame calls per graph (0–1000). The command outputs per-round timings and P50/P95/mean durations for each graph. A ratio of Identity median to ONNX median is displayed as a descriptive comparison only, **not** a causal attribution of scheduler time or a direct ONNX kernel benchmark. Input batch cloning, checksums, model loading and graph compilation are outside timed sections, while worker/thread setup, bounded channels, inference and capture are included. Both modes preserve identical graph topology but use different middle operators. Results on synthetic 3-float Relu should not guide production performance conclusions.

## Three-path paired profiling (v0.2)

`bench-compare` now records **three** repeated paths from the same process, rotating their order across rounds:

1. Tensor Identity graph: three nodes, no ONNX inference.
2. ONNX Graph: three nodes with ONNX CPU inference.
3. Direct ONNX FlowUnit: invokes the **same already loaded ONNX FlowUnit** directly in the caller thread, without graph workers, channels or tensor.output.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare examples/relu.onnx examples/relu-input.json 1000 9 10
```

The direct path still incurs ONNX Runtime marshaling, mutex acquisition, native inference and output construction, and retains output buffers for the batch. Input vectors, model session loading, graph compilation and checksum calculations are outside timing. Direct and Graph ONNX output checksums must match. Differences between graph/direct wall-clock medians are descriptive and **must not** be labeled pure scheduler overhead, because there are differences in cloning, FlowUnit/output validation and capture semantics. Use the same macOS Release build and several invocations before deciding whether to optimize thread scheduling versus tensor copies versus ONNX invocation. No claim is made that the graph path supports steady-state streaming throughput.

Recorded 9-round Apple Silicon data: [2026-10-11-macos-arm64-nine-rounds.json](baselines/2026-10-11-macos-arm64-nine-rounds.json). Hardware subtype was not reported.

## Runtime owned-frame dispatch experiment

An experimental Runtime dispatch optimization moves the output `Value` into a single downstream channel or terminal capture instead of cloning it. For fan-out, it clones one value for each additional reader and moves the final value to the last reader. Backpressure and cancellation retry semantics are unchanged; a new 256-frame tensor fan-out test exercises capacity-one queues and validates both terminal sequences.

This avoids redundant `Vec<f32>` allocations in the Graph transport path but **does not remove** the `TensorF32` cloning done inside the built-in `tensor.output` FlowUnit, nor the ONNX Runtime adapter's output allocation. It is not zero-copy or an alternative CPU execution provider.

The before-change Apple Silicon three-mode baseline is captured in [baselines/2026-10-11-macos-arm64-three-path.json](baselines/2026-10-11-macos-arm64-three-path.json). To establish a credible effect, run `bench-compare` before and after on the *same machine* (repeated invocations, Release mode, consistent power state and identical input), preferably with interleaved A/B commits. No performance improvement is claimed before this is measured.

## Owned-input FlowUnit forwarding experiment

The `Runtime` can now pass owned inputs to a `FlowUnit::process_owned` hook. Existing custom implementations remain source-compatible because its default delegates to the original borrowed `process` method; the built-in `tensor.output` and `core.output` forward the owned values rather than cloning their buffers. A regression test verifies `tensor.output` preserves the exact original `Vec<f32>` allocation for a single-reader graph, and invalid input types still fail.

This is one step toward lower-copy CPU dataflows. It does **not** change ONNX tensor output allocation, the ONNX session mutex, graph thread lifecycle, or multi-reader fan-out clones. Run the three-path benchmark on the same Mac before claiming performance improvements.

The preceding optimization's local Apple Silicon 9-round measurements are archived in [baselines/2026-10-11-macos-arm64-owned-dispatch-observed.json](baselines/2026-10-11-macos-arm64-owned-dispatch-observed.json). They are **one invocation**, not a statistically controlled A/B estimate.

## Trained MNIST CNN CPU workload

A genuinely trained MNIST convolutional network is checked in as `examples/mnist.onnx` (26,454 bytes, upstream Microsoft ONNX Runtime inference-examples, MIT license). The fixture `examples/mnist-1-input.json` is a deterministic, **hand-generated digit-like grayscale image**, not a labeled MNIST evaluation sample.

The graph consumes `[1,1,28,28]` f32 and emits `[1,10]` raw float logits. The `onnx_inference` integration tests compare Direct ONNX CPU and Graph ONNX CPU numeric results with a tolerance and check shape rejection. **This is a functional integration smoke test**, not an independent classification-accuracy assessment.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  infer examples/mnist.onnx examples/mnist-1-input.json
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare examples/mnist.onnx examples/mnist-1-input.json 32 5 5
```

Start with a small finite batch (32) for this much larger workload. Larger batches may take significantly longer than synthetic Relu. The benchmark does not include image decode, model load, input preparation or stream ingestion; collect several Release runs on the same machine. See `third_party/mnist/LICENSE` and `third_party/mnist/README.md` for third-party provenance.

## Real CNN: PNG file-to-model validation

The 28×28, 8-bit grayscale PNG `examples/mnist-1.png` is numerically identical after `/255` normalization to `examples/mnist-1-input.json`. Linux and native macOS ARM64 CI now exercise the file-to-Graph command, test pixel-level equivalence, and reject wrong dimensions and RGB images.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  infer-mnist-png examples/mnist.onnx examples/mnist-1.png
```

The Mac mini's five-round 32-frame MNIST observations are archived as `benchmarks/baselines/2026-10-11-macos-arm64-mnist-cnn.json`; the ten observed logits (class index 1) are also archived in `examples/mnist-1-observed-logits.json` with tolerance-based regression checks. These values were generated by ModelBox Rust itself and **are not independent ONNX Runtime reference outputs**. Neither the PNG smoke test nor batch throughput establishes classification accuracy or production camera stream throughput.

## Labeled MNIST test-set subset (10 rows)

The fixed `benchmarks/fixtures/mnist-test-10.csv` contains ten **externally labeled** MNIST test records, each with 1 digit label followed by 784 integer pixels. Original upstream: [Make Your Own Neural Network / mnist_test_10.csv](https://github.com/makeyourownneuralnetwork/makeyourownneuralnetwork/blob/master/mnist_dataset/mnist_test_10.csv), Git blob `2eb1befad4869b1b824d9158df4e0f273fd86097`. Reuse terms and provenance are in `third_party/mnist-test-10/`.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  eval-mnist-csv examples/mnist.onnx benchmarks/fixtures/mnist-test-10.csv
```

The command returns sample count, exact per-image labels, confusion matrix, mistakes and observed accuracy. It validates every row, all 784 uint8 pixels, and every `float32[1,10]` prediction. It supports optional `min_correct` for a count-based CI threshold, and processes at most 10,000 examples in 64-image chunks.

**Interpretation:** the labels come from an external MNIST test sample set rather than our synthetic image. However, n=10 is only a smoke/diagnostic fixture and is not enough to claim a meaningful population accuracy estimate. Independent numerical reference comparisons, full 10,000-example evaluation, and object-detection acceptance are still pending.
