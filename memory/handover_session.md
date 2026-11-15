# Handover log

Newest entry first. Each entry: what was done, what changed architecturally, what was
verified and how, what is open. Timestamps are local time, `YYYY-MM-DD HH:MM:SS`.

---

## 2026-11-15 05:10:00 - Session 2: first review loop (code review + ponytail review), first push

**Owner's standing rule.** Every push to GitHub comes after a code review and a ponytail
(over-engineering) review with all findings fixed; then the next item in `memory/todo.md`.
It is now rule 8 in `AGENTS.md`.

**Code review: 10 findings, all fixed, each runtime one with a test in `features.rs`.**
- `parse_svg_path` looped forever on a number after `Z` (reachable from a file, a key, or a bound string).
- Nested artboards were capped in depth but not in number: 8 self-nests per level meant 8^8 instances. Now 4096 in total, then an error.
- A reference that was never set read as object 0, so a `NestedArtboard` without `artboard` nested its own parent. Now it points at nothing.
- Cross-fades compounded when the outgoing state did not animate a property (several times too fast, frame-rate dependent). The fade now starts every frame from the pose captured when the transition fired.
- Shader paints inside nested artboards never drew and leaked requests. Textures and requests now live on the root instance.
- Playback clocks were f32 seconds (jitter after hours, frozen after days). Now f64.
- `zuri render --set` picked the setter from the value's shape, not the property's type.
- `loadRuntime()` cached a failed fetch for the life of the page.
- `browser.sh` skipped silently without Chrome and the gate accepted it. Skipping now needs `ZURI_SKIP_BROWSER=1`.
- Any property change dropped a node's cached outline (text was re-laid-out every frame when only moved).
- Also fixed from the reviewer's overflow list: a segment after `Z` lost the subpath start; `set_image` after a scene settled never appeared; `ZuriPlayer.tick` did not drain events without a listener; `zuri_file_image_bytes` returned a dangling pointer for an unknown id (now NULL); `-0` did not survive dump + build.

**Ponytail review: about 250 lines removed** (crates and web 81, editor 86 net, engine plugins 86 net), by the reviewers that had read each area. No public behaviour changed except: Unity `ZuriInstance.Render(IntPtr, ...)` and `ZuriAsset.Bytes` are gone (unused), decompile's "invalid parent" message is now the loader's.

**Not applied, on purpose.** Unity `SafeHandle` (needs an IL2CPP build to confirm), removing `ZuriPlayer`'s pass-through setters, merging Unity's playback enum into one name, merging Unreal's four pointer functions (public API; the enum disambiguates same-named animations and state machines). Recorded under "Known issues and debt" in `memory/todo.md` where they are debt.

**Editor.** Escape in a number field committed instead of reverting (reproduced, fixed). Drags did not clean up on `pointercancel` (fixed by one shared `drag`). Tauri file commands now refuse paths outside the extensions the editor handles: this stops a compromised page reading keys or writing scripts; it is not a path scope (any `.json` / `.zur` is still reachable), see the `ponytail:` note in `src-tauri/src/main.rs`.

**Repository decisions taken for the first push (reversible).** `research/` is ignored (public repo, PDFs: the owner has not decided). Built native libraries are not stored in git (Unity now matches Unreal): run `scripts/build-unity-plugins.sh` after cloning.

**Verified.** `bash scripts/verify.sh` exits 0 (58 Rust tests, 6 Node tests, 14 browser checks, editor 16 tests + build, Unity dotnet harness). wasm brotli: 83 / 111 / 145 KB. Unity EditMode and Unreal build + automation were re-run after the plugin edits; the result against the final native library is in the commit that follows this entry.

**Not verified.** Everything listed as unverified in the previous entries still is, plus: the Tauri commands' new extension check through real dialogs; the Unity inspector after `DrawPropertiesExcluding`; `SZuriWidget` with `TStrongObjectPtr` at runtime.

---

## 2026-11-15 03:25:00 - Session 1 (end): editor result, full gate green

**Editor: agent report, checked by the integrator where cheap.** Integrator observed:
16 unit tests pass, `tsc --noEmit` + `vite build` succeed, `cargo check` of the Tauri shell
succeeds, screenshots show the Tailwind + brand restyle with the logo. Agent observed
(Playwright against the dev server, console clean): the feature list in `memory/todo.md`,
and a `pnpm tauri dev` window rendering the editor. Not exercised: native dialogs, Tauri
import / build commands, the release bundle. The agent was interrupted once by a usage
limit and resumed; its work was intact.

