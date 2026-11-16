# Testing and verification

A change is incomplete until the relevant tests pass and `bash scripts/verify.sh` exits 0.
Tests are how this project keeps its zero-hallucination promise: if a behaviour is not
driven by a test, treat it as unverified, in code and in reports.

## Levels and where they live

| Level | What it proves | Where |
|---|---|---|
| Unit | One module's logic and its edge cases | `#[cfg(test)]` in `crates/zuri-core/src/*.rs` |
| Integration | The public API end to end, to pixels | `crates/zuri-core/tests/runtime.rs`, `crates/zuri-import/tests/import.rs` |
| Regression | The picture and the bytes do not drift | Golden PNGs in `crates/zuri-core/tests/golden`; byte-for-byte dump/rebuild fixpoints |
| Smoke across the ABI | Every host language can drive the runtime | `crates/zuri-ffi/tests/smoke.c` (C), `packages/web/test` (wasm in Node), `integrations/unity/Tests~/DotnetHarness` (C#) |
| Contract | Declared surface equals real surface | `crates/zuri-ffi/tests/abi.rs` (header vs exports), `verify.sh` docs step (schema reference vs code) |
| Robustness | Hostile input errors, never panics | `format` and `runtime` tests corrupt every byte of real files |
| End to end | A person's workflow works | CLI runs in `verify.sh`; editor and demo pages driven in a browser (Playwright) |
| Browser | The web player and the React bindings do what they say in a real browser | `packages/web/test/browser.mjs`, `packages/react/test/browser.mjs` (headless Chrome through `packages/web/test/chrome.mjs`; both in `verify.sh`) |
| Performance | No regression against the baseline | `zuri bench` numbers recorded in `docs/dev/rendering.md` |

## Rules
1. Every feature, bug fix and refactor comes with the test that fails without it.
2. Test behaviour through the public API; test to pixels when the feature draws.
3. A bug fix starts with a test that reproduces the bug.
4. Goldens are compared with a small tolerance (channel difference > 3 in under 0.05 % of
   values) that absorbs last-bit `libm` differences between platforms and nothing else.
   Regenerate with `ZURI_BLESS=1`, then look at every changed image before keeping it.
5. Cross-backend truth: the wasm build must match the native goldens (it does; that test
   is `wasm renders the same pixels as the native goldens`).
6. Never skip, weaken or delete a failing test to get green. A broken gate is a finding:
   write it in the handover log.
7. Validate incrementally: `verify.sh --fast` after each change, the full script before
   handing over.
8. Do not assert on things you did not check. A test named for a behaviour must drive it.

## What must be checked for a change (as applicable)
Functionality and edge cases; API contracts (C header, TypeScript types); architectural
integrity (schema is still the single source; no host re-implements runtime logic);
failure recovery (bad files, bad names, null handles); security assumptions (untrusted
input); user workflows (CLI, player, editor); determinism (same input, same bytes/pixels).
Concurrency: a handle is single-threaded by contract; if that changes, add tests first.

## Resource profiling and budgets
Measure, record, compare with the previous baseline, and treat a regression as a failure.

| Resource | How to measure | Baseline (2026-11-15, Apple M4, one core) |
|---|---|---|
| CPU per frame | `target/release/zuri bench <file> [--width W --height H]` | 0.14 - 0.48 ms at example sizes; 4.0 - 6.6 ms at 1920x1080 for 2D scenes; 3D model 4.1 ms at 400x400 |
| Advance cost | same command | 0.2 - 0.5 microseconds |
| Idle cost | Node: count renders over idle frames | 0 of 600 frames rendered |
| Memory (native) | `/usr/bin/time -l target/release/zuri render ...` | 4.8 MB at 400x300; 28 MB at 1920x1080 |
| Memory (wasm) | `runtime.x.memory.buffer.byteLength` | 3 MB after a 640x320 render |
| Size (wasm, brotli) | printed by `pnpm --filter @seedexr/zuri build`, which fails when the lite build is over `LITE_LIMIT` (80 KB) | 78 KB lite, 131 KB with text, 179 KB editor build |
| GPU | not applicable yet (no GPU backend) | - |
| Disk / network | file sizes from `zuri build` | hello 639 B, button 1.1 KB, text 30 KB (a subset font with its kerning; 111 KB with the whole font), model 43 KB (mesh) |

**Development-stack memory budget.** `research/memory_structure.md` leaves the target
figure blank ("memory budget of ... whenever feasible"). Until the project owner sets one,
the working rule is: the runtime stays within the baselines above (regressions over 10 %
need a justification in the handover log), and tooling should not need more than a normal
laptop. Replace this paragraph when the owner supplies the number.

## Engines
Unity and Unreal are verified by compiling and running inside the installed editors
(Unity 6000.4.8f1, UE 5.8 on this machine): `bash scripts/verify-engines.sh` (2026-11-15:
Unity EditMode 14 of 14, .NET harness OK, Unreal BuildPlugin and both automation tests;
2026-11-16: Unity EditMode 19, PlayMode 5 in each color space, Unreal 8). The Unreal run needs
an audio output (`Zuri.Sound` watches the audio device start a 60 ms click), and its
throwaway project turns off the editor's crash-report and usage uploads to Epic: an editor
that crashes unattended otherwise uploads its log, local paths included. Keep that file if
the gate's project setup changes. What was and was not run is recorded in each
integration's README and in `memory/handover_session.md`; never report an engine path as
working unless it was run in that engine.
