# Implementation instructions

Conventions and recipes. If you follow a recipe and it does not work, fix the recipe here.

## Commands

```bash
bash scripts/verify.sh            # the gate: format, lint, tests, examples, docs, web, graph
bash scripts/verify.sh --fast     # inner loop: format, lint, Rust tests
# The browser step fails when Chrome is missing. Set CHROME=/path/to/chrome, or ZURI_SKIP_BROWSER=1 to skip it on purpose.
cargo build -p zuri-cli --release # the CLI at target/release/zuri
bash scripts/build-examples.sh    # examples/*.json -> examples/dist/*.zur
pnpm --filter @seedexr/zuri build # wasm (3 variants) + TypeScript -> packages/web/dist
pnpm --filter @seedexr/zuri test  # Node tests against the wasm, incl. native goldens
ZURI_BLESS=1 cargo test -p zuri-core --features json,text --test runtime   # rewrite goldens (look at them!)
```

## Coding standards

**Rust**
- `cargo fmt` (width 160, see `rustfmt.toml`) and `cargo clippy -- -D warnings` are law.
- `zuri-core` takes no required dependencies and must build for wasm32. New dependencies
  go behind a feature and need a reason in the handover log.
- No `unwrap` / indexing on data that came from a file. Errors are `Error(String)` with a
  message that says where and what. `bail!` is the shorthand.
- No panics across the C ABI: every exported function goes through `guard`.
- A file starts with a `//!` comment that says what it owns and why it exists.
- Comments say why, not what. Deliberate shortcuts get `// ponytail: <ceiling>, <upgrade path>`.
- Tests live next to the code (unit) or in `crates/<crate>/tests` (through the public API).

**TypeScript**
- Strict mode, ES modules, no runtime dependencies in `packages/web`.
- Public API has JSDoc. Errors thrown to users start with `Zuri:` and say what to do.
- Nothing allocates per frame on the hot path (reuse wasm memory, cache encoded names).

**C# (Unity)** - Unity 2021.3-compatible C#, blittable P/Invoke only, the wrapper classes
are free of `UnityEngine` so they can be tested with plain `dotnet`.

**C++ (Unreal)** - UE naming and macros, game thread only for Zuri handles, RAII for handles.

## Module conventions
- The schema is the only place a type or property is declared.
- A host never re-implements runtime behaviour. If a host needs something, it goes into
  `zuri-core`, then `zuri.h`, then every wrapper.
- Authoring-only code lives in `zuri-import` or the CLI, never in `zuri-core` without a feature.
- Generated files say so in their first lines and are checked by `verify.sh`
  (`docs/reference/schema.md`).

## Recipes

### Add a property
1. `schema.rs`: add a line to `props!` with the next free key (never reuse a key), a name,
   a kind, a default and a one-sentence doc.
2. List the key under each type that carries it in `types!`.
3. Read it where it is used (`self.f(n, p::NAME)` etc. in `instance.rs` / `draw.rs`).
4. If the key is >= 128, raise `MAX_KEY` in `schema.rs`.
5. Add a test that fails without the read. Run `target/release/zuri schema --markdown >
   docs/reference/schema.md`.
The JSON twin, validation, CLI schema output and the editor inspector pick it up on their own.

### Add an object type
1. `schema.rs`: add a line to `types!` (next free key, name, property groups, allowed
   parents, doc). Scene types must stay inside the `ARTBOARD..=SHADER_PAINT` key range used
   by `is_scene`, or extend that function.
2. If it draws: handle it in `draw.rs` (`draw_node`), and in `instance.rs` `contours` /
   `hit_node` / `bounds` if it has an outline.
3. If it is logic (animation, state machine, data): compile it in `file.rs::build_artboard`.
4. If other objects refer to it by name, give it a class in `schema::ref_class`.
5. Add an example or extend one, and a runtime test with a golden.

### Add a C ABI function
1. Implement in `crates/zuri-ffi/src/lib.rs` (`#[no_mangle] pub unsafe extern "C" fn zuri_...`, wrapped in `guard`).
2. Declare it in `include/zuri.h` (the sync test fails until you do).
3. Exercise it in `tests/smoke.c`.
4. Expose it in `packages/web/src/index.ts`, `integrations/unity/Runtime/ZuriNative.cs`
   and the Unreal plugin. Bump `zuri_abi_version` only for incompatible changes.

### Add an importer
Put it in `crates/zuri-import/src`, return JSON source (never objects), dispatch on the
file extension in `import_file`, and add a test in `tests/import.rs` that imports a real
file and checks pixels. Record what is not imported in a `ponytail:` comment and in
`docs/user/importing.md`.

### Add an example
Add `examples/<name>.json` (assets under `examples/assets`, referenced with `"@assets/..."`).
It is built and smoke-tested automatically by `every_example_builds_runs_and_survives_a_dump_and_rebuild`.

### Change the look of rendered output
Goldens will fail. Regenerate with `ZURI_BLESS=1`, **look at every changed PNG**, and say
in the handover log why the pixels changed.

## Process flow for a change
understand -> smallest change -> test that fails without it -> `verify.sh --fast` ->
full `verify.sh` -> update `memory/todo.md` -> append `memory/handover_session.md`.

## Optimization principles
Measure first (`zuri bench`, `twiggy` for wasm size). Prefer doing less work (skip, cache,
clip) over doing the same work faster. Never trade determinism for speed in the reference
renderer; put speed in a backend.

## Release and CI
- CI should run `bash scripts/verify.sh` on macOS and Linux (not set up yet: see todo).
- Web: `pnpm --filter @seedexr/zuri build` then `pnpm publish` from `packages/web` and
  `packages/react` (publishing is a human decision; nothing publishes automatically).
- Unity / Unreal native libraries are built per platform by `scripts/build-unity-plugins.sh`
  and `scripts/build-unreal-libs.sh`. `bash scripts/verify-engines.sh` builds both and runs
  the Unity EditMode tests and the Unreal automation tests headless, in throwaway projects
  under `target/engines/` (macOS; `UNITY` and `UE_ROOT` override where it looks).
- Versioning: crates and npm packages share one version. The file format has its own
  major/minor in the header; the C ABI has `zuri_abi_version`.
