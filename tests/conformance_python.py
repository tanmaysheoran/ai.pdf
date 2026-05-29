"""Conformance: the Python SDK reproduces the Rust-authored golden fixtures.

Run: .venv/bin/python tests/conformance_python.py
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk" / "python"))

from aipdf.core import xml_to_onto, xml_to_markdown  # noqa: E402

CASES = {
    "minimal": ROOT / "samples" / "minimal.xml",
    "rich": ROOT / "tests" / "conformance" / "rich.xml",
}

failures = 0
for name, xml_path in CASES.items():
    xml = xml_path.read_text()
    for fmt, fn in (("onto", xml_to_onto), ("md", xml_to_markdown)):
        got = fn(xml).rstrip()
        want = (ROOT / "tests" / "conformance" / f"{name}.{fmt}").read_text().rstrip()
        if got == want:
            print(f"OK {name}.{fmt}")
        else:
            failures += 1
            print(f"MISMATCH {name}.{fmt}")
            g, w = got.splitlines(), want.splitlines()
            for i in range(max(len(g), len(w))):
                a = g[i] if i < len(g) else "<none>"
                b = w[i] if i < len(w) else "<none>"
                if a != b:
                    print(f"  {i}: GOT {a!r}\n     WANT {b!r}")

if failures:
    print(f"FAILED ({failures} mismatches)")
    sys.exit(1)
print("Python SDK conformance OK")
