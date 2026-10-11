# ModelBox Studio v0.2 — executable orchestration contract

Status: **design/acceptance contract, not a shipped web editor**.

## Revival objective

The Studio is a visual way to author **real ModelBox-RS dataflow applications**, not an unrelated drawing tool. Research and migration from the original [modelbox-ai](https://github.com/modelbox-ai) runtime, WebUI, FlowUnit concepts and graph formats are first-class work. No C++ ABI or legacy config compatibility is claimed today.

## Single executable source of truth

The Rust `GraphSpec` from `crates/modelbox-core/src/graph.rs` is authoritative:

```json
{
  "name": "hello",
  "queue_capacity": 2,
  "nodes": [{"id": "source", "unit": "core.input", "config": null}],
  "edges": [{"from": "source.out", "to": "result.in"}]
}
```

The example above illustrates shape only; it is **not runnable** unless a `result` node is also defined. For a real executable round trip, use `examples/hello.json`.

- Node IDs consist of ASCII letters, digits, `-` and `_` and are unique.
- Edge endpoints are `node_id.port_name`; port names and datatypes come from the registered Rust `FlowUnit`, not UI guesses.
- A `GraphSpec` has precisely one source today and is a DAG; required input ports each have exactly one writer. These constraints must be checked by `Runtime::compile`.
- `queue_capacity` must be within 1..=65536; node and edge count limits remain enforced by Core.
- FlowUnit `config` JSON is executable data and must survive save/reload unchanged.
- The **canvas document** (positions, zoom, selection, groups, colors, notes) is a distinct artifact and never silently injected into executable GraphSpec. Do not rely on Rust serde ignoring arbitrary unknown keys.
- The editor must report compile errors; never delete mismatched edges, change types, or execute a modified graph without user awareness.

## MVP interaction

1. Load `examples/hello.json` and derive draggable nodes and typed ports.
2. Reposition and reconnect visually; edit `text.prefix` configuration.
3. Export executable JSON and reload it, preserving node IDs, units, configs, and edges.
4. Run `cargo run -p modelbox-cli -- validate <graph.json>` and `cargo run -p modelbox-cli -- run <graph.json> <TEXT>` via a local, explicitly authorized bridge.
5. Display the actual Rust output or error. Never claim execution when only visual simulation occurred.

## Security boundary

A browser cannot directly access local models, unrestricted paths, processes, cameras or NPU devices. A bridge will need explicit allowlists, file sandboxing, no arbitrary shell command construction, local-only binding and an execution consent model. For the first version, avoid a remote service or automatic execution on opening a graph.

## Version roadmap

- **S0-01**: contract + real example smoke gate (this PR).
- **S0-02**: read-only FlowUnit catalog exported by Core, with port type and config contracts.
- **S0-03**: browser editor with graph serialization and a local CLI adapter; test a full round trip.
- **S0-04**: legacy ModelBox WebUI/DOT-TOML discovery and read-only migration diagnostics.
- **Later**: streaming live preview, typed subgraphs, traces, plugin catalog, deployment and AI-assisted composition backed by actual runtime APIs.

## Acceptance

The first runnable proof remains `examples/hello.json` validated and executed by real Rust; an invalid edge must be rejected rather than auto-healed. Model performance measurements and deployment support are tracked separately.
