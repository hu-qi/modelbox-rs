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
