// crates/optrs-cabi/src/handle.rs
//! Opaque handle holding engine configuration. Callers create one, tune it with
//! setters, price many options, then free it. Keeping config off the per-call
//! struct means adding a knob does not change any existing signature.

use optrs_engine::Config;

pub struct Pricer {
    pub config: Config,
}

impl Pricer {
    pub fn new() -> Self {
        Self { config: Config::default() }
    }
}

/// Cast an opaque pointer back to a `Pricer`.
///
/// # Safety
/// `ptr` must have come from `opt_pricer_new` and not yet been freed.
pub unsafe fn as_ref<'a>(ptr: *const Pricer) -> &'a Pricer {
    &*ptr
}

/// # Safety
/// As `as_ref`, and no other reference to the handle may be live.
pub unsafe fn as_mut<'a>(ptr: *mut Pricer) -> &'a mut Pricer {
    &mut *ptr
}
