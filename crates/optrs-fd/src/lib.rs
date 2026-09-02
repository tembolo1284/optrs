// crates/optrs-fd/src/lib.rs
//! Crank-Nicolson on a uniform log-spot grid with Rannacher startup.
//!
//! PDE in x = ln(S), backward time tau = T - t:
//!     V_tau = 0.5·sig^2·V_xx + (r - q - 0.5·sig^2)·V_x - r·V

pub mod solver;

use optrs_core::analytic::{BsmInputs, OptionType};
use optrs_core::error::{Error, Result};
use optrs_core::exercise::Exercise;
use solver::{psor, thomas};

#[derive(Clone, Copy, Debug)]
pub struct FdConfig {
    pub space_steps: usize,
    pub time_steps: usize,
    /// Grid half-width in standard deviations of log-spot.
    pub width: f64,
    pub psor_omega: f64,
    pub psor_tol: f64,
}

impl Default for FdConfig {
    fn default() -> Self {
        Self { space_steps: 512, time_steps: 512, width: 6.0, psor_omega: 1.5, psor_tol: 1e-10 }
    }
}

fn terminal_payoff(x: &[f64], strike: f64, kind: OptionType) -> Vec<f64> {
    x.iter().map(|&xi| (kind.sign() * (xi.exp() - strike)).max(0.0)).collect()
}

pub fn price(
    inp: &BsmInputs,
    kind: OptionType,
    exercise: &Exercise,
    cfg: &FdConfig,
) -> Result<f64> {
    if cfg.space_steps < 4 || cfg.time_steps < 3 {
        return Err(Error::Domain("grid too coarse"));
    }
    let m = cfg.space_steps;
    let n = cfg.time_steps;
    let var = inp.vol * inp.vol;
    let drift = inp.rate - inp.div_yield - 0.5 * var;

    // Grid centred on the forward log-spot so the strike is not on a boundary.
    let centre = (inp.spot / inp.strike).ln() + drift * inp.time;
    let half = cfg.width * inp.vol * inp.time.sqrt();
    let (lo, hi) = ((inp.strike.ln() + centre - half), (inp.strike.ln() + centre + half));
    let dx = (hi - lo) / m as f64;
    let dt = inp.time / n as f64;
    let x: Vec<f64> = (0..=m).map(|i| lo + i as f64 * dx).collect();

    let mut v = terminal_payoff(&x, inp.strike, kind);
    let mask = exercise.step_mask(n);
    let is_american = matches!(exercise, Exercise::American { .. });

    // Interior operator coefficients (constant in x under GBM).
    let diff = 0.5 * var / (dx * dx);
    let conv = drift / (2.0 * dx);
    let (l, dg, u) = (diff - conv, -2.0 * diff - inp.rate, diff + conv);

    let interior = m - 1;
    let (mut a, mut b, mut c) = (vec![0.0; interior], vec![0.0; interior], vec![0.0; interior]);
    let mut rhs = vec![0.0; interior];
    let mut bwork = vec![0.0; interior];
    let mut sol = vec![0.0; interior];

    for step in (0..n).rev() {
        // Rannacher: two fully implicit half-steps first to damp the payoff kink,
        // otherwise Crank-Nicolson oscillates and gamma is garbage near the strike.
        let theta = if step >= n.saturating_sub(2) { 1.0 } else { 0.5 };
        let tau_next = (n - step) as f64 * dt;

        for i in 0..interior {
            a[i] = -theta * dt * l;
            b[i] = 1.0 - theta * dt * dg;
            c[i] = -theta * dt * u;
        }

        let ex = 1.0 - theta;
        for i in 0..interior {
            let k = i + 1;
            rhs[i] = v[k] + ex * dt * (l * v[k - 1] + dg * v[k] + u * v[k + 1]);
        }

        // Dirichlet boundaries from the deterministic limit.
        let (bl, bh) = boundaries(&x, inp, kind, tau_next, m);
        rhs[0] -= a[0] * bl;
        rhs[interior - 1] -= c[interior - 1] * bh;

        if is_american || mask[step] {
            let payoff = terminal_payoff(&x[1..m], inp.strike, kind);
            sol.copy_from_slice(&v[1..m]);
            if is_american {
                psor(&a, &b, &c, &rhs, &payoff, &mut sol, cfg.psor_omega, cfg.psor_tol, 10_000);
            } else {
                bwork.copy_from_slice(&b);
                let mut d = rhs.clone();
                thomas(&a, &mut bwork, &c, &mut d, &mut sol);
                // Bermudan: discrete projection only on exercise dates.
                for (s, p) in sol.iter_mut().zip(payoff.iter()) {
                    *s = s.max(*p);
                }
            }
        } else {
            bwork.copy_from_slice(&b);
            let mut d = rhs.clone();
            thomas(&a, &mut bwork, &c, &mut d, &mut sol);
        }

        v[1..m].copy_from_slice(&sol);
        v[0] = bl;
        v[m] = bh;
    }

    Ok(interpolate(&x, &v, inp.spot.ln()))
}

fn boundaries(x: &[f64], inp: &BsmInputs, kind: OptionType, tau: f64, m: usize) -> (f64, f64) {
    let dfr = (-inp.rate * tau).exp();
    let dfq = (-inp.div_yield * tau).exp();
    match kind {
        OptionType::Call => (0.0, x[m].exp() * dfq - inp.strike * dfr),
        OptionType::Put => (inp.strike * dfr - x[0].exp() * dfq, 0.0),
    }
}

/// Cubic interpolation at the spot — linear leaves a visible kink in delta.
fn interpolate(x: &[f64], v: &[f64], target: f64) -> f64 {
    let n = x.len();
    let dx = x[1] - x[0];
    let raw = ((target - x[0]) / dx).floor() as isize;
    let i = raw.clamp(1, n as isize - 3) as usize;
    let t = (target - x[i]) / dx;
    let (p0, p1, p2, p3) = (v[i - 1], v[i], v[i + 1], v[i + 2]);
    let (a0, a1) = (p1, 0.5 * (p2 - p0));
    let a2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let a3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    a0 + t * (a1 + t * (a2 + t * a3))
}
