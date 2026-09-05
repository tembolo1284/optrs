#!/usr/bin/env bash
# optrs/build.sh
# Staged build: fmt, clippy, test, release cdylib, header, C smoke test.
# Python stages get appended here later, same shape as libfirisk.
#
# Usage:
#   ./build.sh              full pipeline
#   ./build.sh test         stop after the Rust tests
#   ./build.sh quick        tests only, skip fmt and clippy
#   ./build.sh clean        cargo clean, then full pipeline
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

MODE="${1:-all}"

stage() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }
skip()  { printf '\n\033[1;33m--- skipped: %s\033[0m\n' "$1"; }

if [[ "$MODE" == "clean" ]]; then
    stage "clean"
    cargo clean
    MODE="all"
fi

if [[ "$MODE" == "quick" ]]; then
    skip "format and lint"
else
    stage "format"
    cargo fmt --all -- --check

    stage "lint"
    cargo clippy --workspace --all-targets -- -D warnings
fi

stage "test (debug)"
cargo test --workspace

if [[ "$MODE" == "test" || "$MODE" == "quick" ]]; then
    stage "stopping after tests"
    exit 0
fi

stage "build (release)"
cargo build --workspace --release

stage "header"
if [[ ! -f include/optrs.h ]]; then
    echo "include/optrs.h not generated — check cbindgen output above"
    exit 1
fi
echo "include/optrs.h: $(wc -l < include/optrs.h) lines"

stage "C smoke test"
LIBDIR="$ROOT/target/release"
# The cdylib is named from [lib] name = "optrs" in optrs-cabi/Cargo.toml.
if [[ ! -f "$LIBDIR/liboptrs.so" ]]; then
    echo "liboptrs.so missing in $LIBDIR"
    ls "$LIBDIR"/*.so 2>/dev/null || echo "(no shared libraries built)"
    exit 1
fi
cc -O2 -Wall -Wextra -Iinclude \
   crates/optrs-cabi/tests/smoke.c \
   -L"$LIBDIR" -loptrs -lm \
   -Wl,-rpath,"$LIBDIR" \
   -o "$LIBDIR/smoke"
"$LIBDIR/smoke"

stage "done"
echo "cdylib: $LIBDIR/liboptrs.so"
echo "header: $ROOT/include/optrs.h"
echo "cli:    $LIBDIR/optrs"
