# bindings/python/optrs/_ffi.py
"""CFFI loader for liboptrs. ABI mode: we declare the header subset we use
rather than compiling against it, so no C toolchain is needed to install."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from cffi import FFI

_CDEF = """
typedef enum {
    OPT_STATUS_OK = 0,
    OPT_STATUS_DOMAIN = 1,
    OPT_STATUS_NO_SOLUTION = 2,
    OPT_STATUS_UNSUPPORTED = 3,
    OPT_STATUS_NOT_CONVERGED = 4,
    OPT_STATUS_NULL_POINTER = 5,
    OPT_STATUS_BAD_SIZE = 6,
    OPT_STATUS_BUFFER_TOO_SMALL = 7,
    OPT_STATUS_PANIC = 8
} opt_status_t;

typedef enum { OPT_KIND_CALL = 0, OPT_KIND_PUT = 1 } opt_kind_t;

typedef enum {
    OPT_STYLE_EUROPEAN = 0,
    OPT_STYLE_AMERICAN = 1,
    OPT_STYLE_BERMUDAN = 2
} opt_style_t;

typedef struct {
    uint32_t size;
    opt_kind_t kind;
    opt_style_t style;
    uint32_t _pad;
    double spot;
    double strike;
    double rate;
    double div_yield;
    double vol;
    double time;
    const double *dates;
    size_t n_dates;
} opt_option_t;

typedef struct {
    uint32_t size;
    int32_t engine;
    int32_t has_std_error;
    int32_t has_greeks;
    double price;
    double std_error;
    double delta;
    double gamma;
    double vega;
    double theta;
    double rho;
    uint32_t refinements;
    int32_t extrapolated;
} opt_result_t;

typedef struct opt_pricer_t opt_pricer_t;

const char *opt_version(void);
uint32_t opt_abi_version(void);
size_t opt_sizeof_option(void);
size_t opt_sizeof_result(void);

const char *opt_last_error_message(void);
void opt_clear_error(void);

opt_pricer_t *opt_pricer_new(void);
void opt_pricer_free(opt_pricer_t *p);
opt_status_t opt_pricer_reset(opt_pricer_t *p);

opt_status_t opt_set_tree_steps(opt_pricer_t *p, size_t value);
opt_status_t opt_set_fd_space_steps(opt_pricer_t *p, size_t value);
opt_status_t opt_set_fd_time_steps(opt_pricer_t *p, size_t value);
opt_status_t opt_set_fd_width(opt_pricer_t *p, double value);
opt_status_t opt_set_mc_paths(opt_pricer_t *p, size_t value);
opt_status_t opt_set_mc_steps(opt_pricer_t *p, size_t value);
opt_status_t opt_set_mc_seed(opt_pricer_t *p, uint64_t value);
opt_status_t opt_set_cos_terms(opt_pricer_t *p, size_t value);
opt_status_t opt_set_tolerance(opt_pricer_t *p, double value);
opt_status_t opt_set_max_refinements(opt_pricer_t *p, size_t value);
opt_status_t opt_set_spot_bump(opt_pricer_t *p, double value);
opt_status_t opt_set_vol_bump(opt_pricer_t *p, double value);
opt_status_t opt_set_mc_antithetic(opt_pricer_t *p, int32_t on);
opt_status_t opt_set_mc_control_variate(opt_pricer_t *p, int32_t on);
opt_status_t opt_set_richardson(opt_pricer_t *p, int32_t on);

const char *opt_engine_name(int32_t engine);
int32_t opt_engine_count(void);
opt_status_t opt_engine_supports(int32_t engine, const opt_option_t *option, int32_t *out);

opt_status_t opt_option_init(opt_option_t *out);
opt_status_t opt_result_init(opt_result_t *out);

opt_status_t opt_price(const opt_pricer_t *p, int32_t engine,
                       const opt_option_t *option, opt_result_t *out);
opt_status_t opt_price_auto(const opt_pricer_t *p,
                            const opt_option_t *option, opt_result_t *out);
opt_status_t opt_price_converged(const opt_pricer_t *p, int32_t engine,
                                 const opt_option_t *option, opt_result_t *out);
opt_status_t opt_greeks(const opt_pricer_t *p, int32_t engine,
                        const opt_option_t *option, opt_result_t *out);
opt_status_t opt_price_all(const opt_pricer_t *p, const opt_option_t *option,
                           opt_result_t *results, size_t capacity, size_t *n_written);
opt_status_t opt_implied_vol(const opt_option_t *option, double target_price, double *out);
"""

ffi = FFI()
ffi.cdef(_CDEF)


def _library_name() -> str:
    if sys.platform == "darwin":
        return "liboptrs.dylib"
    if sys.platform == "win32":
        return "optrs.dll"
    return "liboptrs.so"


def _candidate_paths() -> list[Path]:
    name = _library_name()
    paths: list[Path] = []

    env = os.environ.get("OPTRS_LIB_DIR")
    if env:
        paths.append(Path(env) / name)

    # Bundled inside the wheel.
    paths.append(Path(__file__).parent / name)

    # Development: walk up to the workspace root and look in target/.
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "Cargo.toml").exists() and (parent / "crates").is_dir():
            paths.append(parent / "target" / "release" / name)
            paths.append(parent / "target" / "debug" / name)
            break

    return paths


def _load():
    tried = []
    for path in _candidate_paths():
        tried.append(str(path))
        if path.exists():
            return ffi.dlopen(str(path))
    raise OSError(
        "could not locate the optrs shared library.\n"
        "Build it with `cargo build -p optrs-cabi --release`, or set "
        "OPTRS_LIB_DIR to the directory containing it.\n"
        "Searched:\n  " + "\n  ".join(tried)
    )


lib = _load()

# Fail loudly at import if the loaded library disagrees with the declarations
# above. Silent struct-layout drift corrupts memory instead of raising.
_EXPECTED_ABI = 1
if lib.opt_abi_version() != _EXPECTED_ABI:
    raise RuntimeError(
        f"optrs ABI mismatch: library reports {lib.opt_abi_version()}, "
        f"bindings expect {_EXPECTED_ABI}"
    )
if lib.opt_sizeof_option() != ffi.sizeof("opt_option_t"):
    raise RuntimeError("opt_option_t layout mismatch between library and bindings")
if lib.opt_sizeof_result() != ffi.sizeof("opt_result_t"):
    raise RuntimeError("opt_result_t layout mismatch between library and bindings")
