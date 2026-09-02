// crates/optrs-cabi/src/lib.rs
//! Flat C ABI for optrs. Symbol prefix `opt_`, opaque handles, versioned PODs,
//! thread-local error channel, no panics across the boundary.
//!
//! Every exported function follows the same shape:
//!   - returns opt_status_t (0 == success)
//!   - writes results through out-parameters
//!   - on failure, opt_last_error_message() holds a description

pub mod error;
pub mod handle;
pub mod types;

use error::{guard, record, require_non_null, set};
use handle::Pricer;
use optrs_engine::{Engine, PriceResult};
use std::ffi::{c_char, CStr};
use types::{OptOption, OptResult, OptStatus, OPT_ABI_VERSION};

// ---------------------------------------------------------------- versioning

/// Semantic version of the library as a NUL-terminated static string.
#[no_mangle]
pub extern "C" fn opt_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// ABI version. Bump only on a breaking layout change. Callers should compare
/// this against the value they compiled against and refuse to run on mismatch.
#[no_mangle]
pub extern "C" fn opt_abi_version() -> u32 {
    OPT_ABI_VERSION
}

/// Byte size of `opt_option_t` as this build sees it. Lets a binding verify
/// struct layout agreement at load time instead of corrupting memory later.
#[no_mangle]
pub extern "C" fn opt_sizeof_option() -> usize {
    std::mem::size_of::<OptOption>()
}

#[no_mangle]
pub extern "C" fn opt_sizeof_result() -> usize {
    std::mem::size_of::<OptResult>()
}

// -------------------------------------------------------------------- errors

/// Message for the most recent failure on this thread, or NULL if none.
/// Valid until the next optrs call on this thread. Copy it if you need to keep it.
#[no_mangle]
pub extern "C" fn opt_last_error_message() -> *const c_char {
    error::last_message()
}

#[no_mangle]
pub extern "C" fn opt_clear_error() {
    error::clear();
}

// ------------------------------------------------------------------- handles

/// Create a pricer with default configuration. Returns NULL only on allocation
/// failure. Free with `opt_pricer_free`.
#[no_mangle]
pub extern "C" fn opt_pricer_new() -> *mut Pricer {
    match std::panic::catch_unwind(|| Box::into_raw(Box::new(Pricer::new()))) {
        Ok(p) => p,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a pricer. Safe to call with NULL. Double-free is undefined.
///
/// # Safety
/// `p` must be NULL or a pointer from `opt_pricer_new` not yet freed.
#[no_mangle]
pub unsafe extern "C" fn opt_pricer_free(p: *mut Pricer) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

// ------------------------------------------------------------------- setters

/// Macro for the scalar setters: they all null-check, validate, and assign.
macro_rules! setter {
    ($name:ident, $ty:ty, $field:expr, $check:expr, $msg:literal) => {
        /// # Safety
        /// `p` must be a live handle from `opt_pricer_new`.
        #[no_mangle]
        pub unsafe extern "C" fn $name(p: *mut Pricer, value: $ty) -> OptStatus {
            guard(|| {
                require_non_null(p, "pricer")?;
                let check: fn($ty) -> bool = $check;
                if !check(value) {
                    set($msg);
                    return Err(OptStatus::Domain);
                }
                let pricer = handle::as_mut(p);
                let f: fn(&mut Pricer, $ty) = $field;
                f(pricer, value);
                Ok(())
            })
        }
    };
}

setter!(opt_set_tree_steps, usize, |p, v| p.config.tree.steps = v,
        |v| v >= 3, "tree steps must be at least 3");
setter!(opt_set_fd_space_steps, usize, |p, v| p.config.fd.space_steps = v,
        |v| v >= 4, "fd space steps must be at least 4");
setter!(opt_set_fd_time_steps, usize, |p, v| p.config.fd.time_steps = v,
        |v| v >= 3, "fd time steps must be at least 3");
setter!(opt_set_fd_width, f64, |p, v| p.config.fd.width = v,
        |v: f64| v.is_finite() && v > 0.0, "fd grid width must be positive");
setter!(opt_set_mc_paths, usize, |p, v| p.config.mc.paths = v,
        |v| v >= 2, "mc paths must be at least 2");
setter!(opt_set_mc_steps, usize, |p, v| p.config.mc.steps = v,
        |v| v >= 1, "mc steps must be at least 1");
setter!(opt_set_mc_seed, u64, |p, v| p.config.mc.seed = v,
        |_| true, "");
setter!(opt_set_cos_terms, usize, |p, v| p.config.cos.terms = v,
        |v| v >= 8, "cos terms must be at least 8");
setter!(opt_set_tolerance, f64, |p, v| p.config.convergence.tolerance = v,
        |v: f64| v.is_finite() && v > 0.0, "tolerance must be positive");
setter!(opt_set_max_refinements, usize, |p, v| p.config.convergence.max_refinements = v,
        |v| v >= 1, "max refinements must be at least 1");
setter!(opt_set_spot_bump, f64, |p, v| p.config.greeks.spot_rel = v,
        |v: f64| v.is_finite() && v > 0.0, "spot bump must be positive");
setter!(opt_set_vol_bump, f64, |p, v| p.config.greeks.vol_abs = v,
        |v: f64| v.is_finite() && v > 0.0, "vol bump must be positive");

/// Toggle flags take int for C89-friendliness; non-zero means true.
///
/// # Safety
/// `p` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn opt_set_mc_antithetic(p: *mut Pricer, on: i32) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        handle::as_mut(p).config.mc.antithetic = on != 0;
        Ok(())
    })
}

