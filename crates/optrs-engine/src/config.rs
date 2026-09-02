// crates/optrs-engine/src/config.rs
//! One config struct holding every engine's knobs. Callers set only what the
//! chosen engine reads, which keeps the C ABI a single flat POD later rather
//! than a tagged union.

use optrs_cos::CosConfig;
use optrs_fd::FdConfig;
use optrs_mc::McConfig;
use optrs_tree::TreeConfig;

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    pub tree: TreeConfig,
    pub fd: FdConfig,
    pub mc: McConfig,
    pub cos: CosConfig,
    pub greeks: GreekConfig,
    pub convergence: ConvergenceConfig,
}

/// Bump sizes for finite-difference greeks. Defaults are the usual market
/// conventions: 1% relative spot, 1 vol point, 1bp rate, 1 calendar day.
#[derive(Clone, Copy, Debug)]
pub struct GreekConfig {
    /// Relative spot bump. Absolute bump is `spot_rel * spot`.
    pub spot_rel: f64,
    pub vol_abs: f64,
    pub rate_abs: f64,
    pub theta_days: f64,
    /// Reuse the RNG seed across bumps. Essential for MC: without it the noise
    /// swamps the bump and delta is meaningless.
    pub common_random_numbers: bool,
}

impl Default for GreekConfig {
    fn default() -> Self {
        Self {
            spot_rel: 0.01,
            vol_abs: 0.01,
            rate_abs: 1e-4,
            theta_days: 1.0,
            common_random_numbers: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConvergenceConfig {
    pub tolerance: f64,
    /// Maximum doublings of the discretisation parameter.
    pub max_refinements: usize,
    /// Apply Richardson extrapolation on the final pair. Only sound for engines
    /// with smooth monotone error — Leisen-Reimer and Crank-Nicolson, not CRR.
    pub richardson: bool,
}

impl Default for ConvergenceConfig {
    fn default() -> Self {
        Self { tolerance: 1e-6, max_refinements: 6, richardson: true }
    }
}
