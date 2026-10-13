"""Strict, limited legacy ModelBox DOT/TOML -> native Rust GraphSpec importer.

This is a compatibility subset, NOT full ModelBox Graphviz or driver support.
Unsupported constructs raise LegacyImportError rather than silently changing semantics.
"""
import json
import re
import tomllib

from legacy_mappings import MAPPINGS, KNOWN_UNMAPPED


class LegacyImportError(ValueError):
    pass


IDENT = r"[A-Za-z_][A-Za-z0-9_-]*"
DOT_HEAD = re.compile(r"^\s*digraph\s+(" + IDENT + r")\s*\{(.*)\}\s*$", re.S | re.I)
NODE = re.compile(r"^(" + IDENT + r")\s*\[(.*)\]\s*$", re.S)
ENDPOINT = r"(" + IDENT + r")(?::(" + IDENT + r"))?"
EDGE = re.compile(r"^" + ENDPOINT + r"\s*->\s*" + ENDPOINT + r"$")
ATTRIBUTE = re.compile(r"^([A-Za-z_][\w-]*)\s*=\s*(.+)$", re.S)

# Explicit migration allowlist. New native FlowUnits must be reviewed before
# being added; legacy CPU plugin ABI is not compatible with Rust FlowUnit.
UNITS = {
    name: (tuple(entry["inputs"]), tuple(entry["outputs"]), tuple(entry["options"]))
    for name, entry in MAPPINGS.items()
}


def _split(source, separators):
    """Split only outside quoted DOT strings and attribute brackets."""
    items, chunk, quote, escaped, depth = [], [], False, False, 0
    i = 0
    while i < len(source):
        char = source[i]
        if quote:
            chunk.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quote = False
        elif char == '"':
            quote = True
            chunk.append(char)
        elif char == "[":
            depth += 1
            chunk.append(char)
        elif char == "]":
            depth -= 1
            if depth < 0:
                raise LegacyImportError("unmatched closing attribute bracket")
            chunk.append(char)
        elif (char == "#" or source[i:i+2] == "//") and depth == 0:
            while i < len(source) and source[i] not in "\r\n":
                i += 1
            continue
        elif char in separators and depth == 0:
            value = "".join(chunk).strip()
            if value:
                items.append(value)
            chunk = []
        else:
            chunk.append(char)
        i += 1
    if quote or depth != 0:
        raise LegacyImportError("unterminated DOT quoted value or attribute list")
    value = "".join(chunk).strip()
    if value:
        items.append(value)
    return items


def _parse_attrs(source):
    attrs = {}
    for item in _split(source, ","):
        match = ATTRIBUTE.fullmatch(item)
        if not match:
            raise LegacyImportError(f"unsupported DOT node attribute: {item}")
        key, value = match.groups()
        if key in attrs:
            raise LegacyImportError(f"duplicate attribute: {key}")
        value = value.strip()
        if value.startswith('"'):
            try:
                parsed = json.loads(value)
            except json.JSONDecodeError as error:
                raise LegacyImportError(f"invalid quoted attribute {key}: {error}") from error
            if not isinstance(parsed, str):
                raise LegacyImportError(f"attribute {key} must be a string")
            attrs[key] = parsed
        elif re.fullmatch(r"[A-Za-z0-9_.:/+-]+", value):
            attrs[key] = value
        else:
            raise LegacyImportError(f"unsupported unquoted attribute {key}: {value}")
    return attrs


