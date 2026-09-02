// crates/optrs-core/src/lib.rs
//! Model-agnostic types shared by every engine. No engine code lives here.

pub mod analytic;
pub mod error;
pub mod exercise;
pub mod instrument;
pub mod model;

pub use error::{Error, Result};
