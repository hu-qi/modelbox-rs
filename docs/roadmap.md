# Roadmap and acceptance gates

**Current actionable backlog and handoff:** [TODO.md](TODO.md) · [NEXT-SESSION-CONTEXT.md](NEXT-SESSION-CONTEXT.md). This high-level roadmap is not the active sprint checklist. The handoff snapshot records main at PR #24 (2026-10-11); verify later commits before continuing.

## v0.1 — Rust Core (prototype implemented)

- [x] Rust workspace and CPU-only Core API
- [x] Typed node/port schema and JSON GraphSpec
- [x] Graph compilation: DAG, missing input, conflicting writer, port types
- [x] Registered FlowUnit trait and built-in text nodes
- [x] Bounded queues and parallel per-node worker execution
- [x] Sequence-preserving fan-out/fan-in for finite batches
- [x] Cooperative cancellation and error propagation
- [x] CLI, two examples, regression tests, GitHub Actions workflow
- [x] Confirm GitHub Actions passes all OS targets (main 5/5 verified 2026-10-11, run 38107754576)
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
- [x] Experimental raw YOLOv8/YOLO11 one-output detection postprocessing: coordinate conversion, confidence filtering, class-aware NMS and synthetic regression fixture (no model inference claim).
- [x] Real pretrained YOLOv8n ONNX + JPEG/PNG photo ingestion, CHW letterbox preprocessing, NMS and source-coordinate projection (checksum-verified external model/image, smoke test; no mAP claim).
- [x] Real-model YOLOv8n `bus.jpg` observed Mac release detection-box and score regression archived with numeric tolerance gates on Linux and Apple Silicon (first-party golden, not an independent accuracy reference).
- [x] Experimental YOLOv8n single-file-to-detections stage benchmark (decode, preprocessing, Graph/ONNX CPU, NMS/project, total), with same-session reuse, stage reconciliation and cross-OS smoke.
- [x] Real Mac mini Release stage baseline: 28.9208 ms mean file→YOLOv8n detections (30 timed frames, 5 warmups), with stage shares and strict timing reconciliation archived.
- [x] Mac mini Release paired direct ONNX FlowUnit vs Graph: 21 shared-session pairs, 18.7114 vs 20.2150 ms mean, +1.5037 ms (8.04%) Graph gap; identical 705,600-value model output.
- [x] Separate same-sized owned-identity Graph transport benchmark to quantify model-free worker/channel/capture control (not subtraction-ready scheduler attribution).
- [ ] Measure the identity transport control in Mac Release; then decide whether persistent worker reuse or a specialized single-image path is warranted.
- [ ] Independent operator-level inference oracle and labeled detection dataset mAP baseline.


- ONNX Runtime CPU backend and tensor value design.
- Meaningful microbenchmarks: latency, throughput, memory, repeated run stability.
- Stream-oriented API with bounded ingress/egress and explicit shutdown.
- Thread pool configuration; tracing and telemetry.

Exit gate: run image classification and object detection reference fixtures; compare results against an independent ONNX inference reference.

## v0.2 parallel track — ModelBox Studio MVP (not yet implemented)

- [ ] Real node/port editor bound to native GraphSpec JSON, with palette backed by registered FlowUnit descriptions.
- [ ] Typed edge validation, graph compile errors, save/reopen without losing executable semantics.
- [ ] Invoke existing runtime validation and execution (CLI bridge first); display actual outputs/errors, never simulated execution.
- [ ] Document Studio canvas-only metadata separately from executable GraphSpec so layout does not break CLI consumers.
- [ ] Validate existing examples/hello.json end-to-end, including a negative incompatible-port fixture.
- [ ] Investigate original ModelBox WebUI, DOT-in-TOML graph import, and migration coverage; unsupported constructs produce explicit reports.

MVP exit gate: edit → validate → save → reload → execute the original hello graph through Rust Runtime, with preserved node config and exact typed connections.

## v0.3 — Edge AI

- Rockchip RK3588/RKNN as first NPU target, strictly versioned vendor integration.
- GStreamer/FFmpeg video pipeline adapter; RTSP disconnect/reconnect tests.
- Device buffer ownership, allocation failure recovery and teardown.
- Long-running soak tests plus watchdog/process supervision.

Exit gate: real RK3588 board tests and reproducible evidence of clean restart/stop under injected failures.

## v0.4 — ModelBox Studio advanced workflow

- Live stream previews, logs, traces, node profiling and diagnostics tied to actual Runtime evidence.
- Subgraphs, reusable templates, controlled plugins and deployment configuration.
- AI-assisted authoring with explicit validation and human-controlled execution.
- Expand read-only migration support from legacy DOT-in-TOML, with a compatibility report.

Exit gate: edit, validate, run, export and re-import one complete video AI workflow.

## v0.5 — Ecosystem

- Python SDK and stable versioned C ABI; TypeScript control SDK.
- Plugin packaging, signed releases, docs site and publish automation.
- Additional CUDA and Ascend backend feasibility validation.

These are targets, not implemented capabilities or committed release dates.
