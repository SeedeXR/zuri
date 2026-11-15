# Handover log

Newest entry first. Each entry: what was done, what changed architecturally, what was
verified and how, what is open. Timestamps are local time, `YYYY-MM-DD HH:MM:SS`.

---

## 2026-11-16 03:30:00 - Session 2 (continued): view-model depth

**Done.**
- `enum` Properties with `Option` children. A number (the Option's position) wherever
  numbers are used; a name when bound to a text or when the host uses the string calls.
  C ABI: `ZURI_ENUM = 5`, `zuri_instance_property_option_count` / `_name` (additions; no
  signature changed). Web `properties[].options`; Unity `ZuriProperty.Options` and an
  inspector popup; Unreal `FZuriProperty::Options`; CLI `--set mode=busy`.
- Bind `direction`: `toSource` (read an animated value from the view model) and `twoWay`.
- Number to text: `decimals` and `format` (`"Score: {}"`).
- Listener `"event": "change"` with `"with": <Property>`: runs when the Property changes.

**Size.** This unit first took the lite wasm from 88.4 to 95.6 KB brotli. Two standard
library calls were the cause: `str::replacen` (the string searcher) and slicing a `str`
(its panic formatting), about 10 KB. The `{}` fill now works on bytes, and `fixed()`
writes digits by hand. Final: 91.9 KB (budget 100). Per-function growth was measured by
the reviewer; the largest remaining parts are `apply_binds` and load-time validation.

**Code review: 10 findings, all fixed.** Formatting a huge number panicked; a Bind that
writes back could push an enum off its Options or write text into a number Property (now
a load error: the Property must be able to hold the value); Option names collided with
objects' names (Options have their own name class now); an enum's start value was not
checked (and may now be given by name); enums passed `into` a nested artboard must list
the same Options; a `change` Listener missed a trigger fired by a later Listener
(triggers fired by a `change` Listener now fire for the whole next frame); a text bind
allocated every frame (cached by the Property's value).

**Ponytail review: 12 findings, 9 applied.** Not applied: removing the text cache (the
project standard is no per-frame allocation), the Unity loop-variable copy (harmless),
folding the Unreal warning into the generic one (it would lose the Option's name).

**Also.** The session was interrupted by a usage limit while the two reviews ran; both
were resumed and completed.

**Verified** on the final tree. `bash scripts/verify.sh` exits 0: 74 Rust tests, 8 Node
tests, browser checks, editor 20 tests + build, Unity dotnet harness; wasm brotli
92 / 120 / 158 KB. `bash scripts/verify-engines.sh`: Unity EditMode 17 of 17, PlayMode 4
in gamma and 4 in linear on Metal; Unreal BuildPlugin and 6 of 6 automation tests on the
Metal RHI (`Zuri.Enum` is new). CI run 43719162543: macOS, Linux and Windows green.
**Not verified.** Choosing an Option in the Unity inspector popup (it draws; a menu cannot
be driven in batch mode); `Options` from an Unreal Blueprint; nothing of this unit in the
editor app's UI.

---

## 2026-11-16 00:40:00 - Session 2 (continued): Unity library on Linux and Windows

**Done.** `scripts/build-unity-plugins.sh` builds the native library for the system it
runs on (x86_64 on Linux and Windows, universal on macOS). All plugin `.meta` files now
live in `integrations/unity/PluginMeta~/` and are copied next to the library when it is
built; `Plugins/` holds only generated files (and a `.gitkeep`). CI builds the plugin on
all three systems. The root README has a table of what has run where.

**How the Linux / Windows metas were verified.** An agent imported placeholder libraries
in Unity 6000.4 on this Mac, set the importer through the `PluginImporter` API and read
it back (editor OS and x86_64, Standalone Linux64 / Win64 only). Unity wrote them in a
newer format (`serializedVersion: 3`); the stored ones use the same format as the macOS
meta (version 2), which Unity 6000.4 reads back to the same settings, because the package
declares Unity 2021.3 as its minimum and only 6000.4 is installed here to check.

**Reviews.** Code review 8 findings, ponytail 3. Fixed: the build used the host's
architecture but always filed the library under `x86_64`; the macOS metas would have been
orphans on other systems (one rule for all three now); the importer test threw on a
folder it did not know; the READMEs claimed CI results before CI had run them.
Not done: stripping local symbols on Linux and Windows; a test that sees the stored
Linux / Windows metas on every run (they are checked only when their library is present).

**Verified.** CI run 43710631608: macOS, Linux, Windows green including the new step.
`bash scripts/verify-engines.sh unity` on the Mac: EditMode 16 of 16, PlayMode 4 + 4.

---

## 2026-11-15 23:30:00 - Session 2 (continued): three platforms, engines on the GPU

**Owner's instruction (2026-11-15):** "the software should work in mac, windows and linux",
and the Xcode Metal Toolchain is now installed. Recorded in `memory/project_context.md`.

**CI (`.github/workflows/ci.yml`).** `scripts/verify.sh`, then the Tauri editor shell
(build + tests), then the Unreal static library, on macos-latest, ubuntu-latest and
windows-latest. Developed on the branch `ci` (a scratch branch, so that `main` only gets
reviewed work). What the first runs found, all fixed:
- Windows: the C smoke test needed `clang` and `-D_CRT_SECURE_NO_WARNINGS`.
- macOS runner: the "binary embeds the home directory" check fired falsely. Rust's own
  standard library is built on GitHub's macOS runners and carries `/Users/runner/work/rust`.
  The check now looks for this build's three source roots, plus `$HOME` everywhere except
  on that runner.
- Portability, found by review before the first run: the browser checks were bash +
  Python (now `packages/web/test/browser.mjs`: node only, real MIME types, a timeout,
  Chrome's stderr on failure); `tsc` was spawned through a shell script; `node --test
  test/` only works on Node 26; Windows paths in `cargo-ship.sh`; LF line endings.
- Linux passed everything on its first run.
The state of each system is in `memory/todo.md` (only what has run there counts).

**Engines on a real GPU (macOS).** Until now every engine run was headless.
- Unity (Metal, gamma and linear): 4 PlayMode tests draw a Canvas `ZuriRawImage` and a
  world-space `ZuriQuad` through a camera and read the pixels back (upright, right
  colours), and click through `GraphicRaycaster` / `PhysicsRaycaster`. An EditMode test
  draws the `ZuriPlayer` inspector and clicks its Fire button.
- Unreal (Metal RHI): `Zuri.Texture` (GPU texture read back equals the runtime's pixels),
  `Zuri.Widget` (`UZuriWidget` through Slate at layout scale 2, pointer and touch input),
  `Zuri.Component` (dynamic material on a mesh actor).
- No plugin bug was found. Each agent broke a scratch copy on purpose (un-flipped UVs,
  ignored widget offset, skipped texture update...) and the new tests caught every one.
- `scripts/verify-engines.sh` runs all of it and fails if a GPU test is skipped or
  missing. It needs the Metal Toolchain and has no headless mode any more.
- Not possible here: an IL2CPP build (the macOS IL2CPP module is not installed in Unity).

**Reviews.** The CI change: a correctness review (9 findings) and a ponytail review (14),
before its first push to the scratch branch. The engine tests and gate: code review (10)
and ponytail (12). Fixed from them, beyond the list above: the engine gate could pass
with the drawing tests left out (a silent `-nullrhi` fallback and two "headless"
switches, all removed); the quad test could vanish without notice (the gate now expects
4 PlayMode tests); a failed render crashed the Unreal test run instead of failing an
assertion; the path check had been narrowed too far (the `$HOME` check is back).
Not taken: a cargo cache in CI (the slowest job is 12 minutes), merging the two Unity
test fixtures' helpers, deleting the dedicated "upright" test.

**Verified.** CI run 43708678100: macOS, Linux and Windows all green. Locally
`bash scripts/verify.sh` exits 0 and `bash scripts/verify-engines.sh` passes both halves
on the final tree (Unity 15 EditMode + 4 PlayMode x 2 colour spaces; Unreal 5 of 5 on
the Metal RHI).
**Not verified.** Anything on Linux or Windows beyond what `verify.sh` and the two extra
CI steps run (see `memory/todo.md`); a person has still not looked at a scene in either
engine's editor window.

---

## 2026-11-15 21:00:00 - Session 2 (continued): constraints, engine gate

**Done.**
- `Constraint` (child of any drawable): `translation`, `rotation`, `scale`, `transform`,
  `distance` (closer / further / exact) and `followPath` (with `orient`), blended by
  `strength`. A second pass of `Instance::update_world` applies them in document order,
  in artboard space. Each part moves in place (the matrix is never rebuilt), so skew and
  mirroring survive and strength 0 changes nothing.
- `scripts/verify-engines.sh`: the Unity and Unreal checks, from throwaway projects it
  creates under `target/engines/`. Until now they lived in a session scratch folder.
- `advance` no longer recomputes world transforms when no property changed.

**Code review: 10 findings.**
- Fixed: the first version decomposed and rebuilt the matrix, which turned a mirrored
  object upside down and dropped skew (a pop as soon as strength left 0); `followPath`
  re-flattened its target on every idle frame (transforms are now skipped when nothing
  changed); the engine gate accepted zero Unity tests, a half-finished Unreal run, the
  wrong engine version when two are installed (plain `sort`), and a warm project whose
  compiler warnings were cached away; the editor created a Bind under a Constraint with
  a property a Constraint does not have.
- Known ceiling (in the code and in `docs/user/format.md`): a target later in the
  document is read before its own constraints.

**Ponytail review: 4 findings, 3 applied** (no `hypot`, `point_along` without copying
the contours, a shorter pass loop). Kept `sort -V` in the engine gate: `ls` alone sorts
5.10 before 5.8.

**Not done.** IK (planned with bones), min / max limits, per-axis copies.

**Verified.** `bash scripts/verify.sh` exits 0: 71 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 88 / 116 / 154 KB.
`bash scripts/verify-engines.sh` exits 0 on the final tree: Unity EditMode 14 of 14,
Unreal BuildPlugin and 2 of 2 automation tests.

---

## 2026-11-15 19:20:00 - Session 2 (continued): feather, trim, fit and alignment

**Grounding.** Semantics were read from Rive's docs on 2026-11-15 (rive.app/docs:
vector-feathering, manipulating-shapes/trim-path, runtimes/web/layouts, events/overview):
feather has a direction (outer / inner, "like an inner shadow or a vignette") and an
offset; trim modes are Sequential and Synced; fits are Layout, Contain, ScaleDown, Cover,
FitWidth, FitHeight, Fill, None with nine alignments; sending data to runtime code through
event properties is deprecated there, so that todo item was dropped.

**Done.**
- Fill / Stroke: `featherX`, `featherY` (the paint moves with the outline: a drop shadow);
  Fill: `featherInner` (crisp outline times one minus the blurred, offset outline).
- Stroke: `trimMode: synced`.
- Fit: `fitWidth`, `fitHeight`, `scaleDown`, and alignment. Core `Instance::fit` takes the alignment;
  C ABI: the alignment is OR-ed into the `fit` argument (`ZURI_ALIGN_*`), no signature
  changed; web `align`; CLI `--align`; Unity `ZuriAlignment`; Unreal `EZuriAlignment`.
- Behaviour change: a `fit` value the runtime does not know now means contain (it used to
  mean none). Written in `zuri.h`.

**Code review: 10 findings.**
- Fixed: a feather offset moved the outline but not its gradient or shader; the CLI fitted
  once instead of every frame (wrong for a scene that animates its artboard size);
  `--align` accepted anything containing the right words; the inner feather copied the
  contours and rasterized a padded region it cannot paint; `docs/dev/rendering.md` did
  not describe the inner pass.
- Not changed: `ZURI_ABI_VERSION` stays 1 although an old runtime would misread the new
  `fit` values. Nothing has been released; the policy is now written in `docs/dev/abi.md`
  (additions before the first release do not bump it).

**Ponytail review: 5 findings, 4 applied** (one `fit` function that takes the alignment,
a five-line `--align` parser, a loop for synced trim, no clamping in the web wrapper).
Not applied: moving the offset into `Raster::fill` (the reviewer measured no gain).

**Verified.** `bash scripts/verify.sh` exits 0: 70 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 87 / 115 / 153 KB.
Engines, on the final library: Unity EditMode 14 of 14 with no compiler errors or
warnings, dotnet harness 49 checks, Unreal `BuildPlugin` BUILD SUCCESSFUL, `Zuri.Button`
and `Zuri.Image` succeed (the alignment check is inside `Zuri.Button`).
**Not verified.** Anything on screen in either engine; `UZuriWidget` / `UZuriComponent`
alignment (compiles, no test); only `None` + `BottomRight` is tested through the wrappers.

---

## 2026-11-15 17:10:00 - Session 2 (continued): Milestone 1, state machine depth

**Done** (file format and core only; C ABI unchanged).
- Transition: `interp` (linear / hold / cubic with the ease points), `pause` (freeze the
  state being left), `chance` (random choice among the transitions that could fire; a
  xorshift generator that restarts with the machine, so runs repeat exactly).
- `Action` children of Transitions and Listeners change view-model Properties.
- Condition `with`: compare against another Property.
- State `kind: direct`: BlendAnimations each at its own weight (`value` or `input`).

**Code review of this unit: 10 findings.** The first design of `direct` was wrong and was
replaced: it mixed over a pose captured on entry, which crept when fading out, popped
when a fade in ended, froze what earlier layers animated and never settled. It is now a
pure pose: design values, then each animation at its weight (`instance.rs: apply_state`),
so it fades, seeks and settles like an animation state (test:
`direct_states_mix_animations_over_the_design_pose`).
- Also fixed: `chance` draws depended on history after a restart or seek; a trigger fired
  by a Transition's Action was lost for layers that had already run (it now fires for the
  whole next frame); an overshooting ease curve over-mixed; an Action without `input`
  and a `with` on a string or color were silently accepted; the editor's tooltip for the
  new state kind.
- Kept: a `direct` state overrides earlier layers on the properties it animates, as every
  state does (documented).

**Ponytail review: 9 findings, 8 applied.** They brought the lite wasm from 90.7 KB back
to 86.8 KB brotli, below where the unit started (87.1): the schema's doc strings are no
longer compiled into player builds (3.2 KB), `direct` and `blend` share one list, and
the `chance` draw is a single pass without a Vec. Not applied: folding the final mix of a
`direct` state into one loop, because a property keyed by two of its animations would be
mixed twice during a fade (the test now has such a pair).

**Watch.** Size: each feature batch adds 2 to 3 KB brotli to the lite wasm (budget 100).
Measure after every batch; `memory/todo.md` has a size-pass item.

**Verified.** `bash scripts/verify.sh` exits 0: 68 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness.
**Not verified.** Nothing of this batch was tried in the editor UI (the inspector is
schema-driven, so the properties appear; the state graph was not looked at).

---

## 2026-11-15 15:30:00 - Session 2 (continued): Milestone 1, first batch

**Done** (all in the file format and core; the C ABI did not change).
- Components: `play` on a NestedArtboard, and `Bind` with `into` to write the parent's
  view-model Properties into the child's (triggers fire through in the same frame).
  Resolved at load in `file.rs: build_nests`; mistakes are load errors.
- Timeline events: `Cue` children of an Animation (`instance.rs: cues`).
- Draw order: `order` (number, animatable) and `solo` on a Node (whole number, 1-based,
  0 = all). Drawing, hit-testing, listeners and `bounds` all go through `drawn_children`.
- Blend modes hue, saturation, color, luminosity (`raster.rs: blend_hsl`).
- `examples/components.json` uses all of it; rendered and looked at.
- wasm size: every runtime sort now goes through `geom::sort_keyed`, because each
  `sort_by` call site compiled its own ~6 KB copy of the standard sort. Lite build
  87 KB brotli with the four features in (84.6 before them, 88.8 without this).

**Reviews of this unit.** Code review: 10 findings. Ponytail: 5, all applied.
- Fixed: listeners and nested artboards hidden by a `solo` still took pointer input;
  `solo` was a float and passed through other children during a cross-fade (now a whole
  number, which steps); `seek` made nested artboards report their cues; a blend state's
  animations reported cues at weight 0; a `Bind` with `into` outside a NestedArtboard was
  silently dropped; `bounds` ignored `solo`; `drawn_children` allocated per node per
  frame (now borrows unless `order` is used); `order` / `solo` changes dropped the
  cached outline.
- Kept, documented in `docs/user/format.md`: `order` glides in a cross-fade like any
  number; a state that is being faded out does not report cues.

**Verified.** `bash scripts/verify.sh` exits 0: 65 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 87 / 115 / 149 KB.
**Not verified.** The editor now offers whole-number properties (`solo`, `points`) for
keying: its unit tests pass, the UI was not driven. No engine run (bindings unchanged).

---

## 2026-11-15 13:00:00 - Session 2 (continued): small runtime follow-ups

**Done.**
- Gradients are dithered (8.8 fixed-point ramp, position threshold `(143x + 79y) mod 256`).
  A Bayer matrix was tried first and rejected: on a vertical gradient its rows do not
  average evenly, which drew the very lines it was meant to remove.
- The compiler checks what a reference reaches (`schema::ref_types`: asset kinds, Listener
  targets, Clip sources) and that host-facing names are unique (`schema::host_named`).
  Schema JSON carries `types`; the editor's `ASSET_FOR` / `POINTER_TYPES` mirrors are gone.
- A cross-fade that interrupts one already on screen now starts from the picture as it is
  (nothing plays underneath; documented in `docs/user/format.md`). The old code jumped to
  the interrupted state's full pose first: reproduced (x went 50 -> 99) by running the new
  test against the previous `instance.rs`, then fixed. Two fades that fire in one frame
  are one fade from the state on screen.
- `scripts/cargo-ship.sh`: the wasm builds embedded the home directory in panic paths
  (15 strings in `zuri.wasm`). It wraps cargo with `--config build.rustflags` remaps (so
  paths with spaces and config-file flags keep working) and `--check`s shipped binaries;
  web, Unity and Unreal builds all go through it.

**Decided not to do** (reasons in `memory/todo.md`): the bundler `zuri.wasm` copy, and
`zuri_instance_fit_view` / `flip_y`.

**Reviews of this unit.** Ponytail: 4 findings, all applied. Code review: 10 findings.
- Fixed: duplicate Artboard names were not caught (wrong scope key); a fade chained in the
  same frame froze the state on screen; the path script broke on paths with spaces, had no
  failure checks, overrode config-file flags, and only the web build checked its output;
  the dither step was even (half the thresholds per row).
- Kept, now documented: an interrupting fade stops what the interrupted states animated
  (the alternative pops); a Clip whose source is a Text clips everything away in the lite
  build (the text itself is absent there too); Property names are unique per artboard
  across ViewModels (the host's lookup is per artboard).

**Verified.** `bash scripts/verify.sh` exits 0: 62 Rust tests, 7 Node tests, browser checks,
editor 20 tests + build, Unity dotnet harness. wasm brotli 85 / 112 / 147 KB.
`build-unity-plugins.sh` and `build-unreal-libs.sh` exit 0 through `cargo-ship.sh`, and
their `--check` finds no home path in the dylib or the static library.
**Not verified.** No in-engine run for this unit (Unity EditMode, Unreal automation): the
bindings did not change, only the library under them.

---

## 2026-11-15 10:40:00 - Session 2 (continued): runtime read-back APIs, second review loop

**Done (todo "Runtime API follow-ups" and one C ABI follow-up).**
- Core + C ABI + web: `seek` (scrub a directly played animation), `states` / `layer_state`
  (the State each layer is in), `property` / `property_color` (what an object draws with
  after animation and binds), `event_names` / `declared_event_*`. Web also gained
  `playStateMachine` / `playAnimation`. No signature changed; `ZURI_ABI_VERSION` stays 1.
- Schema JSON carries each property's `key`, each type's `class` and `scene`, and each
  reference's `target` class.
- Editor: track sampling, easing and reference-class mirrors are deleted; values come from
  the runtime (`liveValue`), scrubbing uses `seek`, the state graph rings the active state.
  Kept by hand, because the schema does not carry them: `ASSET_FOR`, `POINTER_TYPES`,
  `GEOMETRY_TYPES`, `REQUIRED`; `mapFrame` (the editor owns the playhead).
- Unity and Unreal wrappers bind the new calls: `Seek`, `DeclaredEvents`, `LayerStates`,
  `TryGetProperty` / `TryGetPropertyColor` (Unity); `Seek`, `GetDeclaredEvents`,
  `GetLayerStates` (Unreal).

**Code review of this unit: 10 findings.**
- Fixed: `seek` did not reach nested artboards (they now restart and run to the same time;
  one step, see the `ponytail:` note); declared events missed nested artboards' events and
  rescanned the file per call (now built once per artboard, nested ones appended); keying
  during a state machine preview wrote the machine's pose; the inspector re-rendered on
  every stage draw; six new exports lacked `guard`; property keys were truncated to u16;
  three doc comments were displaced; a size row in `memory/test.md` was wrongly changed.
- Web `property` no longer falls back to colors: `propertyColor` is separate (no failed
  first call on the hot path).

**Ponytail review of this unit: 5 findings, 4 applied.** Not applied: deleting the Unity
EditMode test that repeats the dotnet harness (it is the only in-engine run of the new
bindings).

**Editor bugs found on the way and fixed.** A second layer's entry State could get a
duplicate name; an unnamed state machine threw every preview frame (playing now names it);
two arrow nudges inside one frame lost a step.

**Open.** Two animations (or state machines) may share a name when unreferenced; hosts
play by name and get the first (in `memory/todo.md`).

**Verified.** `bash scripts/verify.sh` exits 0: 60 Rust tests, 7 Node tests, 14 browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 84 / 112 / 146 KB.
Engines, on the final native library (macOS only): Unity EditMode 13 of 13 passed with no
compiler errors or warnings, dotnet harness 48 checks; Unreal `RunUAT BuildPlugin` BUILD
SUCCESSFUL, automation tests `Zuri.Button` and `Zuri.Image` succeed. Not verified: anything
on screen in either engine, other platforms, IL2CPP.

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
