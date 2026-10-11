# Object detection plan and interface boundary

## What is implemented

The pure Rust CLI `postprocess-yolo` accepts a serialized `TensorF32` (or a `Runtime::run` Graph JSON result with a single `result.out` tensor) from a YOLOv8/YOLO11 *raw, NMS-free, single-output* detector.

The supported contract is deliberately narrow:

- Shape: `[1, 4 + num_classes, num_candidates]`, channel-first.
- Box channels: center-x, center-y, width, height (`xywh`) in **model input pixels**.
- Remaining channels: one *already activated confidence* per class, no independent objectness multiplication and **no softmax**.
- Choose one highest-scoring class per candidate; filter score using a configurable threshold.
- Convert to `xyxy`, clip to the stated model canvas, suppress overlapping boxes **within the same class**, and limit the count.
- Sort descending confidence, then anchor index; cap pre-NMS work at 3,000 high-confidence candidates to limit CPU complexity.
- Report `class_id`, confidence, `xyxy` and candidate index, explicitly in *model_input_pixels*.

Non-goals: raw YOLOv5 `[1,N,85]` with objectness, YOLO end-to-end `[1,300,6]` with embedded NMS, rotated boxes, segmentation, multi-label-per-anchor output, and batch size > 1. Real model output rank and channel order **must** be checked against the pinned export; name similarity is not validation.

Reference contracts: [Ultralytics ONNX export](https://docs.ultralytics.com/modes/export/) and [Ultralytics YOLO ONNX Runtime example](https://github.com/ultralytics/ultralytics/blob/main/examples/YOLOv8-ONNXRuntime/main.py).

## Functional test

```bash
cargo run -p modelbox-cli -- \
  postprocess-yolo examples/yolo-raw-output.json 640 640 0.25 0.5 10
```

This synthetic fixture contains three candidates and two classes. The adapter should retain the strongest class-0 candidate and a co-located class-1 candidate, but suppress an overlapping lower-scoring class-0 candidate. It uses no real ONNX model or images.

## Next independent acceptance gates

1. Pin a genuinely trained, redistributable and checksum-verified ONNX detection model, its graph outputs, class labels and license.
2. Decode source-image pixels, implement RGB/CHW normalization and **model-specific letterbox** preserving geometric scale and pad metadata. Undo letterbox in output boxes and clip to original-image dimensions.
3. Exercise Graph ONNX inference and adapter together against a licensed actual photograph. Compare boxes/classes/scores to an **independent reference implementation** with numeric tolerances, not merely Graph vs direct ONNX using the same CPU backend.
4. Add a labeled detection dataset and report precision/recall/mAP, with clear preprocessing and IoU definitions. Validate Linux x86_64 and native macOS ARM64.
5. Measure full file/image-to-detection latency and throughput; the synthetic decoder test is not a model inference or production-stream benchmark.

Current `onnx.cpu` is restricted to *exactly one f32 input and one f32 output*. Multi-output detection models require an adapter/runtime design change before they can run.
