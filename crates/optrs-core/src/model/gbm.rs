// crates/optrs-core/src/model/gbm.rs
use num_complex::Complex64;
use super::CharFn;

#[derive(Clone, Copy, Debug)]
pub struct Gbm {
    pub rate: f64,
    pub div_yield: f64,
    pub vol: f64,
}

impl CharFn for Gbm {
    fn cf(&self, u: f64, t: f64) -> Complex64 {
        let var = self.vol * self.vol;
        let drift = self.rate - self.div_yield - 0.5 * var;
        (Complex64::i() * u * drift * t - Complex64::from(0.5 * var * u * u * t)).exp()
    }

    fn cumulants(&self, t: f64) -> (f64, f64, f64) {
        let var = self.vol * self.vol;
        ((self.rate - self.div_yield - 0.5 * var) * t, var * t, 0.0)
    }
}
