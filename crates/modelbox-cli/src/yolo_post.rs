//! Decode single-output, NMS-free YOLOv8/YOLO11 raw detection tensors.
//!
//! Contract: f32[1, 4 + num_classes, num_predictions] in channels-first
//! layout; channels 0..4 are cx,cy,w,h in model-input pixels, remaining
//! channels are per-class confidences (already probabilities, no softmax).
//! This is NOT the YOLOv5 objectness format, NMS-in-model [N,6] output,
//! segmentation/keypoints, or a generic detector tensor parser.

use std::fs;

use modelbox_core::{FlowError, TensorF32};
use serde_json::{json, Value as JsonValue};

const MAX_CANDIDATES: usize = 30_000;
const PRE_NMS_LIMIT: usize = 3_000;
const MAX_DETECTIONS: usize = 1_000;

#[derive(Debug, Clone, PartialEq)]
struct Detection {
    class_id: usize,
    confidence: f32,
    xyxy: [f32; 4],
    anchor: usize,
}

fn invalid(msg: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("YOLO postprocessing: {msg}"))
}

fn overlap(a: &Detection, b: &Detection) -> f32 {
    let left = a.xyxy[0].max(b.xyxy[0]);
    let top = a.xyxy[1].max(b.xyxy[1]);
    let right = a.xyxy[2].min(b.xyxy[2]);
    let bottom = a.xyxy[3].min(b.xyxy[3]);
    let intersection = (right - left).max(0.0) * (bottom - top).max(0.0);
    let area_a = (a.xyxy[2] - a.xyxy[0]) * (a.xyxy[3] - a.xyxy[1]);
    let area_b = (b.xyxy[2] - b.xyxy[0]) * (b.xyxy[3] - b.xyxy[1]);
    let denominator = area_a + area_b - intersection;
    if denominator <= 0.0 {
        0.0
    } else {
        intersection / denominator
    }
}

fn decode(
    tensor: &TensorF32,
    width: usize,
    height: usize,
    confidence_threshold: f32,
    iou_threshold: f32,
    max_detections: usize,
) -> modelbox_core::Result<Vec<Detection>> {
    let shape = tensor.shape();
    if shape.len() != 3 || shape[0] != 1 || shape[1] < 5 {
        return Err(invalid("expects raw [1, 4+classes, candidates] tensor"));
    }
    let classes = shape[1] - 4;
    let count = shape[2];
    if count == 0 || count > MAX_CANDIDATES {
        return Err(invalid("candidate count must be in 1..=30000"));
    }
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err(invalid("model input dimensions must be in 1..=16384"));
    }
    if !confidence_threshold.is_finite()
        || !(0.0..=1.0).contains(&confidence_threshold)
        || !iou_threshold.is_finite()
        || !(0.0..=1.0).contains(&iou_threshold)
    {
        return Err(invalid(
            "confidence and IoU thresholds must be within 0..=1",
        ));
    }
    if max_detections == 0 || max_detections > MAX_DETECTIONS {
        return Err(invalid("max_detections must be in 1..=1000"));
    }

    let values = tensor.data();
    let channel = |index: usize, candidate: usize| values[index * count + candidate];
    let wmax = width as f32;
    let hmax = height as f32;
    let mut candidates = Vec::new();

    for index in 0..count {
        let mut best_class = 0;
        let mut best_confidence = channel(4, index);
        for class in 1..classes {
            let score = channel(4 + class, index);
            if score > best_confidence {
                best_class = class;
                best_confidence = score;
            }
        }
        if !(confidence_threshold..=1.0).contains(&best_confidence) {
            continue;
        }
        let center_x = channel(0, index);
        let center_y = channel(1, index);
        let box_width = channel(2, index);
        let box_height = channel(3, index);
        if box_width <= 0.0 || box_height <= 0.0 {
            continue;
        }
        let xyxy = [
            (center_x - box_width / 2.0).clamp(0.0, wmax),
            (center_y - box_height / 2.0).clamp(0.0, hmax),
            (center_x + box_width / 2.0).clamp(0.0, wmax),
            (center_y + box_height / 2.0).clamp(0.0, hmax),
        ];
        if xyxy[2] <= xyxy[0] || xyxy[3] <= xyxy[1] {
            continue;
        }
        candidates.push(Detection {
            class_id: best_class,
            confidence: best_confidence,
            xyxy,
            anchor: index,
        });
    }

    candidates.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.anchor.cmp(&b.anchor))
    });
    candidates.truncate(PRE_NMS_LIMIT);

    let mut selected = Vec::new();
    for candidate in candidates {
        if selected.iter().any(|previous: &Detection| {
            previous.class_id == candidate.class_id && overlap(previous, &candidate) > iou_threshold
        }) {
            continue;
        }
        selected.push(candidate);
        if selected.len() == max_detections {
            break;
        }
    }
    Ok(selected)
}

