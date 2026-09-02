// crates/optrs-tree/src/lib.rs
//! Binomial lattice engine. Handles European, American and Bermudan uniformly
//! via `Exercise::step_mask`.

pub mod lattice;

use lattice::{Parameterisation, TreeParams};
use optrs_core::analytic::{BsmInputs, Greeks, OptionType};
use optrs_core::error::{Error, Result};
use optrs_core::exercise::Exercise;

#[derive(Clone, Copy, Debug)]
pub struct TreeConfig {
    pub steps: usize,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self { steps: 801 }
    }
}

#[inline]
fn intrinsic(spot: f64, strike: f64, kind: OptionType) -> f64 {
    (kind.sign() * (spot - strike)).max(0.0)
}

/// Price plus lattice greeks. Delta/gamma/theta come out of nodes at steps 1
/// and 2 at zero extra cost; vega and rho still need a bump.
pub fn price_with_greeks(
    inp: &BsmInputs,
    kind: OptionType,
    exercise: &Exercise,
    param: &impl Parameterisation,
    cfg: &TreeConfig,
) -> Result<Greeks> {
    if cfg.steps < 3 {
        return Err(Error::Domain("tree needs at least 3 steps"));
    }
    if (exercise.expiry() - inp.time).abs() > 1e-12 {
        return Err(Error::Domain("exercise expiry must match input time"));
    }

    let n = param.adjust_steps(cfg.steps);
    let TreeParams { up, down, p, dt, disc } = param.build(inp, n);
    let mask = exercise.step_mask(n);

    // Asset prices at maturity: node j has j up-moves.
    let mut spot_row: Vec<f64> = (0..=n)
        .map(|j| inp.spot * up.powi(j as i32) * down.powi((n - j) as i32))
        .collect();
    let mut value: Vec<f64> = spot_row.iter().map(|&s| intrinsic(s, inp.strike, kind)).collect();

    // Retain the step-2 and step-1 slices for greeks.
    let mut v2 = [0.0; 3];
    let mut s2 = [0.0; 3];
    let mut v1 = [0.0; 2];
    let mut s1 = [0.0; 2];

    for step in (0..n).rev() {
        for j in 0..=step {
            let cont = disc * (p * value[j + 1] + (1.0 - p) * value[j]);
            spot_row[j] = inp.spot * up.powi(j as i32) * down.powi((step - j) as i32);
            value[j] = if mask[step] {
                cont.max(intrinsic(spot_row[j], inp.strike, kind))
            } else {
                cont
            };
        }
        if step == 2 {
            v2.copy_from_slice(&value[..3]);
            s2.copy_from_slice(&spot_row[..3]);
        }
        if step == 1 {
            v1.copy_from_slice(&value[..2]);
            s1.copy_from_slice(&spot_row[..2]);
        }
    }

    let delta = (v1[1] - v1[0]) / (s1[1] - s1[0]);
    let d_up = (v2[2] - v2[1]) / (s2[2] - s2[1]);
    let d_dn = (v2[1] - v2[0]) / (s2[1] - s2[0]);
    let gamma = (d_up - d_dn) / (0.5 * (s2[2] - s2[0]));
    // s2[1] == spot for a recombining tree, so this is a clean time difference.
    let theta = (v2[1] - value[0]) / (2.0 * dt);

    Ok(Greeks { price: value[0], delta, gamma, theta, vega: 0.0, rho: 0.0 })
}

pub fn price(
    inp: &BsmInputs,
    kind: OptionType,
    exercise: &Exercise,
    param: &impl Parameterisation,
    cfg: &TreeConfig,
) -> Result<f64> {
    price_with_greeks(inp, kind, exercise, param, cfg).map(|g| g.price)
}
