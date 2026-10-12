"""Studio bridge contract tests: mock subprocess; never execute cargo here."""
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
from unittest import TestCase
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "studio_bridge.py"
spec = importlib.util.spec_from_file_location("studio_bridge", SCRIPT)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class StudioBridgeTests(TestCase):
    def test_missing_graph_is_rejected(self):
        status, body = bridge.invoke("run", {})
        self.assertEqual(status, 400)
        self.assertIn("graph", body["error"])

    def test_oversize_input_is_rejected(self):
        status, _ = bridge.invoke("run", {"graph": {}, "input": "a" * 4097})
        self.assertEqual(status, 400)

    def test_unknown_action_is_rejected(self):
        self.assertEqual(bridge.invoke("shell", {})[0], 404)

    def test_catalog_decodes_cli_result(self):
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=0, stdout='[{"name":"core.input"}]', stderr="")) as mock:
            code, body = bridge.invoke("catalog", {})
        self.assertEqual(code, 200)
        self.assertEqual(body[0]["name"], "core.input")
        mock.assert_called_once_with("catalog", "--json")

    def test_run_uses_temporary_graph_and_fixed_args(self):
        graph = {"name": "hi", "nodes": [], "edges": []}
        def fake_cargo(*args):
            self.assertEqual(args[0], "run")
            self.assertEqual(json.loads(Path(args[1]).read_text()), graph)
            self.assertEqual(args[2], "hello")
            return SimpleNamespace(returncode=0, stdout='{"outputs":{},"input_count":1}', stderr="")
        with patch.object(bridge, "cargo", side_effect=fake_cargo):
            code, data = bridge.invoke("run", {"graph": graph, "input": "hello"})
        self.assertEqual(code, 200)
        self.assertEqual(data["result"]["input_count"], 1)
        self.assertEqual(data["diagnostics"]["scope"], "bridge_cli_wall_time")
        self.assertGreaterEqual(data["diagnostics"]["elapsed_ms"], 0)

    def test_cli_failure_is_reported_without_success_claim(self):
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=1, stdout="", stderr="invalid graph")):
            code, data = bridge.invoke("validate", {"graph": {"name": "invalid"}})
        self.assertEqual(code, 422)
        self.assertIn("invalid graph", data["error"])
        self.assertEqual(data["diagnostics"]["scope"], "bridge_cli_wall_time")

    def test_profiled_run_preserves_outputs_and_per_node_counts(self):
        graph = {"name": "hi", "nodes": [], "edges": []}
        output = '{"result":{"outputs":{},"input_count":1},"node_process":{"upper":{"calls":1,"process_ns":1500}}}'
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=0, stdout=output, stderr="")) as mock:
            code, data = bridge.invoke("run-profile", {"graph": graph, "input": "hello"})
        self.assertEqual(code, 200)
        self.assertEqual(data["result"]["input_count"], 1)
        self.assertEqual(data["node_process"]["upper"]["calls"], 1)
        mock.assert_called_once()
        self.assertEqual(mock.call_args.args[0], "run-profile")

    def test_error_origin_is_forwarded_from_cli_metadata(self):
        stderr = 'modelbox: execution error at node upper: deliberate failure\nMODELBOX_ERROR_JSON:{"message":"failed","node_id":"upper"}\n'
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=1, stdout="", stderr=stderr)):
            status, body = bridge.invoke("run", {"graph": {}, "input": "hello"})
        self.assertEqual(status, 422)
        self.assertEqual(body["node_id"], "upper")

    def test_traced_run_forwards_event_timeline_without_payloads(self):
        graph = {"name": "hello", "nodes": [], "edges": []}
        stdout = json.dumps({
            "result": {"outputs": {"result.out": [{"text": "hello"}]}, "input_count": 1},
            "events": [{"node_id": "upper", "sequence": 0, "process_started_ns": 100,
                        "process_duration_ns": 45, "ok": True}],
            "dropped_events": 0,
        })
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=0, stdout=stdout, stderr="")) as mock:
            code, data = bridge.invoke("run-trace", {"graph": graph, "input": "hello"})
        self.assertEqual(code, 200)
        self.assertEqual(data["result"]["input_count"], 1)
        self.assertEqual(data["events"][0]["node_id"], "upper")
        self.assertEqual(data["dropped_events"], 0)
        self.assertEqual(mock.call_args.args[0], "run-trace")