fn parse_output(contents: &str) -> modelbox_core::Result<TensorF32> {
    if let Ok(tensor) = serde_json::from_str::<TensorF32>(contents) {
        return Ok(tensor);
    }
    let wrapped: JsonValue = serde_json::from_str(contents)?;
    let array = wrapped
        .pointer("/outputs/result.out")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| invalid("expects a raw TensorF32 JSON or Graph output result.out"))?;
    if array.len() != 1 || array[0].get("type").and_then(JsonValue::as_str) != Some("tensor_f32") {
        return Err(invalid("result.out must contain exactly one tensor_f32"));
    }
    serde_json::from_value(array[0]["value"].clone())
        .map_err(|error| invalid(format!("invalid result.out tensor: {error}")))
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(3..=6).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let width = args[1]
        .parse::<usize>()
        .map_err(|_| invalid("invalid model input width"))?;
    let height = args[2]
        .parse::<usize>()
        .map_err(|_| invalid("invalid model input height"))?;
    let optional_f32 = |index: usize, fallback: f32| -> modelbox_core::Result<f32> {
        match args.get(index) {
            Some(raw) => raw
                .parse::<f32>()
                .map_err(|_| invalid("invalid confidence/IoU threshold")),
            None => Ok(fallback),
        }
    };
    let conf = optional_f32(3, 0.25)?;
    let iou = optional_f32(4, 0.45)?;
    let max = match args.get(5) {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| invalid("invalid maximum detections"))?,
        None => 300,
    };
    let tensor = parse_output(&fs::read_to_string(&args[0])?)?;
    let detections = decode(&tensor, width, height, conf, iou, max)?;
    let objects: Vec<_> = detections
        .iter()
        .map(|d| {
            json!({
                "class_id": d.class_id,
                "confidence": d.confidence,
                "xyxy": d.xyxy,
                "candidate_index": d.anchor
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "format": "yolov8_yolo11_raw_xywh_channels_first_without_nms",
            "coordinate_space": "model_input_pixels",
            "model_input_width": width,
            "model_input_height": height,
            "number_of_classes": tensor.shape()[1] - 4,
            "input_candidates": tensor.shape()[2],
            "postprocess": {
                "confidence_threshold": conf,
                "iou_threshold": iou,
                "max_detections": max,
                "class_aware_nms": true,
                "pre_nms_top_k": PRE_NMS_LIMIT,
                "one_class_per_anchor": true
            },
            "detections": objects,
            "detection_count": detections.len(),
            "warning": "Coordinates are in model input pixels; no image decode, letterbox undo, source-image rescaling, real-model inference, or mAP evaluation is performed"
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use modelbox_core::TensorF32;

    use super::{decode, overlap, parse_output};

    fn fixture() -> TensorF32 {
        TensorF32::new(
            vec![1, 6, 3],
            vec![
                100.0, 110.0, 100.0, // center x
                100.0, 110.0, 100.0, // center y
                100.0, 100.0, 100.0, // width
                100.0, 100.0, 100.0, // height
                0.9, 0.8, 0.1, // class 0
                0.2, 0.1, 0.85, // class 1
            ],
        )
        .unwrap()
    }

    #[test]
    fn suppresses_same_class_overlap_but_preserves_other_classes() {
        let result = decode(&fixture(), 640, 640, 0.25, 0.5, 10).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!((result[0].class_id, result[0].anchor), (0, 0));
        assert_eq!((result[1].class_id, result[1].anchor), (1, 2));
        assert_eq!(result[0].xyxy, [50.0, 50.0, 150.0, 150.0]);
        assert_eq!(overlap(&result[0], &result[1]), 1.0);
    }

    #[test]
    fn honors_threshold_cap_and_boundary_clipping() {
        assert!(decode(&fixture(), 640, 640, 0.95, 0.5, 10)
            .unwrap()
            .is_empty());
        assert_eq!(decode(&fixture(), 640, 640, 0.25, 0.5, 1).unwrap().len(), 1);
        assert!(decode(&fixture(), 640, 640, -0.1, 0.5, 10).is_err());
        assert!(decode(&fixture(), 640, 640, 0.25, 1.1, 10).is_err());
        assert!(decode(&fixture(), 0, 640, 0.25, 0.5, 10).is_err());
        assert!(decode(&fixture(), 640, 640, 0.25, 0.5, 0).is_err());

        let clipped = TensorF32::new(vec![1, 5, 1], vec![2.0, 2.0, 20.0, 20.0, 0.9]).unwrap();
        let output = decode(&clipped, 10, 10, 0.25, 0.5, 10).unwrap();
        assert_eq!(output[0].xyxy, [0.0, 0.0, 10.0, 10.0]);
    }

    #[test]
    fn rejects_different_yolo_layouts() {
        let wrong = TensorF32::new(vec![1, 3, 6], vec![0.0; 18]).unwrap();
        assert!(decode(&wrong, 640, 640, 0.25, 0.5, 10).is_err());
        let yolo_v5 = TensorF32::new(vec![6], vec![0.0; 6]).unwrap();
        assert!(decode(&yolo_v5, 640, 640, 0.25, 0.5, 10).is_err());
    }

    #[test]
    fn parses_graph_output_and_raw_tensor_json() {
        let raw = serde_json::to_string(&fixture()).unwrap();
        assert_eq!(parse_output(&raw).unwrap(), fixture());
        let wrapped = serde_json::json!({
            "outputs": {
                "result.out": [{
                    "type": "tensor_f32",
                    "value": fixture()
                }]
            }
        });
        assert_eq!(parse_output(&wrapped.to_string()).unwrap(), fixture());
        assert!(parse_output("{}").is_err());
    }
}
