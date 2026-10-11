# Roadmap and acceptance gates

## v0.1 — Rust Core (this pull request)

- [x] Rust workspace and CPU-only Core API
- [x] Typed node/port schema and JSON GraphSpec
- [x] Graph compilation: DAG, missing input, conflicting writer, port types
- [x] Registered FlowUnit trait and built-in text nodes
- [x] Bounded queues and parallel per-node worker execution
- [x] Sequence-preserving fan-out/fan-in for finite batches
- [x] Cooperative cancellation and error propagation
- [x] CLI, two examples, regression tests, GitHub Actions workflow
- [ ] Confirm GitHub Actions passes all OS targets after PR creation
- [ ] Publish first tag after review

## v0.2 — Inference foundations

- [x] Validated owned CPU f32 Tensor and typed graph ports.
- [x] Experimental opt-in ONNX Runtime CPU unit for one f32 input/output.
- [x] Synthetic ONNX Relu fixture, inference regression, and CLI command.
- [ ] Multiple input/output ports, dynamic shapes, and additional dtypes.
- [x] Reproducible per-graph-call ONNX CPU latency command (warmup, p50/p95, checksum).
- [x] Dedicated macOS Apple Silicon ONNX regression job.
- [x] Explicit finite-batch measured throughput benchmark and captured macOS arm64 baseline.
- [x] Tensor identity graph overhead control (same graph topology, no ONNX model).
- [x] Repeated alternating paired ONNX/Identity batch timings to measure run-to-run variability.
- [x] Three-path control: direct ONNX FlowUnit, ONNX Graph and Identity Graph.
- [x] Runtime dispatch moves owned values into single-consumer channels and terminal captures; only additional fan-out readers cause clones.
- [x] Optional FlowUnit `process_owned` path with backward-compatible borrowed default; built-in tensor.output and core.output forward their input buffers without cloning.
- [x] Trained MNIST CNN f32 classification model and graph-vs-direct shape/numeric integration smoke tests (no accuracy claim).
- [x] Strict grayscale PNG file-to-MNIST CPU classification CLI and user-observed logit regression.
- [x] Small independent-label MNIST test subset (10 rows) and inference/evaluation CLI with per-example predictions/confusion matrix (Linux and macOS ARM64 each observed 10/10; gated as fixed regression).
- [x] Complete official 10,000-image MNIST IDX test accuracy baseline on Linux x86_64 and native macOS ARM64: 9,890/10,000 = 98.90%, identical confusion matrices; CI gate >=9,850/10,000.
- [ ] Independent operator-level inference oracle, object-detection fixtures and representative performance baselines.


- ONNX Runtime CPU backend and tensor value design.
- Meaningful microbenchmarks: latency, throughput, memory, repeated run stability.
- Stream-oriented API with bounded ingress/egress and explicit shutdown.
- Thread pool configuration; tracing and telemetry.

Exit gate: run image classification and object detection reference fixtures; compare results against an independent ONNX inference reference.

## v0.3 — Edge AI

- Rockchip RK3588/RKNN as first NPU target, strictly versioned vendor integration.
- GStreamer/FFmpeg video pipeline adapter; RTSP disconnect/reconnect tests.
- Device buffer ownership, allocation failure recovery and teardown.
- Long-running soak tests plus watchdog/process supervision.

Exit gate: real RK3588 board tests and reproducible evidence of clean restart/stop under injected failures.

## v0.4 — Flow Studio

- Browser graph editor and typed node palette.
- Live graph validation, execution traces, logs and runs.
- Read-only preliminary import from ModelBox Graphviz-DOT-in-TOML.
- Document compatibility matrix; unsupported legacy attributes produce actionable diagnostics.

Exit gate: edit, validate, run, export and re-import one complete video AI workflow.

## v0.5 — Ecosystem

- Python SDK and stable versioned C ABI; TypeScript control SDK.
- Plugin packaging, signed releases, docs site and publish automation.
- Additional CUDA and Ascend backend feasibility validation.

These are targets, not implemented capabilities or committed release dates.
