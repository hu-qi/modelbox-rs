#!/usr/bin/env python3
"""Compare YOLOv8n bus.jpg inference with an OBSERVED Mac mini golden output.

First-party numerical regression only; does not certify detection mAP or
independent model accuracy. No external Python dependencies.
"""

import json
import math
import pathlib
import sys

DEFAULT_GOLDEN = pathlib.Path(__file__).resolve().parents[1] / "benchmarks" / "baselines" / "2026-10-11-macos-arm64-yolov8n-bus-observed.json"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check(actual, golden):
    expected = golden["expected"]
    tolerances = golden["ci_tolerances"]
    max_px = tolerances["max_coordinate_absolute_error_px"]
    max_score = tolerances["max_confidence_absolute_error"]
    for field in (
        "model_input_shape",
        "model_output_shape",
        "source_image_width",
        "source_image_height",
        "coordinate_space",
        "detection_count",
    ):
        require(actual.get(field) == expected[field], f"{field} mismatch: {actual.get(field)!r}")

    geometry = actual.get("letterbox", {})
    for key in ("left", "top", "scaled_width", "scaled_height", "padding_rgb"):
        require(geometry.get(key) == expected["letterbox"][key], f"letterbox.{key} mismatch")
    require(
        abs(float(geometry["gain"]) - expected["letterbox"]["gain"]) < 0.00001,
        "letterbox gain mismatch",
    )
    detections = actual.get("detections", [])
    require(isinstance(detections, list) and len(detections) == expected["detection_count"], "detection count mismatch")
    width = expected["source_image_width"]
    height = expected["source_image_height"]
    for n, item in enumerate(detections):
        coords = item.get("xyxy", [])
        require(isinstance(coords, list) and len(coords) == 4, f"bad xyxy at detection {n}")
        require(all(isinstance(v, (int, float)) and math.isfinite(v) for v in coords), f"nonfinite xyxy at {n}")
        require(
            0 <= coords[0] < coords[2] <= width and 0 <= coords[1] < coords[3] <= height,
            f"invalid original-image bounding box {n}: {coords}",
        )
        score = item.get("confidence")
        require(isinstance(score, (int, float)) and math.isfinite(score) and 0 <= score <= 1, f"invalid confidence at {n}")

    unmatched = set(range(len(detections)))
    observed_max_px = 0.0
    observed_max_score = 0.0
    for reference in expected["detections"]:
        # Match by class and box location, not by array position or anchor ID.
        possible = [
            (max(abs(a - b) for a, b in zip(reference["xyxy"], detections[index]["xyxy"])), index)
            for index in unmatched
            if detections[index].get("class_id") == reference["class_id"]
            and detections[index].get("class_name") == reference["class_name"]
        ]
        require(possible, f"missing class {reference['class_name']} ({reference['class_id']})")
        box_delta, index = min(possible)
        score_delta = abs(detections[index]["confidence"] - reference["confidence"])
        require(box_delta <= max_px, f"{reference['class_name']} box drift: {box_delta:.3f}px > {max_px}px")
        require(score_delta <= max_score, f"{reference['class_name']} confidence drift: {score_delta:.5f} > {max_score}")
        observed_max_px = max(observed_max_px, box_delta)
        observed_max_score = max(observed_max_score, score_delta)
        unmatched.remove(index)

    require(not unmatched, f"unexpected additional detections {unmatched}")
    return {
        "matched": len(detections),
        "max_coordinate_delta_px": observed_max_px,
        "max_confidence_delta": observed_max_score,
        "coordinate_tolerance_px": max_px,
        "confidence_tolerance": max_score,
        "golden_type": "first-party-observed-mac-mini-run",
    }


def main(argv):
    require(len(argv) in (2, 3), "usage: verify-yolov8n-demo.py <detection-output.json> [golden.json]")
    actual_path = pathlib.Path(argv[1])
    golden_path = pathlib.Path(argv[2]) if len(argv) == 3 else DEFAULT_GOLDEN
    actual = json.loads(actual_path.read_text(encoding="utf-8"))
    golden = json.loads(golden_path.read_text(encoding="utf-8"))
    print(json.dumps(check(actual, golden), indent=2, sort_keys=True))


if __name__ == "__main__":
    try:
        main(sys.argv)
    except (ValueError, OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        print(f"YOLOv8n observed-baseline regression FAILED: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
