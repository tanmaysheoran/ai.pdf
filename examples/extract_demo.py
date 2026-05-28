from pathlib import Path

from aipdf import AIPDF


doc = AIPDF.open(Path("samples/minimal.aipdf"))
print(doc.to_markdown())

