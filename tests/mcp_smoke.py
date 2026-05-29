"""Smoke test for the aipdf MCP stdio server.

Drives the server over stdin/stdout with JSON-RPC messages and checks the
handshake plus each tool. Run: .venv/bin/python tests/mcp_smoke.py
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = ROOT / "samples" / "minimal.pdf"

messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},  # notification: no reply
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
     "params": {"name": "aipdf_inspect", "arguments": {"path": str(SAMPLE)}}},
    {"jsonrpc": "2.0", "id": 4, "method": "tools/call",
     "params": {"name": "aipdf_extract", "arguments": {"path": str(SAMPLE), "format": "onto"}}},
    {"jsonrpc": "2.0", "id": 5, "method": "tools/call",
     "params": {"name": "aipdf_extract", "arguments": {"path": "/no/such/file.pdf"}}},
    {"jsonrpc": "2.0", "id": 6, "method": "tools/call",
     "params": {"name": "aipdf_validate", "arguments": {"path": str(SAMPLE)}}},
    {"jsonrpc": "2.0", "id": 7, "method": "tools/call",
     "params": {"name": "aipdf_extract", "arguments": {"path": str(SAMPLE), "format": "markdown-ast"}}},
]
stdin_data = "".join(json.dumps(m) + "\n" for m in messages)

env = {"PYTHONPATH": str(ROOT / "sdk" / "python")}
proc = subprocess.run(
    [sys.executable, "-m", "aipdf.mcp_server"],
    input=stdin_data, capture_output=True, text=True, env={**env}, cwd=str(ROOT),
)
assert proc.returncode == 0, f"server crashed: {proc.stderr}"

responses = {}
for line in proc.stdout.splitlines():
    if line.strip():
        msg = json.loads(line)
        responses[msg.get("id")] = msg

# initialize handshake
init = responses[1]["result"]
assert init["serverInfo"]["name"] == "aipdf"
assert "tools" in init["capabilities"]
print("OK initialize")

# tools/list — full CLI-parity surface
tools = {t["name"] for t in responses[2]["result"]["tools"]}
expected = {
    "aipdf_inspect", "aipdf_extract", "aipdf_reading_order", "aipdf_validate",
    "aipdf_build", "aipdf_extract_images", "aipdf_convert", "aipdf_bench",
}
assert expected <= tools, (expected - tools, tools)
print("OK tools/list:", sorted(tools))

# inspect — now reports byte counts too
inspect_text = responses[3]["result"]["content"][0]["text"]
inspect = json.loads(inspect_text)
assert inspect["is_pdf"] and inspect["has_semantic_layer"], inspect
assert inspect["semantic_compressed_bytes"] and inspect["semantic_xml_bytes"], inspect
print("OK aipdf_inspect")

# extract onto
onto = responses[4]["result"]["content"][0]["text"]
assert "Document[1]:" in onto and "Blocks[" in onto, onto[:80]
print("OK aipdf_extract(onto)")

# error path surfaces as isError result, not a crash
err = responses[5]["result"]
assert err.get("isError") is True, err
print("OK error handling")

# validate
val = json.loads(responses[6]["result"]["content"][0]["text"])
assert val["valid"] is True, val
print("OK aipdf_validate")

# extract markdown-ast
ast = responses[7]["result"]["content"][0]["text"]
assert '"type": "root"' in ast, ast[:80]
print("OK aipdf_extract(markdown-ast)")

print("MCP server smoke OK")