/// # Safety
/// `p` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn opt_set_mc_control_variate(p: *mut Pricer, on: i32) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        handle::as_mut(p).config.mc.control_variate = on != 0;
        Ok(())
    })
}

/// # Safety
/// `p` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn opt_set_richardson(p: *mut Pricer, on: i32) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        handle::as_mut(p).config.convergence.richardson = on != 0;
        Ok(())
    })
}

/// Restore every knob to its default.
///
/// # Safety
/// `p` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn opt_pricer_reset(p: *mut Pricer) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        handle::as_mut(p).config = Default::default();
        Ok(())
    })
}

// ------------------------------------------------------------------- engines

fn engine_from_i32(v: i32) -> Result<Engine, OptStatus> {
    Engine::ALL
        .iter()
        .copied()
        .find(|e| *e as i32 == v)
        .ok_or_else(|| {
            set(format!("unknown engine id {v}"));
            OptStatus::Domain
        })
}

/// Human-readable engine name, or NULL for an unknown id.
#[no_mangle]
pub extern "C" fn opt_engine_name(engine: i32) -> *const c_char {
    // Static table so the returned pointer outlives any call.
    const NAMES: [&str; 6] = [
        "analytic\0", "cos\0", "tree-crr\0", "tree-lr\0",
        "finite-difference\0", "monte-carlo\0",
    ];
    match usize::try_from(engine).ok().and_then(|i| NAMES.get(i)) {
        Some(s) => s.as_ptr() as *const c_char,
        None => std::ptr::null(),
    }
}

/// Number of engines. Ids are contiguous in `[0, opt_engine_count())`.
#[no_mangle]
pub extern "C" fn opt_engine_count() -> i32 {
    Engine::ALL.len() as i32
}

/// Writes 1 into `*out` when the engine can price this option, 0 otherwise.
///
/// # Safety
/// `option` and `out` must be valid pointers; see `OptOption::to_request`.
#[no_mangle]
pub unsafe extern "C" fn opt_engine_supports(
    engine: i32,
    option: *const OptOption,
    out: *mut i32,
) -> OptStatus {
    guard(|| {
        require_non_null(option, "option")?;
        require_non_null(out, "out")?;
        let e = engine_from_i32(engine)?;
        let req = (*option).to_request().map_err(|err| record(&err))?;
        *out = e.supports(&req) as i32;
        Ok(())
    })
}

// -------------------------------------------------------------------- pricing

