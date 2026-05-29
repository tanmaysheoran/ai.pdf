# Bundled font: DejaVu Sans

`DejaVuSans.ttf` is bundled as the default embedded font for the visible PDF
layer. It is distributed under the **DejaVu Fonts License** (a permissive,
redistributable license derived from the Bitstream Vera and Arev fonts).

- Project: https://dejavu-fonts.github.io/
- License: https://dejavu-fonts.github.io/License.html

Summary: the fonts and derivatives may be redistributed freely, including
bundled inside other software and embedded in documents, provided the font
software itself is not sold by itself. The full license text is available at
the URL above.

To embed a different face (e.g. a Noto CJK or RTL font), pass `--font <path.ttf>`
to `aipdf build`; the CID/Type0 embedding machinery is glyph-agnostic.
