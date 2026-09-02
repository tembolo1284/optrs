// crates/optrs-engine/src/engine.rs
//! Static dispatch over the four methods. Adding an engine means adding a
//! variant here plus arms in `supports` and `price_raw` — the compiler finds
//! every site you missed.

use crate::config::Config;
use optrs_core::error::{Error, Result};
use optrs_core::instrument::PriceRequest;
use optrs_core::model::gbm::Gbm;
use optrs_tree::lattice::{Crr, LeisenReimer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Engine {
    /// Closed-form BSM. European only, exact.
    Analytic = 0,
    /// Fourier-cosine expansion. European only for now; Bermudan recursion later.
    Cos = 1,
    /// Cox-Ross-Rubinstein binomial.
    TreeCrr = 2,
    /// Leisen-Reimer binomial. Preferred lattice: smooth second-order error.
    TreeLr = 3,
    /// Crank-Nicolson with Rannacher startup; PSOR for American.
    FiniteDifference = 4,
    /// Monte Carlo; Longstaff-Schwartz when early exercise is present.
    MonteCarlo = 5,
}

impl Engine {
    pub const ALL: [Engine; 6] = [
        Engine::Analytic,
        Engine::Cos,
        Engine::TreeCrr,
        Engine::TreeLr,
        Engine::FiniteDifference,
        Engine::MonteCarlo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Engine::Analytic => "analytic",
            Engine::Cos => "cos",
            Engine::TreeCrr => "tree-crr",
            Engine::TreeLr => "tree-lr",
            Engine::FiniteDifference => "finite-difference",
            Engine::MonteCarlo => "monte-carlo",
        }
    }

    /// True when this engine can price the request as stated.
    pub fn supports(self, req: &PriceRequest) -> bool {
        match self {
            Engine::Analytic | Engine::Cos => !req.exercise.is_early_exercise(),
            _ => true,
        }
    }

    /// Deterministic engines give an exact answer for a given config; stochastic
    /// ones carry sampling error. Drives whether the convergence ladder uses a
    /// price delta or a standard-error criterion.
    pub fn is_stochastic(self) -> bool {
        matches!(self, Engine::MonteCarlo)
    }

    /// Cheapest engine that can handle the request. Used when the caller passes
    /// no explicit engine — analytic where possible, lattice otherwise.
    pub fn best_for(req: &PriceRequest) -> Engine {
        if req.exercise.is_early_exercise() {
            Engine::TreeLr
        } else {
            Engine::Analytic
        }
    }

    fn unsupported(self, reason: &'static str) -> Error {
        Error::Unsupported { engine: self.name(), reason }
    }

    /// Single price at the configured discretisation, no refinement.
    pub fn price_raw(self, req: &PriceRequest, cfg: &Config) -> Result<PriceResult> {
        req.validate()?;
        if !self.supports(req) {
            return Err(self.unsupported("early exercise not implemented for this engine"));
        }
        let inp = &req.inputs;

        match self {
            Engine::Analytic => {
                let g = optrs_core::analytic::greeks(inp, req.kind)?;
                Ok(PriceResult { price: g.price, std_error: None, engine: self, greeks: Some(g) })
            }
            Engine::Cos => {
                let model =
                    Gbm { rate: inp.rate, div_yield: inp.div_yield, vol: inp.vol };
                let p = optrs_cos::price(
                    &model, inp.spot, inp.strike, inp.rate, inp.time, req.kind, &cfg.cos,
                )?;
                Ok(PriceResult { price: p, std_error: None, engine: self, greeks: None })
            }
            Engine::TreeCrr => {
                let g = optrs_tree::price_with_greeks(
                    inp, req.kind, &req.exercise, &Crr, &cfg.tree,
                )?;
                Ok(PriceResult { price: g.price, std_error: None, engine: self, greeks: Some(g) })
            }
            Engine::TreeLr => {
                let g = optrs_tree::price_with_greeks(
                    inp, req.kind, &req.exercise, &LeisenReimer, &cfg.tree,
                )?;
                Ok(PriceResult { price: g.price, std_error: None, engine: self, greeks: Some(g) })
            }
            Engine::FiniteDifference => {
                let p = optrs_fd::price(inp, req.kind, &req.exercise, &cfg.fd)?;
                Ok(PriceResult { price: p, std_error: None, engine: self, greeks: None })
            }
            Engine::MonteCarlo => {
                let r = optrs_mc::price(inp, req.kind, &req.exercise, &cfg.mc)?;
                Ok(PriceResult {
                    price: r.price,
                    std_error: Some(r.std_error),
                    engine: self,
                    greeks: None,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PriceResult {
    pub price: f64,
    /// Present only for stochastic engines.
    pub std_error: Option<f64>,
    pub engine: Engine,
    /// Some engines produce greeks for free during pricing. When `None`, the
    /// facade falls back to bump-and-reprice.
    pub greeks: Option<optrs_core::analytic::Greeks>,
}
