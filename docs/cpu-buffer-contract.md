# Native CPU Buffer contract (foundation, not legacy ABI compatibility)

Original ModelBox represents each Buffer as Data + Meta, groups buffers as BufferLists, and distinguishes shallow sharing from deep copies. Its FlowUnits also have Open/StreamOpen/Process/StreamClose/Close hooks. See [original ModelBox C++ Buffer guide](https://modelbox-ai.com/modelbox-book/use-modelbox/standard-mode/flowunit/c%2B%2B.html).

This milestone adds a limited, native Rust CPU Buffer. It DOES NOT load original C++ drivers/plugins or claim external plugin equivalence.

## Typed storage and validation

- CpuBuffer stores immutable CPU bytes (maximum 16 MiB) and typed metadata (maximum 64 fields).
- Metadata keys are 1–128 ASCII letters, digits, underscore or hyphen. Values are UTF-8 strings (maximum 4096 bytes), i64 integers, and booleans. Other types are rejected.
- CpuBuffer clone() uses Arc-backed shallow sharing. deep_copy() creates independent data and metadata. No unsafe pointers are exposed.
- Value::CpuBuffer has its own DataType::CpuBuffer. Rust Graph compilation enforces type-correct connections; the older Bytes and TensorF32 value types are unchanged.
- JSON round-trip preserves bytes 0–255 and metadata types, rejecting unsupported metadata instead of dropping it.

## Native Rust units

| FlowUnit | Typed input | Typed output | Semantics |
| --- | --- | --- | --- |
| buffer.input | none | out: cpu_buffer | Runtime-managed source |
| buffer.output | in: cpu_buffer | out: cpu_buffer | Owned passthrough |
| buffer.identity | in: cpu_buffer | out: cpu_buffer | Shallow buffer handoff |
| buffer.deep_copy | in: cpu_buffer | out: cpu_buffer | Detached data and metadata |

All reject configuration options. They are not automatically aliased to any original legacy ModelBox FlowUnit.

## Run the actual typed CPU Buffer graph

    cargo run -p modelbox-cli -- run-value examples/cpu_buffer_fanout.json examples/cpu_buffer_value.json

The typed input file contains byte values 0, 1, 127, 128 and 255, plus metadata fields width, height, format and ready. The demo graph fans out through identity and deep_copy to two terminal outputs. CLI caps serialized input JSON at 64 MiB. Rust tests verify bytes, typed metadata, shallow vs deep allocation identity, 256 ordered frames, fanout and type errors.

## Not yet implemented

BufferList batch semantics; DataContext/SessionContext; Open/StreamOpen/StreamClose/Close; per-stream EOF callbacks; writable/borrowed/foreign memory; GPU/NPU buffers; original C++ ABI and driver loading; legacy plugin functional equivalence; production release benchmarking. None is implied by these types or passing tests.

## Studio typed Value mode

The optional Studio runner now accepts tagged `Value` JSON and executes it
through the local-only Python bridge and the Rust `run-value` CLI. The
bridge uses temporary files and fixed command arguments; it never launches a
legacy ModelBox driver or evaluates JavaScript as binary code. Typed input is
limited to 192 KiB of serialized JSON in Studio (smaller than the 16 MiB core
buffer limit). The original text runner and profilers are unaffected; current
profilers are disabled in Studio when using typed input.

A real Chromium E2E test imports the example CPU Buffer graph, passes typed
binary data (including 255 and zero) and typed metadata, verifies both
terminal outputs, and confirms invalid byte values fail through the Rust
decoder rather than being silently converted.
