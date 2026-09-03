// crates/optrs-cos/src/coefficients.rs
//! Payoff cosine coefficients, Fang & Oosterlee (2008) eqs. 20-22.

use std::f64::consts::PI;

/// chi_k(c,d): cosine coefficients of exp(y) on [c,d].
pub fn chi(k: usize, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let w = k as f64 * PI / (b - a);
    let (uc, ud) = (w * (c - a), w * (d - a));
    (1.0 / (1.0 + w * w))
        * (ud.cos() * d.exp() - uc.cos() * c.exp() + w * ud.sin() * d.exp() - w * uc.sin() * c.exp())
}

/// psi_k(c,d): cosine coefficients of 1 on [c,d].
pub fn psi(k: usize, a: f64, b: f64, c: f64, d: f64) -> f64 {
    if k == 0 {
        d - c
    } else {
        let w = k as f64 * PI / (b - a);
        (w * (d - a)).sin() / w - (w * (c - a)).sin() / w
    }
}
