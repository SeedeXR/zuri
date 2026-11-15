#!/usr/bin/env bash
# Compile every example scene to examples/dist/*.zur (used by the demo page and the plugins' samples).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p zuri-cli --release
mkdir -p examples/dist
for src in examples/*.json; do
  target/release/zuri build "$src" -o "examples/dist/$(basename "${src%.json}").zur"
done
