//! End-to-end YOLOv8n ONNX image inference (single-output COCO80, 640x640).
//! NOTE: Ultralytics YOLO weights are AGPL-3.0 and downloaded separately;
//! this module does not bundle them or claim commercial licensing rights.

use std::sync::Arc;

use image::{imageops::FilterType, RgbImage};
use modelbox_core::{FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value};
use serde_json::json;

use crate::yolo_post;

const SIDE: u32 = 640;
const PIXELS: usize = (SIDE as usize) * (SIDE as usize);
const COCO80: [&str; 80] = [
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck",
    "boat", "traffic light", "fire hydrant", "stop sign", "parking meter", "bench",
    "bird", "cat", "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra",
    "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
    "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove",
    "skateboard", "surfboard", "tennis racket", "bottle", "wine glass", "cup",
    "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
    "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch",
    "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse",
    "remote", "keyboard", "cell phone", "microwave", "oven", "toaster", "sink",
    "refrigerator", "book", "clock", "vase", "scissors", "teddy bear",
    "hair drier", "toothbrush",
];

fn invalid(message: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("YOLO image inference: {message}"))
}

#[derive(Debug, Clone, Copy)]
struct Letterbox {
    source_width: u32,
    source_height: u32,
    gain: f32,
    pad_left: u32,
    pad_top: u32,
    scaled_width: u32,
    scaled_height: u32,
}

fn letterbox(rgb: &RgbImage) -> modelbox_core::Result<(TensorF32, Letterbox)> {
    let (w, h) = rgb.dimensions();
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(invalid("source image dimensions must be 1..=8192"));
    }
    let gain = (SIDE as f32 / w as f32).min(SIDE as f32 / h as f32);
    let scaled_width = ((w as f32 * gain).round() as u32).clamp(1, SIDE);
    let scaled_height = ((h as f32 * gain).round() as u32).clamp(1, SIDE);
    let pad_left = (SIDE - scaled_width) / 2;
    let pad_top = (SIDE - scaled_height) / 2;

    // For an approximate Ultralytics/cv2 INTER_LINEAR preprocessing match,
    // resize with triangular filtering and preserve centered 114 RGB padding.
    let resized = image::imageops::resize(
        rgb,
        scaled_width,
        scaled_height,
        FilterType::Triangle,
    );
    let mut chw = vec![114.0_f32 / 255.0; 3 * PIXELS];
    for (x, y, pixel) in resized.enumerate_pixels() {
        let offset = (y + pad_top) as usize * SIDE as usize + (x + pad_left) as usize;
        for channel in 0..3 {
            chw[channel * PIXELS + offset] = f32::from(pixel[channel]) / 255.0;
        }
    }
    Ok((
        TensorF32::new(vec![1, 3, SIDE as usize, SIDE as usize], chw)?,
        Letterbox {
            source_width: w,
            source_height: h,
            gain,
            pad_left,
            pad_top,
            scaled_width,
            scaled_height,
        },
    ))
}

