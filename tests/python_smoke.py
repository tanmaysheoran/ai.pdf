from pathlib import Path
import os
import shutil
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk" / "python"))

import aipdf
from aipdf import AIPDF

ROOT = Path(__file__).resolve().parents[1]

doc = AIPDF.open(ROOT / "samples" / "minimal.pdf")
assert doc.has_semantic_layer
assert "# Introduction" in doc.to_markdown()
assert "Blocks[" in doc.to_onto()
assert len(doc.get_tables()) == 1
assert len(doc.find_citations()) == 1

# Native inspect (byte counts), validate, markdown-ast — no CLI binary needed.
rep = doc.inspect()
assert rep.is_pdf and rep.has_semantic_layer
assert rep.semantic_compressed_bytes and rep.semantic_xml_bytes
assert doc.validate() is True
assert '"type": "root"' in doc.to_markdown_ast()
assert aipdf.inspect_pdf((ROOT / "samples" / "minimal.pdf").read_bytes()) == rep

maximal = AIPDF.open(ROOT / "samples" / "maximal.pdf")
assert maximal.has_semantic_layer
assert "Mathematical Compression Model" in maximal.to_markdown()
assert "Mathematical Compression Model" in maximal.to_onto()
assert "Figures[1]" in maximal.to_onto()
assert len(maximal.get_tables()) == 1

# CLI-delegating helpers: only exercised when the aipdf binary is reachable.
_bin = shutil.which(aipdf.aipdf_binary()) or (
    str(ROOT / "target" / "debug" / "aipdf")
    if (ROOT / "target" / "debug" / "aipdf").exists()
    else None
)
if _bin:
    os.environ["AIPDF_BIN"] = _bin
    d = tempfile.mkdtemp()
    out = aipdf.build(ROOT / "samples" / "minimal.xml", Path(d) / "m.ai.pdf", render="minimal")
    assert out.exists()
    assert "aipdf_bytes" in aipdf.bench(ROOT / "samples" / "minimal.xml")
    res = aipdf.export(ROOT / "samples" / "minimal.pdf", "markdown", d)
    assert res.output.exists()
    print("python_smoke: CLI-delegation OK")
else:
    print("python_smoke: aipdf binary not found — skipped CLI-delegation checks")
