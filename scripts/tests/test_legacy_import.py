"""Compatibility subset tests: fixtures use legacy ModelBox TOML/Graphviz syntax."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("modelbox_legacy_import", ROOT / "scripts" / "legacy_import.py")
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)


class LegacyImportTests(unittest.TestCase):
    def test_official_shape_toml_fixture_translates_known_cpu_flowunits(self):
        fixture = ROOT / "examples" / "legacy_modelbox_hello.toml"
        graph, report = legacy.convert_legacy(fixture.read_text(), fixture.name)
        self.assertEqual(graph["name"], "hello_legacy")
        self.assertEqual(len(graph["nodes"]), 4)
        self.assertEqual(len(graph["edges"]), 3)
        self.assertEqual(graph["nodes"][0]["unit"], "core.input")
        self.assertEqual(graph["nodes"][2]["config"]["prefix"], "ModelBox Rust says: ")
        self.assertEqual(graph["nodes"][3]["unit"], "core.output")
        self.assertEqual(graph["edges"][0], {"from": "input1.out", "to": "upper.in"})
        self.assertEqual(graph["edges"][-1], {"from": "prefix.out", "to": "output1.in"})
        self.assertEqual(report["migration_scope"], "cpu-text-subset")
        self.assertTrue(report["requires_runtime_validation"])

    def test_dot_fixture_is_equivalent_to_toml_fixture(self):
        dot = ROOT / "examples" / "legacy_modelbox_hello.dot"
        toml = ROOT / "examples" / "legacy_modelbox_hello.toml"
        graph_dot, _ = legacy.convert_legacy(dot.read_text(), dot.name)
        graph_toml, _ = legacy.convert_legacy(toml.read_text(), toml.name)
        self.assertEqual(graph_dot, graph_toml)

    def test_unknown_legacy_flowunit_fails_without_fake_runtime_mapping(self):
        dot = "digraph unsafe { input1[type=input]; model[type=flowunit, flowunit=yolo, device=cpu]; out1[type=output]; input1 -> model:in; model:out -> out1; }"
        with self.assertRaisesRegex(legacy.LegacyImportError, "unsupported legacy FlowUnit 'yolo'"):
            legacy.convert_legacy(dot, "unsafe.dot")

    def test_cuda_is_rejected_not_silently_downgraded(self):
        dot = "digraph gpu { input1[type=input]; x[type=flowunit,flowunit=text.uppercase,device=cuda]; out1[type=output]; input1 -> x:in; x:out -> out1; }"
        with self.assertRaisesRegex(legacy.LegacyImportError, "non-CPU device"):
            legacy.convert_legacy(dot, "gpu.dot")

    def test_unknown_config_is_not_dropped(self):
        dot = "digraph untested { src[type=input]; x[type=flowunit,flowunit=text.uppercase,unsafe_option=1]; sink[type=output]; src -> x:in; x:out -> sink; }"
        with self.assertRaisesRegex(legacy.LegacyImportError, "unsupported FlowUnit options"):
            legacy.convert_legacy(dot, "untested.dot")

    def test_unsupported_subgraph_and_implicit_chain_are_rejected(self):
        for dot in (
            "digraph demo { subgraph cluster_a { x[type=input]; } }",
            "digraph demo { a[type=input]; b[type=output]; a -> b -> a; }",
        ):
            with self.subTest(dot=dot):
                with self.assertRaises(legacy.LegacyImportError):
                    legacy.convert_legacy(dot, "demo.dot")

    def test_multiline_comments_and_quoted_text_are_preserved(self):
        dot = """
        digraph demo {
            a[type=input] // input
            x[type=flowunit, flowunit=text.prefix, prefix="a // safe, literal"]
            z[type=output] # output
            a -> x:in
            x:out -> z
        }
        """
        graph, _ = legacy.convert_legacy(dot, "demo.dot")
        self.assertEqual(graph["nodes"][1]["config"]["prefix"], "a // safe, literal")

    def test_graph_document_rejects_nonempty_legacy_driver(self):
        text = '[driver]\ndir="/var/lib/modelbox/plugins"\n[graph]\nformat="graphviz"\ngraphconf="digraph x {}"\n'
        with self.assertRaisesRegex(legacy.LegacyImportError, "driver"):
            legacy.convert_legacy(text, "graph.toml")


    def test_preflight_collects_multiple_blockers_and_redacts_config_values(self):
        dot = """digraph demo {
          src[type=input]
          x[type=flowunit,flowunit=legacy_resize,device=cuda,secret="supersecret"]
          y[type=flowunit,flowunit=legacy_detector,device=ascend,token="hidden-token"]
          sink[type=output]
          src -> x:in
          x:out -> y:in
          y:out -> sink
        }"""
        report = legacy.analyze_legacy(dot, "legacy.dot")
        self.assertEqual(report["node_count"], 4)
        self.assertEqual(report["edge_count"], 3)
        self.assertFalse(report["candidate_convertible"])
        self.assertTrue(report["inspection_only"])
        self.assertEqual(
            {x["node_id"] for x in report["issues"] if x["code"] == "unmapped_flowunit"},
            {"x", "y"},
        )
        self.assertEqual(
            {x["node_id"] for x in report["issues"] if x["code"] == "device"},
            {"x", "y"},
        )
        self.assertNotIn("supersecret", str(report))
        self.assertNotIn("hidden-token", str(report))

    def test_preflight_marks_compatible_syntax_as_candidate_only(self):
        fixture = ROOT / "examples" / "legacy_modelbox_hello.toml"
        report = legacy.analyze_legacy(fixture.read_text(), fixture.name)
        self.assertTrue(report["candidate_convertible"])
        self.assertTrue(report["requires_runtime_validation"])
        self.assertEqual(report["issues"], [])

    def test_preflight_reports_custom_driver_without_exposing_path(self):
        toml = '[driver]\ndir="/secret/private/plugin"\n[graph]\nformat="graphviz"\ngraphconf="digraph demo {}"'
        report = legacy.analyze_legacy(toml, "legacy.toml")
        self.assertIn("custom_driver", {issue["code"] for issue in report["issues"]})
        self.assertNotIn("/secret/private/plugin", str(report))


if __name__ == "__main__":
    unittest.main()
