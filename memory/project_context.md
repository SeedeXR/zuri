# Project context

## Vision
Zuri is an open alternative to Rive: interactive vector graphics that are designed once
and run everywhere a game or app runs (the web, Unity, Unreal), driven by state machines
and data, at the resolution and frame rate of the host. The runtime file is `.zur`
(as Rive's is `.riv`).

Zuri is **agent-native**: every scene has a plain JSON source that people and AI agents can
read, diff, generate and validate with a CLI, and an editor that shows the same document.

## Objectives
1. A small, fast, deterministic runtime in Rust with one C ABI for every host.
2. An open, versioned, forward-compatible file format (`.zur`) with a text twin (JSON).
3. Feature parity with Rive, tracked honestly in `docs/parity.md`.
4. First-class hosts: web (npm packages), Unity (UPM package), Unreal (plugin).
5. An editor (Tauri + React, lucide icons) with the directness of Rive and Figma.
6. Beyond Rive where it is cheap for us: 3D models (OBJ, glTF/GLB, FBX) inside vector
   scenes, WGSL shader paints, animatable variable-font weight, free export.

## Deliverables
| Deliverable | Where | State |
|---|---|---|
| Runtime core | `crates/zuri-core` | see `memory/todo.md` |
| C ABI + header | `crates/zuri-ffi` | |
| Importers + build | `crates/zuri-import` | |
| CLI `zuri` | `crates/zuri-cli` | |
| Web runtime `@seedexr/zuri`, `@seedexr/zuri-react` | `packages/` | |
| Editor | `apps/editor` | |
| Unity package `com.seedexr.zuri` | `integrations/unity` | |
| Unreal plugin `Zuri` | `integrations/unreal` | |
| Docs | `docs/` | |
| Agent memory | `memory/` | |

`memory/todo.md` is the single place that says what is done, verified, or pending.

## Platforms (owner's requirement, 2026-11-15)
Zuri has to work on **macOS, Windows and Linux**: the runtime libraries, the CLI, the web
tooling, the editor and the engine plugins. A platform counts as working only when the
gate has run there (CI: `.github/workflows/ci.yml`); `memory/todo.md` says which have.

## Success metrics (how we know)
- Correctness: `scripts/verify.sh` passes on macOS, Linux and Windows; wasm output matches native goldens.
- Size: lite web runtime <= 100 KB brotli (measured 2026-11-15: 78 KB lite, 131 KB with text).
- Speed: UI-sized scenes render in well under 1 ms on one core; an idle scene costs no
  render at all (measured: 0 of 600 idle frames rendered). Numbers live in `docs/dev/rendering.md`.
- Parity: the count of "done" rows in `docs/parity.md` only goes up.
- Agent usability: an agent given only `zuri schema` and one example can author a valid scene;
  compiler errors name the JSON path and the fix.

## Constraints
- Zero hallucination: every claim in docs and reports is backed by a command that was run.
- The runtime core has no required dependencies and must build for `wasm32-unknown-unknown`.
- Keys in the schema are permanent once shipped (old files and old runtimes depend on them).
- Licensing: Apache-2.0 (the owner's decision, 2026-11-17; `MIT OR Apache-2.0` until then).
- npm scope `@seedexr` is a provisional default matching the GitHub organisation.

## Target users
- Designers and motion designers who build interactive UI, icons, characters, HUDs.
- Game and app developers embedding that content in Unity, Unreal and web apps.
- AI agents and CI pipelines that generate, check and render scenes without a GUI.

## Evaluation criteria for any change
Does it keep `scripts/verify.sh` green? Is it covered by a test that fails without it?
Is the schema still the single source of truth? Is the doc that describes it updated?
Did the runtime get smaller or stay the same size unless the feature justifies it?

## Source material
`research/` holds the three research reports this project started from and
`memory_structure.md`, which defines this folder. Where the reports disagree with each
other or with what crates.io/npm actually ship, `memory/architecture.md` records the
decision and why.
