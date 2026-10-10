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
