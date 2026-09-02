// crates/optrs-mc/src/lib.rs
//! Monte Carlo engine. European uses exact terminal sampling with antithetics
//! and a control variate; American/Bermudan use Longstaff-Schwartz.

pub mod lsmc;
pub mod rng;

use lsmc::{cholesky_solve, laguerre_basis};
use optrs_core::analytic::{self, BsmInputs, OptionType};
use optrs_core::error::{Error, Result};
use optrs_core::exercise::Exercise;
use rng::{inverse_normal, Pcg64, UniformStream};

#[derive(Clone, Copy, Debug)]
pub struct McConfig {
    pub paths: usize,
    pub steps: usize,
    pub seed: u64,
    pub antithetic: bool,
    /// Uses the analytic European price as a control. Ignored for early exercise.
    pub control_variate: bool,
    pub basis_terms: usize,
}

impl Default for McConfig {
    fn default() -> Self {
        Self {
            paths: 200_000,
            steps: 50,
            seed: 0xC0FF_EE00,
            antithetic: true,
            control_variate: true,
            basis_terms: 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct McResult {
    pub price: f64,
    pub std_error: f64,
    pub paths_used: usize,
}

pub fn price(
    inp: &BsmInputs,
    kind: OptionType,
    exercise: &Exercise,
    cfg: &McConfig,
) -> Result<McResult> {
    if cfg.paths < 2 {
        return Err(Error::Domain("need at least 2 paths"));
    }
    match exercise {
        Exercise::European { .. } => european(inp, kind, cfg),
        _ => longstaff_schwartz(inp, kind, exercise, cfg),
    }
}

fn european(inp: &BsmInputs, kind: OptionType, cfg: &McConfig) -> Result<McResult> {
    let mut rng = Pcg64::new(cfg.seed, 1);
    let drift = (inp.rate - inp.div_yield - 0.5 * inp.vol * inp.vol) * inp.time;
    let vol_t = inp.vol * inp.time.sqrt();
    let df = (-inp.rate * inp.time).exp();
    // Control: discounted terminal spot has known mean S0·exp(-q·T).
    let cv_mean = inp.spot * (-inp.div_yield * inp.time).exp();

    let n = cfg.paths;
    let mut payoffs = Vec::with_capacity(n);
    let mut controls = Vec::with_capacity(n);

    let mut i = 0;
    while i < n {
        let z = inverse_normal(rng.next_uniform());
        let draws: &[f64] = if cfg.antithetic { &[z, -z] } else { &[z] };
        for &zi in draws {
            if i >= n {
                break;
            }
            let st = inp.spot * (drift + vol_t * zi).exp();
            payoffs.push(df * (kind.sign() * (st - inp.strike)).max(0.0));
            controls.push(df * st);
            i += 1;
        }
    }

    let (est, se) = if cfg.control_variate {
        regress_control(&payoffs, &controls, df * cv_mean / df)
    } else {
        moments(&payoffs)
    };
    Ok(McResult { price: est, std_error: se, paths_used: n })
}

/// Optimal-beta control variate: E[Y - beta(X - E[X])].
fn regress_control(y: &[f64], x: &[f64], x_mean: f64) -> (f64, f64) {
    let n = y.len() as f64;
    let ybar = y.iter().sum::<f64>() / n;
    let xbar = x.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut varx = 0.0;
    for (yi, xi) in y.iter().zip(x) {
        cov += (yi - ybar) * (xi - xbar);
        varx += (xi - xbar) * (xi - xbar);
    }
    let beta = if varx > 1e-14 { cov / varx } else { 0.0 };
    let adjusted: Vec<f64> = y
        .iter()
        .zip(x)
        .map(|(yi, xi)| yi - beta * (xi - x_mean))
        .collect();
    moments(&adjusted)
}

fn moments(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0);
    (mean, (var / n).sqrt())
}

fn longstaff_schwartz(
    inp: &BsmInputs,
    kind: OptionType,
    exercise: &Exercise,
    cfg: &McConfig,
) -> Result<McResult> {
    let n_paths = cfg.paths;
    let n_steps = cfg.steps;
    let dt = inp.time / n_steps as f64;
    let drift = (inp.rate - inp.div_yield - 0.5 * inp.vol * inp.vol) * dt;
    let vol_dt = inp.vol * dt.sqrt();
    let df = (-inp.rate * dt).exp();
    let mask = exercise.step_mask(n_steps);

    // Full path storage: LSMC needs a backward sweep, so streaming is not an option.
    let mut paths = vec![0.0_f64; n_paths * (n_steps + 1)];
    let mut rng = Pcg64::new(cfg.seed, 2);
    for p in 0..n_paths {
        paths[p * (n_steps + 1)] = inp.spot;
    }
    for step in 1..=n_steps {
        let mut p = 0;
        while p < n_paths {
            let z = inverse_normal(rng.next_uniform());
            let draws: &[f64] = if cfg.antithetic { &[z, -z] } else { &[z] };
            for &zi in draws {
                if p >= n_paths {
                    break;
                }
                let prev = paths[p * (n_steps + 1) + step - 1];
                paths[p * (n_steps + 1) + step] = prev * (drift + vol_dt * zi).exp();
                p += 1;
            }
        }
    }

    let payoff = |s: f64| (kind.sign() * (s - inp.strike)).max(0.0);
    let mut cash: Vec<f64> = (0..n_paths)
        .map(|p| payoff(paths[p * (n_steps + 1) + n_steps]))
        .collect();

    let k = cfg.basis_terms.clamp(1, 4);
    let mut basis = vec![0.0; k];

    for step in (1..n_steps).rev() {
        for c in cash.iter_mut() {
            *c *= df;
        }
        if !mask[step] {
            continue;
        }

        let itm: Vec<usize> = (0..n_paths)
            .filter(|&p| payoff(paths[p * (n_steps + 1) + step]) > 0.0)
            .collect();
        if itm.len() < 4 * k {
            continue;
        }

        let mut xtx = vec![0.0; k * k];
        let mut xty = vec![0.0; k];
        for &p in &itm {
            laguerre_basis(paths[p * (n_steps + 1) + step] / inp.strike, &mut basis);
            for a in 0..k {
                xty[a] += basis[a] * cash[p];
                for b in 0..k {
                    xtx[a * k + b] += basis[a] * basis[b];
                }
            }
        }

        let Some(beta) = cholesky_solve(xtx, xty, k, 1e-10) else { continue };

        for &p in &itm {
            laguerre_basis(paths[p * (n_steps + 1) + step] / inp.strike, &mut basis);
            let cont: f64 = (0..k).map(|a| beta[a] * basis[a]).sum();
            let exercise_now = payoff(paths[p * (n_steps + 1) + step]);
            if exercise_now > cont {
                cash[p] = exercise_now;
            }
        }
    }

    for c in cash.iter_mut() {
        *c *= df;
    }
    let (mean, se) = moments(&cash);
    // t=0 exercise is a max, not part of the regression.
    let price = mean.max(payoff(inp.spot));
    Ok(McResult { price, std_error: se, paths_used: n_paths })
}
