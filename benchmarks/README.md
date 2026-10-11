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

**Observed CI functional result:** both Linux and macOS ARM64 classify all 10 fixture rows correctly (10/10) in a single documented check. The CI acceptance gate now enforces `min_correct=10` for this fixed regression fixture. The raw labels and predictions are archived at [baselines/2026-10-11-labeled-mnist-test-10-ci.json](baselines/2026-10-11-labeled-mnist-test-10-ci.json).

**Interpretation:** the labels come from an external MNIST test sample set rather than our synthetic image. However, n=10 is only a smoke/diagnostic fixture and is not enough to claim a meaningful population accuracy estimate. Independent numerical reference comparisons, full 10,000-example evaluation, and object-detection acceptance are still pending.

## Canonical full MNIST test split: 10,000 labeled images

The **official test split** is 10,000 images and labels, stored as IDX unsigned-byte arrays with big-endian headers. Source documentation: [original MNIST database](https://yann.lecun.org/exdb/mnist/), [CVDF Foundation's supported Google storage mirror](https://github.com/cvdfoundation/mnist). We download and checksum the test split but **do not vendor it**.

```bash
bash scripts/fetch-mnist-test.sh
cargo run --release -p modelbox-cli --features onnx -- \
  eval-mnist-idx examples/mnist.onnx \
  benchmarks/datasets/mnist/t10k-images-idx3-ubyte \
  benchmarks/datasets/mnist/t10k-labels-idx1-ubyte
```

These are the compressed archive SHA256 values that the download script verifies before extraction:

| File | SHA256 |
|---|---|
| `t10k-images-idx3-ubyte.gz` | `8d422c7b0a1c1c79245a5bcf07fe86e33eeafee792b84584aec276f5a2dbc4e6` |
| `t10k-labels-idx1-ubyte.gz` | `f7ae60f92e00ec6debd23a6088c31dbd2371eca3ffa0defaefb259924204aec6` |

Both hashes are published independently by the [Guild AI MNIST resource configuration](https://guildai.org/t/guild-file-cheatsheet/192). The decompressed images are 7,840,016 bytes (16-byte header + 10,000 × 784 bytes), and labels are 10,008 bytes (8-byte header + 10,000 labels); these lengths and magic fields are strictly validated by `eval-mnist-idx`.

Run summary includes exact `sample_count`, overall `accuracy`, `correct`, per-digit total/correct/recall, 10×10 confusion matrix, a full mistake count and the first 50 indexed errors. To keep CI logs bounded, `predicted_labels` is `null` for more than 100 samples. The benchmark has no timing instrumentation: it measures **correctness**, not model load time, FPS, or production throughput.

The existing 10-row CSV mode and its fixed 10/10 CI regression remain unchanged. **Verified full-set result:** Ubuntu Linux x86_64 and native macOS ARM64 ONNX CPU jobs each classified exactly **9,890 / 10,000 (98.90%)** images, with the same confusion matrix and 110 mistakes. The full report is in [baselines/2026-10-11-mnist-full-10000-ci.json](baselines/2026-10-11-mnist-full-10000-ci.json). After observing that cross-platform baseline, we set a pragmatic CI acceptance gate of **9,850 correct (98.50%)**, allowing a 40-image drift margin. This is not a guarantee of accuracy outside MNIST, nor a confidence interval.

## Experimental YOLOv8 / YOLO11 NMS-free output adapter

The CLI command `postprocess-yolo` accepts a raw `TensorF32` or Graph inference output whose shape matches `[1,4+classes,candidates]`. It assumes already activated, per-class confidence scores and `xywh` boxes **in the model input canvas**. It is *not* a universal YOLO decoder. Default thresholds: confidence 0.25, class-aware IoU NMS 0.45, max detections 300. Candidates are sorted by score with anchor-order tie breaks; the top 3,000 candidates enter NMS. One class (argmax) is chosen per anchor.

```bash
cargo run -p modelbox-cli -- \
  postprocess-yolo examples/yolo-raw-output.json 640 640 0.25 0.5 10
```

Three synthetic anchors produce two detections: the lower-scoring same-class overlapping candidate is suppressed, while a co-located candidate in a different class remains. The test covers clipping, thresholds, maximum detections, input validation and the Graph result wrapper. It **does not** call ONNX Runtime. A real detector release will need a pinned trained ONNX model, licensed test images, explicit channel ordering, image decoding/letterbox preprocessing, projection to original-image coordinates, an independent reference output and labeled detection accuracy metrics (e.g. mAP).

## Actual YOLOv8n CPU photograph inference

```bash
bash scripts/fetch-yolov8n-demo.sh
cargo run --release -p modelbox-cli --features onnx -- \
  detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 0.25 0.45
```

For provenance, the external model is `webml/yolov8n/onnx/yolov8n.onnx`, 12.8 MB, SHA256 `190ba5f1e61411a001683e349d6b2cdb0804c0dc67a5e34cd8ff6fd00ee54b4d`; the photo is `ultralytics/assets/im/bus.jpg`, Git blob SHA1 `40eaaf5c330d0c498fbe1dcacf9bb8bf566797fe`. Model weights have **AGPL-3.0 usage restrictions**. Neither external binary is committed.

This stage measures correctness of the file→image→CHW→Graph ONNX CPU→raw tensor→NMS→image-box pipeline on a specific sample photograph. It is **not** a model FPS/mAP claim, and triangle-resize interpolation is not numerically identical to Ultralytics' OpenCV path.

## YOLOv8n observed Mac mini box regression

The original **release-mode Mac mini** detection output is archived at [baselines/2026-10-11-macos-arm64-yolov8n-bus-observed.json](baselines/2026-10-11-macos-arm64-yolov8n-bus-observed.json). It recorded five objects (four `person`, one `bus`), including bounding boxes in the **810×1080 original-image** coordinate space and model output shape `[1,84,8400]`.

Linux and native macOS ARM64 CI now run [../scripts/verify-yolov8n-demo.py](../scripts/verify-yolov8n-demo.py) against this Mac observation; detections are class-and-location matched, then required to be within ±8 px per box corner and ±0.03 confidence, all boxes in bounds. Anchors and array ordering are not assumed equal across runtimes. This is a first-party **regression** golden, not independent reference inference, annotated ground-truth labels or mAP. The image/library license obligations remain unchanged.

## Full YOLOv8n image-to-box stage timings

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 30 5
```

This reuses **one loaded ONNX session and compiled Graph**. Each timed iteration intentionally reads `bus.jpg` afresh, decodes it into RGB, resizes/letterboxes into CHW f32 tensor, invokes the real three-node Graph, then decodes YOLO outputs, performs class-aware NMS and undoes the Letterbox projection. A single `Instant` timeline marks boundaries; the four stage durations must sum exactly to total duration for every iteration (recorded as `max_stage_total_reconciliation_delta_ns = 0`). Measured request wall times exclude model/session initialization, graph compilation, logging and JSON serialization; include model CPU computation, graph scheduling/worker lifetime, heap copies in the normal path and OS-cached file I/O.

Report `min/mean/p50/p95/max` for each stage and for total separately. The sum of per-stage means equals the mean total (subject to tiny floating rounding). Percentiles are **not additive** across stages. Sequential reciprocal-of-mean estimated images/sec is explicitly not a concurrent or camera-stream throughput metric; do not compare against earlier batch throughput workloads as if identical.

CI runs a short **debug-profile smoke test (2 iterations, 1 warmup)** only to verify the benchmark works on Linux and Apple Silicon. A Mac mini **Release** baseline must come from the user's real command output, with hardware, Rust version and power state noted, before any performance claims.

## Mac mini YOLOv8n Release baseline (2026-10-11)

The user executed the pinned **Release**, fully warmed `bench-detect-yolov8` with 30 measured iterations and five warmups on an Apple Silicon Mac mini; full reported statistics are archived at [baselines/2026-10-11-macos-arm64-yolov8n-release-stages.json](baselines/2026-10-11-macos-arm64-yolov8n-release-stages.json).

| Sequential stage | Mean | p50 | p95 | % of mean total |
|---|---:|---:|---:|---:|
| JPEG file read/decode + RGB | 4.4717 ms | 4.4655 ms | 4.5776 ms | 15.46% |
| Letterbox resize + CHW f32 | 4.1538 ms | 4.1471 ms | 4.2340 ms | 14.36% |
| Rust Graph + ONNX CPU | 19.8086 ms | 19.6575 ms | 20.9635 ms | 68.49% |
| NMS + source-box projection | 0.4868 ms | 0.4861 ms | 0.5090 ms | 1.68% |
| **File to detections (true total)** | **28.9208 ms** | **28.68 ms** | **30.3207 ms** | **100%** |

The stage sum equals the total for every run (max reconciliation delta 0 ns), and the last output contained 5 detections with numeric fingerprint `9498.732394576073`. The reciprocal of total mean is **34.58 images/sec** under sequential, OS-cached requests; it is *not* live camera throughput. The specific Apple Silicon CPU model, thread count and thermal state were not provided and must not be fabricated.

### Direct ONNX vs Graph, same session: paired control

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 21 5
```

This loads the ONNX model **only once** and creates one Graph plan. Each measured round alternates the direct FlowUnit and Graph invocation ordering on the **same** preprocessed, read-only input. Excludes input tensor construction, image decode/Letterbox, NMS, model loading and graph plan compilation from the two timed intervals. Numerically checks all `[1,84,8400]` output elements (allowed max absolute difference `1e-4`), then outside timing confirms the expected five objects. Reports descriptive direct-vs-Graph duration distributions and individual paired gaps.

`Graph minus direct` is **not strictly scheduler overhead**; these paths differ in worker startup, channel traffic, memory ownership and output handling. No numerical performance assertions run in cloud CI, only functionality smoke tests. A hardware-specific performance claim requires the user's Mac mini Release result.

## Mac mini Release: same-session YOLOv8n Graph vs Direct (2026-10-11)

Archived first-party observation: [baselines/2026-10-11-macos-arm64-yolov8n-graph-vs-direct-release.json](baselines/2026-10-11-macos-arm64-yolov8n-graph-vs-direct-release.json). On the same 640×640×3 preprocessed `bus.jpg` tensor and **one shared ONNX Runtime session**, 21 pairs alternated Graph and direct calls after five warmups:

| Same-session path | Mean | P50 | P95 |
|---|---:|---:|---:|
| Direct `FlowUnit::process` | 18.7114 ms | 18.6619 ms | 19.3440 ms |
| Three-node Graph | 20.2150 ms | 20.2655 ms | 20.9689 ms |
| **Observed mean difference** | **+1.5037 ms** | — | — |

All 21 paired differences were positive (range **+0.4543 to +2.5231 ms**, median +1.4786 ms). The complete model output `[1,84,8400]` matched elementwise exactly; the final five detections retained numeric fingerprint `9498.732394576073`. Direct and Graph use the *same* ORT CPU session, so this comparison is primarily about invoking the FlowUnit through the Graph path, but does **not** uniquely identify thread creation, queue time or buffer overhead.

### Follow-up: owned identity transport on the same tensor shape

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-transport-yolov8 benchmarks/datasets/detection/bus.jpg 21 5
```

This benchmark preprocesses the same photograph once to a **4,915,200-byte** `[1,3,640,640]` f32 input, compiles a three-node Graph (`tensor.input → bench.tensor_identity → tensor.output`), and compares against a direct **owned** identity `FlowUnit`. Neither call performs ONNX inference; both get independently prepared input buffers, cloned **before** the timer starts. It measures worker/channel/capture overhead on the same input shape and verifies exact output equality. Sequential paired runs alternate order and report min/mean/P50/P95/max.

Do **not** subtract this identity benchmark's mean from the 1.50 ms real-model gap: lack of ONNX work, different cache/locking and scheduling effects prevent straightforward decomposition. CI exercises functionality on Linux/Apple Silicon with short, **debug** smoke checks; performance claims require actual Mac mini Release measurements.

### Ownership invariant for model-free transport

The `bench-transport-yolov8` control also asserts **pointer identity** for the 4,915,200-byte tensor after both the three-node Graph pipeline and direct owned FlowUnit. Both input allocations are constructed **before** the timed windows, then the single-consumer route must preserve each buffer's backing allocation unchanged. The JSON fields `graph_tensor_buffer_reused` and `direct_tensor_buffer_reused` only report `true` after explicit checks pass; the CLI exits nonzero if a hidden copy is introduced. An additional Rust integration test checks the Graph output pointer directly. This invariant applies to the identity-flow path, **not** ONNX Runtime's private tensor marshaling or memory allocations.
