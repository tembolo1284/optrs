// crates/optrs-tree/src/lattice.rs
//! Parameterisation and backward induction are separated: CRR and Leisen-Reimer
//! differ only in how they produce `TreeParams`.

use optrs_core::analytic::{BsmInputs, OptionType};

#[derive(Clone, Copy, Debug)]
pub struct TreeParams {
    pub up: f64,
    pub down: f64,
    /// Risk-neutral up probability.
    pub p: f64,
    pub dt: f64,
    /// Per-step discount factor exp(-r·dt).
    pub disc: f64,
}

pub trait Parameterisation {
    fn build(&self, inp: &BsmInputs, steps: usize) -> TreeParams;
    /// Leisen-Reimer needs odd step counts; CRR does not care.
    fn adjust_steps(&self, steps: usize) -> usize {
        steps
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Crr;

impl Parameterisation for Crr {
    fn build(&self, inp: &BsmInputs, steps: usize) -> TreeParams {
        let dt = inp.time / steps as f64;
        let up = (inp.vol * dt.sqrt()).exp();
        let down = 1.0 / up;
        let growth = ((inp.rate - inp.div_yield) * dt).exp();
        TreeParams {
            up,
            down,
            p: ((growth - down) / (up - down)).clamp(0.0, 1.0),
            dt,
            disc: (-inp.rate * dt).exp(),
        }
    }
}

/// Leisen-Reimer with Peizer-Pratt inversion method 2. Converges smoothly and
/// at second order, so Richardson extrapolation actually works — CRR's sawtooth
/// error makes extrapolation meaningless.
#[derive(Clone, Copy, Debug, Default)]
pub struct LeisenReimer;

fn peizer_pratt(z: f64, n: usize) -> f64 {
    let nf = n as f64;
    let denom = nf + 1.0 / 3.0 + 0.1 / (nf + 1.0);
    let inner = (z / denom) * (z / denom) * (nf + 1.0 / 6.0);
    0.5 + z.signum() * 0.5 * (1.0 - (-inner).exp()).sqrt()
}

impl Parameterisation for LeisenReimer {
    fn adjust_steps(&self, steps: usize) -> usize {
        if steps % 2 == 0 { steps + 1 } else { steps }
    }

    fn build(&self, inp: &BsmInputs, steps: usize) -> TreeParams {
        let dt = inp.time / steps as f64;
        let sqt = inp.vol * inp.time.sqrt();
        let d1 = ((inp.spot / inp.strike).ln()
            + (inp.rate - inp.div_yield + 0.5 * inp.vol * inp.vol) * inp.time)
            / sqt;
        let d2 = d1 - sqt;

        let p = peizer_pratt(d2, steps);
        let p_star = peizer_pratt(d1, steps);
        let growth = ((inp.rate - inp.div_yield) * dt).exp();
        let up = growth * p_star / p;
        let down = (growth - p * up) / (1.0 - p);

        TreeParams { up, down, p, dt, disc: (-inp.rate * dt).exp() }
    }
}
