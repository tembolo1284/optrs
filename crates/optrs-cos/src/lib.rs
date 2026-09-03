// crates/optrs-cos/src/lib.rs
//! COS method for European vanillas. Spectral accuracy for smooth densities:
//! N = 128 typically matches the analytic price to ~1e-12 under GBM.

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
fn truncation(model: &impl CharFn, x: f64, t: f64, l: f64) -> (f64, f64) {
    let (c1, c2, c4) = model.cumulants(t);
    let half = l * (c2.abs() + c4.abs().sqrt()).sqrt();
    (x + c1 - half, x + c1 + half)
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

    let x = (spot / strike).ln();
    let (a, b) = truncation(model, x, time, cfg.trunc_l);
    let span = b - a;

    let mut sum = 0.0_f64;
    for k in 0..cfg.terms {
        // Payoff coefficients: call integrates over [0,b], put over [a,0].
        let vk = match kind {
            OptionType::Call => {
                let hi = b.max(0.0);
                (2.0 / span) * (chi(k, a, b, 0.0, hi) - psi(k, a, b, 0.0, hi))
            }
            OptionType::Put => {
                let lo = a.min(0.0);
                (2.0 / span) * (psi(k, a, b, lo, 0.0) - chi(k, a, b, lo, 0.0))
            }
        };

        let u = k as f64 * PI / span;
        let phase = (Complex64::i() * u * (x - a)).exp();
        let term = (model.cf(u, time) * phase).re * vk;

        // Primed sum: halve the k = 0 term.
        sum += if k == 0 { 0.5 * term } else { term };
    }

    Ok(strike * (-rate * time).exp() * sum)
}
