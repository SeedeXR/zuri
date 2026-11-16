# Design system and UX

The single source of truth for how Zuri's editor and tools look and behave. Colour, type
and radius tokens are defined once, in `brand/tokens.css`; the editor is styled with
Tailwind CSS v4 through `brand/tailwind.css`, which maps those tokens onto utilities.
This file records the intent behind them. Brand rules and the logo: `brand/README.md`.

## Who we design for
A designer who knows Figma or Rive should be productive in minutes. A developer should be
able to find a property's exact name. An agent should be able to do everything through
the document (JSON) without the GUI.

## Principles
1. **Direct manipulation first.** Select and drag on the stage; the inspector is for
   precision. Every stage action has an inspector equivalent and vice versa.
2. **What you see is what ships.** The stage is drawn by the real runtime. Overlays
   (selection, handles, guides) are never part of the render.
3. **One document, visible.** The hierarchy, the inspector, the timeline and the Source
   tab are four views of the same JSON tree. Nothing is hidden state.
4. **Visible system status.** Mode (Design / Animate), tool, zoom, playback, compile
   errors and unsaved changes are always on screen. Errors say where and how to fix.
5. **Minimal cognitive load.** Familiar layout (tools top, layers left, stage centre,
   properties right, time bottom), familiar shortcuts, progressive disclosure.
6. **Forgiving.** Everything is undoable. Invalid edits keep the last good frame on stage.
7. **Fast.** Edits show on the next frame. No spinners for local work.

## Information architecture
```
Toolbar     tools | mode: Design / Animate | play | zoom | undo redo | file
Left        Hierarchy (frontmost on top) | Assets
Stage       artboard, selection overlay, handles
Right       Inspector (schema-driven) : transform, geometry, paints, references
Bottom      Timeline (Animate) | State machine | Source (JSON)
```

## Core flows
- **Draw:** pick a shape tool, drag on the stage, adjust in the inspector.
- **Animate:** switch to Animate, pick or add an animation, move the playhead, change a
  property (a key is written at the playhead), press play.
- **Make it interactive:** add view-model properties, add states and transitions with
  conditions, add listeners, press play and try it on the stage.
- **Ship:** Export `.zur`; or save the JSON source and run `zuri build`.
- **Bring things in:** import SVG, images, fonts, OBJ / GLB / FBX from the Assets tab.

## Interaction standards
| Action | Input |
|---|---|
| Select / move | V, click, drag |
| Shapes | R rectangle, O ellipse, P pen, T text, A artboard |
| Pan | Space + drag, middle drag, wheel |
| Zoom | Ctrl/Cmd + wheel, pinch; zoom to fit, 100 % |
| Undo / redo | Ctrl/Cmd + Z, Shift + Ctrl/Cmd + Z |
| Duplicate / delete / group | Ctrl/Cmd + D, Delete, Ctrl/Cmd + G |
| Nudge | Arrow keys (Shift = 10) |
| Play / pause | Space when the stage is not panning, or the play button |

## Visual language
- **Brand.** The logo is a monochrome maze "Z" in a square frame (`brand/zuri-logo.svg`). The palette is borrowed from Rive: the
  values were read from rive.app's page source on 2026-11-15 (owner's direction).
- Dark, neutral surfaces (`#0c0c0c` to `#333333`) so artwork carries the colour.
- One accent, blue `#3084ff`, for selection, focus rings, the active tool and the playhead.
- Cyan `#19f2ff` marks keyframes and animated values; red / amber / green mean danger /
  warning / success; pink `#f30058` is identity only (the mark, a primary call to action).
- Type: Inter for UI, JetBrains Mono for code and numbers, with system fallbacks.
- Density of a pro tool: 12-13 px UI text, 24-28 px controls, 4 px spacing grid.
- Icons: lucide only, 16 px, 1.5 px stroke, with a text label or tooltip always available.
- Motion in the UI is brief (under 150 ms) and never blocks input.

## Styling rules (Tailwind)
- Tailwind CSS v4 with the Vite plugin. The stylesheet imports `tailwindcss`, then
  `brand/tokens.css` and `brand/tailwind.css`. No second styling system.
- Use the semantic utilities, never raw hex or Tailwind's default palette:
  `bg-app bg-stage bg-panel bg-raised bg-input bg-hover`, `border-line border-line-strong`,
  `text-fg text-fg-dim text-fg-faint`, `bg-accent text-accent ring-accent`, `text-key`,
  `text-danger text-warning text-success`, `text-brand`, `font-ui font-mono`,
  `rounded-s rounded-m`.
- To change the look, change `brand/tokens.css`. To add a token, add it there, map it in
  `brand/tailwind.css`, and list it in `brand/README.md`.
- Computed geometry (stage overlays, timeline positions) stays in inline styles.
- Surfaces without a build step (the demo page) link `brand/tokens.css` and use the
  `--zuri-*` variables directly.

## Design tokens
The list and values live in `brand/README.md` (one home per fact).

## Component states
Every interactive component defines: default, hover, active/pressed, focus-visible
(always a visible ring), selected, disabled, error. Drag targets show where a drop will land.

## Feedback patterns
Inline validation next to the field; compile errors in a persistent bar with the JSON
path; non-blocking toasts for completed file actions; empty states that say what to do next.

## Accessibility
Real `<button>` / `<input>` elements, labels or `aria-label` on icon buttons, full
keyboard reachability of panels, visible focus, contrast of at least 4.5:1 for text,
no information by colour alone, respect `prefers-reduced-motion`.

## CLI and API design (they are UX too)
Verbs that say what they do (`build`, `dump`, `import`, `render`), `--json` everywhere,
non-zero exit on failure, error messages that name the path and list valid alternatives.
The TypeScript API reads like the concepts: `player.instance.setColor('accent', '#f36')`.

## Keeping this current
When a flow, component or token changes, change this file in the same commit. Heuristics
to check a design against: Nielsen's ten (status, match to the real world, control and
freedom, consistency, error prevention, recognition over recall, flexibility, minimalism,
error recovery, help).
