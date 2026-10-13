"""Regression tests for safe batch legacy inventory and its CLI exit status."""
import contextlib
import io
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from legacy_audit import audit_paths, main, MAX_BYTES


class LegacyBatchAuditTests(unittest.TestCase):
    def test_project_directory_counts_candidates_and_multiple_blockers(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            shutil.copyfile(ROOT / "examples/legacy_modelbox_hello.dot", root / "one.dot")
            (root / "sub").mkdir()
            (root / "sub" / "bad.conf").write_text(
                "[graph]\nformat='graphviz'\ngraphconf='''digraph x { "
                "a[type=input]; detector[type=flowunit,flowunit=model_detect,device=cuda,credential=\"SECRET-KEY\"]; "
                "z[type=output]; a -> detector:in; detector:out -> z; }'''\n"
            )
            report = audit_paths([root])
            self.assertEqual(report["total_files"], 2)
            self.assertEqual(report["candidate_files"], 1)
            self.assertEqual(report["blocked_files"], 1)
            self.assertEqual(report["unmapped_flowunits"]["model_detect"], 1)
            self.assertEqual(report["devices"]["cuda"], 1)
            self.assertEqual(report["files"][0]["file"], "one.dot")
            self.assertEqual(report["files"][1]["file"], "sub/bad.conf")
            self.assertNotIn("SECRET-KEY", str(report))
            self.assertTrue(report["inspection_only"])
            self.assertTrue(report["requires_runtime_validation"])

    def test_large_or_invalid_utf8_file_is_blocked_without_content(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "huge.dot").write_bytes(b"A" * (MAX_BYTES + 1))
            (root / "utf8.dot").write_bytes(b"\xff\xffSECRET")
            report = audit_paths([root])
            self.assertEqual(report["blocked_files"], 2)
            self.assertEqual(report["issue_counts"]["unreadable_or_invalid"], 2)
            self.assertNotIn("SECRET", str(report))

    def test_explicit_input_and_duplicate_file_not_scanned_twice(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture = root / "hello.dot"
            shutil.copyfile(ROOT / "examples/legacy_modelbox_hello.dot", fixture)
            result = audit_paths([fixture, root])
            self.assertEqual(result["total_files"], 1)
            self.assertEqual(result["candidate_files"], 1)

    def test_invalid_inputs_and_empty_directory_fail_explicitly(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaisesRegex(ValueError, "no .dot"):
                audit_paths([root])
            with self.assertRaisesRegex(ValueError, "input does not exist"):
                audit_paths([root / "missing"])

    def test_exit_status_can_gate_on_blockers(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "bad.dot").write_text(
                "digraph x { a[type=input]; x[type=flowunit,flowunit=unknown]; "
                "z[type=output]; a -> x:in; x:out -> z; }"
            )
            with contextlib.redirect_stdout(io.StringIO()) as output:
                code = main([str(root), "--json", "--fail-on-blockers"])
            self.assertEqual(code, 2)
            self.assertIn('"blocked_files": 1', output.getvalue())
            self.assertNotIn('"candidate_convertible": true', output.getvalue())

    def test_symbolic_link_inputs_are_never_followed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture = root / "hello.dot"
            shutil.copyfile(ROOT / "examples/legacy_modelbox_hello.dot", fixture)
            link = root / "indirect.dot"
            link.symlink_to(fixture)
            report = audit_paths([root])
            self.assertEqual(report["total_files"], 1)
            with self.assertRaisesRegex(ValueError, "symlink input"):
                audit_paths([link])


if __name__ == "__main__":
    unittest.main()
