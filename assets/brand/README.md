# aisw brand assets

The AI Switcher mark: a terminal prompt between two switch arrows, on a
periwinkle tile. Use these files rather than redrawing or recoloring the mark.

| File | Use it for |
| --- | --- |
| `aisw-mark.svg` | Master. Any size from 48 px up: READMEs, docs, slides, app listings. |
| `aisw-mark-small.svg` | 32 px and below. Drops the arrows so the prompt stays legible. |
| `aisw-glyph-dark.svg` | The mark without its tile, in ink, for light backgrounds. |
| `aisw-glyph-light.svg` | The mark without its tile, in light gray, for dark backgrounds. |
| `png/aisw-mark-{16…1024}.png` | Raster mark, transparent corners. 16 and 32 come from the small mark. |
| `png/aisw-glyph-{dark,light}-512.png` | Raster glyph for tools that cannot take SVG. |
| `favicon.ico` | Browser favicon, 16, 32 and 48 px in one file. |
| `apple-touch-icon.png` | iOS home screen, 180 px, full-bleed square (iOS rounds it). |
| `icon-maskable-512.png` | Android and PWA `purpose: "maskable"`, artwork inside the safe zone. |

## Colors

| Role | Hex |
| --- | --- |
| Tile periwinkle | `#7c8cff` |
| Ink | `#0b0d10` |
| Light glyph | `#e8eaed` |

## Rules

- Keep clear space around the tile of at least a quarter of its width.
- Use the tile mark on any background; use a glyph only where the tile does
  not fit, and pick the variant that contrasts with the background.
- Do not stretch, rotate, recolor, add shadows, or place the mark on a
  periwinkle background.

## Where it is used

- `README.md` header: `png/aisw-mark-256.png`, shown at 96 px.
- Docs site (`website/public`): `aisw-logo.png`, `aisw-512.png`,
  `aisw-192.png`, `apple-touch-icon.png`, `favicon-16.png`, `favicon-32.png`
  and `favicon.ico` are copies of the matching files here.
- VS Code extension: `integrations/vscode/media/icon.png` is
  `png/aisw-mark-256.png`.

## Regenerating rasters

The SVGs are the source. Rasters were rendered in headless Chromium with a
transparent background at the sizes in their names, and `favicon.ico` was
packed with ImageMagick from the 16, 32 and 48 px PNGs. Render in a browser
engine rather than with ImageMagick's built-in SVG renderer, which drops the
mark's strokes. The same mark ships on aiswitcher.dev, where
`scripts/generate-icons.mjs` does this rendering.
