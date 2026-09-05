#!/usr/bin/env bash
# optrs/build.sh
# Staged build plus a runner for the CLI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

BIN_DEBUG="$ROOT/target/debug/optrs"
BIN_RELEASE="$ROOT/target/release/optrs"

stage() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }
skip()  { printf '\n\033[1;33m--- skipped: %s\033[0m\n' "$1"; }

usage() {
cat <<'EOF'
optrs build and run script

BUILD
  ./build.sh              full pipeline: fmt, clippy, test, release, header, C smoke
  ./build.sh quick        tests only (skips fmt and clippy)
  ./build.sh test         tests, with fmt and clippy
  ./build.sh clean        cargo clean, then full pipeline
  ./build.sh fmt          cargo fmt --all (fixes formatting in place)

RUN
  ./build.sh run ARGS...  build release if needed, then run the CLI with ARGS
  ./build.sh examples     run a set of worked examples
  ./build.sh bench        cargo bench

CLI USAGE
  optrs price   -s SPOT -k STRIKE -v VOL -t TIME [options]
  optrs compare -s SPOT -k STRIKE -v VOL -t TIME [options]

  Required:
    -s, --spot        underlying spot price
    -k, --strike      strike
    -v, --vol         volatility, annualised (0.30 = 30%)
    -t, --time        time to expiry in years

  Optional:
    -r, --rate        risk-free rate            (default 0.0)
    -q, --div-yield   continuous dividend yield (default 0.0)
        --kind        call | put                (default call)
        --american    American exercise
        --bermudan    comma-separated dates in years, e.g. 0.25,0.5,0.75,1.0
    -e, --engine      analytic | cos | tree-crr | tree-lr | fd | mc
        --greeks      print full greeks instead of just price   (price only)
        --converged   refine until tolerance is met             (price only)
        --tree-steps  override lattice steps
        --mc-paths    override Monte Carlo path count
        --fd-steps    override finite-difference grid steps

EXAMPLES
  ./build.sh run price -s 60 -k 65 -r 0.08 -v 0.30 -t 0.25 --kind call
  ./build.sh run compare -s 100 -k 100 -r 0.05 -q 0.02 -v 0.25 -t 1
  ./build.sh run price -s 100 -k 100 -r 0.05 -v 0.25 -t 1 --kind put --american -e tree-lr
EOF
}

ensure_release() {
    if [[ ! -x "$BIN_RELEASE" || -n "$(find crates -name '*.rs' -newer "$BIN_RELEASE" -print -quit 2>/dev/null)" ]]; then
        stage "building release binary"
        cargo build --release -p optrs-cli
    fi
}

run_examples() {
    ensure_release
    local sep='--------------------------------------------------------------'

    stage "Duffy batch 1 — call, expect 2.13337"
    "$BIN_RELEASE" price -s 60 -k 65 -r 0.08 -v 0.30 -t 0.25 --kind call
    echo "$sep"

    stage "Duffy batch 1 — put, expect 5.84628"
    "$BIN_RELEASE" price -s 60 -k 65 -r 0.08 -v 0.30 -t 0.25 --kind put
    echo "$sep"

    stage "Duffy batch 4 — call, expect 92.17570 (T=30 stress case)"
    "$BIN_RELEASE" price -s 100 -k 100 -r 0.08 -v 0.30 -t 30 --kind call
    echo "$sep"

    stage "all engines, European call"
    "$BIN_RELEASE" compare -s 100 -k 100 -r 0.05 -q 0.02 -v 0.25 -t 1
    echo "$sep"

    stage "all engines, American put (analytic and cos drop out)"
    "$BIN_RELEASE" compare -s 100 -k 100 -r 0.05 -q 0.02 -v 0.25 -t 1 --kind put --american
    echo "$sep"

    stage "Bermudan put, quarterly exercise"
    "$BIN_RELEASE" price -s 100 -k 100 -r 0.05 -v 0.25 -t 1 --kind put \
        --bermudan 0.25,0.5,0.75,1.0 -e tree-lr
    echo "$sep"

    stage "greeks off the finite-difference engine"
    "$BIN_RELEASE" price -s 100 -k 100 -r 0.05 -q 0.02 -v 0.25 -t 1 -e fd --greeks
    echo "$sep"

    stage "convergence ladder"
    "$BIN_RELEASE" price -s 100 -k 100 -r 0.05 -v 0.25 -t 1 -e tree-lr --converged
}

MODE="${1:-all}"
[[ $# -gt 0 ]] && shift || true

case "$MODE" in
    -h|--help|help)
        usage
        exit 0
        ;;
    fmt)
        stage "format (in place)"
        cargo fmt --all
        exit 0
        ;;
    run)
        if [[ $# -eq 0 ]]; then
            echo "error: 'run' needs CLI arguments. See ./build.sh help" >&2
            exit 1
        fi
        ensure_release
        exec "$BIN_RELEASE" "$@"
        ;;
    examples)
        run_examples
        exit 0
        ;;
    bench)
        stage "bench"
        exec cargo bench
        ;;
    clean)
        stage "clean"
        cargo clean
        MODE="all"
        ;;
    all|quick|test)
        ;;
    *)
        echo "error: unknown mode '$MODE'" >&2
        echo
        usage
        exit 1
        ;;
esac

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
echo "cli:    $BIN_RELEASE"
echo
echo "Try: ./build.sh examples"
