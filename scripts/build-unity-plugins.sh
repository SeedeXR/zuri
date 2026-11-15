#!/usr/bin/env bash
#
# Builds the native Zuri runtime for the Unity package (integrations/unity) and refreshes
# the .zur files the package ships.
#
#   bash scripts/build-unity-plugins.sh
#
# What it does:
#   1. builds crates/zuri-ffi for the system it runs on and puts the library where Unity
#      looks for it:
#        macOS    Plugins/macOS/libzuri.dylib            arm64 + x86_64, merged with lipo
#        Linux    Plugins/Linux/x86_64/libzuri.so
#        Windows  Plugins/Windows/x86_64/zuri.dll        (run it in Git Bash)
#      Unity ignores a library that has no .meta files. They are kept in PluginMeta~/
#      (which Unity does not look at) and copied next to the library when it is built: a
#      .meta left in Plugins/ without its library would be an orphan on the other systems.
#      The Linux and Windows ones were checked by importing placeholder libraries in Unity
#      on a Mac; nobody has opened the package in Unity on Linux or Windows yet.
#   2. compiles examples/*.json into Samples~/Basic and the test fixtures into Tests/Editor.
#
# Android, iOS and WebGL are listed at the bottom as commented commands. They have NOT been run.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGE="$ROOT/integrations/unity"
cd "$ROOT"

# host_plugin <Plugins subfolder> <library file> <rust target>: build, copy, add the metas.
host_plugin() {
    local out="$PACKAGE/Plugins/$1/x86_64/$2"
    # The explicit target keeps this build apart from target/release, which other steps build
    # without cargo-ship.sh's flags.
    bash scripts/cargo-ship.sh build -p zuri-ffi --release --target "$3"
    cp -R "$PACKAGE/PluginMeta~/$1" "$PACKAGE/PluginMeta~/$1.meta" "$PACKAGE/Plugins/"
    cp "target/$3/release/$2" "$out"
    bash scripts/cargo-ship.sh --check "$out"
    echo "built $out"
}

# The macOS library holds both architectures.
mac_plugin() {
    local target slices=()
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        # cargo-ship.sh keeps this machine's paths out of the binary (checked below).
        if bash scripts/cargo-ship.sh build -p zuri-ffi --release --target "$target"; then
            slices+=("target/$target/release/libzuri.dylib")
        else
            echo "warning: the $target slice did not build (try: rustup target add $target)" >&2
        fi
    done
    if [ ${#slices[@]} -eq 0 ]; then
        echo "error: no macOS slice built" >&2
        exit 1
    fi

    local out="$PACKAGE/Plugins/macOS/libzuri.dylib"
    cp -R "$PACKAGE/PluginMeta~/macOS" "$PACKAGE/PluginMeta~/macOS.meta" "$PACKAGE/Plugins/"
    lipo -create "${slices[@]}" -output "$out"
    strip -x "$out"                                   # local symbols only; the zuri_* exports stay
    install_name_tool -id @rpath/libzuri.dylib "$out" # replaces the absolute build path
    codesign --force --sign - "$out"                  # ad-hoc; arm64 refuses unsigned code
    bash scripts/cargo-ship.sh --check "$out"
    echo "built $out ($(lipo -archs "$out"))"
}

case "$(uname -s)" in
    Darwin) mac_plugin ;;
    Linux) host_plugin Linux libzuri.so x86_64-unknown-linux-gnu ;;
    MINGW* | MSYS* | CYGWIN*) host_plugin Windows zuri.dll x86_64-pc-windows-msvc ;;
    *) echo "error: no Unity plugin build for $(uname -s); see the commented commands at the end of $0" >&2; exit 1 ;;
esac

# ---- .zur files shipped in the package -----------------------------------------------

cargo build -p zuri-cli --release
zuri="$ROOT/target/release/zuri"
"$zuri" build examples/button.json -o "$PACKAGE/Samples~/Basic/button.zur"
"$zuri" build examples/hello.json -o "$PACKAGE/Samples~/Basic/hello.zur"
"$zuri" build examples/button.json -o "$PACKAGE/Tests/Editor/button.zur"
"$zuri" build "$PACKAGE/Tests~/Fixtures/image.json" -o "$PACKAGE/Tests/Editor/image.zur"
"$zuri" build "$PACKAGE/Tests~/Fixtures/enum.json" -o "$PACKAGE/Tests/Editor/enum.zur"

# ---- other platforms: commands, not run anywhere yet ----------------------------------
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
