// crates/optrs-core/src/instrument.rs
//! A priceable request: market inputs + payoff type + exercise policy.
//! Engines take this whole thing so the facade has one signature to dispatch on.

use crate::analytic::{BsmInputs, OptionType};
use crate::error::{Error, Result};
use crate::exercise::Exercise;

#[derive(Clone, Debug)]
pub struct PriceRequest {
    pub inputs: BsmInputs,
    pub kind: OptionType,
    pub exercise: Exercise,
}

impl PriceRequest {
    pub fn european(inputs: BsmInputs, kind: OptionType) -> Self {
        let expiry = inputs.time;
        Self { inputs, kind, exercise: Exercise::European { expiry } }
    }

    pub fn american(inputs: BsmInputs, kind: OptionType) -> Self {
        let expiry = inputs.time;
        Self { inputs, kind, exercise: Exercise::American { expiry } }
    }

    pub fn bermudan(inputs: BsmInputs, kind: OptionType, dates: Vec<f64>) -> Self {
        let expiry = inputs.time;
        Self { inputs, kind, exercise: Exercise::Bermudan { expiry, dates } }
    }

    pub fn validate(&self) -> Result<()> {
        if (self.exercise.expiry() - self.inputs.time).abs() > 1e-12 {
            return Err(Error::Domain("exercise expiry must equal inputs.time"));
        }
        if self.inputs.time <= 0.0 {
            return Err(Error::Domain("time to expiry must be positive"));
        }
        Ok(())
    }

    /// Roll the whole request forward by `dt` years. Used by the theta bump:
    /// shortening time to expiry must shorten the exercise schedule with it.
    pub fn rolled_forward(&self, dt: f64) -> Result<Self> {
        let new_t = self.inputs.time - dt;
        if new_t <= 0.0 {
            return Err(Error::Domain("theta bump exceeds time to expiry"));
        }
        let mut out = self.clone();
        out.inputs.time = new_t;
        out.exercise = self.exercise.rolled_forward(dt);
        Ok(out)
    }
}
