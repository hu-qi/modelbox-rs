#!/usr/bin/env python3
"""Local-only, fixed-command ModelBox Studio bridge. Never expose on a public interface."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import pathlib
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOST, PORT = "127.0.0.1", 8765
MAX_BODY = 256 * 1024
TIMEOUT = 60


def cargo(*args):
    return subprocess.run(
        ["cargo", "run", "-q", "-p", "modelbox-cli", "--", *args],
        cwd=ROOT, text=True, capture_output=True, timeout=TIMEOUT, check=False,
    )


def invoke(action, payload):
    started = time.perf_counter()
    if action == "catalog":
        result = cargo("catalog", "--json")
    elif action in ("validate", "run", "run-profile"):
        graph = payload.get("graph")
        if not isinstance(graph, dict):
            return 400, {"error": "graph must be an object"}
        # The CLI remains authoritative for GraphSpec schema, ports and config.
        with tempfile.TemporaryDirectory(prefix="modelbox-studio-") as temp:
            path = pathlib.Path(temp) / "graph.json"
            path.write_text(json.dumps(graph), encoding="utf-8")
            if action in ("run", "run-profile"):
                value = payload.get("input")
                if not isinstance(value, str) or len(value) > 4096:
                    return 400, {"error": "input must be a text string of at most 4096 characters"}
                result = cargo(action, str(path), value)
            else:
                result = cargo("validate", str(path))
    else:
        return 404, {"error": "unknown action"}

    # End-to-end CLI invocation time, including process startup and (if cold)
    # compilation. This is NOT inference latency or per-node execution time.
    elapsed_ms = round((time.perf_counter() - started) * 1000, 3)
    diagnostics = {"elapsed_ms": elapsed_ms, "scope": "bridge_cli_wall_time"}
    if result.returncode:
        detail = {"error": (result.stderr or result.stdout)[-4096:], "diagnostics": diagnostics}
        for line in result.stderr.splitlines():
            if line.startswith("MODELBOX_ERROR_JSON:"):
                try:
                    parsed = json.loads(line.removeprefix("MODELBOX_ERROR_JSON:"))
                    if isinstance(parsed, dict) and isinstance(parsed.get("node_id"), str):
                        detail["node_id"] = parsed["node_id"]
                except json.JSONDecodeError:
                    pass
        return 422, detail
    if action == "catalog":
        return 200, json.loads(result.stdout)
    if action in ("run", "run-profile"):
        output = json.loads(result.stdout)
        if action == "run-profile":
            return 200, {"result": output["result"], "node_process": output["node_process"], "diagnostics": diagnostics}
        return 200, {"result": output, "diagnostics": diagnostics}
    return 200, {"message": result.stdout.strip(), "diagnostics": diagnostics}


class Handler(BaseHTTPRequestHandler):
    def reply(self, status, body):
        raw = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Access-Control-Allow-Origin", "http://127.0.0.1:5173")
        self.send_header("Vary", "Origin")
        self.end_headers()
        self.wfile.write(raw)

    def origin_allowed(self):
        return self.headers.get("Origin") in (None, "http://127.0.0.1:5173")

    def do_GET(self):
        if self.path != "/api/catalog" or not self.origin_allowed():
            self.reply(403, {"error": "forbidden"})
            return
        try:
            code, body = invoke("catalog", {})
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            code, body = 500, {"error": str(error)}
        self.reply(code, body)

    def do_POST(self):
        if self.path not in ("/api/validate", "/api/run", "/api/run-profile") or not self.origin_allowed():
            self.reply(403, {"error": "forbidden"})
            return
        # Localhost is still vulnerable to cross-site requests: require JSON and Origin.
        if self.headers.get("Content-Type", "").split(";")[0].strip() != "application/json":
            self.reply(415, {"error": "application/json required"})
            return
        try:
            size = int(self.headers.get("Content-Length", "-1"))
            if not (0 <= size <= MAX_BODY):
                self.reply(413, {"error": "request too large or missing length"})
                return
            payload = json.loads(self.rfile.read(size))
            if not isinstance(payload, dict):
                self.reply(400, {"error": "expected a JSON object"})
                return
            code, body = invoke(self.path.removeprefix("/api/"), payload)
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            code, body = 400, {"error": str(error)}
        self.reply(code, body)


if __name__ == "__main__":
    server = ThreadingHTTPServer((HOST, PORT), Handler)
    print(f"ModelBox Studio local bridge: http://{HOST}:{PORT} (Ctrl-C to stop)")
    server.serve_forever()
