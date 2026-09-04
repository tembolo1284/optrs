// crates/optrs-core/src/model/mod.rs
//! Model = characteristic function of the log-return plus its cumulants.
//! Adding Heston/VG/CGMY means implementing this trait, nothing else.

pub mod gbm;

use num_complex::Complex64;

pub trait CharFn {
    /// E[exp(i·u·ln(S_T/S_0))] under the risk-neutral measure.
    fn cf(&self, u: f64, t: f64) -> Complex64;

    /// (c1, c2, c4) of the log-return — drives COS truncation width.
    fn cumulants(&self, t: f64) -> (f64, f64, f64);
}
