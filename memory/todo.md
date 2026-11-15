# Todo

The single tracker for features, bugs, research and milestones.

Conventions:
- `[x]` done **and verified** (the proof is named). `[~]` partly done (the gap is named). `[ ]` not started.
- When a requirement changes, strike the old line through and add the replacement with the reason:
  `- ~~old task~~ -> replaced by "new task" (reason, date)`.
- Feature-by-feature status against Rive lives in `docs/parity.md`; this file is the work order.

Last updated: 2026-11-15.

## Milestone 0 - Foundation (this is where we are)

### Runtime core (`crates/zuri-core`)
- [x] `.zur` container: header, ToC, flat objects, forward-compatible skipping, deterministic writer (`format.rs` tests)
- [x] Schema tables as the single source of truth (`schema.rs` test: keys and names unique)
- [x] JSON source twin: compile, decompile, schema output; errors with JSON path and alternatives (`json.rs` tests)
- [x] Geometry: shapes, SVG path data, flatten, stroke, trim, dash, hit test (`geom.rs` tests)
- [x] Reference renderer: exact-area coverage, gradients, images, 12 blend modes, feather, clip masks, scissor (`raster.rs` tests)
- [x] Scene: nodes, shapes, paints, clips, opacity, nested artboards, images, text, 3D models (`tests/features.rs`, `tests/runtime.rs`)
- [x] Animation: tracks, keys, linear / hold / cubic, loop modes, path morph, color keys (`tests/features.rs`)
- [x] State machines: layers, animation / entry / any / exit / blend states, transitions, conditions, exit time, triggers (`tests/features.rs`, `tests/runtime.rs`)
- [x] Listeners (enter, exit, down, up, click, move) and events, including through nested artboards
- [x] View model + binds (number with mul/add, color, string, path data)
- [x] Settle detection: `advance` returns false when nothing changed
- [x] Hostile-input safety: every byte of a real file corrupted through load / advance / pointer / render
- [x] Golden images, compared natively and in wasm

### Hosts and tools
- [x] C ABI + header, sync test, C smoke test (`crates/zuri-ffi/tests`)
- [x] CLI: new, build, dump, import, validate, info, render, bench, schema (exercised in `verify.sh` and by hand)
- [x] Importers: SVG, OBJ, glTF / GLB, FBX; WGSL -> GLSL / HLSL (`crates/zuri-import/tests/import.rs`)
- [x] Web package `@seedexr/zuri`: wasm builds, TypeScript wrapper, player, custom element, WebGL2 shader host (Node tests + headless Chrome `test/browser.sh`)
- [~] React package `@seedexr/zuri-react`: builds and type-checks; not run in a browser
- [~] Editor (`apps/editor`, Tauri v2 + React 19 + Tailwind v4 + lucide): typecheck, 16 unit tests and the production build pass (in `verify.sh`); `cargo check` of the Tauri shell passes. Driven in Chromium by the editor agent after the Tailwind migration with a clean console: shell and panels, stage select / move / resize / rotate, rectangle / pen / artboard / text tools, hierarchy editing, schema-driven inspector, timeline scrub + auto-key + play, state-machine graph and live preview (hover, press, click, event log), Source tab with compiler errors, save / export (outputs pass `zuri validate`), font and image import. The agent reports `pnpm tauri dev` opened a window showing the editor. **Not verified:** native dialogs and file IO, SVG / 3D import and shader translation through Tauri commands, the release bundle, and the items listed under "Built, not verified" in `apps/editor/README.md`.
- [~] Unity package (`integrations/unity`, `com.seedexr.zuri`): compiles in Unity 6000.4.8f1 with no C# errors or warnings; 12 EditMode tests pass through the native plugin (import, first frame, settling, click + event, recolor, image decode orientation, pointer mapping, play-mode round trip); a standalone macOS player ran as arm64 and x86_64; the plain-dotnet P/Invoke harness passes 41 checks (in `verify.sh`). **Not verified:** anything on screen (all runs were `-nographics`), the inspector GUI, real EventSystem raycasts, IL2CPP, Unity 2021.3 / 2022.3, and every platform other than macOS (no binaries built).
- [~] Unreal plugin (`integrations/unreal/Zuri`): compiles and links with UE 5.8 on Mac (Editor, Game Development, Game Shipping targets; `RunUAT BuildPlugin`), and its two automation tests (`Zuri.Button`, `Zuri.Image`) pass in the headless editor (`-nullrhi`). **Not verified:** anything on screen (the editor cannot start with a real RHI here: the Xcode Metal Toolchain is not installed), so `UZuriWidget` / `SZuriWidget` drawing and input and `UZuriComponent` are compile-verified only; Win64 and Linux were never built.

