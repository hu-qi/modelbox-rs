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


## Channel API timing (opt-in)

`modelbox run-transport examples/hello.json "hi"` returns the unchanged
`result` and a `node_transport` map with the following counters by worker
node ID:

| Field | Meaning |
| --- | --- |
| `receive_operations` | Calls to bounded-channel receive, including final end-of-stream probes |
| `receive_wall_ns` | Total time inside receive calls, including upstream processing delays, scheduling and 20 ms timeout polling |
| `send_operations` | Downstream bounded-channel send calls (fan-out counts each recipient separately) |
| `send_wall_ns` | Total time inside send calls, including API overhead, scheduler stalls and blocking backpressure |
| `send_timeouts` | Number of 20 ms bounded-send timeout retries |

These are **measured channel API wall times**, not isolated waiting,
per-edge queue residence or network latency. A receive can be delayed by
upstream `process_owned`, and successful send operations can return
immediately even if consumers have not processed the data. The source node has
no input receive operations; unconnected terminal outputs are captured rather
than sent through a channel.

Telemetry is disabled for ordinary `run` and `run-trace`. Instrumentation
adds overhead and should not be used as a replacement for uninstrumented
Release graph-vs-direct benchmarks. A future Studio integration may present
these counters, with their limitations, separately from process-time traces.


## Per-Edge delivery events (experimental)

`modelbox run-edges examples/hello.json "hello"` returns the standard
`result`, per-link `edges` statistics, up to **4,096** `deliveries` and a
`dropped_deliveries` counter. Each Edge is labeled with its exact validated
`from` and `to` ports, including separate fan-out links.

A delivery correlates the source/destination ports and input `sequence` with
a monotonic **send-attempt start** and **receive completion** timestamp, plus
their interval. This is measured in Rust at the actual bounded-channel
interfaces. The interval includes any bounded-send backpressure, worker
scheduling and channel API overhead; it is **NOT a precise queue-residence
time** or a network latency. It must not be subtracted from the FlowUnit
process trace to infer scheduler overhead.

Per-Edge counters report send/receive calls, the wall time spent inside those
calls, 20 ms send-timeout retries, and successfully delivered frames. Receive
call counts include final end-of-stream probes, so receive count can exceed
delivered frames. Events and counters contain no input/output payload content.
The regular `run` path does not record Edge timestamps. Results and timeline
events are available for successful runs only; partial failed-run traces are
not exposed yet. Collection adds overhead and is not a Release benchmark.

These events are an attribution foundation; future work can integrate Edge
visualizations in Studio and explore tighter queue-residence instrumentation
without mislabeling send-attempt intervals as exact enqueue timestamps.