fn project_xyxy(coords: [f32; 4], geometry: Letterbox) -> [f32; 4] {
    let x1 = ((coords[0] - geometry.pad_left as f32) / geometry.gain)
        .clamp(0.0, geometry.source_width as f32);
    let y1 = ((coords[1] - geometry.pad_top as f32) / geometry.gain)
        .clamp(0.0, geometry.source_height as f32);
    let x2 = ((coords[2] - geometry.pad_left as f32) / geometry.gain)
        .clamp(0.0, geometry.source_width as f32);
    let y2 = ((coords[3] - geometry.pad_top as f32) / geometry.gain)
        .clamp(0.0, geometry.source_height as f32);
    [x1, y1, x2, y2]
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(2..=4).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let optional_threshold = |index: usize, default: f32| -> modelbox_core::Result<f32> {
        match args.get(index) {
            Some(s) => s.parse().map_err(|_| invalid("invalid confidence / IoU value")),
            None => Ok(default),
        }
    };
    let confidence = optional_threshold(2, 0.25)?;
    let iou = optional_threshold(3, 0.45)?;
    let (model_path, image_path) = (&args[0], &args[1]);
    let raw_image = image::open(image_path).map_err(invalid)?;
    let (tensor, geometry) = letterbox(&raw_image.to_rgb8())?;

    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model_path)?))?;
    let graph: GraphSpec = serde_json::from_str(crate::ONNX_GRAPH)?;
    let result = runtime.run(&graph, Value::TensorF32(tensor))?;
    let output = match result.outputs.get("result.out").map(Vec::as_slice) {
        Some([Value::TensorF32(output)]) => output,
        _ => return Err(invalid("Graph must emit exactly one f32 output tensor")),
    };
    // Do not guess the layout of arbitrary ONNX object detectors.
    if output.shape() != [1, 84, 8400] {
        return Err(invalid(format!(
            "expected pinned 80-class YOLOv8n output [1,84,8400], got {:?}",
            output.shape()
        )));
    }
    let detections = yolo_post::decode(output, SIDE as usize, SIDE as usize, confidence, iou, 300)?;
    let mut objects = Vec::new();
    for detection in detections {
        let box_xyxy = project_xyxy(detection.xyxy, geometry);
        if box_xyxy[2] <= box_xyxy[0] || box_xyxy[3] <= box_xyxy[1] {
            continue;
        }
        objects.push(json!({
            "class_id": detection.class_id,
            "class_name": COCO80[detection.class_id],
            "confidence": detection.confidence,
            "xyxy": box_xyxy,
            "candidate_index": detection.anchor
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "model": model_path,
            "image": image_path,
            "source_image_width": geometry.source_width,
            "source_image_height": geometry.source_height,
            "model_input_shape": [1,3,640,640],
            "model_output_shape": output.shape(),
            "letterbox": {
                "gain": geometry.gain,
                "left": geometry.pad_left,
                "top": geometry.pad_top,
                "scaled_width": geometry.scaled_width,
                "scaled_height": geometry.scaled_height,
                "padding_rgb": [114,114,114],
                "resize_filter": "triangle_approximation_of_cv2_linear"
            },
            "coordinate_space": "source_image_pixels",
            "class_names": "COCO80",
            "confidence_threshold": confidence,
            "iou_threshold": iou,
            "detection_count": objects.len(),
            "detections": objects,
            "warning": "Trained weights may be AGPL-3.0; verify your model's license. Triangle RGB interpolation differs slightly from OpenCV/Ultralytics. This demo is not a labeled mAP or independent numerical-accuracy evaluation."
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use image::{Rgb, RgbImage};

    use super::{letterbox, project_xyxy, Letterbox};

    #[test]
    fn preprocesses_rgb_channels_with_letterbox_padding() {
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, Rgb([255, 0, 20]));
        img.put_pixel(1, 0, Rgb([0, 255, 30]));
        let (tensor, geometry) = letterbox(&img).unwrap();
        assert_eq!(tensor.shape(), &[1, 3, 640, 640]);
        assert_eq!((geometry.pad_left, geometry.pad_top), (0, 160));
        assert_eq!((geometry.scaled_width, geometry.scaled_height), (640, 320));
        assert_eq!(tensor.data()[0], 114.0 / 255.0);
        // The top-left red channel is 1 and green is 0 at first content pixel.
        let origin = 160 * 640;
        assert_eq!(tensor.data()[origin], 1.0);
        assert_eq!(tensor.data()[640 * 640 + origin], 0.0);
    }

    #[test]
    fn reverses_letterbox_and_clips_source_coordinates() {
        let g = Letterbox {
            source_width: 320,
            source_height: 160,
            gain: 2.0,
            pad_left: 0,
            pad_top: 160,
            scaled_width: 640,
            scaled_height: 320,
        };
        assert_eq!(project_xyxy([40.0, 180.0, 400.0, 440.0], g), [20.0, 10.0, 200.0, 140.0]);
        assert_eq!(project_xyxy([0.0, 0.0, 640.0, 640.0], g), [0.0, 0.0, 320.0, 160.0]);
    }
}
