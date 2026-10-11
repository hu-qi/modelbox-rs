import copy
import importlib.util
import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "verify-yolov8n-demo.py"
GOLDEN = ROOT / "benchmarks" / "baselines" / "2026-10-11-macos-arm64-yolov8n-bus-observed.json"
SPEC = importlib.util.spec_from_file_location("yolo_validator", SCRIPT)
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


class YoloBaselineTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.baseline = json.loads(GOLDEN.read_text(encoding="utf-8"))

    def test_matching_observation(self):
        expected = copy.deepcopy(self.baseline["expected"])
        result = validator.check(expected, self.baseline)
        self.assertEqual(result["matched"], 5)
        self.assertEqual(result["max_coordinate_delta_px"], 0)

    def test_tolerates_detection_reordering(self):
        expected = copy.deepcopy(self.baseline["expected"])
        expected["detections"].reverse()
        self.assertEqual(validator.check(expected, self.baseline)["matched"], 5)

    def test_small_cross_platform_float_variations(self):
        expected = copy.deepcopy(self.baseline["expected"])
        expected["detections"][0]["xyxy"][0] += 2.0
        expected["detections"][0]["confidence"] -= 0.015
        self.assertEqual(validator.check(expected, self.baseline)["matched"], 5)

    def test_fails_class_mismatch(self):
        expected = copy.deepcopy(self.baseline["expected"])
        expected["detections"][0]["class_name"] = "dog"
        with self.assertRaises(ValueError):
            validator.check(expected, self.baseline)

    def test_fails_box_drift(self):
        expected = copy.deepcopy(self.baseline["expected"])
        expected["detections"][3]["xyxy"][0] += 20.0
        with self.assertRaises(ValueError):
            validator.check(expected, self.baseline)

    def test_fails_score_drift(self):
        expected = copy.deepcopy(self.baseline["expected"])
        expected["detections"][4]["confidence"] -= 0.1
        with self.assertRaises(ValueError):
            validator.check(expected, self.baseline)


if __name__ == "__main__":
    unittest.main()
