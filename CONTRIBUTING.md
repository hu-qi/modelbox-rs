# Contributing

Thanks for helping revive the ModelBox dataflow ecosystem with a Rust-first design.

1. Open an issue for behavior changes, new device backends, or compatibility guarantees.
2. Work on a feature branch and submit a focused pull request with tests.
3. Run cargo fmt --all, cargo clippy --workspace --all-targets -- -D warnings, and cargo test --workspace.
4. For hardware-specific work, include the exact hardware revision, kernel version, runtime/driver version, build steps, and reproducible logs.
5. Never commit credentials, private model weights, proprietary SDKs, or third-party files without verified redistribution terms.

Public Rust APIs are experimental until a separately announced stability milestone.
Code contributions are made under Apache-2.0, with appropriate attribution for external material.
