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

## First trained model and actual photograph

```bash
bash scripts/fetch-yolov8n-demo.sh
cargo run --release -p modelbox-cli --features onnx -- \
  detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 0.25 0.45
```

The downloader fetches [webml/yolov8n ONNX](https://huggingface.co/webml/yolov8n/blob/main/onnx/yolov8n.onnx) (model SHA256 `190ba5f1e61411a001683e349d6b2cdb0804c0dc67a5e34cd8ff6fd00ee54b4d`) and [Ultralytics bus.jpg](https://github.com/ultralytics/assets/blob/main/im/bus.jpg) (Git blob SHA1 `40eaaf5c330d0c498fbe1dcacf9bb8bf566797fe`). Downloaded files are not tracked in Git. Model weights have AGPL-3.0 obligations; see the upstream license and obtain commercial licensing where applicable.

This narrow end-to-end adapter uses the trained 80-class 640×640 YOLOv8n exporter with a `[1,84,8400]` NMS-free output. It accepts JPEG/PNG input, RGB conversion, 114-letterbox preprocessing, CHW /255 normalization, Rust Graph / ONNX Runtime CPU inference, existing class-aware NMS, and inverse letterbox projection. Outputs include COCO class labels, confidence scores and boxes in **original image pixel space**.

The interpolation is a triangle filter, which approximates but may differ from OpenCV's bilinear implementation. The CI smoke on one sample checks for person+bus; **it does not establish independent reference box agreement or mAP**. Multi-output models and Ultralytics end-to-end NMS exports remain unsupported.