**State at the end of the session.** `bash scripts/verify.sh` exits 0: format, clippy,
53 Rust tests, examples, generated schema reference, wasm build + 6 Node tests +
type-level test, 14 headless-Chrome checks, editor tests + build, Unity dotnet harness,
graphify refresh. Unity (12 EditMode tests) and Unreal (build + 2 automation tests) were
re-run against the final runtime; those two are not part of `verify.sh` because they need
the engines installed.

**Nothing is committed, pushed or published.** Owner decisions are listed at the top of
`memory/todo.md` (license, npm scope, package ids, memory budget, whether to commit `research/`).

**Where to start next session.** `memory/todo.md`: the three "follow-ups" lists (they are
small and came from real use by the editor and engine agents), then Milestone 1.

---

## 2026-11-15 03:10:00 - Session 1 (continued): Unity result, runtime fixes, engines re-verified

**Unity package: agent report, then re-run by the integrator.** Compiles in Unity
6000.4.8f1 with no C# errors or warnings; 12 EditMode tests pass through the native
plugin; a standalone macOS player ran as arm64 and x86_64 (agent's observation); the
plain-dotnet P/Invoke harness passes 41 checks. Not verified: on-screen appearance (all
runs were `-nographics`), inspector GUI, real EventSystem raycasts, IL2CPP, other Unity
versions, other platforms.

**Runtime bugs the Unity agent found, fixed with tests (`features.rs`).**
- A press stuck when the pointer was released outside the artboard: an `up` listener
  without a target now hears a release anywhere, and leaving the surface ends a press.
- View-model accessors were not type-checked (a number could be written into a color
  property): each accessor now only touches properties of its own type.
- `zuri.h` declares the JSON functions only under `#ifdef ZURI_JSON`.

**Final engine verification against the finished runtime (run by the integrator, not
taken from agent reports).** Native libraries rebuilt with `scripts/build-unity-plugins.sh`
and `scripts/build-unreal-libs.sh`, then:
- Unity `-runTests -testPlatform EditMode`: `Passed, total 12, passed 12, failed 0`;
  0 `error CS` / `warning CS` lines in the log.
- Unreal `RunUAT BuildPlugin`: `BUILD SUCCESSFUL`, three targets succeeded;
  `Automation RunTests Zuri` with `-nullrhi`: `Zuri.Button` and `Zuri.Image` succeed, exit code 0.
- dotnet harness: `OK` (41 checks). Web: 6 Node tests + 14 headless-Chrome checks pass.

**Note for whoever reruns Unreal tests on macOS:** the editor writes its log to
`~/Library/Logs/Unreal Engine/<Project>Editor/<Project>.log`, not to stdout.

**Still open.** The editor's migration to Tailwind + brand tokens + logo (agent in
progress); C ABI follow-ups listed in `memory/todo.md`.

---

## 2026-11-15 03:02:00 - Session 1 (continued): brand, Tailwind, Unreal result, ABI fixes

**Owner direction received.** A logo (`zuri-logo.svg`), "brand and colours borrow from
Rive", and "CSS is Tailwind". This replaced the earlier plain-CSS decision for the editor.

**Done.**
- `brand/` created: the logo as supplied, a `currentColor` variant, icon PNGs rendered by
  Zuri's own SVG importer, `tokens.css` (CSS variables), `tailwind.css` (Tailwind v4
  `@theme`), `README.md` (palette, roles, rules). The palette values were read from
  rive.app's page source (not recalled); contrast ratios in the README were computed.
- The editor agent was told to migrate to Tailwind v4 + these tokens + the logo.
- `memory/design.md` now describes the Tailwind styling rules; the demo page uses the tokens.

**Unreal plugin: agent report, spot-checked against its logs.** Compiles and links with
UE 5.8 (three targets, `BUILD SUCCESSFUL`); automation tests `Zuri.Button` and
`Zuri.Image` pass with `-nullrhi`. Not verified: anything on screen (Metal Toolchain
missing on this machine), Win64, Linux. Details in `memory/todo.md`.

