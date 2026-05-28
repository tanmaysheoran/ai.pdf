# aipdf TypeScript SDK

```ts
import { AIPDF } from "@aipdf/sdk";

const doc = AIPDF.open("samples/minimal.aipdf");
console.log(doc.hasSemanticLayer);
console.log(doc.toMarkdown());
```

