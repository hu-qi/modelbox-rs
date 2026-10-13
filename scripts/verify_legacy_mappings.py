#!/usr/bin/env python3
"""CI contract check against the authoritative Rust FlowRegistry catalog."""
import json
import subprocess

from legacy_mappings import audit_against_catalog


def main():
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "modelbox-cli", "--", "catalog", "--json"],
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    )
    failures = audit_against_catalog(json.loads(result.stdout))
    if failures:
        for failure in failures:
            print("legacy mapping mismatch: " + failure)
        raise SystemExit(1)
    print("Reviewed legacy mapping names/ports/required options match Rust Registry.")


if __name__ == "__main__":
    main()
