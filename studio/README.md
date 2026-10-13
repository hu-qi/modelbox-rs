# ModelBox Studio MVP (experimental)

This is an early **runnable** visual editor for the native ModelBox-RS `GraphSpec`, not a finished legacy ModelBox WebUI replacement.

## Prerequisites

- Rust toolchain and project workspace
- Node.js 22 or later
- Python 3.10 or later

From the repository root, use two local terminals:

```bash
python3 scripts/studio_bridge.py
```

```bash
cd studio
npm install
npm run dev
```

Open **http://127.0.0.1:5173**. The bridge listens only on 127.0.0.1:8765. Do not expose either port or proxy the bridge publicly.

## Demonstration

The editor starts with the real executable `examples/hello.json` graph topology and uses `GET /api/catalog` for the actual built-in FlowUnit port definitions. Use **校验 Graph** to invoke `modelbox validate` through the local bridge and **执行 Graph** to invoke `modelbox run`. The result displayed is real CLI JSON. Invalid graphs return an error.

- Add FlowUnits from the registry-backed palette; connect only matching typed ports.
- Click nodes to inspect/edit JSON config; delete nodes or select/delete edges.
- Export **GraphSpec JSON** for `modelbox validate/run`.
- Save a separate **Studio document** holding graph plus node positions; import either JSON format.
- Browser-side validation is a usability hint; Rust Runtime compile remains authoritative.
- Node configuration schema discovery is incomplete; incorrect configuration may be rejected by Rust at validation time.

**Current limitations:** text-based `modelbox run` input only, no ONNX graph model uploading or video source, no legacy ModelBox DOT/TOML migration, no realtime tracing, no remote execution, no graph runtime daemon, no published/hosted Studio. Initial editor needs end-to-end browser QA beyond CI build.

## Security

The bridge launches only fixed cargo CLI commands, no shell string evaluation. Untrusted graphs can still select registered FlowUnits, consume local CPU/memory or access data allowed by future plugins. Use on a trusted developer machine only. Requests are capped and operations are time-limited. This is not a hardened production sandbox.

## Acceptance

CI builds the frontend and unit-tests bridge fixed-argument behavior. Manual acceptance: import `examples/hello.json`, validate, run, edit prefix, save, reload, and compare Rust output. This tracks S0-03 initial integration; complete MVP acceptance must be checked on a real browser as well.

## Run diagnostics (initial)

The run panel displays actual CLI output, errors, and a measured `bridge_cli_wall_time` in milliseconds. This interval includes Rust CLI process startup and may include a cold Cargo compilation; **it is not a FlowUnit timing, pure Graph scheduler overhead, or model inference latency**. Runtime traces and per-node timing remain future work.

## Real per-frame timeline (experimental)

Click **查看逐帧轨迹** to invoke `modelbox run-trace` through the existing
localhost bridge. Studio shows the retained `process_owned` events in their
actual monotonic timestamp order, with node ID, input sequence, relative start,
duration, and completion status. Select a timeline event to select its node
in the React Flow canvas. Changing the executable GraphSpec hides stale events.

The runtime bounds the trace at 4,096 events and counts dropped events.
The panel displays at most the first 100 retained events to avoid thousands of
DOM nodes; the retained/dropped/displayed counts are shown separately. The
timeline is not a queue/transport trace, an inference-only latency graph or a
complete scheduling profile. Events contain no payloads. Failed runs currently
return errors without a partial timeline.

## Channel API timing (experimental)

Click **查看通道API计时** to invoke `modelbox run-transport`.
Studio displays actual bounded-channel send and receive API invocation counts,
aggregate measured call wall times, and 20 ms send timeout retries, grouped by
FlowUnit worker. Editing GraphSpec invalidates the displayed diagnostic panel.

**Important:** receive wall time includes upstream computation, worker
scheduling and end-of-stream probes; send wall time includes bounded-channel
backpressure but also API and scheduler overhead. They do **not** isolate
per-edge data residency, congestion, pure transport latency or model inference
time. Measurements add overhead and should not be compared directly against
Release-mode uninstrumented benchmarks.

## Per-Edge delivery inspector

Click **分析逐边交付** to invoke the real `modelbox run-edges` command.
The React Flow graph labels each known Edge with its measured delivered-frame
count. The inspector lists each validated `from` → `to` port, per-link
send/receive counts, measured channel call wall-time, timeout retries, and
monotonic frame delivery intervals. Click a link to select its downstream
FlowUnit. Changing the GraphSpec hides any stale measurements and labels.

To bound rendering, the inspector shows up to 25 Edge summaries and the first
30 of the maximum 4,096 retained frame-delivery events; full retained/dropped
counts remain visible. Runtime does not retain frame payloads in the trace.

**Measurement accuracy:** delivery intervals start *before* a bounded send
attempt, not at the exact instant of enqueue, so they include send blocking
(backpressure) and scheduling overhead. These are *not* precise per-Edge queue
residence values or network latency, and the UI must not label them as such.
The collector is opt-in and unsuitable for uninstrumented Release baselines.

## ModelBox legacy DOT / TOML import (first subset)

The **导入 JSON** picker also accepts `.dot`, `.toml`, and `.conf` files using the old ModelBox `[graph]` Graphviz syntax. This first step is intentionally **CPU text-only**, requires Python 3.11+, and accepts only the exact built-in Rust text FlowUnits. The local bridge calls **the real Rust `modelbox validate`** on the converted GraphSpec before updating Studio. Unsupported plugins/devices/options yield errors without changing the current canvas. See [legacy migration guide](../docs/legacy-migration.md).

## Native CPU Buffer execution in Studio (typed Value mode)

Import `examples/cpu_buffer_fanout.json`, choose **Typed Value JSON（CPU Buffer）**
under **运行输入模式**, and paste the tagged JSON from
`examples/cpu_buffer_value.json`. Click **执行 Graph**.

The local bridge serializes the complete tagged Value to a temporary JSON
file, invokes native Rust `modelbox run-value`, then displays the actual
terminal Buffer bytes and typed metadata. Invalid bytes or metadata are
rejected by the Rust serde/CpuBuffer validator, not inferred in JavaScript.
The bridge accepts up to 192 KiB of serialized tagged Value JSON; the core
itself caps each CPU Buffer at 16 MiB.

Ordinary text input and its existing performance/trace/edge diagnosis buttons
remain available when **文本** is selected. Those diagnosis commands do not
yet support typed Buffer input and are disabled in typed mode; they must not
be presented as compatible measurements. This is a native CPU-only example,
not original ModelBox C++ BufferList/driver compatibility.
