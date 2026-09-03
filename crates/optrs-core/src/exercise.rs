// crates/optrs-core/src/exercise.rs
//! Exercise policy. Engines convert this into a per-step boolean mask, which is
//! why European/American/Bermudan cost the same code path everywhere downstream.

#[derive(Clone, Debug)]
pub enum Exercise {
    European { expiry: f64 },
    American { expiry: f64 },
    /// `dates` need not include expiry; it is always exercisable.
    Bermudan { expiry: f64, dates: Vec<f64> },
}

impl Exercise {
    pub fn expiry(&self) -> f64 {
        match self {
            Exercise::European { expiry }
            | Exercise::American { expiry }
            | Exercise::Bermudan { expiry, .. } => *expiry,
        }
    }

    /// Mask of length `steps + 1` over a uniform grid on [0, expiry].
    /// Index 0 (today) is never marked: pricing at t=0 is the caller's max().
    pub fn step_mask(&self, steps: usize) -> Vec<bool> {
        let mut mask = vec![false; steps + 1];
        match self {
            Exercise::European { .. } => {
                mask[steps] = true;
            }
            Exercise::American { .. } => {
                mask[1..].fill(true);
            }
            Exercise::Bermudan { expiry, dates } => {
                let dt = expiry / steps as f64;
                for &t in dates {
                    if t > 0.0 && t <= *expiry {
                        // Snap to nearest step; never snap onto today.
                        let i = ((t / dt).round() as usize).clamp(1, steps);
                        mask[i] = true;
                    }
                }
                mask[steps] = true;
            }
        }
        mask
    }

    /// Shift the schedule forward in calendar time, dropping elapsed dates.
    pub fn rolled_forward(&self, dt: f64) -> Self {
        match self {
            Exercise::European { expiry } => Exercise::European { expiry: expiry - dt },
            Exercise::American { expiry } => Exercise::American { expiry: expiry - dt },
            Exercise::Bermudan { expiry, dates } => Exercise::Bermudan {
                expiry: expiry - dt,
                dates: dates.iter().map(|t| t - dt).filter(|t| *t > 0.0).collect(),
            },
        }
    }

    pub fn is_early_exercise(&self) -> bool {
        !matches!(self, Exercise::European { .. })
    }
}
