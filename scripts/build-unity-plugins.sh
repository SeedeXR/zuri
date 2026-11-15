#!/usr/bin/env bash
#
# Builds the native Zuri runtime for the Unity package (integrations/unity) and refreshes
# the .zur files the package ships.
#
#   bash scripts/build-unity-plugins.sh
#
# What it does on macOS (the only host it has been run on):
#   1. builds crates/zuri-ffi for arm64 and x86_64 and merges them with lipo into
#      integrations/unity/Plugins/macOS/libzuri.dylib (arm64 only if the x86_64 slice fails);
#   2. compiles examples/*.json into Samples~/Basic and the test fixtures into Tests/Editor.
#
# The other platforms are listed at the bottom as commented commands for CI on those hosts.
# They have NOT been run. Each new binary also needs a .meta with plugin import settings
# (select the file in Unity, tick its platform, and commit the .meta Unity writes).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGE="$ROOT/integrations/unity"
cd "$ROOT"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "error: only the macOS build is automated; see the commented commands at the end of $0" >&2
    exit 1
fi

# Keep this machine's paths out of the shipped binary (panic messages embed source paths).
# Passed to the plugin builds only: they use target/<triple>, so target/release keeps its cache.
plugin_rustflags="${RUSTFLAGS:-} --remap-path-prefix=$ROOT=zuri --remap-path-prefix=$HOME/.cargo=cargo --remap-path-prefix=$(rustc --print sysroot)=rust"

# ---- macOS: universal dylib ----------------------------------------------------------

slices=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if RUSTFLAGS="$plugin_rustflags" cargo build -p zuri-ffi --release --target "$target"; then
        slices+=("target/$target/release/libzuri.dylib")
    else
        echo "warning: the $target slice did not build (try: rustup target add $target)" >&2
    fi
done
if [ ${#slices[@]} -eq 0 ]; then
    echo "error: no macOS slice built" >&2
    exit 1
fi

out="$PACKAGE/Plugins/macOS/libzuri.dylib"
mkdir -p "$(dirname "$out")"
lipo -create "${slices[@]}" -output "$out"
strip -x "$out"                                   # local symbols only; the zuri_* exports stay
install_name_tool -id @rpath/libzuri.dylib "$out" # replaces the absolute build path
codesign --force --sign - "$out"                  # ad-hoc; arm64 refuses unsigned code
echo "built $out ($(lipo -archs "$out"))"

# ---- .zur files shipped in the package -----------------------------------------------

cargo build -p zuri-cli --release
zuri="$ROOT/target/release/zuri"
"$zuri" build examples/button.json -o "$PACKAGE/Samples~/Basic/button.zur"
"$zuri" build examples/hello.json -o "$PACKAGE/Samples~/Basic/hello.zur"
"$zuri" build examples/button.json -o "$PACKAGE/Tests/Editor/button.zur"
"$zuri" build "$PACKAGE/Tests~/Fixtures/image.json" -o "$PACKAGE/Tests/Editor/image.zur"

# ---- other platforms: commands for CI, not run here ----------------------------------
#
# Windows x86_64 (on a Windows host with the MSVC toolchain):
#   cargo build -p zuri-ffi --release --target x86_64-pc-windows-msvc
#   copy target\x86_64-pc-windows-msvc\release\zuri.dll integrations\unity\Plugins\Windows\x86_64\zuri.dll
#
# Linux x86_64 (on a Linux host):
#   cargo build -p zuri-ffi --release --target x86_64-unknown-linux-gnu
#   cp target/x86_64-unknown-linux-gnu/release/libzuri.so integrations/unity/Plugins/Linux/x86_64/libzuri.so
#
# Android (any host, with the NDK and `cargo install cargo-ndk`):
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o integrations/unity/Plugins/Android/libs \
#       build -p zuri-ffi --release
#   (writes Plugins/Android/libs/<abi>/libzuri.so; set each .meta to Android + its CPU)
#
# iOS (on macOS with Xcode; a static library, linked as "__Internal"):
#   rustup target add aarch64-apple-ios
#   cargo build -p zuri-ffi --release --target aarch64-apple-ios
#   cp target/aarch64-apple-ios/release/libzuri.a integrations/unity/Plugins/iOS/libzuri.a
#
# WebGL: Unity links with Emscripten, so this needs the wasm32-unknown-emscripten target
# (not wasm32-unknown-unknown). Not attempted yet:
#   rustup target add wasm32-unknown-emscripten
#   cargo build -p zuri-ffi --release --target wasm32-unknown-emscripten
#   cp target/wasm32-unknown-emscripten/release/libzuri.a integrations/unity/Plugins/WebGL/libzuri.a
