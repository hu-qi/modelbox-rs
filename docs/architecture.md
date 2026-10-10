# Architecture decision record: v0.1 Rust Core

Status: accepted for prototype, 2026-10-11.

## Target

A small, testable CPU-first execution engine which preserves the design value of the old ModelBox dataflow system without recreating its historical C++ ABI. The repository must build without any vendor SDK.

## Pipeline

1. Deserialize native JSON into GraphSpec.
2. Validate node IDs and port schemas, resolve registered units, ensure every input has exactly one producer, reject mismatched types and directed cycles.
3. Construct an ExecutionPlan and bounded channel for every graph edge.
4. Spawn one worker per node. The source publishes numbered frames. Non-source workers read the frame with the next sequence from every input, invoke a registered FlowUnit, and publish validated outputs.
5. Capture every unconnected output port, sort frames by source sequence, return a serializable result.

## Invariants

- Graphs are static DAGs; one source per graph. No multi-writer input.
- Exactly one frame per input per source sequence, or execution fails.
- Port types must be equal at compile time. All produced values are checked at runtime.
- Every edge uses a finite-size channel. A slow consumer causes an upstream producer to block.
- Cancellation is signaled using a shared atomic; timed channel operations periodically check it.
- Source order is preserved at each terminal independently of scheduling.
- Rust trait objects are a **source-compatible Rust extension API**, not a stable shared-library ABI.
- No claim of cross-device zero-copy or original ModelBox plugin compatibility.

## Operational boundaries

This version materializes all input frames before execution and all captured output frames after execution. It is not a zero-copy streaming system. A worker per node is chosen for correctness and clarity, not scaling to thousands of nodes. Backpressure bounds edge queues, but not source batch storage or returned outputs.

There is no forced thread termination. A third-party FlowUnit which blocks indefinitely inside process cannot currently be cancelled. Device isolation, worker supervision, and process-level recovery are future milestones.

## Near-term design choices

- Keep Flow IR independent of hardware vendors; design versioned format before exposing an external compatibility promise.
- DeviceBackend capability negotiation must report whether zero-copy transport actually exists. Explicit copies are allowed.
- Use C ABI/FFI or IPC for legacy C++ interoperability, not Rust trait-object layout.
- Use ONNX Runtime to bootstrap inference rather than implementing operators anew.
- Share portable IR concepts with PyOH-Flow where useful; never assume its MCU runtime can use host-sized threads or buffers.

## Provenance

The original open-source project is at https://github.com/modelbox-ai/modelbox. This repository is independently developed and is not an official project of its upstream maintainers.
