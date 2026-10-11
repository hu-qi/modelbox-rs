# ModelBox Rust (modelbox-rs)

Rust-first, CPU-first foundation for a typed AI dataflow runtime. ModelBox Rust is an independent community continuation inspired by [ModelBox](https://github.com/modelbox-ai/modelbox), not an official release or a binary-compatible port.

**v0.1 status:** a prototype with a working JSON Graph IR, validation, bounded concurrent dataflow processing, custom Rust FlowUnits, a CLI, and automated tests. This is **not yet** a production edge AI inference engine.

## Quick start

Prerequisites: Rust 1.81+ (stable recommended).

~~~bash
cargo test --workspace
cargo run -p modelbox-cli -- demo
cargo run -p modelbox-cli -- validate examples/fanout.json
cargo run -p modelbox-cli -- run examples/hello.json "hello edge ai"
cargo run -p modelbox-cli -- run-batch examples/fanout.json one two three
~~~

The demo emits terminal outputs keyed by node ID and output port, including their values and the source input count.

## Architecture

~~~text
Workflow JSON -> GraphSpec -> DAG / type validator -> ExecutionPlan
                                                   |
CLI -> text inputs -> core.input -> bounded channels -> FlowUnits
                                                     |
                                                terminal outputs
~~~

- Typed ports and compile-time edge checking; duplicate IDs, invalid ports, multi-writer inputs, cycles, and missing required inputs are rejected.
- Dedicated OS-thread worker per node, bounded crossbeam channels, deterministic per-port source ordering, fan-out/fan-in synchronizing on frame sequence.
- Custom Rust FlowUnits via FlowRegistry; built-ins: core.input, core.output, text.uppercase, text.prefix, text.concat.
- Error propagation, cooperative cancellation, and output type checking.
- CI includes formatting, Clippy, unit/integration tests, and a CLI smoke test on Linux, macOS and Windows.

## Scope and limitations

The v0.1 execution model is deliberately narrow: exactly one externally driven source, finite batches, one frame per sequence per edge, static DAG, strict type equality, owned in-memory values, and one thread per node. It has no unbounded stream API, windowed join, multi-source semantics, tensor/device allocation, inference backend, UI, dynamic plugin ABI, or ModelBox TOML/DOT importer. This limitation is intentional, not an implicit compatibility claim.

A future version will add Tokio-based I/O, CPU task pools, hardware backends through safe FFI boundaries, telemetry, and graph import. We will measure throughput, latency, peak RSS, error recovery and hardware behavior against reproducible reference workloads before claiming performance parity.

## Repository layout

~~~text
crates/modelbox-core/     GraphSpec, registry, FlowUnits, compiler, runtime, tests
crates/modelbox-cli/      modelbox binary
examples/                 runnable native v0.1 graph JSON
docs/                     architecture, compatibility boundary, roadmap
.github/workflows/        cross-platform Rust CI
~~~

See [architecture](docs/architecture.md), [roadmap](docs/roadmap.md), and [contributing](CONTRIBUTING.md).

## License and attribution

Licensed under Apache-2.0. See LICENSE and NOTICE. No upstream C++ source is copied in this prototype. Original ModelBox trademarks and third-party SDKs remain with their respective owners.

## Experimental v0.2 ONNX CPU feature

Default tests still run without ONNX Runtime. Opt in for a real ONNX Runtime CPU inference using the checked-in synthetic Relu model:

~~~bash
cargo test -p modelbox-core --features onnx
cargo run -p modelbox-cli --features onnx -- infer examples/relu.onnx examples/relu-input.json
~~~

The optional feature uses pinned `ort = 2.0.0-rc.13` for ONNX Runtime 1.28 and requires Rust >=1.88. The initial build downloads platform-specific ONNX Runtime binaries; network access is required.

This first prototype supports exactly one input and one output, f32 tensors, CPU only, and a mutex-protected session. Model pre/postprocessing, dynamic streaming, multi-input and GPU/NPU remain out of scope. `examples/relu.onnx` is a minimal synthetic Relu model generated specifically for regression tests.

### Repeatable CPU inference latency baseline

With the optional `onnx` feature enabled:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench examples/relu.onnx examples/relu-input.json 100 10
```

The last arguments are measured iterations (default 100, max 10,000) and warmup iterations (default 10, max 1,000). The command emits machine-readable JSON with p50/p95/mean/min latency in milliseconds, inferred tensor shape, output checksum, and approximate items/sec. ONNX session loading, model parsing and Graph IR compilation are excluded from the timed section. Each measured iteration **includes** input cloning, spawning graph worker threads, bounded channel transmission, Tensor marshaling, model inference, and capturing the output. The value is therefore **end-to-end per-graph-call latency**, not pure ONNX kernel latency. Timings from the synthetic three-element Relu model must not be extrapolated to real models; do not compare Debug and Release timings.

CI now validates the opt-in ONNX model and CLI on the `macos-15` arm64 GitHub-hosted runner (in addition to Linux). This tests software portability, not GPU/NPU.

### Actual finite-batch throughput (v0.2 experimental)

Single-request `bench` reports an estimated rate from the reciprocal of mean latency. It is **not** sustained throughput.

The new `bench-batch` instead measures one complete `run_plan` processing a prepared finite batch of N items. It reports measured wall-clock items/sec:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-batch examples/relu.onnx examples/relu-input.json 1000 10
```

Batch preparation, graph compilation and ONNX session loading are excluded from the timed interval. Thread startup/shutdown, bounded queues, CPU ONNX inference, and terminal output capture **are included**. The command accepts up to 10,000 items, with a 64 MiB input-batch safety limit. It is not a continuous streaming benchmark and retains all outputs in memory, so its throughput should not be equated with a long-running RTSP pipeline.

The single-call baseline from an Apple Silicon Mac mini is recorded in [benchmarks/baselines/2026-10-11-macos-arm64-relu.json](benchmarks/baselines/2026-10-11-macos-arm64-relu.json). Hardware subtype, macOS version, and a true sustained throughput result remain unrecorded.

### Comparing ONNX execution against graph-only overhead

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-overhead examples/relu-input.json 1000 10
```

`bench-overhead` runs an identically shaped three-node tensor graph, replacing `onnx.cpu` with a no-op tensor identity operator. Its throughput provides a first control group for bounded-channel and worker/capture overhead. It is **not** a direct measurement of ONNX kernel time. Run `bench-batch` on the same machine with the same item count, in alternating orders, and repeat measurements before prioritizing runtime optimizations.

The user-reported 1,000-item ONNX Relu throughput result is stored in [benchmarks/baselines/2026-10-11-macos-arm64-relu-batch.json](benchmarks/baselines/2026-10-11-macos-arm64-relu-batch.json).

### Repeated paired ONNX-vs-Identity graph comparison

Run multiple alternating rounds from a single Release process to reduce run-to-run variability:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare examples/relu.onnx examples/relu-input.json 1000 9 10
```

The JSON includes raw paired observations, P50/P95/mean elapsed time, output checksums, and the percentage of the ONNX median represented by the Identity median. This is a descriptive control comparison, **not** an exact decomposition of ONNX execution costs.

### Three-way inference-cost control

The `bench-compare` command now includes direct ONNX FlowUnit execution alongside the existing Identity and ONNX graph paths, with a round-robin execution order. No new command syntax is necessary. This helps investigate the cost of sending ONNX inference through a Graph, but neither the Identity duration nor the difference between direct and Graph is a pure measure of scheduler overhead.

### Pre-trained MNIST CNN inference acceptance

The repository includes an externally trained ONNX MNIST classification model (licensed MIT; source: Microsoft's ONNX Runtime examples) under `examples/mnist.onnx` and a reproducible synthetic 28×28 grayscale digit input. This is a **real trained convolutional network**, unlike the tiny Relu fixture.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  infer examples/mnist.onnx examples/mnist-1-input.json

cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare examples/mnist.onnx examples/mnist-1-input.json 32 5 5
```

The model consumes a `[1,1,28,28]` f32 image tensor and emits a `[1,10]` f32 logit tensor (softmax and image preprocessing are outside this CLI demo). CI checks output shape, finite numeric values, direct-FlowUnit/Graph consistency within tolerance, and invalid image-shape rejection. The user-created digit-like fixture is **not** a labeled MNIST test-set sample; passing these checks demonstrates integration, **not classification accuracy**. Real image preprocessing, curated labeled accuracy evaluation, and object detection remain future work. License and source attribution: `third_party/mnist/LICENSE`.

### MNIST grayscale PNG → CNN → predicted digit

The opt-in `infer-mnist-png` CLI reads an actual PNG image, validates its dimensions and pixel format, normalizes each 8-bit grayscale pixel to `f32 / 255`, and passes the `[1,1,28,28]` tensor through the ONNX CPU Graph.

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  infer-mnist-png examples/mnist.onnx examples/mnist-1.png
```

Output JSON includes the top-scoring digit and all ten raw logits. Input must be exactly 28×28 grayscale PNG (8 bit). **No resizing, automatic RGB conversion, inversion or adaptive normalization is performed**. The PNG depicts the same synthetic hand-drawn-style pattern as the existing tensor fixture; this validates ingestion and prediction plumbing, not accuracy on labeled MNIST data. The user-observed Mac logits are recorded as a regression baseline, not an independent oracle.

### Evaluation against labeled MNIST test samples

A new experimental evaluation command reads **externally labeled MNIST test CSV records** (label + 784 grayscale pixel values). Unlike the hand-generated PNG demo, this makes an actual labeled comparison and reports per-image predictions, number correct, accuracy, confusion matrix, and mistakes:

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  eval-mnist-csv examples/mnist.onnx benchmarks/fixtures/mnist-test-10.csv
```

An optional third argument `[min_correct]` sets a minimum *count* of correct classifications. A failure exits nonzero after printing all results. The model is loaded and the graph compiled once; inputs are executed in finite chunks. CSV values are normalized by dividing each uint8 pixel by 255. The reference file is a **10-row excerpt claimed as MNIST test samples by its external maintainer**, not the model's self-generated logits. The sample size is too small to estimate general classification accuracy or justify performance parity with other frameworks.

Fixture attribution and reuse terms are documented in `third_party/mnist-test-10/README.md` and `LICENSE`. This phase does **not** establish independent floating-point operator correctness; the existing graph-vs-direct check still uses a shared backend.

### Full 10,000-image MNIST test-set validation (v0.2 experimental)

The standard MNIST test set consists of **10,000 labeled 28×28 grayscale images** in two binary IDX files. The image/label files are **not committed**. Download the canonical test split from the CVDF Foundation's Google-hosted MNIST mirror with verified SHA256 checksums:

```bash
bash scripts/fetch-mnist-test.sh

cargo run --release -p modelbox-cli --features onnx -- \
  eval-mnist-idx examples/mnist.onnx \
  benchmarks/datasets/mnist/t10k-images-idx3-ubyte \
  benchmarks/datasets/mnist/t10k-labels-idx1-ubyte
```

The `eval-mnist-idx` command validates big-endian IDX headers (2051 images / 2049 labels), exact file lengths, 28×28 image shape, matching counts and labels in 0–9. It normalizes each pixel by `/255`, evaluates at most 10,000 examples in bounded 64-frame Graph batches, and reports overall accuracy, all ten per-digit recalls, a 10×10 confusion matrix, total misclassifications and up to 50 indexed error examples. The optional final `[min_correct]` argument enforces an acceptance count. In the first full CI run, Linux x86_64 and native macOS ARM64 both correctly classified 9,890/10,000 images (**98.90%**) with identical 10×10 confusion matrices. CI now gates at 9,850 correct (**98.50%**) to detect meaningful regressions while allowing a 40-image tolerance for backend/platform changes. This threshold is an engineering acceptance gate, not a statistical confidence bound.

By design, the command omits the 10,000-element `predicted_labels` array from the JSON report to avoid enormous CI logs; smaller input sets retain it. Full dataset validation does **not** guarantee accuracy on real photographs or handwriting from other sources. Data sources, checksums and scope: [benchmarks/README.md](benchmarks/README.md).

### Experimental YOLO raw prediction postprocessing (no model inference)

`postprocess-yolo` is a **detector-output adapter only**. It decodes *single-output, NMS-free* YOLOv8/YOLO11-style `float32[1,4+num_classes,num_candidates]` channel-first tensors: center-x, center-y, width, height and already-activated class scores. It selects the highest-scoring class per candidate, applies a confidence cutoff, converts `xywh → xyxy`, clips boxes to **model-input pixels**, and runs deterministic class-aware non-maximum suppression. It intentionally rejects incompatible tensor shapes.

```bash
cargo run -p modelbox-cli -- \
  postprocess-yolo examples/yolo-raw-output.json 640 640 0.25 0.5 10
```

The input is either a raw `TensorF32` JSON file or a complete serialized Graph `infer` result containing `outputs.result.out[0].value`. No trained YOLO ONNX weights are bundled, no camera input is processed, and **no real object detection inference, letterbox correction, or mAP is claimed** by this fixture. The supplied fixture contains three intentionally synthetic candidates exercising box conversion, confidence gating and same-class NMS. YOLOv5 objectness outputs, YOLO end-to-end NMS exports, segmentation, pose and multi-output detectors require separate adapters. See `docs/object-detection.md` for the phase boundary and acceptance criteria.

### Real ONNX detection: JPEG/PNG → YOLOv8n CPU → labeled boxes

The optional `detect-yolov8` command runs a **real, pretrained YOLOv8n detection model** through the existing one-input/one-output `onnx.cpu` Graph. It decodes a JPEG or PNG image, letterboxes to 640×640 with 114 RGB padding, builds a normalized `[1,3,640,640]` f32 CHW tensor, checks for the expected `[1,84,8400]` COCO80 output, performs class-aware NMS and projects boxes back to **original photo coordinates**.

```bash
bash scripts/fetch-yolov8n-demo.sh
cargo run --release -p modelbox-cli --features onnx -- \
  detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 0.25 0.45
```

The model and photograph are **not bundled**: the download script validates the model's published SHA256 and the sample photograph's upstream Git blob hash. Both are kept under ignored `benchmarks/datasets/`. This model is from [Hugging Face webml/yolov8n](https://huggingface.co/webml/yolov8n) and based on Ultralytics weights, which have **AGPL-3.0 obligations**; commercial use requires an appropriate license. The Ultralytics bus image is also a third-party asset.

The demo emits COCO labels, confidence scores, and `xyxy` pixel coordinates for the actual input image. CI checks for *person* and *bus* detections on the real sample on Linux and Apple Silicon. Triangle image resizing is only an approximation of OpenCV's interpolation; this is **functional real-model acceptance, not independent box-coordinate equality, production latency, mAP, or unrestricted licensing**.

### Numeric regression for real YOLOv8n photo detections

The Mac mini's release-mode `detect-yolov8` output on the pinned `bus.jpg` photograph is now archived in `benchmarks/baselines/2026-10-11-macos-arm64-yolov8n-bus-observed.json`: five detections, including four persons and one bus, with raw COCO classes, scores, boxes and candidate IDs. It is an **observed first-party fixture**, not manually annotated ground truth or a second independent model implementation.

On Linux and Apple Silicon, CI compares actual detections with the observed baseline by class and spatial box matching, rather than relying on prediction ordering. It checks that all five objects survive postprocessing, source-image boxes are valid, and numeric drift remains within **8 original-image pixels per box coordinate** and **0.03 absolute class-confidence points**. The gates are tolerance-based engineering regression checks; a separately licensed labeled dataset and independent reference inference remain necessary for mAP and accuracy claims.

```bash
# After running the command in the YOLO demo, redirect its output to yolo-detections.json.
python3 scripts/verify-yolov8n-demo.py yolo-detections.json
```
