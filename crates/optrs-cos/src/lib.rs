// crates/optrs-cos/src/lib.rs
//! COS method for European vanillas. Spectral accuracy for smooth densities:
//! N = 128 typically matches the analytic price to ~1e-12 under GBM.
//!
//! Reference: Fang & Oosterlee (2008), "A Novel Pricing Method for European
//! Options Based on Fourier-Cosine Series Expansions".

pub mod coefficients;

use coefficients::{chi, psi};
use num_complex::Complex64;
use optrs_core::analytic::OptionType;
use optrs_core::error::{Error, Result};
use optrs_core::model::CharFn;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct CosConfig {
    /// Number of cosine terms.
    pub terms: usize,
    /// Truncation width in standard deviations; 10 is the Fang-Oosterlee default.
    pub trunc_l: f64,
}

impl Default for CosConfig {
    fn default() -> Self {
        Self { terms: 256, trunc_l: 10.0 }
    }
}

/// Truncation range [a,b] for y = ln(S_T/K), centred on x + c1.
///
/// The width scales with sqrt(c2) so long maturities widen the interval
/// automatically — batch 4 (T = 30) spans roughly [-15, 17] in log-space.
fn truncation(model: &impl CharFn, x: f64, t: f64, l: f64) -> (f64, f64) {
    let (c1, c2, c4) = model.cumulants(t);
    let half = l * (c2.abs() + c4.abs().sqrt()).sqrt();
    (x + c1 - half, x + c1 + half)
}

/// Cosine coefficients of the payoff over [a,b].
///
/// The integration range is the intersection of [a,b] with the region where the
/// payoff is non-zero: [0, inf) for a call, (-inf, 0] for a put. Clamping here
/// matters — `chi` and `psi` are only valid on subintervals of [a,b], and for
/// deep in- or out-of-the-money strikes the truncation interval can sit entirely
/// on one side of zero. An empty intersection means the payoff vanishes on the
/// whole grid, so the coefficient is zero.
fn payoff_coefficient(k: usize, a: f64, b: f64, span: f64, kind: OptionType) -> f64 {
    let (lo, hi) = match kind {
        OptionType::Call => (a.max(0.0), b),
        OptionType::Put => (a, b.min(0.0)),
    };
    if lo >= hi {
        return 0.0;
    }
    let (u_k, chi_k) = (psi(k, a, b, lo, hi), chi(k, a, b, lo, hi));
    match kind {
        OptionType::Call => (2.0 / span) * (chi_k - u_k),
        OptionType::Put => (2.0 / span) * (u_k - chi_k),
    }
}

pub fn price(
    model: &impl CharFn,
    spot: f64,
    strike: f64,
    rate: f64,
    time: f64,
    kind: OptionType,
    cfg: &CosConfig,
) -> Result<f64> {
    if spot <= 0.0 || strike <= 0.0 || time <= 0.0 {
        return Err(Error::Domain("spot, strike and time must be positive"));
    }
    if cfg.terms == 0 {
        return Err(Error::Domain("terms must be positive"));
    }
    if !(cfg.trunc_l.is_finite() && cfg.trunc_l > 0.0) {
        return Err(Error::Domain("truncation width must be positive and finite"));
    }

    let x = (spot / strike).ln();
    let (a, b) = truncation(model, x, time, cfg.trunc_l);
    let span = b - a;
    if !(span.is_finite() && span > 0.0) {
        return Err(Error::Domain("degenerate truncation range"));
    }

    let mut sum = 0.0_f64;
    for k in 0..cfg.terms {
        let vk = payoff_coefficient(k, a, b, span, kind);
        let u = k as f64 * PI / span;
        let phase = (Complex64::i() * u * (x - a)).exp();
        let term = (model.cf(u, time) * phase).re * vk;

        // Primed sum: halve the k = 0 term.
        sum += if k == 0 { 0.5 * term } else { term };
    }

    let value = strike * (-rate * time).exp() * sum;
    if !value.is_finite() {
        return Err(Error::NoSolution("COS series diverged"));
    }
    // Series truncation can leave a tiny negative value for far out-of-the-money
    // options; an option price is never negative.
    Ok(value.max(0.0))
}
