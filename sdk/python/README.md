# aipdf Python SDK

```python
from aipdf import AIPDF

doc = AIPDF.open("samples/minimal.aipdf")
print(doc.has_semantic_layer)
print(doc.to_markdown())
```