### Brand (added 2026-11-15 on the owner's direction)
- [x] `brand/`: logo (`zuri-logo.svg`, themeable `zuri-mark.svg`, icon PNGs rendered by Zuri itself), tokens (`tokens.css`), Tailwind v4 theme (`tailwind.css`), rules (`README.md`). Palette borrowed from Rive: values read from rive.app's page source; contrast ratios computed.
- [x] Demo page uses the brand tokens and logo; Unreal plugin icon is the logo
- [x] Editor restyled with Tailwind v4 + brand tokens + logo; Tauri app icons generated from the logo (build passes; screenshots in `apps/editor/docs/screenshots`)
- [ ] Unity package icon / docs site use the brand

### Project hygiene
- [x] `scripts/verify.sh` (format, lint, tests, examples, generated docs, web, browser, graphify)
- [x] `memory/` (this folder), `AGENTS.md`, docs, parity table
- [x] graphify: `graphify update .` in the verify loop; post-commit / post-checkout hooks installed locally
- [x] First commit and push to `github.com/SeedeXR/zuri` (2026-11-15, after the first code review + ponytail review; `research/` and built native libraries are not in git)
- [x] Review loop: code review + ponytail review, all findings fixed, before every push (`AGENTS.md` rule 8)
- [ ] CI running `scripts/verify.sh` on macOS and Linux
- [ ] Publish npm packages (**owner decision**: scope `@seedexr` and the license are provisional defaults)