def _read_graph_text(text, filename):
    if filename.lower().endswith(".dot"):
        return text
    if not filename.lower().endswith((".toml", ".conf")):
        raise LegacyImportError("expected a .dot, .toml or .conf ModelBox graph")
    try:
        document = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise LegacyImportError(f"invalid TOML: {error}") from error
    if not isinstance(document, dict) or set(document) - {"graph", "driver"}:
        raise LegacyImportError("only [graph] and an empty [driver] are supported")
    graph = document.get("graph")
    if not isinstance(graph, dict) or set(graph) - {"format", "graphconf"}:
        raise LegacyImportError("[graph] requires only format and graphconf")
    if graph.get("format") != "graphviz" or not isinstance(graph.get("graphconf"), str):
        raise LegacyImportError("[graph] requires format=\"graphviz\" and string graphconf")
    driver = document.get("driver", {})
    if not isinstance(driver, dict) or set(driver) - {"dir"} or driver.get("dir", "") != "":
        raise LegacyImportError("nonempty or custom legacy driver configuration is unsupported")
    return graph["graphconf"]


def convert_legacy(text, filename):
    if not isinstance(text, str) or len(text.encode("utf-8")) > 128 * 1024:
        raise LegacyImportError("legacy graph must be a UTF-8 text document up to 128 KiB")
    source = _read_graph_text(text, filename)
    match = DOT_HEAD.fullmatch(source)
    if not match:
        raise LegacyImportError("expected simple 'digraph NAME { ... }' (no subgraphs)")
    name, body = match.groups()
    records = _split(body, ";\n\r")
    nodes = {}
    node_contracts = {}
    edges = []
    for statement in records:
        if "->" in statement:
            match_edge = EDGE.fullmatch(statement)
            if not match_edge:
                raise LegacyImportError(f"unsupported DOT edge: {statement}")
            edges.append(match_edge.groups())
            continue
        match_node = NODE.fullmatch(statement)
        if not match_node:
            raise LegacyImportError(f"unsupported DOT statement: {statement}")
        node_id, attr_text = match_node.groups()
        if node_id in nodes:
            raise LegacyImportError(f"duplicate node: {node_id}")
        attrs = _parse_attrs(attr_text)
        kind = attrs.pop("type", None)
        if kind == "input":
            if attrs:
                raise LegacyImportError(f"unsupported input settings at {node_id}: {sorted(attrs)}")
            unit, config = "core.input", {}
        elif kind == "output":
            if attrs:
                raise LegacyImportError(f"unsupported output settings at {node_id}: {sorted(attrs)}")
            unit, config = "core.output", {}
        elif kind == "flowunit":
            legacy_unit = attrs.pop("flowunit", None)
            mapping = MAPPINGS.get(legacy_unit)
            if mapping is None:
                hint = KNOWN_UNMAPPED.get(legacy_unit)
                reason = "known legacy operator needs a separate Rust implementation" if hint else "no reviewed Rust mapping"
                raise LegacyImportError(
                    f"unsupported legacy FlowUnit {legacy_unit!r} at {node_id}: {reason}"
                )
            if attrs.pop("device", "cpu") != mapping["device"]:
                raise LegacyImportError(f"non-CPU device at node {node_id} is unsupported")
            if attrs.pop("deviceid", "0") != "0":
                raise LegacyImportError(f"nonzero deviceid at {node_id} is unsupported")
            options = mapping["options"]
            allowed = set(options)
            if set(attrs) - allowed:
                raise LegacyImportError(
                    f"unsupported FlowUnit options at {node_id}: {sorted(set(attrs) - allowed)}"
                )
            required = {key for key, value in options.items() if value["required"]}
            if required - set(attrs):
                raise LegacyImportError(f"missing required options at {node_id}: {sorted(required - set(attrs))}")
            config = {spec["rust_key"]: attrs[key] for key, spec in options.items() if key in attrs}
            unit = mapping["rust_name"]
            node_contracts[node_id] = mapping
        else:
            raise LegacyImportError(f"unsupported or missing node type at {node_id}: {kind!r}")
        nodes[node_id] = {"id": node_id, "unit": unit, **({"config": config} if config else {})}
    if not nodes or not edges:
        raise LegacyImportError("legacy graph needs declared nodes and explicit edges")
    compiled_edges = []
    for from_id, from_port, to_id, to_port in edges:
        if from_id not in nodes or to_id not in nodes:
            raise LegacyImportError(f"edge references undeclared node: {from_id} -> {to_id}")
        source_unit, dest_unit = nodes[from_id]["unit"], nodes[to_id]["unit"]
        if source_unit == "core.output" or dest_unit == "core.input":
            raise LegacyImportError(f"reversed input/output direction: {from_id} -> {to_id}")
        source_map = {"out": "out"} if source_unit == "core.input" else node_contracts[from_id]["outputs"]
        target_map = {"in": "in"} if dest_unit == "core.output" else node_contracts[to_id]["inputs"]
        source_ports = tuple(source_map)
        target_ports = tuple(target_map)
        if from_port is None:
            if len(source_ports) != 1:
                raise LegacyImportError(f"explicit source port required at {from_id}")
            from_port = source_ports[0]
        if to_port is None:
            if len(target_ports) != 1:
                raise LegacyImportError(f"explicit destination port required at {to_id}")
            to_port = target_ports[0]
        if from_port not in source_ports or to_port not in target_ports:
            raise LegacyImportError(
                f"unsupported edge ports {from_id}:{from_port} -> {to_id}:{to_port}"
            )
        compiled_edges.append({
            "from": f"{from_id}.{source_map[from_port]}",
            "to": f"{to_id}.{target_map[to_port]}",
        })
    graph = {
        "name": name,
        "queue_capacity": 16,
        "nodes": list(nodes.values()),
        "edges": compiled_edges,
    }
    return graph, {
        "source_format": "modelbox-graphviz",
        "migration_scope": "cpu-text-subset",
        "mapping_schema_version": 1,
        "mapped_flowunits": sorted({node["unit"] for node in nodes.values() if node["unit"] not in ("core.input", "core.output")}),
        "node_count": len(nodes),
        "edge_count": len(compiled_edges),
        "requires_runtime_validation": True,
        "notes": [
            "No legacy plugin or device drivers are loaded.",
            "ModelBox Rust currently supports one source and acyclic graphs.",
            "The Rust Runtime must validate ports, types, configs and graph topology before use.",
        ],
    }


