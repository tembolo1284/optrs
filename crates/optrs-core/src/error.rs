// crates/optrs-core/src/error.rs
//! Flat error enum. Every variant maps to a stable integer for the C ABI later,
//! so avoid reordering: add new variants at the end.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Domain(&'static str),
    NoSolution(&'static str),
    /// Engine cannot handle this exercise style or payoff.
    Unsupported { engine: &'static str, reason: &'static str },
    /// Convergence ladder exhausted its refinement budget.
    NotConverged { last: f64, delta: f64 },
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::Domain(_) => 1,
            Error::NoSolution(_) => 2,
            Error::Unsupported { .. } => 3,
            Error::NotConverged { .. } => 4,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Domain(m) => write!(f, "domain error: {m}"),
            Error::NoSolution(m) => write!(f, "no solution: {m}"),
            Error::Unsupported { engine, reason } => write!(f, "{engine} unsupported: {reason}"),
            Error::NotConverged { last, delta } => {
                write!(f, "not converged: last={last:.10}, delta={delta:.3e}")
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