## Open decisions for the project owner
1. License: currently `MIT OR Apache-2.0` (from the research's recommendation).
2. npm scope: currently `@seedexr/zuri`, `@seedexr/zuri-react` (the unscoped name `zuri` was free on npm on 2026-11-15).
3. Unity package id `com.seedexr.zuri`.
4. The memory budget figure that `research/memory_structure.md` leaves blank (see `memory/test.md`).
5. Whether to commit `research/` (PDFs included) to the public repository. Until decided it is in `.gitignore`.

## Runtime API follow-ups (found by the editor agent, not yet done)
- [x] `seek(seconds)` on an instance: core, C ABI, web (`features.rs`, `smoke.c`, web test: seek(1) equals the golden reached by 64 steps)
- [x] Separate `playAnimation` / `playStateMachine` in the web API (web test)
- [x] Read back a node's current property value (`property`, `property_color`) and each layer's current state (`states`): core, C ABI, web (`features.rs`, `smoke.c`, web test)
- [~] Schema JSON now carries `key`, `class`, `scene` and each reference's `target` class (web test). Still mirrored by hand in the editor: the asset type each `asset` wants, and which types are hit-tested
- [ ] Web package: do not make bundlers ship `zuri.wasm` when another build is loaded
- [ ] Gradient dithering: 8-bit banding is visible as faint lines in dark gradients when zoomed

## C ABI follow-ups (found by the engine agents, not yet done)
- [ ] `zuri_instance_fit_view(inst, fit, w, h, out[6])` or a `flip_y` render flag, so engines with bottom-up textures (Unity) get upright pixels without a UV flip
- [x] Enumerate declared events: `zuri_instance_declared_event_count / _name`, web `declaredEvents` (`smoke.c`, web test)
- ~~`zuri_instance_get_string`: distinguish "missing" from "empty" in the return value~~ -> not needed (2026-11-15): the header documents it and hosts check the property list first, as the Unity and web wrappers do
- [ ] Release dylibs embed absolute build paths (the Unity script strips them; do it in the cargo profile)
- [ ] Let a host choose what a nested artboard plays

## Milestone 1 - Rive parity, core (ordered by value / cost)
- [ ] Pass data into nested artboards (parent sets child view-model properties; choose what the child plays)
- [ ] Timeline events (keys that emit events) and event properties
- [ ] Draw order animation and solos
- [ ] Non-separable blend modes (hue, saturation, color, luminosity)
- [ ] Inner feather; trim "synchronized" mode
- [ ] Fit alignment and scale-down
- [ ] Rich text: multiple runs / styles, kerning, shaping for complex scripts (harfrust or rustybuzz), text on a path, text modifiers
- [ ] Font subsetting at build time (text.zur is 111 KB because the whole font is embedded)
- [ ] View-model enums, lists, nested view models; two-way binds; more converters
- [ ] Additive blend states; transition easing; random transitions
- [ ] Keyboard, focus and gamepad listeners
- [ ] Bones, skinning, mesh deformation
- [ ] Constraints (IK, distance, transform, follow path)
- [ ] Layout (flexbox via taffy), N-slicing, scrolling
- [ ] Audio assets and audio events
- [ ] Scripting (decide: Luau for Rive compatibility vs WASM components)

## Milestone 2 - GPU rendering (see `docs/dev/rendering.md`)
- [ ] Lift the three raster calls in `draw.rs` into a renderer trait
- [ ] Sparse-strip coverage on the CPU path
- [ ] WebGL2 / WebGPU compositor in `packages/web`; shader paints run in place
- [ ] Unity backend (command buffer + HLSL) and Unreal backend (RDG + .usf)
- [ ] Evaluate `vello_hybrid` once it is API-stable
- [ ] Worker / OffscreenCanvas mode for the web player

## Milestone 3 - Reach
- [ ] Unity and Unreal shader paints (HLSL is already embedded in files)
- [ ] 3D: textures, skinning, imported animation; GPU model rendering
- [ ] Importers: Lottie, `.riv`
- [ ] iOS, Android, Flutter, React Native wrappers over the C ABI
- [ ] `zuri mcp` (expose the CLI as MCP tools for agents)
- [ ] Editor: collaboration, libraries, cloud

## To verify in a Unity editor with graphics
- [ ] Add a `ZuriPlayer` + `ZuriRawImage` to a Canvas with an EventSystem: check it is upright, hover / click work through real raycasts, linear and gamma colour spaces look right
- [ ] The `ZuriPlayer` inspector (dropdowns, live property controls in play mode)
- [ ] An IL2CPP build; Windows / Linux / Android / iOS plugin builds (commands are in `scripts/build-unity-plugins.sh`)

## To verify on a machine with the Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`)
- [ ] Unreal: open a project with the plugin, put a `ZuriWidget` in a UMG widget, check drawing, DPI, hover / click, and the `OnZuriEvent` delegate
- [ ] Unreal: `UZuriComponent` on a mesh with a dynamic material

## Known issues and debt
- `grep -rn "ponytail:" crates packages` lists every deliberate shortcut with its upgrade path.
- Shader paints have one frame of latency and one GPU readback per paint per frame on the web.
- The software 3D path is slow at full-screen sizes (22.7 ms at 1920x1080 for 1,646 triangles).
- Imported SVG shapes have baked transforms (absolute coordinates).
- No CI yet; `scripts/verify.sh` has only been run on macOS arm64.
- Two animations (or two state machines) in one artboard may share a name when nothing refers to them; hosts play by name and get the first. Decide: make it a compile error, or play by index everywhere.
- A `Clip` clears and rasterizes a full-surface mask every frame, whatever its size (`raster.rs` `push_clip`): bound it to the clip's box when a profile shows it.
- Instances of one component share a shader texture per `ShaderPaint` (the first instance's size).
- A cross-fade that is interrupted mid-way restarts the outgoing state's own tracks from that state's pose (read from the code, not yet reproduced: expect a pop on properties both states animate).
- Unity wrappers free native handles with a finalizer + `GC.KeepAlive`; `SafeHandle` would be shorter and safer but needs an IL2CPP build to confirm marshalling (review 2026-11-15).
