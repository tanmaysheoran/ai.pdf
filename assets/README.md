# ai.pdf logo assets

Stacked-layers mark, blue accent. Front card = the visible PDF page;
the offset card behind it = the embedded semantic layer.

| File | Use |
|------|-----|
| `logo-light.svg` | Full lockup (icon + wordmark) on light backgrounds |
| `logo-dark.svg`  | Full lockup on dark backgrounds |
| `logo-mark-light.svg` | Icon only — light bg (favicon / avatar) |
| `logo-mark-dark.svg`  | Icon only — dark bg |

## README header (auto light/dark on GitHub)

```html
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="ai.pdf" width="300">
  </picture>
</div>
```

> The wordmark uses **JetBrains Mono** with a system-mono fallback, so it renders
> everywhere. For a pixel-locked banner, render either SVG to PNG at 2–3×.

Colors — ink `#16161a` / `#f4f4f1`, accent blue `#2f7ed6` (light) · `#4f95e6` (dark).
