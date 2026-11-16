# Feature parity with Rive

The goal is every Rive feature. This table is the honest state of that goal.

- **Done** - implemented and driven by an automated test (named in the last column).
- **Partial** - works within the stated limits.
- **Planned** - not started. Nothing in this state should be assumed to work.

Rive's feature list is taken from its public documentation as summarised in `research/`.
Last updated 2026-11-16.

## Drawing

| Feature | Status | Notes / proof |
|---|---|---|
| Artboards | Done | `runtime.rs` |
| Groups (nodes), transforms, opacity inheritance | Done | `features.rs: opacity_multiplies_down_the_tree...` |
| Rectangle (with corner radius), ellipse, polygon, star | Done | `features.rs: geometry_kinds_and_stroke_options` |
| Pen paths (lines, cubics, quadratics, arcs) | Done | SVG path data; `geom.rs: svg_path_commands` |
| Solid fills and strokes | Done | goldens |
| Linear and radial gradients (animatable stops) | Done | `raster.rs`, goldens |
| Fill rules (non-zero, even-odd) | Done | `features.rs`, `raster.rs` |
| Stroke caps (butt, round, square) and joins (miter, round, bevel) | Done | `geom.rs: trim_and_stroke`; miter limit fixed at 4 |
| Trim path | Done | Sequential and synced modes, offset with wrap-around (`features.rs: synced_trim...`). |
| Dashes (with animatable offset) | Done | `features.rs: dashed_strokes...`, `geom.rs: dash_cuts_by_arc_length` |
| Blend modes | Done | The 16 W3C modes: 12 separable, plus hue, saturation, color, luminosity (`raster.rs` test against hand-worked values). |
| Clipping by a shape | Done | `features.rs: clip_restricts_a_subtree...` |
| Feather (soft edges, glows, shadows) | Done | Gaussian feather of a paint's coverage, outward or inward (`featherInner`), with an offset (`featherX`, `featherY`) (`raster.rs`, `features.rs: synced_trim...`). Blurred pixels, not Rive's analytic feather: see `docs/dev/rendering.md`. |
| Images | Partial | Draws host-decoded pixels (`features.rs: images_draw...`). No mesh deformation of images. |
| Nested artboards (components) | Partial | Live instances that play the state machine or animation the parent names (`play`), receive pointer input, bubble their events to the host, and take data from the parent's view model (`Bind` with `into`) (`features.rs: nested_artboards...`). Data flows down only; no lists of components. |
| Text | Partial | Wrapping; alignment; line height; bound strings; animatable variable-font weight (`runtime.rs: text_lays_out...`); `Run`s: several fonts, sizes, weights and paints in one text (`runtime.rs: text_runs...`). Pair kerning from the font (`runtime.rs: letters_are_kerned...`). Text on a path: `follow` a Shape's outline (`runtime.rs: text_follows...`, `examples/badge.json`). No modifiers, text input, ligatures or complex-script shaping; no styles shared between runs; a variable font's kerning does not follow its weight. |
| Fonts | Partial | Embedded TTF / OTF; glyph subsetting at build time for TrueType outlines, variable fonts included (`glyphs`; `import.rs: a_font_with_glyphs...`). No subsetting of CFF fonts, no fallback fonts. |
| Draw order / z-index animation, solos | Done | `order` on any drawable (animatable, hit-testing follows it) and `solo` on a Node (keyable; hidden children take no input) (`features.rs: order_rearranges...`). Order is among siblings; Rive's draw rules can also move an object across groups. |
| N-slicing | Done | `sliceLeft` / `sliceTop` / `sliceRight` / `sliceBottom` on an Image (`features.rs: nine_sliced_images...`). Images only, one stretch region per axis. |
| Layout (flexbox), scrolling | Partial | `Layout`: a row or a column with `gap`, `padding`, `justify`, `alignItems`, sizes that fit the content, `grow` and `stretch` for Layouts inside Layouts, a painted box (`features.rs: layouts_arrange...`, `runtime.rs: a_layout_sizes...`, `examples/card.json`); `wrap` onto further lines, a padding per side, and a Clip that names the Layout cuts to its box (`features.rs: layouts_wrap...`, `examples/chips.json`). A least and a most for each side (`minWidth`, `maxWidth`, `minHeight`, `maxHeight`; `features.rs: layouts_arrange...`). No absolute children or scrolling; the lines of a wrapping Layout are packed at the start; only Layouts change size (a Shape does not fill its slot). |
| Bones, skinning, mesh deformation | Partial | `Bone` chains that carry Shapes, Images and Nodes rigidly, posed by animation or IK (`features.rs: bones_chain...`). `Skin`: the points of a Path weighted to bones, so the outline bends (`features.rs: skinned_paths...`, `examples/tail.json`). Weights are written by hand (no automatic weighting); Images cannot be deformed (no meshes). |
| Constraints (IK, distance, transform, follow path, ...) | Partial | Translation, rotation, scale, transform, distance, follow path and IK over a chain of bones, with strength, one axis or both (`axis`), a copy `factor`, `offset` (the object's own value on top) and `min` / `max` limits on what is copied (`features.rs: constraints_make...`, `constraints_copy_at_a_rate...`, `bones_chain...`). Rive's page (rive.app/docs/editor/constraints/translation-constraint, read 2026-11-16) names Copy with a rate, Offset and the limits but not their arithmetic: here it is target x factor, plus the own value, then the limits, and a rotation counts on past 180 as the target's own `rotation` does. Everything is in artboard space: no source / destination / limit spaces (Rive: World or Local each), and Constraints apply in document order, not by dependency. |
| Joysticks | Partial | `Joystick`: `handleX` and `handleY` (-1..1) each scrub an animation; keyed, or bound to the view model (`features.rs: joysticks_scrub...`, `examples/joystick.json`). Rive's page (rive.app/docs/editor/manipulating-shapes/joysticks, read 2026-11-16) does not say how the handle maps onto the timeline: here -1 is the first frame and 1 the last. No invert toggle (key the animation the other way), no handle source (an object whose position moves the handle), and no handle to drag on the editor's stage. |
| Audio | Partial | `AudioAsset`, and an Event that plays one at a `volume` (`features.rs: an_event_names_the_sound...`, `smoke.c`). The web player decodes and plays it (`test/browser.mjs`), and so does the editor's preview. Unity plays it through an `AudioSource`, uncompressed WAV only (`ZuriPlayModeTests: Player_PlaysTheSoundOfAnEvent`, `DotnetHarness`). Unreal makes a Sound Wave of it on import and plays it in a game world (`Zuri.Sound`, which watches the audio device start it). No volume or pitch animation, no looping or stopping, no audio on the timeline other than through a Cue's event. |

## Animation

| Feature | Status | Notes / proof |
|---|---|---|
| Timelines with keyframes on any number or color property | Done | `runtime.rs: animation_plays_loops...` |
| Interpolation: linear, hold, cubic bezier | Done | `features.rs: keys_interpolate_hold...`, `instance.rs: easing_endpoints_and_shape` |
| Loop modes: one-shot, loop, ping-pong | Done | `runtime.rs` |
| Animation speed | Done | property `speed` |
| Path morphing | Done | Key a Path's `d`; `features.rs` |
| String, enum and boolean keys (stepped) | Done | hold semantics |
| Work area, elastic and custom easing | Partial | Elastic keys: `interp` `elastic`, `elasticIn`, `elasticInOut` with `amplitude` and `period` (Rive: an Easing of in / out / in-out, Amplitude, Period) (`features.rs: elastic_keys...`). Rive's page (rive.app/docs/editor/animate-mode/interpolation-easing, read 2026-11-16) gives no formula or defaults: the curve here is a sine that dies away as (1 - t)^6, period 0.5 and amplitude 1 by default, so a file made to match Rive by eye may differ. Custom easing is `cubic` with four numbers, as CSS has them. No work area (a part of the timeline to loop while editing) in the editor. |
| Events on the timeline | Done | `Cue` children of an Animation (`features.rs: cues_report...`). |

## State machine

| Feature | Status | Notes / proof |
|---|---|---|
| Layers | Done | `runtime.rs: state_machine_listeners...` (two layers) |
| States: animation, entry, any, exit | Done | `features.rs: any_state_exit_time_and_triggers` |
| 1D blend states | Done | `features.rs: blend_states_mix_animations...` |
| Additive / direct blend states | Done | `kind: direct`: each animation over the design pose at its own weight (`features.rs: direct_states...`). |
| Transitions with conditions, cross-fade duration, exit time | Done | `features.rs`, `runtime.rs` |
| Transition easing curves, random transitions | Done | `interp` / ease on a Transition, `pause`, and `chance` (`features.rs: transitions_ease...`, `chance_picks...`). |
| Transition actions; conditions between two properties | Done | `Action` children; `with` on a Condition (`features.rs: transitions_ease...`). |
| Inputs: number, boolean, trigger | Done | They are view-model properties. |
| Listeners: pointer enter, exit, down, up, click, move | Done | `runtime.rs` |
| Listeners: keyboard, focus, gamepad, text input | Partial | `keyDown` / `keyUp` Listeners, fed by the host (`zuri_instance_key`; the web player forwards the focused canvas's keys) (`features.rs: key_listeners...`). No focus management, gamepad or text input. |
| Events reported to the host | Partial | Named events, with an optional sound (row "Audio"). No event properties or Open URL. |

## Data binding

| Feature | Status | Notes / proof |
|---|---|---|
| View models with number, boolean, trigger, color, string, enum properties | Done | `runtime.rs`, `features.rs: enums_binds...` |
| Host reads and writes properties by name | Done | C, wasm/TS, C# and Unreal APIs |
| Bind a property to any scene property | Done | `features.rs: binds_scale_numbers...` |
| Converters | Partial | On numbers: multiply + add, a range (`min` / `max`) and rounding to a whole number (`round`: nearest, down, up) (`features.rs: binds_keep_a_number...`); number to text with `decimals` and a `format` (`features.rs: numbers_bound_to_text...`). A Bool is turned round with `"mul": -1, "add": 1`. No string operations (pad, trim, remove zeros), no formulas, no converter that is shared between Binds or chained. |
| Two-way binding (scene writes back) | Done | `direction`: `toSource` and `twoWay` (`features.rs: enums_binds...`). |
| Lists, nested view models | Planned | Enums are done (row above). Lists need components repeated per item and layout. |

## Beyond Rive's 2D feature set

| Feature | Status | Notes / proof |
|---|---|---|
| 3D models inside a scene (OBJ, glTF / GLB, FBX) | Partial | Geometry, transforms, normals, material and vertex colors; animatable rotation (`import.rs`). No textures, skinning or imported animation. |
| WGSL shader paints | Partial | Web: drawn with WebGL2 (`test/browser.mjs`). CLI, Unity, Unreal: fallback color. |
| Scripting | Planned | Rive uses Luau. |

## Runtimes

| Feature | Status | Notes / proof |
|---|---|---|
| Web: canvas player, custom element | Done | `packages/web/test` (Node + headless Chrome) |
| Web: React | Done | `packages/react/test` (headless Chrome, StrictMode) |
| Web: GPU renderer (WebGL2 / WebGPU), worker mode | Planned | See `docs/dev/rendering.md`. |
| Unity (6000.4) | Partial | UPM package compiles; `.zur` import, rendering to a texture, settling, data binding, pointer input and events pass EditMode tests through the native plugin, and a standalone macOS player ran on arm64 and x86_64. Not yet seen on screen; macOS only. CPU-rendered texture; no shader paints. See `integrations/unity/README.md`. |
| Unreal (UE 5.8) | Partial | Plugin compiles and links on Mac; file import, rendering to a texture, data binding, pointer input and events pass automation tests in the headless editor. On-screen widget behaviour is not yet verified. CPU-rendered texture; no shader paints. See `integrations/unreal/Zuri/README.md`. |
| iOS, Android, Flutter, React Native | Planned | The C ABI is ready for them. |
| Skip rendering when nothing changed | Done | `runtime.rs`, `smoke.c`, web tests |
| Fit: contain, cover, fill, none, fitWidth, fitHeight, scaleDown; nine alignments | Done | `features.rs: fit_modes_and_alignment...`; C ABI, web, CLI, Unity and Unreal. Rive's `layout` fit needs the layout engine (planned). |
| Hit testing, bounds, transforms for tools | Done | `smoke.c`, web tests |

## Format and tools

| Feature | Status | Notes / proof |
|---|---|---|
| Binary format with forward compatibility | Done | `format.rs` |
| Text source format | Done | JSON; byte-exact round trip (`json.rs`, `import.rs`) |
| Command line: build, dump, import, validate, info, render, bench, schema | Done | `scripts/verify.sh` |
| Importers: SVG, OBJ, glTF / GLB, FBX | Partial | Limits in `docs/user/importing.md`. |
| Importers: Lottie, `.riv` | Planned | |
| Editor | Partial | Tauri + React + Tailwind app: stage drawn by the real runtime, hierarchy, schema-driven inspector, timeline with auto-key, state-machine graph with live preview, JSON source view, save / export. Verified in a browser; desktop file dialogs and importers are built but not exercised. No path-point editing, snapping, copy / paste or multi-artboard stage yet. See `apps/editor/README.md`. |
| Collaboration, libraries, cloud | Planned | |
