#!/usr/bin/env bash
# The gate, and the loop: run it after every change and before every commit.
#
# Deterministic: it uses no network and no LLM, so the same tree always gives the same
# verdict. Work is not done until this exits 0. Each step names what it proves.
#
#   bash scripts/verify.sh          # everything
#   bash scripts/verify.sh --fast   # Rust only (format, lint, tests): the inner loop
set -euo pipefail
cd "$(dirname "$0")/.."
FEATURES="zuri-core/json,zuri-core/text,zuri-ffi/json"
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "format: code is rustfmt-clean"
cargo fmt --all -- --check

step "lint: clippy with warnings as errors"
cargo clippy --workspace --all-targets --features "$FEATURES" -- -D warnings

step "rust tests: format, schema, geometry, raster, runtime goldens, C ABI, importers"
# Quiet when green; everything when red (a failure's reason is in the lines the summary drops).
if out=$(cargo test --workspace --features "$FEATURES" -q 2>&1); then
  grep -E "^test result" <<<"$out"
else
  echo "$out"
  exit 1
fi
# The player build (no features) is what ships in engines and the lite wasm; the workspace
# run above only ever sees the authoring build, because cargo unifies features.
cargo test -p zuri-core --lib -q 2>&1 | grep -E "^test result|FAILED|panicked|^error" ; [[ ${PIPESTATUS[0]} == 0 ]]

if [[ "${1:-}" == "--fast" ]]; then
  printf '\n\033[32mOK (fast)\033[0m\n'
  exit 0
fi

step "examples: every scene in examples/ builds"
bash scripts/build-examples.sh

step "docs: the schema reference matches the code"
target/release/zuri schema --markdown | diff -u docs/reference/schema.md - \
  || { echo "docs/reference/schema.md is stale. Run: target/release/zuri schema --markdown > docs/reference/schema.md"; exit 1; }

step "web: wasm builds, TypeScript compiles, wasm output matches the native goldens"
pnpm install --frozen-lockfile --silent
pnpm --filter @seedexr/zuri build
pnpm --filter @seedexr/zuri test
pnpm --filter @seedexr/zuri-react build

step "browser: player, pointer input, WebGL2 shader paints, text, 3D, custom element, React bindings (headless Chrome)"
node packages/web/test/browser.mjs
node packages/react/test/browser.mjs

if [[ -f apps/editor/package.json ]]; then
  step "editor: typecheck, unit tests, production build, and the editor driven in headless Chrome"
  pnpm --filter ./apps/editor run --if-present test
  pnpm --filter ./apps/editor build
  node apps/editor/test/browser.mjs
fi

if [[ -x integrations/unity/Tests~/DotnetHarness/run.sh ]] && command -v dotnet >/dev/null; then
  step "unity: C# bindings drive the native library (plain dotnet)"
  cargo build -q -p zuri-ffi --release
  integrations/unity/Tests~/DotnetHarness/run.sh
fi

step "graph: refresh the code knowledge graph (graphify, AST only)"
if command -v graphify >/dev/null; then
  graphify update . >/dev/null 2>&1 && echo "graphify-out/ updated (query it with: graphify query \"...\")"
else
  echo "graphify not installed; skipping (pip install graphifyy)"
fi

printf '\n\033[32mOK\033[0m\n'
