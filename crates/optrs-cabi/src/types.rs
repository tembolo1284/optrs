// crates/optrs-cabi/src/types.rs
//! Versioned PODs crossing the boundary. Every struct leads with `size`, set by
//! the caller to `sizeof(struct)`. Adding a field at the END is then backward
//! compatible: old callers pass a smaller size and we skip the new fields.
//! Never reorder or remove a field — only append.

use optrs_core::analytic::{BsmInputs, OptionType};
use optrs_core::error::{Error, Result};
use optrs_core::exercise::Exercise;
use optrs_core::instrument::PriceRequest;

pub const OPT_ABI_VERSION: u32 = 1;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptStatus {
    Ok = 0,
    Domain = 1,
    NoSolution = 2,
    Unsupported = 3,
    NotConverged = 4,
    NullPointer = 5,
    BadSize = 6,
    BufferTooSmall = 7,
    Panic = 8,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptKind {
    Call = 0,
    Put = 1,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptStyle {
    European = 0,
    American = 1,
    Bermudan = 2,
}

/// Option specification. `dates`/`n_dates` are read only when
/// `style == OPT_STYLE_BERMUDAN` and may be null otherwise.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OptOption {
    pub size: u32,
    pub kind: OptKind,
    pub style: OptStyle,
    pub _pad: u32,
    pub spot: f64,
    pub strike: f64,
    pub rate: f64,
    pub div_yield: f64,
    pub vol: f64,
    pub time: f64,
    pub dates: *const f64,
    pub n_dates: usize,
}

/// Pricing output. `has_greeks` and `has_std_error` tell the caller which
/// optional fields were actually populated.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct OptResult {
    pub size: u32,
    pub engine: i32,
    pub has_std_error: i32,
    pub has_greeks: i32,
    pub price: f64,
    pub std_error: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub rho: f64,
    /// Refinements used by `opt_price_converged`; 0 otherwise.
    pub refinements: u32,
    /// Non-zero when Richardson extrapolation was applied.
    pub extrapolated: i32,
}

impl OptOption {
    /// Validate the caller's struct and lift it into a Rust request.
    ///
    /// # Safety
    /// `dates` must point to `n_dates` readable f64 when style is Bermudan.
    pub unsafe fn to_request(&self) -> Result<PriceRequest> {
        // Minimum size accepted: everything through `n_dates`. Bump only if a
        // future field becomes mandatory.
        if (self.size as usize) < std::mem::size_of::<OptOption>() {
            return Err(Error::Domain("opt_option_t size smaller than expected"));
        }

        let inputs = BsmInputs {
            spot: self.spot,
            strike: self.strike,
            rate: self.rate,
            div_yield: self.div_yield,
            vol: self.vol,
            time: self.time,
        };
        let kind = match self.kind {
            OptKind::Call => OptionType::Call,
            OptKind::Put => OptionType::Put,
        };
        let exercise = match self.style {
            OptStyle::European => Exercise::European { expiry: self.time },
            OptStyle::American => Exercise::American { expiry: self.time },
            OptStyle::Bermudan => {
                if self.dates.is_null() || self.n_dates == 0 {
                    return Err(Error::Domain("bermudan requires at least one exercise date"));
                }
                let dates = std::slice::from_raw_parts(self.dates, self.n_dates).to_vec();
                if dates.iter().any(|t| !t.is_finite() || *t <= 0.0) {
                    return Err(Error::Domain("exercise dates must be positive and finite"));
                }
                Exercise::Bermudan { expiry: self.time, dates }
            }
        };

        let req = PriceRequest { inputs, kind, exercise };
        req.validate()?;
        Ok(req)
    }
}

impl OptResult {
    pub fn init() -> Self {
        Self { size: std::mem::size_of::<Self>() as u32, engine: -1, ..Default::default() }
    }

    pub fn check_size(&self) -> Result<()> {
        if (self.size as usize) < std::mem::size_of::<Self>() {
            return Err(Error::Domain("opt_result_t size smaller than expected"));
        }
        Ok(())
    }
}
