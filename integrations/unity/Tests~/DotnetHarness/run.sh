#!/usr/bin/env bash
# Builds the native library and runs the P/Invoke harness against it. Usage: ./run.sh
set -euo pipefail
cd "$(dirname "$0")/../../../.."
cargo build -p zuri-ffi -p zuri-cli --release
./target/release/zuri build examples/button.json -o target/release/harness-button.zur
dotnet run --project "integrations/unity/Tests~/DotnetHarness" -- target/release/harness-button.zur
