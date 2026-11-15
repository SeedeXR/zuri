# AGENTS.md

Operating rules for anyone working in this repository, human or AI. Short on purpose:
the detail lives in `memory/`.

## Start here
1. Read `memory/session_start.md` and follow it (it tells you what else to read).
2. `bash scripts/verify.sh --fast` - know whether the tree is green before you touch it.

## What this is
Zuri is an open alternative to Rive: interactive vector animation with state machines and
data binding, in a Rust runtime that runs on the web, in Unity and in Unreal through one
C ABI. Scenes are authored as JSON (by people, the editor or agents), compiled to `.zur`.

## Map
| Path | Owns |
|---|---|
| `crates/zuri-core` | Runtime: format, schema, geometry, raster, animation, state machines, data binding |
| `crates/zuri-ffi` | C ABI (`include/zuri.h`) for all hosts, including wasm |
| `crates/zuri-import` | Authoring: importers (SVG, OBJ, glTF, FBX), shader translation, full build |
| `crates/zuri-cli` | The `zuri` command line |
| `packages/web`, `packages/react` | npm packages `@seedexr/zuri`, `@seedexr/zuri-react` |
| `apps/editor` | Tauri + React editor |
| `integrations/unity`, `integrations/unreal` | Engine plugins |
| `examples` | Example scenes and assets |
| `brand` | Logo, colour / type tokens (`tokens.css`), Tailwind v4 theme. UI colours come from here only |
| `docs` | User and developer docs (`docs/reference/schema.md` is generated) |
| `memory` | Project memory: context, architecture, instructions, standards, todo, handover |
| `scripts/verify.sh` | The gate |
| `scripts/verify-engines.sh` | The engine gate: Unity and Unreal builds and tests (needs the engines installed) |

## Rules
1. **Zero hallucination.** State only what you observed this session (file read, command
   run). Label everything else as unverified. Check versions and APIs; do not recall them.
2. **The schema is the single source of truth** (`crates/zuri-core/src/schema.rs`). Keys
   are permanent. Derived things are generated or checked, never hand-kept.
3. **One runtime.** Hosts never re-implement behaviour. New behaviour goes into
   `zuri-core`, then `zuri.h`, then every wrapper.
4. **Done means verified.** A change comes with the test that fails without it, and
   `bash scripts/verify.sh` exits 0. Never weaken a gate to pass it.
5. **Smallest correct change.** No speculative abstractions, no new dependency for what a
   few lines do. Mark deliberate shortcuts with `// ponytail: <ceiling>, <upgrade path>`.
6. **Docs move with code.** See `memory/docs.md`.
7. **Leave a trail.** Update `memory/todo.md`; append to `memory/handover_session.md` with
   a `YYYY-MM-DD HH:MM:SS` timestamp.
8. **Review, fix, then push.** The owner's standing rule (2026-11-15): a push to GitHub
   happens only after a code review and a ponytail (over-engineering) review of the
   change, with every finding fixed or its refusal written in the handover, and
   `verify.sh` green. Then take the next item in `memory/todo.md`. Publishing (npm,
   releases) and anything under `research/` still need the owner's explicit yes.

## Commands
```bash
bash scripts/verify.sh                 # everything (format, lint, tests, examples, docs, web, browser, graph)
bash scripts/verify.sh --fast          # format, lint, Rust tests
bash scripts/verify-engines.sh         # after touching the C ABI or integrations/: Unity + Unreal, headless
cargo build -p zuri-cli --release      # target/release/zuri
target/release/zuri schema             # the vocabulary: every type and property
target/release/zuri validate x.json --json
target/release/zuri render x.json -o out.png --time 1
pnpm --filter @seedexr/zuri build && pnpm --filter @seedexr/zuri test
graphify update . && graphify query "where are transitions evaluated?"
```

## The loop
```
read memory -> understand the flow -> smallest change -> test that fails without it
     ^                                                            |
     +---- update memory <- verify.sh green <- verify.sh --fast --+
                |
                +-> code review + ponytail review -> fix all -> verify.sh -> commit, push -> next todo
```
`scripts/verify.sh` is deterministic (no network, no LLM) and ends by refreshing the code
knowledge graph with `graphify update .` (AST only). Query the graph before sweeping the
tree by hand. `graphify hook install` makes every commit refresh it.

## Recipes
"Add a property", "add an object type", "add a C ABI function", "add an importer", "add an
example", "change rendered output" are in `memory/instruction.md`.

## Authoring scenes as an agent
`zuri schema` -> write JSON -> `zuri validate --json` (errors name the JSON path and the
valid alternatives) -> `zuri render` and look at the PNG. Start from `examples/`.
