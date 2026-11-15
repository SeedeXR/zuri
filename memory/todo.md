# Todo

The single tracker for features, bugs, research and milestones.

Conventions:
- `[x]` done **and verified** (the proof is named). `[~]` partly done (the gap is named). `[ ]` not started.
- When a requirement changes, strike the old line through and add the replacement with the reason:
  `- ~~old task~~ -> replaced by "new task" (reason, date)`.
- Feature-by-feature status against Rive lives in `docs/parity.md`; this file is the work order.

Last updated: 2026-11-16.

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
- [x] Web package `@seedexr/zuri`: wasm builds, TypeScript wrapper, player, custom element, WebGL2 shader host (Node tests + headless Chrome `test/browser.mjs`)
- [x] React package `@seedexr/zuri-react`: builds, type-checks (its test app too), and runs in headless Chrome under StrictMode (`packages/react/test/browser.mjs`, in `verify.sh`). Not tried in Next.js or with server rendering (the component is client-only)
- [~] Editor (`apps/editor`, Tauri v2 + React 19 + Tailwind v4 + lucide): typecheck, 25 unit tests and the production build pass (in `verify.sh`); `cargo check` of the Tauri shell passes. Driven in Chromium by the editor agent after the Tailwind migration with a clean console: shell and panels, stage select / move / resize / rotate, rectangle / pen / artboard / text tools, hierarchy editing, schema-driven inspector, timeline scrub + auto-key + play, state-machine graph and live preview (hover, press, click, event log), Source tab with compiler errors, save / export (outputs pass `zuri validate`), font and image import. The agent reports `pnpm tauri dev` opened a window showing the editor. **Not verified:** native dialogs and file IO, SVG / 3D import and shader translation through Tauri commands, the release bundle, and the items listed under "Built, not verified" in `apps/editor/README.md`.
- [x] Editor font picker (owner request, 2026-11-15): a catalogue of 66 open-licence families (OFL 1.1, Apache 2.0, Ubuntu Font Licence; `apps/editor/src/fonts/catalog.json`, written by `scripts/font-catalog.mjs` from `github.com/google/fonts` at a pinned commit, downloaded on first use and hash-checked) and the fonts installed on the machine (desktop shell `system_fonts` / `read_font`, macOS / Windows / Linux; a font inside a `.ttc` is made a font of its own). Driven in a browser; the Installed tab has not been used in the desktop window
- [ ] Font picker, the rest: use it once in the desktop window on each system (Installed tab; a catalogue download under the packaged app's content security policy); Linux fonts that fontconfig knows of outside the usual folders; more families or other sources (the Google Fonts repository has no Creative Commons fonts); remembering downloaded fonts between sessions
- [~] Unity package (`integrations/unity`, `com.seedexr.zuri`), macOS, Unity 6000.4.8f1, `scripts/verify-engines.sh unity`: compiles with no C# errors or warnings; 15 EditMode tests (import, first frame, settling, click + event, recolor, image orientation, pointer mapping, play-mode round trip, alignment, the inspector drawing and its Fire button); 4 PlayMode tests on the Metal device in gamma and in linear colour space (a Canvas `ZuriRawImage` and a `ZuriQuad` drawn by a camera and read back: upright, right colours; hover and click through `GraphicRaycaster` and `PhysicsRaycaster`); the plain-dotnet harness passes 49 checks (in `verify.sh`). **Not verified:** Screen Space - Overlay and world-space canvases on screen, a real mouse through an input module, the inspector's look and dropdowns, URP / HDRP, transparent blending, IL2CPP (the macOS module is not installed), Unity 2021.3 / 2022.3, and every platform other than macOS.
- [~] Unreal plugin (`integrations/unreal/Zuri`), macOS, UE 5.8, `scripts/verify-engines.sh unreal`: compiles and links (Editor, Game Development, Game Shipping; `RunUAT BuildPlugin`); five automation tests pass on the Metal RHI: `Zuri.Button`, `Zuri.Image`, `Zuri.Texture` (the GPU texture read back equals the runtime's pixels), `Zuri.Widget` (`UZuriWidget` drawn through Slate at layout scale 2, pointer and touch input, `OnZuriEvent`), `Zuri.Component` (dynamic material on a mesh actor). **Not verified:** a widget in an on-screen window or the UMG designer, input routed by Slate from a real mouse, a mesh drawn in a level, translucent blending, any RHI other than Metal, Win64 and Linux (never built there).

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
- [x] `scripts/verify-engines.sh`: the Unity and Unreal checks no longer depend on a session's scratch projects (run 2026-11-15 from empty `target/engines/`: all passed)
- [x] CI (`.github/workflows/ci.yml`) runs `scripts/verify.sh`, the Tauri editor shell's build and tests, and the Unreal static library build on macOS, Linux and Windows (**owner's requirement 2026-11-15: all three must work**). All three passed on 2026-11-15 (GitHub Actions run 43708678100: Linux 5 min, macOS 8 min, Windows 12 min). So verified on Linux and Windows: the runtime and its goldens, the C ABI through a C program, the CLI, the importers, the wasm builds and web tests, the browser checks in headless Chrome, the editor's web build and unit tests, the C# bindings, the editor shell linking, the Unreal static library. Not yet on those systems: the editor running, the engine plugins inside the engines
- [~] Unity native plugin on Windows and Linux: `scripts/build-unity-plugins.sh` builds `Plugins/Linux/x86_64/libzuri.so` and `Plugins/Windows/x86_64/zuri.dll` and copies their metas from `PluginMeta~/` (CI run 43710631608, 2026-11-15: built and path-checked on both). The metas' importer settings were read back in Unity 6000.4 on a Mac with placeholder libraries. **Not verified:** the package opened in Unity on Linux or Windows; local symbols are not stripped there as they are on macOS
- [x] Editor installers on all three systems: `.github/workflows/editor.yml` (run by hand, on a `v*` tag, or by pushing the branch `bundle`) builds the app with `tauri build`, starts the packaged program and checks it is still running after 20 s, and keeps the installers as artifacts. Run 43734090861 (2026-11-16): macOS .dmg 3 MB, Windows .msi + .exe 6 MB, Linux .deb + .rpm + AppImage 89 MB, 6 minutes each. **Not verified:** that the window shows the editor (a blank window would also stay up); nobody has used the app on Linux or Windows; the installers are unsigned. Run again on 2026-11-16 at a305511 (run 43764260398), after the font picker and its shell commands: green on all three; installers 3.6 MB (macOS), 6.8 MB (Windows), 93.7 MB (Linux)
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
- [x] Schema JSON carries `key`, `class`, `scene`, each reference's `target` class and, where narrower, the `types` it accepts; the compiler enforces them (`json.rs` and `features.rs` tests). The editor's only hand-kept fact is which types are geometry
- ~~Web package: do not make bundlers ship `zuri.wasm` when another build is loaded~~ -> not doing (2026-11-15): the file is copied into the app's output but never downloaded unless used (observed in `apps/editor/dist/assets`); avoiding the copy needs a second package entry point, which is not worth a 336 KB file on disk. Revisit if a user asks
- [x] Gradient dithering (`raster.rs` test: every row and column of a 4-level ramp averages within 0.04 of a level; goldens re-blessed)

## C ABI follow-ups (found by the engine agents, not yet done)
- ~~`zuri_instance_fit_view(inst, fit, w, h, out[6])` or a `flip_y` render flag, so engines with bottom-up textures (Unity) get upright pixels without a UV flip~~ -> not doing (2026-11-15): the Unity wrapper flips when sampling, which costs nothing per frame and is tested; `zuri_instance_render_view` already takes any transform for a host that wants flipped pixels
- [x] Enumerate declared events: `zuri_instance_declared_event_count / _name`, web `declaredEvents` (`smoke.c`, web test)
- ~~`zuri_instance_get_string`: distinguish "missing" from "empty" in the return value~~ -> not needed (2026-11-15): the header documents it and hosts check the property list first, as the Unity and web wrappers do
- [x] Shipped binaries no longer embed the build machine's paths: `scripts/cargo-ship.sh` wraps cargo for the web, Unity and Unreal builds and `--check`s each binary for the home directory (found 2026-11-15: all three wasm builds embedded it). Cargo's `trim-paths` profile option is not stable in 1.98, so it is a script
- [x] What a nested artboard plays: the document says (`play`), and the host steers it through data bound `into` it (`features.rs`). No host call addresses a nested instance directly

## Milestone 1 - Rive parity, core (ordered by value / cost)
- [ ] Keep the lite wasm under 100 KB brotli (77.1 KB on 2026-11-16 with skinning, Constraint limits, text Runs, text on a path and Layout; 72.9 KB after a pass that took std's float parsing and formatting out: `geom::decimal`, `geom::within`). `packages/web/scripts/build.mjs` fails over `LITE_LIMIT` (80 KB): raise it with the feature that needs the room. Largest left (`twiggy top`, raw): rodata 19.8 KB, `build_artboard` 14.6 KB, `advance` 8.7 KB, `draw_node` 8.0 KB, `fill` 7.4 KB, the allocator 5.1 KB, `rem_pio2_large` 4.0 KB (sin / cos of large angles)
- [x] Pass data into nested artboards: `Bind` with `into`, and `play` on the NestedArtboard (`features.rs: nested_artboards_take_data...`, `examples/components.json`)
- [x] Timeline events: `Cue` (`features.rs: cues_report_events...`)
- ~~Event properties (an event carries values to the host)~~ -> not doing (2026-11-15): Rive's own docs call sending information to runtime code through events deprecated in favour of data binding (rive.app/docs/editor/events/overview, read 2026-11-15); Zuri hosts read the view model
- [x] Draw order (`order`) and solos (`solo`) (`features.rs: order_rearranges...`)
- [ ] Editor: drive keying of whole-number properties (`solo`, `points`) in the UI; only unit-tested so far
- [x] Non-separable blend modes: hue, saturation, color, luminosity (`raster.rs` test)
- [x] Inner feather and feather offset (`featherInner`, `featherX`, `featherY`); trim `synced` mode (`features.rs: synced_trim_feather_offset...`)
- [x] Fit: fitWidth, fitHeight, scaleDown and nine alignments, in the C ABI (`fit | align`), web, CLI, Unity, Unreal (`features.rs: fit_modes...`, `smoke.c`, web test)
- [~] Rich text: `Run`s (several fonts, sizes, weights and paints in one Text; `runtime.rs: text_runs_are_one_text_in_several_styles`, `examples/text.json`) and pair kerning from the font's GPOS `kern` feature or `kern` table (`text.rs: kerning`, `runtime.rs: letters_are_kerned...`; the subsetter writes the kept pairs as a `kern` table). Text on a path: `follow` a Shape's outline, with `position` and `align` (`runtime.rs: text_follows_the_outline_of_a_shape`, `examples/badge.json`). Missing: ligatures and shaping for complex scripts (harfrust or rustybuzz), text modifiers, styles shared between runs, kerning that follows a variable font's weight. Size lever: reading GPOS through ttf-parser costs 8 KB brotli in the text build (it brings every GPOS / GSUB subtable parser); a hand-written reader of pair adjustments alone would be smaller (not measured)
- [x] Font subsetting at build time: `glyphs` on a FontAsset (`crates/zuri-import/src/font.rs`; `import.rs: a_font_with_glyphs_is_subset_and_draws_the_same`: 111 KB -> 14.9 KB with the kept glyphs' kerning, pixel-identical at two weights). `examples/text.zur` is 30 KB with letters, digits and punctuation kept, and their kerning. CFF fonts are still embedded whole
- [~] View-model depth: enum Properties (with Options, listed through the C ABI), `direction` on a Bind (`toSource`, `twoWay`), number-to-text `format` / `decimals`, `change` Listeners (`features.rs`, web test, `smoke.c`). Missing: lists and nested view models (they need repeated components and layout), more converters
- [x] Direct (additive) blend states, transition easing / hold / pause, random transitions (`chance`), transition Actions, Conditions between two Properties (`features.rs`)
- [~] Keyboard: `keyDown` / `keyUp` Listeners with `key`, `zuri_instance_key`, web `Instance.key` and the player forwarding the focused canvas's keys (`features.rs: key_listeners_react...`, web test, `smoke.c`). Missing: focus (which object has it, tab order), gamepad, text input
- [x] Bones: `Bone` chains (a child bone starts at its parent's far end) carrying Shapes / Images rigidly, the `ik` Constraint over `bones` of them (`features.rs: bones_chain_from_end_to_end_and_ik_reaches_for_a_target`), and skinning: a `Skin` in a Path weights its points to a Bone (`features.rs: skinned_paths_bend_with_their_bones`, `examples/tail.json`)
- [ ] Skinning, the rest: automatic weights (a build step that fills in `weights` from the distance to each bone), a bone overlay and weight editing in the editor, meshes (deforming an Image)
- [~] Constraints: translation, rotation, scale, transform, distance, follow path, IK, with `axis` (one axis of a copy) and `min` / `max` limits (`features.rs`). Missing: limits relative to the parent (a `space` property), copy factor and offset, dependency-sorted evaluation
- [x] Nine-slice images: `sliceLeft` / `sliceTop` / `sliceRight` / `sliceBottom` (`features.rs: nine_sliced_images_keep_their_edges`)
- [~] Layout: `Layout` rows and columns written by hand (taffy would not fit the size budget): `gap`, `padding`, `justify`, `alignItems`, hug sizes, `grow`, `stretch`, painted box (`instance.rs: lay_out`, `features.rs: layouts_arrange_what_is_inside_them`, `examples/card.json`). Missing: wrapping, per-side padding, min / max sizes, absolute children, geometry that fills its slot, clipping to the box, scrolling, putting an existing object into a Layout from the stage (the hierarchy's drag is how, and that has not been tried with a Layout)
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
- [x] `zuri mcp`: the CLI as MCP tools over stdio (`crates/zuri-cli/src/mcp.rs`, `tests/mcp.rs`); `render` returns the picture. Speaks the revision of 2026-07-28 and the `initialize` ones before it. Not tried from a chat client yet
- [ ] Editor: collaboration, libraries, cloud

## Engines: verified on the GPU on 2026-11-15, and what is left
- [x] Unity: `ZuriPlayer` + `ZuriRawImage` on a Canvas with an EventSystem: upright, hover / click through real raycasts, gamma and linear (PlayMode tests)
- [x] Unity: `ZuriQuad` in world space, drawn and clicked through a `PhysicsRaycaster`
- [~] Unity: the `ZuriPlayer` inspector draws and its Fire button works (EditMode test); its dropdowns and look were not checked
- [ ] Unity: an IL2CPP build (needs the macOS IL2CPP module installed in Unity Hub); Windows / Linux / Android / iOS plugin builds
- [x] Unreal: `UZuriWidget` drawing at a layout scale, hover / click / touch, the `OnZuriEvent` delegate (`Zuri.Widget`)
- [x] Unreal: `UZuriComponent` on a mesh with a dynamic material (`Zuri.Component`)
- [ ] Both engines: look at a scene in a real editor window once (nothing has been seen by a person)

## Known issues and debt
- `grep -rn "ponytail:" crates packages` lists every deliberate shortcut with its upgrade path.
- Shader paints have one frame of latency and one GPU readback per paint per frame on the web.
- The software 3D path is slow at full-screen sizes (22.7 ms at 1920x1080 for 1,646 triangles).
- Imported SVG shapes have baked transforms (absolute coordinates).
- A `Clip` clears and rasterizes a full-surface mask every frame, whatever its size (`raster.rs` `push_clip`): bound it to the clip's box when a profile shows it.
- Instances of one component share a shader texture per `ShaderPaint` (the first instance's size).
- Unity wrappers free native handles with a finalizer + `GC.KeepAlive`; `SafeHandle` would be shorter and safer but needs an IL2CPP build to confirm marshalling (review 2026-11-15).
