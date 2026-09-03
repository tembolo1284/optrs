#!/usr/bin/env bash
# optrs/build.sh
# Staged build: fmt, clippy, test, release cdylib, header, C smoke test.
# Python stages get appended here later, same shape as libfirisk.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

stage() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }

stage "format"
cargo fmt --all -- --check

stage "lint"
cargo clippy --workspace --all-targets -- -D warnings

stage "test (debug)"
cargo test --workspace

stage "build (release)"
cargo build --workspace --release

stage "header"
test -f include/optrs.h || { echo "include/optrs.h not generated"; exit 1; }
echo "include/optrs.h $(wc -l < include/optrs.h) lines"

stage "C smoke test"
LIBDIR="$ROOT/target/release"
cc -O2 -Wall -Wextra -Iinclude \
   crates/optrs-cabi/tests/smoke.c \
   -L"$LIBDIR" -loptrs -lm \
   -Wl,-rpath,"$LIBDIR" \
   -o "$LIBDIR/smoke"
"$LIBDIR/smoke"

stage "done"
echo "cdylib: $LIBDIR/liboptrs.so"
echo "header: $ROOT/include/optrs.h"