**ABI gaps the Unreal agent found, now fixed in the core / C ABI.**
- `zuri_last_error` could describe an earlier call: every fallible setter, getter and
  `play_*` now records its own reason (`smoke.c` checks it).
- `zuri.h` gained `#define ZURI_ABI_VERSION 1` and notes on booleans, image formats and
  `get_string`.
- A scene with shader paints never settled on hosts that cannot draw shaders: it now keeps
  redrawing only once a host has supplied a shader texture (`features.rs` test).
These are additive / behavioural; no existing signature changed. The Unreal plugin is
being recompiled and retested against the updated library (result in the next entry).

**Also added since the first entry:** stroke dashes, pointer input and events through
nested artboards, public `ZuriPlayer.tick()`, wider byte-input types in the web package
with a type-level test.

---

## 2026-11-15 02:55:00 - Session 1: foundation built end to end

**Starting point.** An empty repository with `research/` (three reports and
`memory_structure.md`).

**Decisions taken (reasons in `memory/architecture.md`).**
- In-house deterministic CPU reference renderer and a zero-dependency core, instead of
  depending on Linebender's pre-1.0 sparse-strip crates. GPU backends come later behind
  the three raster calls in `draw.rs`.
- One C ABI for every host, including the web (no wasm-bindgen).
- Generic object model driven by schema tables; JSON as the text twin.
- State-machine inputs are view-model properties.
- Shader paints are evaluated by the host and composited by the core.
- 3D files are converted at authoring time into one packed mesh layout.

**Grounding notes (things the research got wrong or left open).**
- Two reports name a crate `vello_gpu`; crates.io has `vello_hybrid` (0.2.0) and
  `vello_cpu` (0.4.0) as of this date.
- Stable Tauri is 2.x (`@tauri-apps/cli` 2.12.4); crates.io also lists a 3.0 alpha.
- `memory_structure.md` leaves the memory budget figure blank (see `memory/test.md`).
- Headless Chrome on this machine does not drive `requestAnimationFrame` (no display
  link), so the browser check ticks players itself through `ZuriPlayer.tick()`.

**Built and verified.**
| Area | Proof |
|---|---|
| `zuri-core` runtime | 27 unit tests, 11 feature tests, 5 runtime tests with 5 golden images |
| C ABI | header/exports sync test; `smoke.c` compiled with `-Wall -Wextra -Werror` and run |
| Importers + shader translation | 6 tests on real Blender 5.3 exports (FBX, GLB, OBJ) and an SVG; the three 3D importers give the same 1,646 triangles and the same picture |
| CLI | every command run by hand and in `verify.sh`; `dump` + `build` reproduces identical bytes |
| Web package | 6 Node tests (wasm output matches the native goldens), type-level test, headless Chrome check with 14 assertions including WebGL2 shader paints |
| Sizes | wasm brotli: 81 KB lite, 108 KB with text, 142 KB editor build |
| Performance | Apple M4, one core: 0.14 - 0.48 ms per frame at example sizes, 4 - 6.6 ms at 1080p for 2D scenes, 0 of 600 idle frames rendered (`docs/dev/rendering.md`) |

**Bugs found by running things, and fixed.**
- A trigger fired every chained transition in a frame (now at most one per layer per frame).
- A transition did not start blending until the frame after it fired.
- `trim` computed wrong ranges when `start + offset` exceeded 1.
- Name resolution was not kind-aware (a State and an Animation could not share a name).
- `Runtime.load()` did not accept the output of `Runtime.compile()` under TypeScript 7's
  typed-array types (found by the editor's typecheck; now pinned by a type-level test).

**Built, not fully verified.**
- React package: builds and type-checks; not run in a browser.
- The demo page (`packages/web/demo`) relies on `requestAnimationFrame`; the player's own
  loop has not been watched in a headed browser (its logic is covered through `tick()`).

**In progress when this entry was written.** Three parallel agents: the editor
(`apps/editor`), the Unity package (`integrations/unity`), the Unreal plugin
(`integrations/unreal`). Their results are recorded in the next entry above this one.

**Open (owner decisions).** License, npm scope, Unity package id, memory budget figure,
whether `research/` is committed; first commit and push (nothing has been committed).

**Next.** `memory/todo.md`, Milestone 1.
