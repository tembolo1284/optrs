#!/usr/bin/env bash
# optrs/build.sh
# Staged build, CLI runner, and Python binding tests.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

BIN_RELEASE="$ROOT/target/release/optrs"
LIBDIR="$ROOT/target/release"
VENV="$ROOT/.venv"
PYBIND="$ROOT/bindings/python"

stage() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }
skip()  { printf '\n\033[1;33m--- skipped: %s\033[0m\n' "$1"; }

usage() {
cat <<'EOF'
optrs build and run script

BUILD
  ./build.sh              full pipeline: fmt, clippy, rust tests, release,
                          header, C smoke test, venv, python tests
  ./build.sh quick        rust tests only (skips fmt and clippy)
  ./build.sh test         rust tests, with fmt and clippy
  ./build.sh clean        cargo clean, remove .venv, then full pipeline
  ./build.sh fmt          cargo fmt --all (fixes formatting in place)

PYTHON
  ./build.sh venv         create .venv and install the bindings editable
  ./build.sh pytest       run the python binding tests (creates venv if needed)
  ./build.sh duffy        run the Duffy batches through the python bindings
  ./build.sh shell        drop into a python REPL with optrs importable

RUN
  ./build.sh run ARGS...  build release if needed, then run the CLI with ARGS
  ./build.sh examples     run a set of worked CLI examples
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

PYTHON USAGE
  source .venv/bin/activate
  python -c "import optrs; print(optrs.price(spot=60, strike=65, rate=0.08, vol=0.30, time=0.25))"

  The bindings locate the shared library by walking up to the workspace root
  and looking in target/release. Override with OPTRS_LIB_DIR if needed.

EXAMPLES
  ./build.sh run price -s 60 -k 65 -r 0.08 -v 0.30 -t 0.25 --kind call
  ./build.sh run compare -s 100 -k 100 -r 0.05 -q 0.02 -v 0.25 -t 1
  ./build.sh duffy
  ./build.sh pytest
EOF
}

need_cargo_build() {
    [[ ! -x "$1" ]] || [[ -n "$(find crates -name '*.rs' -newer "$1" -print -quit 2>/dev/null)" ]]
}

ensure_cli() {
    if need_cargo_build "$BIN_RELEASE"; then
        stage "building release CLI"
        cargo build --release -p optrs-cli
    fi
}

ensure_cdylib() {
    local lib="$LIBDIR/liboptrs.so"
    [[ "$(uname -s)" == "Darwin" ]] && lib="$LIBDIR/liboptrs.dylib"
    if need_cargo_build "$lib"; then
        stage "building release cdylib"
        cargo build --release -p optrs-cabi
    fi
    if [[ ! -f "$lib" ]]; then
        echo "error: $lib not produced" >&2
        exit 1
    fi
}

ensure_venv() {
    ensure_cdylib
    if [[ ! -d "$VENV" ]]; then
        stage "creating virtualenv at .venv"
        python3 -m venv "$VENV"
        "$VENV/bin/pip" install --quiet --upgrade pip
    fi
    # Reinstall when the package metadata changed; editable installs pick up
    # source edits automatically but not new dependencies.
    if [[ ! -f "$VENV/.installed" ]] || [[ "$PYBIND/pyproject.toml" -nt "$VENV/.installed" ]]; then
        stage "installing python bindings (editable)"
        "$VENV/bin/pip" install --quiet -e "$PYBIND[dev]"
        touch "$VENV/.installed"
    fi
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
        ensure_cli
        exec "$BIN_RELEASE" "$@"
        ;;
    examples)
        ensure_cli
        sep='--------------------------------------------------------------'

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
        exit 0
        ;;
    venv)
        ensure_venv
        stage "done"
        echo "activate with: source .venv/bin/activate"
        exit 0
        ;;
    pytest)
        ensure_venv
        stage "python binding tests"
        exec "$VENV/bin/pytest" "$PYBIND/tests" -v "$@"
        ;;
    duffy)
        ensure_venv
        stage "Duffy batches via python bindings"
        exec "$VENV/bin/python" "$PYBIND/examples/duffy.py" "$@"
        ;;
    shell)
        ensure_venv
        stage "python REPL"
        exec "$VENV/bin/python"
        ;;
    bench)
        stage "bench"
        exec cargo bench
        ;;
    clean)
        stage "clean"
        cargo clean
        rm -rf "$VENV"
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

stage "rust tests (debug)"
cargo test --workspace

if [[ "$MODE" == "test" || "$MODE" == "quick" ]]; then
    stage "stopping after rust tests"
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

stage "python bindings"
ensure_venv
"$VENV/bin/pytest" "$PYBIND/tests" -q

stage "Duffy batches (python)"
"$VENV/bin/python" "$PYBIND/examples/duffy.py"

stage "done"
echo "cdylib: $LIBDIR/liboptrs.so"
echo "header: $ROOT/include/optrs.h"
echo "cli:    $BIN_RELEASE"
echo "venv:   $VENV"
echo
echo "Try: ./build.sh examples   or   ./build.sh duffy"
