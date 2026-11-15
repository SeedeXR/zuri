# Engineering philosophy

1. **The best code is the code never written.** Before adding anything, climb the ladder:
   does it need to exist; is it already in the repo; does the standard library do it; does
   the platform do it; does an installed dependency do it; can it be one line. Only then
   write the minimum that works.
2. **One source of truth.** The schema (`crates/zuri-core/src/schema.rs`) drives the binary
   format, the JSON twin, validation, the CLI's schema output, the editor's inspector and
   the reference docs. Anything derived is generated or checked by a test, never hand-kept.
3. **One runtime, one ABI.** The same Rust core, through the same C functions, runs in the
   browser, Unity, Unreal and the CLI. A behaviour is implemented once and tested once.
4. **Deterministic by default.** Same input, same bytes, same pixels. The compiler emits
   canonical bytes; the reference renderer is the picture every backend is compared to.
5. **Verified or it does not count.** Unexercised code is a draft. A feature is done when a
   test drives it end to end (to pixels where it draws) and `scripts/verify.sh` is green.
   Reports separate what was observed from what was inferred.
6. **Never panic on data.** Files are untrusted. Malformed input is an error value with a
   message that says where and what; tests corrupt every byte of a real file to prove it.
7. **Pay only for what changes.** `advance` reports whether the picture changed; hosts skip
   rendering when it did not. Caches are invalidated by writes, not rebuilt every frame.
8. **Small is a feature.** The player is tens of kilobytes. Authoring-only code (JSON,
   importers, shader translation, 3D file parsers) never ships in a runtime.
9. **Forward compatible forever.** Property and type keys are permanent. Old runtimes skip
   what they do not know; new runtimes read old files.
10. **Errors teach.** A compiler error names the JSON path, what was expected and what is
    allowed. An agent should be able to fix its own mistake from the message alone.
11. **Shortcuts are labelled.** A deliberate simplification carries a `ponytail:` comment
    naming its ceiling and the upgrade path. `grep -rn "ponytail:"` is the debt ledger.
12. **Human and agent maintainable.** Short files with a header comment that says what the
    file owns; plain names; no clever indirection; docs next to the code they describe.
