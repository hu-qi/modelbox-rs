# Legacy ModelBox Graphviz / TOML migration (first compatibility subset)

ModelBox-RS does **not** implement the legacy C++/Python FlowUnit plugin ABI, device drivers, scheduler, or the complete Graphviz language. This migration step imports a **strict, CPU-text-only, acyclic subset** of the old graph configuration into the native Rust `GraphSpec`.

## What works now

Use ModelBox Studio's **导入 JSON** file picker to select a `.dot`, `.toml`, or `.conf` graph. The local-only bridge:

1. Parses legacy ModelBox `[graph]` TOML with `format = "graphviz"` and `graphconf = '''digraph ...'''`, or a standalone `digraph` DOT file. Parsing requires Python **3.11+** (`tomllib`).
2. Converts `node[type=input]` into `core.input` and `node[type=output]` into `core.output`, with default Rust `out`/`in` ports.
3. Only maps legacy FlowUnit declarations whose `flowunit` identifier exactly matches currently supported Rust built-in CPU text units: `text.uppercase`, `text.prefix`, and `text.concat`. An explicit `prefix` string can be preserved. Implicit ports are accepted **only** if unambiguous.
4. Runs **`modelbox validate <temporary-graph.json>`** using the actual Rust CLI before sending GraphSpec back to Studio. Invalid ports, incomplete wiring, type mismatches, cycles and multiple sources are rejected by the Rust compiler.
5. On success, replaces the Studio graph and exposes native Rust execution and diagnostics. The original legacy file is not modified.

Fixtures:

- [TOML legacy example](../examples/legacy_modelbox_hello.toml)
- [DOT legacy example](../examples/legacy_modelbox_hello.dot)

Sample ModelBox legacy-style graph:

```toml
[driver]
dir = ""

[graph]
format = "graphviz"
graphconf = '''digraph hello_legacy {
  input1[type=input]
  upper[type=flowunit, flowunit=text.uppercase, device=cpu]
  prefix[type=flowunit, flowunit=text.prefix, device=cpu, prefix="ModelBox Rust says: "]
  output1[type=output]

  input1 -> upper:in
  upper:out -> prefix:in
  prefix:out -> output1
}'''
```

## Fail-closed / unsupported

**No automatic conversion** is performed for unknown legacy FlowUnits, GPU/CUDA/Ascend devices, nonzero `deviceid`, custom driver directories, model loading, external libraries, network/HTTP/video IO, subgraphs, loops/conditional edges, Graphviz implicit node declarations, general Graphviz features, unsupported node attributes or arbitrary options. An unsupported attribute produces a specific error instead of silently changing the graph's semantics.

Legacy ModelBox supports more graph forms and types than ModelBox-RS currently does. Even when a graph converts, this does not demonstrate binary or functional equivalence for the general legacy ModelBox engine: it proves only that a limited set of compatible text-only operators and topology can be validated and executed on Rust.

The import endpoint remains bound to `127.0.0.1` and accepts at most **128 KiB** of legacy text. No arbitrary file paths, commands or plugin directories are accepted from the browser. The source is never executed; only the hardcoded `cargo validate` command is invoked on a temporary JSON graph.

## Verification

```bash
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
cargo test --workspace --all-targets
cd studio && npm test && npm run test:e2e
```

The browser E2E test imports the real TOML fixture, verifies the Rust validator accepted it, and executes the converted graph with the actual Rust Runtime. A separate test ensures unsupported legacy plugins do not mutate the existing canvas.

## Subsequent compatibility work

- Add an explicit, reviewed name/port mapping registry for real legacy CPU FlowUnits; do not infer equivalence by matching names.
- Read-only analysis and migration reports for unsupported production graphs.
- Support alternative endpoints and mapped ports only where the actual Rust FlowUnit contract is known.
- Investigate scheduling/buffer/device/driver semantic differences separately from parsing.

## Read-only preflight and multiple blockers

The local-only bridge additionally exposes `POST /api/analyze-legacy` with
`{"filename":"legacy.toml","legacy_text":"..."}`. It **never executes** a
legacy FlowUnit or calls Rust; it returns a read-only compatibility report with
counts, detected node IDs, FlowUnit identifiers, device types and an aggregated
list of blockers. The `candidate_convertible` flag means only that the
supported text subset *may* be convertible. It does **not** mean that Rust
validation, functional equivalence or performance equivalence has been proven.

When Studio import fails on an unsupported legacy feature, it displays these
blockers and **does not replace the existing Graph**. Multiple unsupported
plugins, devices, custom driver settings and option names can be reported
together. Preflight omits arbitrary attribute values and configured driver
paths so credentials or URLs are not echoed in the UI. Unsupported Graphviz
constructs still receive a generic, explicit unsupported-syntax diagnostic;
the analyzer is deliberately not a complete Graphviz parser.
