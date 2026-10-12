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

    def test_transport_run_forwards_measured_channel_api_metrics(self):
        graph = {"name": "hello", "nodes": [], "edges": []}
        stdout = json.dumps({
            "result": {"outputs": {}, "input_count": 1},
            "node_transport": {
                "source": {"receive_operations": 0, "receive_wall_ns": 0,
                           "send_operations": 1, "send_wall_ns": 100, "send_timeouts": 0}
            },
        })
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(returncode=0, stdout=stdout, stderr="")) as mock:
            code, data = bridge.invoke("run-transport", {"graph": graph, "input": "hello"})
        self.assertEqual(code, 200)
        self.assertEqual(data["result"]["input_count"], 1)
        self.assertEqual(data["node_transport"]["source"]["send_operations"], 1)
        self.assertEqual(mock.call_args.args[0], "run-transport")

    def test_edge_run_forwards_real_link_events_and_bounded_counts(self):
        graph = {"name": "hello", "nodes": [], "edges": []}
        payload = {
            "result": {"outputs": {}, "input_count": 1},
            "edges": [{
                "from": "source.out", "to": "upper.in",
                "send_operations": 1, "send_wall_ns": 24,
                "send_timeouts": 0, "receive_operations": 2,
                "receive_wall_ns": 200, "delivered_frames": 1,
            }],
            "deliveries": [{
                "from": "source.out", "to": "upper.in", "sequence": 0,
                "send_attempt_started_ns": 100, "receive_completed_ns": 250,
                "attempt_to_receive_ns": 150,
            }],
            "dropped_deliveries": 0,
        }
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(
            returncode=0, stdout=json.dumps(payload), stderr=""
        )) as mocked:
            code, body = bridge.invoke("run-edges", {"graph": graph, "input": "hello"})
        self.assertEqual(code, 200)
        self.assertEqual(body["result"]["input_count"], 1)
        self.assertEqual(body["edges"][0]["from"], "source.out")
        self.assertEqual(body["deliveries"][0]["sequence"], 0)
        self.assertEqual(body["dropped_deliveries"], 0)
        self.assertEqual(mocked.call_args.args[0], "run-edges")

    def test_legacy_import_validates_converted_graph_using_rust_cli(self):
        fixture = Path(__file__).resolve().parents[2] / "examples/legacy_modelbox_hello.toml"
        text = fixture.read_text(encoding="utf-8")
        def validate_graph(*args):
            self.assertEqual(args[0], "validate")
            converted = json.loads(Path(args[1]).read_text(encoding="utf-8"))
            self.assertEqual(converted["nodes"][2]["unit"], "text.prefix")
            self.assertEqual(converted["edges"][0], {"from": "input1.out", "to": "upper.in"})
            return SimpleNamespace(returncode=0, stdout="valid graph", stderr="")
        with patch.object(bridge, "cargo", side_effect=validate_graph) as mock:
            code, response = bridge.invoke("import-legacy", {
                "filename": fixture.name, "legacy_text": text,
            })
        self.assertEqual(code, 200)
        self.assertEqual(response["graph"]["nodes"][0]["unit"], "core.input")
        self.assertEqual(response["report"]["node_count"], 4)
        self.assertEqual(response["diagnostics"]["scope"], "legacy_import_and_runtime_validation")
        mock.assert_called_once()

    def test_legacy_import_propagates_rust_validation_failure(self):
        fixture = Path(__file__).resolve().parents[2] / "examples/legacy_modelbox_hello.dot"
        with patch.object(bridge, "cargo", return_value=SimpleNamespace(
            returncode=1, stdout="", stderr="invalid graph: topology rejected"
        )):
            code, response = bridge.invoke("import-legacy", {
                "filename": fixture.name, "legacy_text": fixture.read_text(),
            })
        self.assertEqual(code, 422)
        self.assertIn("Rust Runtime rejected", response["error"])

    def test_legacy_import_fails_closed_on_unsupported_plugin(self):
        dot = "digraph demo { src[type=input]; op[type=flowunit,flowunit=legacy_cv_resize,device=cpu]; dst[type=output]; src -> op:in; op:out -> dst; }"
        with patch.object(bridge, "cargo") as mock:
            code, response = bridge.invoke("import-legacy", {
                "filename": "graph.dot", "legacy_text": dot,
            })
        self.assertEqual(code, 422)
        self.assertIn("unsupported legacy FlowUnit", response["error"])
        mock.assert_not_called()

    def test_preflight_is_read_only_and_does_not_invoke_rust(self):
        dot = 'digraph demo { src[type=input]; gpu[type=flowunit,flowunit=resize,device=cuda]; dst[type=output]; src -> gpu:in; gpu:out -> dst; }'
        with patch.object(bridge, "cargo") as mocked:
            status, body = bridge.invoke("analyze-legacy", {
                "filename": "legacy.dot", "legacy_text": dot,
            })
        self.assertEqual(status, 200)
        self.assertFalse(body["report"]["candidate_convertible"])
        self.assertIn("unmapped_flowunit", {issue["code"] for issue in body["report"]["issues"]})
        mocked.assert_not_called()

    def test_failed_import_attaches_aggregated_blockers(self):
        dot = 'digraph demo { src[type=input]; gpu[type=flowunit,flowunit=resize,device=cuda]; dst[type=output]; src -> gpu:in; gpu:out -> dst; }'
        with patch.object(bridge, "cargo") as mocked:
            status, body = bridge.invoke("import-legacy", {
                "filename": "legacy.dot", "legacy_text": dot,
            })
        self.assertEqual(status, 422)
        self.assertEqual(body["report"]["node_count"], 3)
        self.assertGreaterEqual(len(body["report"]["issues"]), 2)
        mocked.assert_not_called()
