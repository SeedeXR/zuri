# Zuri brand

| File | Use |
|---|---|
| `zuri-logo.svg` | The logo as supplied: black on transparent, square frame + maze "Z". Do not edit. |
| `zuri-mark.svg` | The same artwork with `fill="currentColor"`, so it takes the text colour (white on dark UI). Generated from the logo with `sed`; regenerate rather than edit. |
| `zuri-logo-1024.png`, `zuri-logo-128.png` | Black on white, rendered from the SVG by Zuri itself (`zuri import` + `zuri render`). Source for app icons. |
| `tokens.css` | Colour, type and radius tokens as CSS custom properties. The single source of truth. |
| `tailwind.css` | Tailwind v4 `@theme` that maps the tokens onto utilities (`bg-panel`, `text-fg-dim`, `ring-accent`, ...). |

## Palette

Borrowed from Rive. Every value below was observed in the page source of `https://rive.app/`
on 2026-11-15 (colour custom properties and inline colours); none is from memory.

| Token | Value | Role |
|---|---|---|
| `--zuri-bg-app` | `#0c0c0c` | window background |
| `--zuri-bg-stage` | `#121212` | canvas area |
| `--zuri-bg-panel` | `#181818` | panels, toolbars |
| `--zuri-bg-raised` | `#1d1d1d` | popovers, selected rows |
| `--zuri-bg-input` | `#222222` | fields, buttons |
| `--zuri-bg-hover` | `#333333` | hover, pressed, strong lines |
| `--zuri-line` | `rgba(255,255,255,0.1)` | hairlines |
| `--zuri-fg` | `#f1f1f1` | text |
| `--zuri-fg-dim` | `#a1a1aa` | secondary text |
| `--zuri-fg-faint` | `#666666` | disabled, hints |
| `--zuri-pink` | `#f30058` | brand |
| `--zuri-red` | `#ff483c` | danger |
| `--zuri-amber` | `#ffa41c` | warning |
| `--zuri-green` | `#71ff19` | success |
| `--zuri-cyan` | `#19f2ff` | keyframes, animated values |
| `--zuri-blue` | `#3084ff` | accent: selection, focus, links |

Type: Inter for UI, JetBrains Mono for code and numbers (both are what rive.app loads;
both are open-licensed). The tokens fall back to system fonts when they are not installed.

## Rules
- Surfaces are neutral and dark so artwork carries the colour. One accent (blue) for
  selection and focus; pink is for identity, not for state.
- The spectrum colours are for meaning (danger, warning, success, keyed), never decoration.
- Text on `--zuri-bg-*` uses `--zuri-fg` (11.2:1 or better) or `--zuri-fg-dim` (4.9:1 or
  better) on every surface token. `--zuri-fg-faint` is 2.2 - 3.4:1: disabled states and
  decoration only, never information. White on `--zuri-blue` is 3.6:1: icons and bold
  labels, not small body text. (Ratios computed with the WCAG formula.)
- The logo is monochrome. Use `zuri-mark.svg` so it follows the text colour; keep clear
  space of at least a quarter of its width; never recolour parts of it or stretch it.
- This is Zuri's identity. The palette is borrowed from Rive; Rive's name, logo and
  artwork are not, and must not appear as Zuri branding.
