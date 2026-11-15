#!/usr/bin/env bash
# Builds the Zuri static library for the Unreal plugin and copies it, with zuri.h, into
#   integrations/unreal/Zuri/Source/ThirdParty/ZuriLibrary/{include,lib/<Platform>}
#
#   bash scripts/build-unreal-libs.sh              # this host's platform
#   bash scripts/build-unreal-libs.sh mac win64    # explicit list: mac | win64 | linux
#
# mac    universal (arm64 + x86_64) libzuri.a. Needs a Mac (lipo).
# win64  zuri.lib for x86_64-pc-windows-msvc.
# linux  libzuri.a for x86_64-unknown-linux-gnu.
#
# A Rust staticlib is an archive, so no linker runs and any host with the Rust target
# installed (rustup target add <triple>) can produce win64/linux. Only the Mac library has
# been linked into Unreal and run; build and test the others on their own hosts in CI.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLUGIN="$ROOT/integrations/unreal/Zuri"
DEST="$PLUGIN/Source/ThirdParty/ZuriLibrary"

# build <rust-triple>: prints the directory holding the built archive.
build() {
    bash "$ROOT/scripts/cargo-ship.sh" rustc --manifest-path "$ROOT/Cargo.toml" -p zuri-ffi --release --target "$1" --crate-type staticlib >&2
    echo "$ROOT/target/$1/release"
}

if [ $# -eq 0 ]; then
    case "$(uname -s)" in
        Darwin) set -- mac ;;
        Linux) set -- linux ;;
        *) set -- win64 ;;
    esac
fi

rm -rf "$DEST/include"
mkdir -p "$DEST/include"
cp "$ROOT/crates/zuri-ffi/include/zuri.h" "$DEST/include/zuri.h"

for platform in "$@"; do
    case "$platform" in
        mac)
            rm -rf "$DEST/lib/Mac" && mkdir -p "$DEST/lib/Mac"
            lipo -create "$(build aarch64-apple-darwin)/libzuri.a" "$(build x86_64-apple-darwin)/libzuri.a" -output "$DEST/lib/Mac/libzuri.a"
            ;;
        win64)
            rm -rf "$DEST/lib/Win64" && mkdir -p "$DEST/lib/Win64"
            cp "$(build x86_64-pc-windows-msvc)/zuri.lib" "$DEST/lib/Win64/zuri.lib"
            ;;
        linux)
            rm -rf "$DEST/lib/Linux" && mkdir -p "$DEST/lib/Linux"
            cp "$(build x86_64-unknown-linux-gnu)/libzuri.a" "$DEST/lib/Linux/libzuri.a"
            ;;
        *)
            echo "unknown platform '$platform' (expected mac, win64 or linux)" >&2
            exit 1
            ;;
    esac
done

# The scenes the plugin's automation tests load, rebuilt here so they track the file format.
mkdir -p "$PLUGIN/Resources"
for scene in "$ROOT/examples/button.json" "$ROOT/integrations/unreal/fixtures/image.json" "$ROOT/integrations/unreal/fixtures/enum.json" "$ROOT/integrations/unreal/fixtures/keys.json"; do
    cargo run --manifest-path "$ROOT/Cargo.toml" -p zuri-cli --release --quiet -- build "$scene" -o "$PLUGIN/Resources/$(basename "$scene" .json).zur"
done

find "$DEST/lib" -type f -exec bash "$ROOT/scripts/cargo-ship.sh" --check {} +
find "$DEST/include" "$DEST/lib" -type f -exec ls -l {} +
