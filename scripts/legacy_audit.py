#!/usr/bin/env python3
"""Read-only, deterministic inventory for legacy ModelBox DOT/TOML projects.

No code/FlowUnit/driver execution, no Rust imports, no mutations. Diagnostics
contain option NAMES only; arbitrary values and file contents are not printed.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import sys

from legacy_import import LegacyImportError, analyze_legacy

EXTENSIONS = frozenset({".dot", ".toml", ".conf"})
MAX_FILES = 512
MAX_BYTES = 128 * 1024


def _discover(paths):
    discovered = {}
    for raw in paths:
        root = Path(raw)
        if root.is_symlink():
            raise ValueError(f"symlink input is unsupported: {root.name}")
        if root.is_file():
            if root.suffix.lower() not in EXTENSIONS:
                raise ValueError(f"unsupported file extension: {root.name}")
            discovered.setdefault(str(root.resolve()), (root, root.name))
            if len(discovered) > MAX_FILES:
                raise ValueError(f"too many legacy files (maximum {MAX_FILES})")
            continue
        if not root.is_dir():
            raise ValueError(f"input does not exist or is not a file/directory: {root.name}")
        for directory, dirs, files in os.walk(root, followlinks=False):
            dirs[:] = sorted(name for name in dirs if not (Path(directory) / name).is_symlink())
            for name in sorted(files):
                item = Path(directory) / name
                if item.suffix.lower() not in EXTENSIONS or item.is_symlink():
                    continue
                label = str(item.relative_to(root))
                discovered.setdefault(str(item.resolve()), (item, label))
                if len(discovered) > MAX_FILES:
                    raise ValueError(f"too many legacy files (maximum {MAX_FILES})")
        if len(discovered) > MAX_FILES:
            raise ValueError(f"too many legacy files (maximum {MAX_FILES})")
    if not discovered:
        raise ValueError("no .dot, .toml or .conf files found")
    return sorted(discovered.values(), key=lambda item: (item[1], str(item[0])))


def audit_paths(paths):
    files = []
    issues_count = Counter()
    unmapped = Counter()
    mapped = Counter()
    devices = Counter()
    for source, label in _discover(paths):
        try:
            with source.open("rb") as handle:
                raw = handle.read(MAX_BYTES + 1)
            if len(raw) > MAX_BYTES:
                raise LegacyImportError("file exceeds legacy size limit")
            content = raw.decode("utf-8")
            report = analyze_legacy(content, source.name)
            blockers = report["issues"]
            for node in report["nodes"]:
                if node.get("type") != "flowunit":
                    continue
                name = node.get("flowunit") or "(missing)"
                if node.get("mapping_status") == "reviewed_fixture_subset":
                    mapped[name] += 1
                else:
                    unmapped[name] += 1
                devices[node.get("device") or "(missing)"] += 1
            files.append({
                "file": label,
                "candidate_convertible": report["candidate_convertible"],
                "node_count": report["node_count"],
                "edge_count": report["edge_count"],
                "issues": blockers,
            })
        except (OSError, UnicodeError, LegacyImportError):
            blockers = [{"code": "unreadable_or_invalid", "detail": "file could not be read or parsed as supported UTF-8 DOT/TOML (128 KiB limit)"}]
            files.append({
                "file": label, "candidate_convertible": False,
                "node_count": 0, "edge_count": 0, "issues": blockers,
            })
        issues_count.update(issue["code"] for issue in blockers)
    candidate_count = sum(item["candidate_convertible"] for item in files)
    return {
        "schema_version": 1,
        "inspection_only": True,
        "requires_runtime_validation": True,
        "scope": "project-fixture-cpu-text-subset",
        "total_files": len(files),
        "candidate_files": candidate_count,
        "blocked_files": len(files) - candidate_count,
        "unmapped_flowunits": dict(sorted(unmapped.items())),
        "reviewed_fixture_flowunits": dict(sorted(mapped.items())),
        "devices": dict(sorted(devices.items())),
        "issue_counts": dict(sorted(issues_count.items())),
        "files": files,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Read-only inventory of legacy ModelBox Graphviz/TOML configs")
    parser.add_argument("paths", nargs="+", help="One or more .dot/.toml/.conf files or project directories")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable structured report")
    parser.add_argument("--fail-on-blockers", action="store_true", help="Return code 2 if any file has blockers")
    args = parser.parse_args(argv)
    try:
        report = audit_paths(args.paths)
    except ValueError as error:
        print(f"legacy inventory: {error}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        print(f"Legacy files: {report['total_files']} | candidates: {report['candidate_files']} | blocked: {report['blocked_files']}")
        print("Preflight only: candidates must still pass native Rust validation.")
        for item in report["files"]:
            flag = "candidate" if item["candidate_convertible"] else "blocked"
            problems = ", ".join(sorted({issue["code"] for issue in item["issues"]}))
            print(f"  {flag}: {item['file']}" + (f" ({problems})" if problems else ""))
        if report["unmapped_flowunits"]:
            print("Unmapped legacy FlowUnits:", json.dumps(report["unmapped_flowunits"], sort_keys=True))
    return 2 if args.fail_on_blockers and report["blocked_files"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
