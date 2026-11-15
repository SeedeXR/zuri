# Documentation standards

Documentation is part of the change. A feature without its doc is not done; a doc that
contradicts the code is a bug.

## Where things go

| You are documenting | Put it in |
|---|---|
| What a type or property means | Its `doc` string in `crates/zuri-core/src/schema.rs` (generates `docs/reference/schema.md`, `zuri schema`, editor tooltips) |
| How to use Zuri (authoring, CLI, web, engines) | `docs/user/*.md` |
| How Zuri works inside, and why | `memory/architecture.md`, `docs/dev/*.md` |
| How to work on Zuri | `memory/instruction.md`, `AGENTS.md` |
| A package or plugin's own usage | A `README.md` in that directory |
| Public API | Doc comments in the source (`///` Rust, JSDoc, XML docs in C#, comments in `zuri.h`) |
| What changed and what is open | `memory/handover_session.md`, `memory/todo.md` |
| Feature status against Rive | `docs/parity.md` |

## Rules
1. **One home per fact.** Link to it; do not copy it. Generated docs are generated
   (`docs/reference/schema.md`), and `verify.sh` fails when they are stale.
2. **Only verified claims.** Numbers come from a command that was run, with the date and
   machine. Status words are exact: *done* (tested), *partial* (say what is missing),
   *planned* (not started). Never "should work".
3. **Lead with what the reader needs.** Start a page with what it is for and a working
   example. Put reference tables after.
4. **Show real code.** Every example in the docs is taken from `examples/` or a test, or
   was run before it was written down.
5. **Plain language.** Short sentences, concrete nouns, no marketing. Define a term the
   first time it appears. Write for a reader arriving cold.
6. **Version awareness.** State versions where behaviour depends on them (file format
   major/minor, ABI version, engine versions tested, dependency versions checked).
7. **Diagrams** are ASCII or Mermaid in the Markdown, next to the text they explain, and
   only where a picture carries something prose cannot.
8. **Decisions are recorded with their reason** and the condition under which to revisit
   them (the table in `memory/architecture.md`).
9. **Changelog.** User-visible changes go in the handover log now and in `CHANGELOG.md`
   once releases start.
10. **Docs change with the code**, in the same change. Reviewers check both.

## Page template (user docs)
```
# Title (what the reader can do after reading)
One or two sentences: what this is and when to use it.
## Quick start        <- smallest working example
## How it works       <- the model, briefly
## Reference          <- options, tables
## Limits             <- what is not supported yet, honestly
```

## Checks
- `bash scripts/verify.sh` regenerates the schema reference and diffs it.
- Before finishing a doc: run its commands, open its links, read it once as a newcomer.
