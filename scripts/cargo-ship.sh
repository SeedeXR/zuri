#!/usr/bin/env bash
# cargo, for binaries that ship (the npm wasm, the Unity and Unreal libraries).
#
#   bash scripts/cargo-ship.sh build -p zuri-ffi --release ...   cargo with path remapping
#   bash scripts/cargo-ship.sh --check <binary>...               fail if one embeds this build's paths
#
# Panic messages embed source paths, which would carry the builder's home directory into
# what we publish. The remap is passed as `--config build.rustflags`, so paths with spaces
# work and flags from cargo config files still apply. A RUSTFLAGS variable or a
# `target.<triple>.rustflags` entry replaces `build.rustflags` (cargo's rule), which is what
# `--check` is for: run it on every binary before it ships.
# (Cargo's own `trim-paths` profile option does all this, but is not stable in 1.98.)
set -euo pipefail
# Paths as the compiler writes them: on Windows (Git Bash) that is C:\... not /c/...
native() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
# Where this build's sources live: the repository, cargo's registry, the toolchain.
ROOT="$(native "$(cd "$(dirname "$0")/.." && pwd)")"
CARGO_DIR="$(native "${CARGO_HOME:-$HOME/.cargo}")"
SYSROOT="$(cd "$(dirname "$0")/.." && rustc --print sysroot)"

if [ "${1:-}" = "--check" ]; then
    shift
    for binary in "$@"; do
        # Those three, not $HOME: Rust's own standard library carries the home directory of
        # the machine that built it (/Users/runner on macOS, which is also GitHub's).
        for prefix in "$ROOT" "$CARGO_DIR" "$SYSROOT"; do
            if LC_ALL=C grep -qaF "$prefix" "$binary"; then
                echo "error: $binary embeds $prefix. Is RUSTFLAGS set, or target.<triple>.rustflags in a cargo config? They override the remap in $0." >&2
                exit 1
            fi
        done
    done
    exit 0
fi

remap() { printf "'--remap-path-prefix=%s=%s'" "$1" "$2"; }
exec cargo --config "build.rustflags=[$(remap "$ROOT" zuri), $(remap "$CARGO_DIR" cargo), $(remap "$SYSROOT" rust)]" "$@"
