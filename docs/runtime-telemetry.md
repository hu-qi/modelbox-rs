# Runtime node process telemetry (experimental)

`modelbox run-profile examples/hello.json "hi"` executes the **same graph** through the Rust Runtime, returning `result` and `node_process`. Each non-source node has `calls` and aggregate `process_ns`. The result keeps the original output semantics.

Measurements encompass only **`FlowUnit::process_owned`** within the worker, sampled with `Instant` when opt-in profiling is enabled. They exclude source forwarding, channel waits, transport/dispatch, thread startup/join, and external CLI/cargo startup. Concurrent node times overlap and **must not be added** to claim graph wall-clock time. Profiling itself adds measurement and lock overhead, so it must not be compared directly with existing unprofiled Mac mini Release graph-vs-direct baselines.

This is a foundation for future Studio profiling; the visual UI is not wired to this API in this PR. No model-inference or pure-scheduler latency claim is made.


## Bounded per-frame process trace

`modelbox run-trace examples/hello.json "hi"` returns `result`, an ordered
`events` array, and `dropped_events`. Each event contains only:

- `node_id` and `sequence` (input sequence number)
- `process_started_ns` relative to this run's monotonic trace origin
- `process_duration_ns` (time within `FlowUnit::process_owned`)
- `ok` indicating whether the FlowUnit process returned successfully

Trace collection is **opt-in**. At most 4,096 events are retained per run;
additional events increment `dropped_events`. Event timestamps reflect actual
concurrent worker start times and are sorted by start time for visualization.
They do **not** imply serial execution or express queue waiting time, dispatch
latency, throughput, model-only inference timing or a complete graph timeline.
No input/output values are recorded, but node IDs and sequence numbers are
still metadata and should be handled appropriately. Because failed execution
currently returns a `FlowError`, partial traces are **not yet exposed on error**.

Profiling/tracing adds overhead. Compare uninstrumented performance using the
existing benchmark commands. Native Studio timeline rendering is a follow-up.