def analyze_legacy(text, filename):
    """Read-only discovery of migration blockers; no driver/FlowUnit execution.

    The report deliberately contains node IDs and option NAMES only; it never
    echoes configuration values, which may contain credentials or URLs.
    """
    issues = []
    nodes = []
    edges = []
    graph_text = None

    def issue(code, detail, node_id=None):
        if len(issues) < 100:
            entry = {"code": code, "detail": detail}
            if node_id is not None:
                entry["node_id"] = node_id
            if entry not in issues:
                issues.append(entry)

    if not isinstance(text, str) or len(text.encode("utf-8")) > 128 * 1024:
        raise LegacyImportError("legacy graph must be UTF-8 text up to 128 KiB")
    if filename.lower().endswith(".dot"):
        graph_text = text
    elif filename.lower().endswith((".toml", ".conf")):
        try:
            doc = tomllib.loads(text)
        except tomllib.TOMLDecodeError as error:
            raise LegacyImportError(f"invalid TOML: {error}") from error
        other_sections = sorted(set(doc) - {"graph", "driver"})
        if other_sections:
            issue("unsupported_sections", f"additional TOML sections: {other_sections}")
        driver = doc.get("driver", {})
        if not isinstance(driver, dict) or set(driver) - {"dir"} or driver.get("dir", "") != "":
            issue("custom_driver", "external driver configuration requires native implementation")
        graph = doc.get("graph", {})
        if not isinstance(graph, dict):
            issue("invalid_graph_section", "expected [graph] table")
        else:
            extra_keys = sorted(set(graph) - {"format", "graphconf"})
            if extra_keys:
                issue("unsupported_graph_options", f"additional graph option names: {extra_keys}")
            if graph.get("format") != "graphviz":
                issue("unsupported_graph_format", 'expected format="graphviz"')
            graph_text = graph.get("graphconf")
    else:
        raise LegacyImportError("expected .dot, .toml or .conf")

    if not isinstance(graph_text, str):
        issue("missing_graphconf", "[graph].graphconf must contain DOT text")
    else:
        match = DOT_HEAD.fullmatch(graph_text)
        if not match:
            issue("unsupported_dot", "expected simple digraph NAME { ... } without subgraphs")
        else:
            try:
                statements = _split(match.group(2), ";\n\r")
            except LegacyImportError:
                statements = []
                issue("dot_syntax", "malformed DOT statement delimiters or quoting")
            seen = set()
            for statement in statements:
                if "->" in statement:
                    match_edge = EDGE.fullmatch(statement)
                    if not match_edge:
                        issue("unsupported_edge", "edge must be a simple from:port -> to:port")
                    else:
                        from_id, from_port, to_id, to_port = match_edge.groups()
                        edges.append({"from": from_id, "to": to_id})
                    continue
                match_node = NODE.fullmatch(statement)
                if not match_node:
                    issue("unsupported_statement", "DOT statement outside the supported node/edge subset")
                    continue
                node_id, raw_attrs = match_node.groups()
                if node_id in seen:
                    issue("duplicate_node", "duplicate node declaration", node_id)
                seen.add(node_id)
                try:
                    attrs = _parse_attrs(raw_attrs)
                except LegacyImportError:
                    issue("unsupported_attrs", "invalid or unsupported node attribute format", node_id)
                    continue
                kind = attrs.get("type")
                unit = attrs.get("flowunit") if kind == "flowunit" else (
                    "core.input" if kind == "input" else "core.output" if kind == "output" else None
                )
                device = attrs.get("device", "cpu")
                nodes.append({
                    "id": node_id, "type": kind, "flowunit": unit, "device": device,
                    "mapping_status": "reviewed_fixture_subset" if unit in MAPPINGS else "requires_rust_implementation",
                    "rust_unit": MAPPINGS[unit]["rust_name"] if unit in MAPPINGS else None,
                })
                if kind not in ("input", "output", "flowunit"):
                    issue("node_type", "unsupported node type", node_id)
                elif kind == "flowunit":
                    if unit not in UNITS:
                        issue("unmapped_flowunit", f"no compatible Rust FlowUnit mapping for {unit!r}", node_id)
                    if device != "cpu" or attrs.get("deviceid", "0") != "0":
                        issue("device", "non-CPU or nonzero-deviceid driver unsupported", node_id)
                    allowed = {"type", "flowunit", "device", "deviceid"}
                    if unit in UNITS:
                        allowed.update(UNITS[unit][2])
                        missing = sorted(set(UNITS[unit][2]) - set(attrs))
                        if missing:
                            issue("missing_config", f"required option names: {missing}", node_id)
                    extra = sorted(set(attrs) - allowed)
                    if extra:
                        issue("unmapped_options", f"unsupported option names: {extra}", node_id)
                elif set(attrs) != {"type"}:
                    issue("node_options", "input/output supports only type", node_id)
            declared = {node["id"] for node in nodes}
            for edge in edges:
                if edge["from"] not in declared or edge["to"] not in declared:
                    issue("undeclared_endpoint", "edge references undeclared node")

    # This strict converter is the final syntactic gate. Even a clean report
    # remains only a candidate: Rust Runtime validation is still required.
    if not issues:
        try:
            convert_legacy(text, filename)
        except LegacyImportError as error:
            issue("unsupported_semantics", "DOT structure, edge ports or graph settings exceed the supported migration subset")
    return {
        "source_format": "modelbox-graphviz",
        "node_count": len(nodes),
        "edge_count": len(edges),
        "nodes": nodes[:100],
        "issues": issues,
        "candidate_convertible": len(issues) == 0,
        "requires_runtime_validation": True,
        "inspection_only": True,
    }
