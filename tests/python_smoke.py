from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk" / "python"))

from aipdf import AIPDF


doc = AIPDF.open(Path(__file__).resolve().parents[1] / "samples" / "minimal.pdf")
assert doc.has_semantic_layer
assert "# Introduction" in doc.to_markdown()
assert "Blocks[" in doc.to_onto()
assert len(doc.get_tables()) == 1
assert len(doc.find_citations()) == 1

maximal = AIPDF.open(Path(__file__).resolve().parents[1] / "samples" / "maximal.pdf")
assert maximal.has_semantic_layer
assert "Mathematical Compression Model" in maximal.to_markdown()
assert "Mathematical Compression Model" in maximal.to_onto()
assert "Figures[1]" in maximal.to_onto()
assert len(maximal.get_tables()) == 1
