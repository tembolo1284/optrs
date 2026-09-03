// crates/optrs-core/src/analytic.rs
//! Closed-form Black–Scholes–Merton. This is the reference oracle: every other
//! engine is tested against it for European vanillas.

use crate::error::{Error, Result};

/// Cumulative normal, Graeme West's double-precision Hart algorithm (~1e-15).
pub fn norm_cdf(x: f64) -> f64 {
    let xabs = x.abs();
    if xabs > 37.0 {
        return if x > 0.0 { 1.0 } else { 0.0 };
    }
    let e = (-xabs * xabs * 0.5).exp();
    let build = if xabs < 7.071_067_811_865_47 {
        let mut b = 3.526_249_659_989_11e-2 * xabs + 0.700_383_064_443_688;
        b = b * xabs + 6.373_962_203_531_65;
        b = b * xabs + 33.912_866_078_383;
        b = b * xabs + 112.079_291_497_871;
        b = b * xabs + 221.213_596_169_931;
        b = b * xabs + 220.206_867_912_376;
        let mut d = 8.838_834_764_831_84e-2 * xabs + 1.755_667_163_182_64;
        d = d * xabs + 16.064_177_579_207;
        d = d * xabs + 86.780_732_202_946_1;
        d = d * xabs + 296.564_248_779_674;
        d = d * xabs + 637.333_633_378_831;
        d = d * xabs + 793.826_512_519_948;
        d = d * xabs + 440.413_735_824_752;
        e * b / d
    } else {
        let mut b = xabs + 0.65;
        b = xabs + 4.0 / b;
        b = xabs + 3.0 / b;
        b = xabs + 2.0 / b;
        b = xabs + 1.0 / b;
        e / (b * 2.506_628_274_631)
    };
    if x > 0.0 { 1.0 - build } else { build }
}

#[inline]
pub fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() * std::f64::consts::FRAC_1_SQRT_2 * std::f64::consts::FRAC_2_SQRT_PI * 0.5
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionType {
    Call,
    Put,
}

impl OptionType {
    #[inline]
    pub fn sign(self) -> f64 {
        match self {
            OptionType::Call => 1.0,
            OptionType::Put => -1.0,
        }
    }
}

/// Inputs for a European vanilla under BSM with continuous carry.
#[derive(Clone, Copy, Debug)]
pub struct BsmInputs {
    pub spot: f64,
    pub strike: f64,
    pub rate: f64,
    pub div_yield: f64,
    pub vol: f64,
    pub time: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Greeks {
    pub price: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub rho: f64,
}

impl BsmInputs {
    fn validate(&self) -> Result<()> {
        if self.spot <= 0.0 || self.strike <= 0.0 {
            return Err(Error::Domain("spot and strike must be positive"));
        }
        if self.time < 0.0 || self.vol < 0.0 {
            return Err(Error::Domain("time and vol must be non-negative"));
        }
        Ok(())
    }

    /// d1, d2. Caller must have handled the degenerate `vol * sqrt(t) == 0` case.
    #[inline]
    fn d1_d2(&self) -> (f64, f64) {
        let sqt = self.vol * self.time.sqrt();
        let d1 = ((self.spot / self.strike).ln()
            + (self.rate - self.div_yield + 0.5 * self.vol * self.vol) * self.time)
            / sqt;
        (d1, d1 - sqt)
    }
}

/// Intrinsic value discounted for carry — the limit as vol*sqrt(t) -> 0.
fn deterministic(inp: &BsmInputs, kind: OptionType) -> f64 {
    let fwd = inp.spot * ((inp.rate - inp.div_yield) * inp.time).exp();
    let df = (-inp.rate * inp.time).exp();
    df * (kind.sign() * (fwd - inp.strike)).max(0.0)
}

pub fn price(inp: &BsmInputs, kind: OptionType) -> Result<f64> {
    inp.validate()?;
    if inp.vol * inp.time.sqrt() < 1e-14 {
        return Ok(deterministic(inp, kind));
    }
    let (d1, d2) = inp.d1_d2();
    let w = kind.sign();
    let dfq = (-inp.div_yield * inp.time).exp();
    let dfr = (-inp.rate * inp.time).exp();
    Ok(w * (inp.spot * dfq * norm_cdf(w * d1) - inp.strike * dfr * norm_cdf(w * d2)))
}

pub fn greeks(inp: &BsmInputs, kind: OptionType) -> Result<Greeks> {
    inp.validate()?;
    if inp.vol * inp.time.sqrt() < 1e-14 {
        return Ok(Greeks { price: deterministic(inp, kind), ..Default::default() });
    }
    let (d1, d2) = inp.d1_d2();
    let w = kind.sign();
    let dfq = (-inp.div_yield * inp.time).exp();
    let dfr = (-inp.rate * inp.time).exp();
    let sqt = inp.time.sqrt();
    let nd1 = norm_pdf(d1);

    Ok(Greeks {
        price: w * (inp.spot * dfq * norm_cdf(w * d1) - inp.strike * dfr * norm_cdf(w * d2)),
        delta: w * dfq * norm_cdf(w * d1),
        gamma: dfq * nd1 / (inp.spot * inp.vol * sqt),
        vega: inp.spot * dfq * nd1 * sqt,
        theta: -inp.spot * dfq * nd1 * inp.vol / (2.0 * sqt)
            + w * inp.div_yield * inp.spot * dfq * norm_cdf(w * d1)
            - w * inp.rate * inp.strike * dfr * norm_cdf(w * d2),
        rho: w * inp.strike * inp.time * dfr * norm_cdf(w * d2),
    })
}

/// Implied vol by Newton with a bisection safety net. Bracket is generous
/// because this feeds surface calibration, not a hot loop.
pub fn implied_vol(target: f64, inp: &BsmInputs, kind: OptionType) -> Result<f64> {
    let (mut lo, mut hi) = (1e-9_f64, 5.0_f64);
    let mut probe = *inp;

    probe.vol = hi;
    if price(&probe, kind)? < target {
        return Err(Error::NoSolution("target price above vol=500% bound"));
    }
    probe.vol = lo;
    if price(&probe, kind)? > target {
        return Err(Error::NoSolution("target price below intrinsic"));
    }

    let mut v = 0.2_f64;
    for _ in 0..100 {
        probe.vol = v;
        let g = greeks(&probe, kind)?;
        let diff = g.price - target;
        if diff.abs() < 1e-12 {
            return Ok(v);
        }
        if diff > 0.0 { hi = v } else { lo = v }

        let step = if g.vega > 1e-12 { diff / g.vega } else { f64::INFINITY };
        let next = v - step;
        v = if next > lo && next < hi { next } else { 0.5 * (lo + hi) };
    }
    Ok(v)
}
