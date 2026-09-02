// crates/optrs-cabi/src/error.rs
//! Thread-local last-error, the standard C pattern: functions return a status
//! code, and the caller pulls a human-readable message if it is non-zero. The
//! returned pointer is owned by the library and valid until the next failing
//! call on the same thread — callers must copy it, not store it.

use crate::types::OptStatus;
use optrs_core::error::Error;
use std::cell::RefCell;
use std::ffi::{c_char, CString};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

pub fn clear() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

pub fn set(msg: impl AsRef<str>) {
    // Interior NULs cannot happen from our own Display impls, but be defensive.
    let cleaned = msg.as_ref().replace('\0', " ");
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::new(cleaned).ok());
}

pub fn last_message() -> *const c_char {
    LAST_ERROR.with(|e| {
        e.borrow().as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
    })
}

pub fn record(err: &Error) -> OptStatus {
    set(err.to_string());
    match err {
        Error::Domain(_) => OptStatus::Domain,
        Error::NoSolution(_) => OptStatus::NoSolution,
        Error::Unsupported { .. } => OptStatus::Unsupported,
        Error::NotConverged { .. } => OptStatus::NotConverged,
    }
}

/// Wrap every exported function body. Converts Result into a status code and
/// turns a panic into `OPT_STATUS_PANIC` rather than unwinding into C, which
/// would be undefined behaviour.
pub fn guard(f: impl FnOnce() -> Result<(), OptStatus> + std::panic::UnwindSafe) -> OptStatus {
    clear();
    match std::panic::catch_unwind(f) {
        Ok(Ok(())) => OptStatus::Ok,
        Ok(Err(status)) => status,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            set(format!("panic in optrs: {msg}"));
            OptStatus::Panic
        }
    }
}

/// Null-check helper that records a message before returning.
pub fn require_non_null<T>(ptr: *const T, what: &str) -> Result<(), OptStatus> {
    if ptr.is_null() {
        set(format!("null pointer for {what}"));
        Err(OptStatus::NullPointer)
    } else {
        Ok(())
    }
}
