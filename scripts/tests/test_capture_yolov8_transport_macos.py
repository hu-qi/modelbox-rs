"""Pure validation tests; fixture timings are synthetic, NOT Mac measurements."""
import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "capture-yolov8-transport-macos.py"
spec = importlib.util.spec_from_file_location("transport_capture", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def sample():
    return {
        "benchmark": "yolov8n_input_identity_graph_transport_control",
        "profile": "release",
        "input_tensor_bytes": 4915200,
        "measured_pairs": 21,
        "warmup_pairs": 5,
        "onnx_model_loaded": False,
        "output_matches_input_exactly": True,
        "graph_tensor_buffer_reused": True,
        "direct_tensor_buffer_reused": True,
        "same_graph_topology_as_onnx": True,
        "preprocessing_and_input_clones_outside_timed_intervals": True,
        "paired_graph_minus_direct_ms": [0.1] * 21,
        "graph_identity_ms": {"min": 1, "mean": 2, "p50": 2, "p95": 3, "max": 4},
        "direct_owned_identity_ms": {"min": 0.1, "mean": 0.2, "p50": 0.2, "p95": 0.3, "max": 0.4},
    }


class TransportCaptureValidationTests(unittest.TestCase):
    def test_accepts_structural_report(self):
        module.verify(sample())

    def test_rejects_debug(self):
        report = sample()
        report["profile"] = "debug"
        with self.assertRaises(ValueError):
            module.verify(report)

    def test_rejects_missing_pair(self):
        report = sample()
        report["paired_graph_minus_direct_ms"].pop()
        with self.assertRaises(ValueError):
            module.verify(report)

    def test_rejects_missing_pointer_proof(self):
        report = sample()
        report["graph_tensor_buffer_reused"] = False
        with self.assertRaises(ValueError):
            module.verify(report)

    def test_rejects_bad_percentile_order(self):
        report = sample()
        report["graph_identity_ms"]["p95"] = 0.5
        with self.assertRaises(ValueError):
            module.verify(report)

    def test_rejects_wrong_size_and_wrong_pair_count(self):
        for key, bad in (("input_tensor_bytes", 12), ("measured_pairs", 3)):
            with self.subTest(key=key):
                report = sample()
                report[key] = bad
                with self.assertRaises(ValueError):
                    module.verify(report)


if __name__ == "__main__":
    unittest.main()
