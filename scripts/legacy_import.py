"""Strict, limited legacy ModelBox DOT/TOML -> native Rust GraphSpec importer.

This is a compatibility subset, NOT full ModelBox Graphviz or driver support.
Unsupported constructs raise LegacyImportError rather than silently changing semantics.
"""
import json
import re
import tomllib


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
    "text.uppercase": (("in",), ("out",), ()),
    "text.prefix": (("in",), ("out",), ("prefix",)),
    "text.concat": (("left", "right"), ("out",), ()),
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
            unit = attrs.pop("flowunit", None)
            if unit not in UNITS:
                raise LegacyImportError(
                    f"unsupported legacy FlowUnit {unit!r} at {node_id}; "
                    "only CPU text.uppercase, text.prefix and text.concat can be migrated"
                )
            if attrs.pop("device", "cpu") != "cpu":
                raise LegacyImportError(f"non-CPU device at node {node_id} is unsupported")
            if attrs.pop("deviceid", "0") != "0":
                raise LegacyImportError(f"nonzero deviceid at {node_id} is unsupported")
            allowed = set(UNITS[unit][2])
            if set(attrs) - allowed:
                raise LegacyImportError(
                    f"unsupported FlowUnit options at {node_id}: {sorted(set(attrs) - allowed)}"
                )
            if allowed - set(attrs):
                raise LegacyImportError(f"missing required options at {node_id}: {sorted(allowed - set(attrs))}")
            config = attrs
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
        source_ports = ("out",) if source_unit == "core.input" else UNITS[source_unit][1]
        target_ports = ("in",) if dest_unit == "core.output" else UNITS[dest_unit][0]
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
        compiled_edges.append({"from": f"{from_id}.{from_port}", "to": f"{to_id}.{to_port}"})
    graph = {
        "name": name,
        "queue_capacity": 16,
        "nodes": list(nodes.values()),
        "edges": compiled_edges,
    }
    return graph, {
        "source_format": "modelbox-graphviz",
        "migration_scope": "cpu-text-subset",
        "node_count": len(nodes),
        "edge_count": len(compiled_edges),
        "requires_runtime_validation": True,
        "notes": [
            "No legacy plugin or device drivers are loaded.",
            "ModelBox Rust currently supports one source and acyclic graphs.",
            "The Rust Runtime must validate ports, types, configs and graph topology before use.",
        ],
    }