fn fill(out: &mut OptResult, r: &PriceResult, refinements: u32, extrapolated: bool) {
    out.engine = r.engine as i32;
    out.price = r.price;
    match r.std_error {
        Some(se) => {
            out.std_error = se;
            out.has_std_error = 1;
        }
        None => {
            out.std_error = 0.0;
            out.has_std_error = 0;
        }
    }
    match r.greeks {
        Some(g) => {
            out.delta = g.delta;
            out.gamma = g.gamma;
            out.vega = g.vega;
            out.theta = g.theta;
            out.rho = g.rho;
            out.has_greeks = 1;
        }
        None => out.has_greeks = 0,
    }
    out.refinements = refinements;
    out.extrapolated = extrapolated as i32;
}

/// Price with an explicit engine at the configured discretisation.
///
/// # Safety
/// All pointers must be valid; `out->size` must be set by the caller.
#[no_mangle]
pub unsafe extern "C" fn opt_price(
    p: *const Pricer,
    engine: i32,
    option: *const OptOption,
    out: *mut OptResult,
) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        require_non_null(option, "option")?;
        require_non_null(out, "result")?;
        (*out).check_size().map_err(|e| record(&e))?;

        let e = engine_from_i32(engine)?;
        let req = (*option).to_request().map_err(|err| record(&err))?;
        let cfg = &handle::as_ref(p).config;
        let r = e.price_raw(&req, cfg).map_err(|err| record(&err))?;
        fill(&mut *out, &r, 0, false);
        Ok(())
    })
}

/// Price with the cheapest engine supporting the option.
///
/// # Safety
/// As `opt_price`.
#[no_mangle]
pub unsafe extern "C" fn opt_price_auto(
    p: *const Pricer,
    option: *const OptOption,
    out: *mut OptResult,
) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        require_non_null(option, "option")?;
        require_non_null(out, "result")?;
        (*out).check_size().map_err(|e| record(&e))?;

        let req = (*option).to_request().map_err(|err| record(&err))?;
        let cfg = &handle::as_ref(p).config;
        let r = optrs_engine::price_auto(&req, cfg).map_err(|err| record(&err))?;
        fill(&mut *out, &r, 0, false);
        Ok(())
    })
}

/// Refine the discretisation until successive prices agree within tolerance.
///
/// # Safety
/// As `opt_price`.
#[no_mangle]
pub unsafe extern "C" fn opt_price_converged(
    p: *const Pricer,
    engine: i32,
    option: *const OptOption,
    out: *mut OptResult,
) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        require_non_null(option, "option")?;
        require_non_null(out, "result")?;
        (*out).check_size().map_err(|e| record(&e))?;

        let e = engine_from_i32(engine)?;
        let req = (*option).to_request().map_err(|err| record(&err))?;
        let cfg = &handle::as_ref(p).config;
        let rep = optrs_engine::price_converged(e, &req, cfg).map_err(|err| record(&err))?;
        fill(&mut *out, &rep.result, rep.refinements as u32, rep.extrapolated);
        Ok(())
    })
}

/// Full greeks. Always sets `has_greeks`.
///
/// # Safety
/// As `opt_price`.
#[no_mangle]
pub unsafe extern "C" fn opt_greeks(
    p: *const Pricer,
    engine: i32,
    option: *const OptOption,
    out: *mut OptResult,
) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        require_non_null(option, "option")?;
        require_non_null(out, "result")?;
        (*out).check_size().map_err(|e| record(&e))?;

        let e = engine_from_i32(engine)?;
        let req = (*option).to_request().map_err(|err| record(&err))?;
        let cfg = &handle::as_ref(p).config;
        let g = optrs_engine::greeks(e, &req, cfg).map_err(|err| record(&err))?;

        let out = &mut *out;
        out.engine = e as i32;
        out.price = g.price;
        out.delta = g.delta;
        out.gamma = g.gamma;
        out.vega = g.vega;
        out.theta = g.theta;
        out.rho = g.rho;
        out.has_greeks = 1;
        out.has_std_error = 0;
        Ok(())
    })
}

