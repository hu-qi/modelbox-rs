"""Reviewed fixture-scope legacy FlowUnit mapping contracts, not a plugin ABI."""
import json
from pathlib import Path

CONTRACT = Path(__file__).with_name("legacy_flowunit_mappings.json")


def load_contract():
    data = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1 or not isinstance(data.get("flowunits"), list):
        raise ValueError("unsupported legacy mapping contract version")
    names = set()
    for entry in data["flowunits"]:
        name = entry["legacy_name"]
        if name in names:
            raise ValueError(f"duplicate legacy mapping: {name}")
        names.add(name)
        if entry.get("device") != "cpu" or not entry.get("rust_name"):
            raise ValueError(f"invalid legacy mapping device or target: {name}")
        for port_key in ("inputs", "outputs"):
            if not isinstance(entry.get(port_key), dict) or not entry[port_key]:
                raise ValueError(f"missing explicit {port_key} mapping: {name}")
            if len(set(entry[port_key].values())) != len(entry[port_key]):
                raise ValueError(f"ambiguous {port_key} mapping: {name}")
        for old_key, option in entry["options"].items():
            if option.get("type") != "string" or not option.get("rust_key"):
                raise ValueError(f"unverified config mapping: {name}.{old_key}")
    return data


CONTRACT_DATA = load_contract()
MAPPINGS = {entry["legacy_name"]: entry for entry in CONTRACT_DATA["flowunits"]}
KNOWN_UNMAPPED = {entry["legacy_name"]: entry for entry in CONTRACT_DATA["known_unmapped"]}


def audit_against_catalog(catalog):
    """Fail CI if an allowlisted target diverges from the real Rust Registry."""
    descriptors = {item["name"]: item for item in catalog}
    failures = []
    for old_name, mapping in MAPPINGS.items():
        rust_name = mapping["rust_name"]
        desc = descriptors.get(rust_name)
        if desc is None:
            failures.append(f"{old_name}: Rust Registry missing {rust_name}")
            continue
        for side in ("inputs", "outputs"):
            actual = {p["name"]: p["data_type"] for p in desc[side]}
            mapped = set(mapping[side].values())
            if mapped != set(actual):
                failures.append(f"{old_name}: {side} ports mismatch: mapped={sorted(mapped)} actual={sorted(actual)}")
            for name in mapped & set(actual):
                if actual[name] != "text":
                    failures.append(f"{old_name}: {side} port {name} is not a text port")
        required = set(desc["required_string_config_keys"])
        mapped_required = {
            option["rust_key"]
            for option in mapping["options"].values()
            if option.get("required")
        }
        if required != mapped_required:
            failures.append(f"{old_name}: required string configuration differs")
        if desc["is_source"]:
            failures.append(f"{old_name}: cannot import a source as a FlowUnit")
    return failures
