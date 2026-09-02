// crates/optrs-mc/src/rng.rs
//! PCG64-DXSM plus Acklam inverse normal with one Halley refinement.
//! Self-contained so results are bit-reproducible across platforms and so the
//! Sobol path later slots in behind the same `UniformStream` trait.

use optrs_core::analytic::norm_cdf;

pub trait UniformStream {
    fn next_uniform(&mut self) -> f64;
}

pub struct Pcg64 {
    state: u128,
    inc: u128,
}

impl Pcg64 {
    pub fn new(seed: u64, stream: u64) -> Self {
        let inc = ((stream as u128) << 1) | 1;
        let mut rng = Self { state: 0, inc };
        rng.step();
        rng.state = rng.state.wrapping_add(seed as u128);
        rng.step();
        rng
    }

    #[inline]
    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(0x2360_ED05_1FC6_5DA4_4385_DF64_9FCC_F645)
            .wrapping_add(self.inc);
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.step();
        let hi = (self.state >> 64) as u64;
        let lo = (self.state as u64) | 1;
        let mut x = hi ^ (hi >> 32);
        x = x.wrapping_mul(0xDA94_2042_E4DD_58B5);
        x ^= x >> 48;
        x.wrapping_mul(lo)
    }
}

impl UniformStream for Pcg64 {
    #[inline]
    fn next_uniform(&mut self) -> f64 {
        // Open interval: inverse normal must never see 0 or 1.
        ((self.next_u64() >> 11) as f64 + 0.5) * (1.0 / 9_007_199_254_740_992.0)
    }
}

const A: [f64; 6] = [
    -3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02,
    1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00,
];
const B: [f64; 5] = [
    -5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02,
    6.680131188771972e+01, -1.328068155288572e+01,
];
const C: [f64; 6] = [
    -7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00,
    -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00,
];
const D: [f64; 4] = [
    7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00,
    3.754408661907416e+00,
];

pub fn inverse_normal(p: f64) -> f64 {
    const PLOW: f64 = 0.02425;
    let x = if p < PLOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p > 1.0 - PLOW {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    };
    // Halley step lifts Acklam's 1.15e-9 to full double precision.
    let e = norm_cdf(x) - p;
    let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
    x - u / (1.0 + x * u / 2.0)
}