/// Price under every supporting engine. Writes up to `capacity` entries into
/// `results` and the actual count into `*n_written`. Pass a capacity of
/// `opt_engine_count()` to be safe. Each entry must have `size` pre-set.
///
/// Individual engine failures are not fatal: the entry is written with
/// `engine = -1` and skipped, so one bad engine does not lose the others.
///
/// # Safety
/// `results` must point to `capacity` writable `opt_result_t`.
#[no_mangle]
pub unsafe extern "C" fn opt_price_all(
    p: *const Pricer,
    option: *const OptOption,
    results: *mut OptResult,
    capacity: usize,
    n_written: *mut usize,
) -> OptStatus {
    guard(|| {
        require_non_null(p, "pricer")?;
        require_non_null(option, "option")?;
        require_non_null(results, "results")?;
        require_non_null(n_written, "n_written")?;

        let req = (*option).to_request().map_err(|err| record(&err))?;
        let cfg = &handle::as_ref(p).config;
        let all = optrs_engine::price_all(&req, cfg);

        if capacity < all.len() {
            set(format!("results buffer holds {capacity}, need {}", all.len()));
            *n_written = all.len();
            return Err(OptStatus::BufferTooSmall);
        }

        let slice = std::slice::from_raw_parts_mut(results, all.len());
        for (slot, (engine, res)) in slice.iter_mut().zip(all.iter()) {
            slot.check_size().map_err(|e| record(&e))?;
            match res {
                Ok(r) => fill(slot, r, 0, false),
                Err(_) => {
                    slot.engine = -1;
                    slot.price = f64::NAN;
                    slot.has_greeks = 0;
                    slot.has_std_error = 0;
                    let _ = engine;
                }
            }
        }
        *n_written = all.len();
        Ok(())
    })
}

/// Black-Scholes implied volatility from a European price.
///
/// # Safety
/// `option` and `out` must be valid. `option->vol` is ignored.
#[no_mangle]
pub unsafe extern "C" fn opt_implied_vol(
    option: *const OptOption,
    target_price: f64,
    out: *mut f64,
) -> OptStatus {
    guard(|| {
        require_non_null(option, "option")?;
        require_non_null(out, "out")?;
        let req = (*option).to_request().map_err(|err| record(&err))?;
        let v = optrs_core::analytic::implied_vol(target_price, &req.inputs, req.kind)
            .map_err(|err| record(&err))?;
        *out = v;
        Ok(())
    })
}

/// Convenience for bindings: fill an option struct with correct `size` and
/// zeroed optional fields, so callers never forget the size handshake.
///
/// # Safety
/// `out` must point to a writable `opt_option_t`.
#[no_mangle]
pub unsafe extern "C" fn opt_option_init(out: *mut OptOption) -> OptStatus {
    guard(|| {
        require_non_null(out, "option")?;
        *out = OptOption {
            size: std::mem::size_of::<OptOption>() as u32,
            kind: types::OptKind::Call,
            style: types::OptStyle::European,
            _pad: 0,
            spot: 0.0,
            strike: 0.0,
            rate: 0.0,
            div_yield: 0.0,
            vol: 0.0,
            time: 0.0,
            dates: std::ptr::null(),
            n_dates: 0,
        };
        Ok(())
    })
}

/// # Safety
/// `out` must point to a writable `opt_result_t`.
#[no_mangle]
pub unsafe extern "C" fn opt_result_init(out: *mut OptResult) -> OptStatus {
    guard(|| {
        require_non_null(out, "result")?;
        *out = OptResult::init();
        Ok(())
    })
}

/// Unused today, but exported so bindings can link against a stable free
/// function if a future call returns heap strings.
///
/// # Safety
/// `s` must be NULL or a pointer this library allocated.
#[no_mangle]
pub unsafe extern "C" fn opt_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(std::ffi::CString::from_raw(s));
    }
}

// Silence the unused-import warning when no function needs CStr yet.
const _: Option<&CStr> = None;
