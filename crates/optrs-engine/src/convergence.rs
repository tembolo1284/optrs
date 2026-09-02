// crates/optrs-engine/src/convergence.rs
//! Refine the discretisation until successive prices agree, rather than making
//! the caller guess a step count. Doubling is what makes Richardson valid.

use crate::config::Config;
use crate::engine::{Engine, PriceResult};
use optrs_core::error::{Error, Result};
use optrs_core::instrument::PriceRequest;

#[derive(Clone, Copy, Debug)]
pub struct ConvergenceReport {
    pub result: PriceResult,
    pub refinements: usize,
    /// Absolute change over the final refinement — the practical error estimate.
    pub final_delta: f64,
    pub extrapolated: bool,
}

/// Scale the relevant knob in a copy of the config. Returns None when the
/// engine has no discretisation to refine.
fn refine(engine: Engine, cfg: &Config, level: usize) -> Option<Config> {
    let mut out = *cfg;
    let f = 1usize << level;
    match engine {
        Engine::Analytic => None,
        Engine::Cos => {
            out.cos.terms = cfg.cos.terms * f;
            Some(out)
        }
        Engine::TreeCrr | Engine::TreeLr => {
            out.tree.steps = cfg.tree.steps * f;
            Some(out)
        }
        Engine::FiniteDifference => {
            // Keep dt ~ dx for Crank-Nicolson: both are second order, so refining
            // one alone stalls convergence at the other's error floor.
            out.fd.space_steps = cfg.fd.space_steps * f;
            out.fd.time_steps = cfg.fd.time_steps * f;
            Some(out)
        }
        Engine::MonteCarlo => {
            // Error is O(1/sqrt(N)), so quadruple to halve it per level.
            out.mc.paths = cfg.mc.paths * f * f;
            Some(out)
        }
    }
}

/// Richardson on a doubled pair for a method of order `p`: (2^p·x_fine - x_coarse)/(2^p - 1).
fn richardson(coarse: f64, fine: f64, order: u32) -> f64 {
    let s = 2f64.powi(order as i32);
    (s * fine - coarse) / (s - 1.0)
}

fn convergence_order(engine: Engine) -> Option<u32> {
    match engine {
        // Smooth second-order error: extrapolation is sound.
        Engine::TreeLr | Engine::FiniteDifference => Some(2),
        // CRR oscillates, COS is spectral, MC is stochastic — extrapolation is
        // either meaningless or actively harmful.
        _ => None,
    }
}

pub fn price_converged(
    engine: Engine,
    req: &PriceRequest,
    cfg: &Config,
) -> Result<ConvergenceReport> {
    let cc = cfg.convergence;

    let Some(first) = refine(engine, cfg, 0) else {
        // Exact engine: nothing to refine.
        let result = engine.price_raw(req, cfg)?;
        return Ok(ConvergenceReport { result, refinements: 0, final_delta: 0.0, extrapolated: false });
    };

    let mut prev = engine.price_raw(req, &first)?;
    let mut prev_price = prev.price;

    for level in 1..=cc.max_refinements {
        let Some(step_cfg) = refine(engine, cfg, level) else { break };
        let curr = engine.price_raw(req, &step_cfg)?;

        // Stochastic engines converge in probability; compare against the
        // standard error rather than a raw price delta.
        let delta = (curr.price - prev_price).abs();
        let converged = match curr.std_error {
            Some(se) => se < cc.tolerance,
            None => delta < cc.tolerance,
        };

        if converged {
            let (price, extrapolated) = match (cc.richardson, convergence_order(engine)) {
                (true, Some(order)) => (richardson(prev_price, curr.price, order), true),
                _ => (curr.price, false),
            };
            return Ok(ConvergenceReport {
                result: PriceResult { price, ..curr },
                refinements: level,
                final_delta: delta,
                extrapolated,
            });
        }
        prev_price = curr.price;
        prev = curr;
    }

    Err(Error::NotConverged { last: prev.price, delta: f64::NAN })
}
