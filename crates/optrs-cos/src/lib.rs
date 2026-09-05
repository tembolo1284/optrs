// crates/optrs-cos/src/lib.rs
//! COS method for European vanillas. Spectral accuracy for smooth densities:
//! N = 128 typically matches the analytic price to ~1e-12 under GBM.
//!
//! Reference: Fang & Oosterlee (2008), "A Novel Pricing Method for European
//! Options Based on Fourier-Cosine Series Expansions".
//!
//! Only the put is evaluated by the series; the call comes from put-call
//! parity. See `put_series` for why.

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
/// The width scales with sqrt(c2), so long maturities widen the interval
/// automatically — at T = 30 with 30% vol it spans roughly [-15, 17].
fn truncation(model: &impl CharFn, x: f64, t: f64, l: f64) -> (f64, f64) {
    let (c1, c2, c4) = model.cumulants(t);
    let half = l * (c2.abs() + c4.abs().sqrt()).sqrt();
    (x + c1 - half, x + c1 + half)
}

/// Cosine coefficients of the put payoff over [a,b].
///
/// The integration range is the intersection of [a,b] with (-inf, 0], where the
/// put payoff is non-zero. Clamping matters: `chi` and `psi` are only valid on
/// subintervals of [a,b], and for deep in- or out-of-the-money strikes the
/// truncation interval can sit entirely on one side of zero. An empty
/// intersection means the payoff vanishes on the grid, so the coefficient is 0.
fn put_coefficient(k: usize, a: f64, b: f64, span: f64) -> f64 {
    let (lo, hi) = (a, b.min(0.0));
    if lo >= hi {
        return 0.0;
    }
    (2.0 / span) * (psi(k, a, b, lo, hi) - chi(k, a, b, lo, hi))
}

/// Series evaluation for the put.
///
/// The call is deliberately not evaluated this way. Its payoff coefficients
/// integrate over [max(a,0), b] and contain exp(b); for wide truncation ranges
/// (long maturity or high vol) that factor reaches 1e7 or more and cancels
/// against characteristic-function values of order 1e-8, costing roughly eight
/// digits. The put integrand carries a negative exponent and stays stable, so
/// the call is recovered by parity instead — this is Fang and Oosterlee's own
/// recommendation.
fn put_series(
    model: &impl CharFn,
    x: f64,
    strike: f64,
    rate: f64,
    time: f64,
    cfg: &CosConfig,
) -> Result<f64> {
    let (a, b) = truncation(model, x, time, cfg.trunc_l);
    let span = b - a;
    if !(span.is_finite() && span > 0.0) {
        return Err(Error::Domain("degenerate truncation range"));
    }

    let mut sum = 0.0_f64;
    for k in 0..cfg.terms {
        let vk = put_coefficient(k, a, b, span);
        let u = k as f64 * PI / span;
        let phase = (Complex64::i() * u * (x - a)).exp();
        let term = (model.cf(u, time) * phase).re * vk;

        // Primed sum: halve the k = 0 term.
        sum += if k == 0 { 0.5 * term } else { term };
    }

    Ok(strike * (-rate * time).exp() * sum)
}

/// S·e^(-qT) - K·e^(-rT), taken from the model's own growth factor so that any
/// risk-neutral model satisfying the martingale condition works unchanged.
fn forward_leg(model: &impl CharFn, spot: f64, strike: f64, rate: f64, time: f64) -> f64 {
    (-rate * time).exp() * (spot * model.expected_growth(time) - strike)
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
    let put = put_series(model, x, strike, rate, time, cfg)?;

    let value = match kind {
        OptionType::Put => put,
        OptionType::Call => put + forward_leg(model, spot, strike, rate, time),
    };

    if !value.is_finite() {
        return Err(Error::NoSolution("COS series diverged"));
    }
    // Series truncation can leave a tiny negative value for far out-of-the-money
    // options; an option price is never negative.
    Ok(value.max(0.0))
}
