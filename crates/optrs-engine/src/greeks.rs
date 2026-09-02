// crates/optrs-engine/src/greeks.rs
//! Bump-and-reprice for every engine. Engines that produce greeks natively
//! (analytic, trees) short-circuit delta/gamma/theta; vega and rho always bump.
//!
//! When the `Real` scalar trait grows a dual-number implementation this module
//! gains an AD path and the call sites stay unchanged.

use crate::config::Config;
use crate::engine::Engine;
use optrs_core::analytic::Greeks;
use optrs_core::error::Result;
use optrs_core::instrument::PriceRequest;

const DAYS_PER_YEAR: f64 = 365.0;

/// Price under a mutated request, holding the RNG seed fixed so Monte Carlo
/// bumps share paths. Without common random numbers an MC delta is noise.
fn reprice(
    engine: Engine,
    req: &PriceRequest,
    cfg: &Config,
    mutate: impl FnOnce(&mut PriceRequest),
) -> Result<f64> {
    let mut bumped = req.clone();
    mutate(&mut bumped);
    engine.price_raw(&bumped, cfg).map(|r| r.price)
}

pub fn compute(engine: Engine, req: &PriceRequest, cfg: &Config) -> Result<Greeks> {
    let gc = cfg.greeks;
    let mut cfg = *cfg;
    if gc.common_random_numbers {
        // Freezing the seed is what makes the differences below meaningful.
        cfg.mc.seed = cfg.mc.seed | 1;
    }

    let base = engine.price_raw(req, &cfg)?;
    let mut out = base.greeks.unwrap_or(Greeks { price: base.price, ..Default::default() });
    out.price = base.price;

    let native = base.greeks.is_some();

    if !native {
        let h = gc.spot_rel * req.inputs.spot;
        let up = reprice(engine, req, &cfg, |r| r.inputs.spot += h)?;
        let dn = reprice(engine, req, &cfg, |r| r.inputs.spot -= h)?;
        out.delta = (up - dn) / (2.0 * h);
        out.gamma = (up - 2.0 * base.price + dn) / (h * h);

        // Calendar theta: roll the whole request, schedule included.
        let dt = gc.theta_days / DAYS_PER_YEAR;
        if let Ok(rolled) = req.rolled_forward(dt) {
            let fwd = engine.price_raw(&rolled, &cfg)?;
            out.theta = (fwd.price - base.price) / dt;
        }
    }

    // Vega and rho always bump: no engine here produces them natively.
    let vup = reprice(engine, req, &cfg, |r| r.inputs.vol += gc.vol_abs)?;
    let vdn = reprice(engine, req, &cfg, |r| r.inputs.vol -= gc.vol_abs)?;
    out.vega = (vup - vdn) / (2.0 * gc.vol_abs);

    let rup = reprice(engine, req, &cfg, |r| r.inputs.rate += gc.rate_abs)?;
    let rdn = reprice(engine, req, &cfg, |r| r.inputs.rate -= gc.rate_abs)?;
    out.rho = (rup - rdn) / (2.0 * gc.rate_abs);

    Ok(out)
}
