#!/usr/bin/env bash
# Builds the native library and runs the P/Invoke harness against it. Usage: ./run.sh
set -euo pipefail
cd "$(dirname "$0")/../../../.."
cargo build -p zuri-ffi -p zuri-cli --release
./target/release/zuri build examples/button.json -o target/release/harness-button.zur
./target/release/zuri build "integrations/unity/Tests~/Fixtures/enum.json" -o target/release/harness-enum.zur
./target/release/zuri build examples/sound.json -o target/release/harness-sound.zur
# (The scene the Unreal plugin's scroll test uses: one fixture for both.)
./target/release/zuri build integrations/unreal/fixtures/scroll.json -o target/release/harness-scroll.zur
dotnet run --project "integrations/unity/Tests~/DotnetHarness" -- target/release/harness-button.zur target/release/harness-enum.zur target/release/harness-sound.zur target/release/harness-scroll.zur
