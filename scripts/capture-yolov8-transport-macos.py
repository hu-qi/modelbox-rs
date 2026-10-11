#!/usr/bin/env python3
"""Capture P0-01 Mac mini Release transport measurements without inventing data.

Run on the actual Mac mini from the repository root:
  python3 scripts/capture-yolov8-transport-macos.py --environment "idle, AC power"
The output is a real machine measurement, never a GitHub runner performance baseline.
"""
import argparse
import datetime
import json
import platform
import subprocess
import sys
from pathlib import Path

EXPECTED_BYTES = 4_915_200


def verify(report):
    required = {
        "benchmark": "yolov8n_input_identity_graph_transport_control",
        "profile": "release",
        "input_tensor_bytes": EXPECTED_BYTES,
        "measured_pairs": 21,
        "warmup_pairs": 5,
        "onnx_model_loaded": False,
        "output_matches_input_exactly": True,
        "graph_tensor_buffer_reused": True,
        "direct_tensor_buffer_reused": True,
        "same_graph_topology_as_onnx": True,
        "preprocessing_and_input_clones_outside_timed_intervals": True,
    }
    for name, expected in required.items():
        if type(report.get(name)) is not type(expected) or report[name] != expected:
            raise ValueError(f"{name}: expected {expected!r}, got {report.get(name)!r}")
    values = report.get("paired_graph_minus_direct_ms")
    if not isinstance(values, list) or len(values) != 21:
        raise ValueError("21 paired deltas are required")
    if any(type(v) not in (int, float) or not -1e9 < v < 1e9 for v in values):
        raise ValueError("invalid paired delta")
    for path in ("graph_identity_ms", "direct_owned_identity_ms"):
        stats = report.get(path)
        if not isinstance(stats, dict):
            raise ValueError(f"missing {path}")
        samples = [stats.get(k) for k in ("min", "mean", "p50", "p95", "max")]
        if any(type(v) not in (int, float) or not 0 <= v < 1e9 for v in samples):
            raise ValueError(f"invalid {path} timing")
        a, mean, median, p95, b = samples
        if not a <= median <= p95 <= b or not a <= mean <= b:
            raise ValueError(f"inconsistent {path} statistics")


def command_output(*args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def capture_metadata(environment):
    return {
        "capture_date_local": datetime.date.today().isoformat(),
        "machine": platform.machine(),
        "macos_version": platform.mac_ver()[0] or None,
        "cpu_brand": command_output("sysctl", "-n", "machdep.cpu.brand_string"),
        "rustc_version_verbose": command_output("rustc", "-Vv"),
        "git_commit": command_output("git", "rev-parse", "HEAD"),
        "git_dirty": bool(command_output("git", "status", "--porcelain")),
        "power_and_load_conditions_user_reported": environment,
        "runner": "user-operated local Mac (not GitHub Actions)",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--environment", required=True,
                        help="Observed power/load conditions; do not guess")
    parser.add_argument("--image", default="benchmarks/datasets/detection/bus.jpg")
    parser.add_argument("--output", type=Path, help="Output archive path")
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("P0-01 capture requires a native Apple Silicon macOS machine")
    if not Path("Cargo.toml").is_file():
        parser.error("run from repository root")
    if not Path(args.image).is_file():
        parser.error(f"image missing: {args.image}; run bash scripts/fetch-yolov8n-demo.sh")
    if not args.environment.strip():
        parser.error("--environment must describe actual observed conditions")
    cmd = ["cargo", "run", "--release", "-p", "modelbox-cli",
           "--features", "onnx", "--", "bench-transport-yolov8",
           args.image, "21", "5"]
    print("Running: " + " ".join(cmd), file=sys.stderr)
    completed = subprocess.run(cmd, text=True, capture_output=True, check=False)
    if completed.returncode:
        print(completed.stderr, file=sys.stderr)
        parser.error(f"benchmark failed with exit code {completed.returncode}")
    try:
        report = json.loads(completed.stdout)
        verify(report)
    except (ValueError, json.JSONDecodeError) as exc:
        parser.error(f"invalid Release report: {exc}")
    metadata = capture_metadata(args.environment.strip())
    if not metadata["macos_version"] or not metadata["rustc_version_verbose"] or not metadata["git_commit"]:
        parser.error("required environment provenance could not be collected")
    report["capture_metadata"] = metadata
    date = metadata["capture_date_local"]
    out = args.output or Path(
        f"benchmarks/baselines/{date}-macos-arm64-yolov8n-owned-transport-release.json"
    )
    if out.exists():
        parser.error(f"refusing to overwrite existing baseline: {out}")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Saved validated local baseline: {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
