# Runtime node process telemetry (experimental)

`modelbox run-profile examples/hello.json "hi"` executes the **same graph** through the Rust Runtime, returning `result` and `node_process`. Each non-source node has `calls` and aggregate `process_ns`. The result keeps the original output semantics.

Measurements encompass only **`FlowUnit::process_owned`** within the worker, sampled with `Instant` when opt-in profiling is enabled. They exclude source forwarding, channel waits, transport/dispatch, thread startup/join, and external CLI/cargo startup. Concurrent node times overlap and **must not be added** to claim graph wall-clock time. Profiling itself adds measurement and lock overhead, so it must not be compared directly with existing unprofiled Mac mini Release graph-vs-direct baselines.

This is a foundation for future Studio profiling; the visual UI is not wired to this API in this PR. No model-inference or pure-scheduler latency claim is made.
