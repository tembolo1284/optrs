// crates/optrs-engine/src/lib.rs
//! Public entry point. Everything downstream — CLI, C ABI, Python, Excel —
//! calls only these four functions.

pub mod config;
pub mod convergence;
pub mod engine;
pub mod greeks;

pub use config::{Config, ConvergenceConfig, GreekConfig};
pub use convergence::{price_converged, ConvergenceReport};
pub use engine::{Engine, PriceResult};

use optrs_core::error::Result;
use optrs_core::instrument::PriceRequest;

/// Price with an explicit engine at the configured discretisation.
pub fn price(engine: Engine, req: &PriceRequest, cfg: &Config) -> Result<PriceResult> {
    engine.price_raw(req, cfg)
}

/// Price with the cheapest engine that supports the request.
pub fn price_auto(req: &PriceRequest, cfg: &Config) -> Result<PriceResult> {
    Engine::best_for(req).price_raw(req, cfg)
}

/// Full greeks under the chosen engine.
pub fn greeks(
    engine: Engine,
    req: &PriceRequest,
    cfg: &Config,
) -> Result<optrs_core::analytic::Greeks> {
    greeks::compute(engine, req, cfg)
}

/// Price under every engine that supports the request. This is the
/// cross-validation entry point — the CLI and the test suite both use it.
pub fn price_all(req: &PriceRequest, cfg: &Config) -> Vec<(Engine, Result<PriceResult>)> {
    Engine::ALL
        .iter()
        .filter(|e| e.supports(req))
        .map(|&e| (e, e.price_raw(req, cfg)))
        .collect()
}
