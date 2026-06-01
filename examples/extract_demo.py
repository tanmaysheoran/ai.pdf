from pathlib import Path

from aipdf import AIPDF


doc = AIPDF.open(Path("samples/minimal.ai.pdf"))
print(doc.to_markdown())

