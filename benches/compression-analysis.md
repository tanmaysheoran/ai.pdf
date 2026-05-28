# Compression Analysis

The V1 semantic layer is intended to be compact enough for archival use.

## Method

For each document:

1. Build the visible PDF.
2. Serialize deterministic semantic XML.
3. Brotli-compress the XML.
4. Embed it as `aipdf-semantic.xml.br`.
5. Compare final `.aipdf` size with the ordinary PDF baseline.

## Prototype Sample

| Artifact | Size | Notes |
| --- | ---: | --- |
| semantic XML | 1,107 bytes | normalized XML extracted from sample |
| Brotli semantic payload | 435 bytes | embedded bytes |
| `.aipdf` sample | 2,576 bytes | valid PDF 1.7, one page |
| semantic compression ratio | 0.393 | compressed semantic bytes / XML bytes |

Small one-page samples may exceed the percentage target because PDF object overhead dominates. The percentage target is meaningful for realistic multi-page documents where page content is larger than the semantic attachment.

## Benchmark Comparisons

| Format | Compatibility | Machine structure | Typical overhead |
| --- | --- | --- | --- |
| Standard PDF | excellent | weak unless tagged | baseline |
| Tagged PDF | excellent | moderate | low to medium |
| OCR PDF | excellent | text layer only | high |
| DjVu | limited in modern browsers | text layer possible | low |
| `.aipdf` V1 | ordinary PDF compatible | explicit XML tree | target <3%, max <10% |
