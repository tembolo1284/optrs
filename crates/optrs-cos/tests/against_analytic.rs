// crates/optrs-cos/tests/against_analytic.rs
use optrs_core::analytic::{self, BsmInputs, OptionType};
use optrs_core::model::gbm::Gbm;
use optrs_cos::{price, CosConfig};

#[test]
fn cos_matches_black_scholes() {
    let cfg = CosConfig::default();
    for &strike in &[60.0, 90.0, 100.0, 110.0, 160.0] {
        for &time in &[0.05, 1.0, 5.0] {
            for &vol in &[0.05, 0.2, 0.6] {
                let inp = BsmInputs {
                    spot: 100.0, strike, rate: 0.03, div_yield: 0.01, vol, time,
                };
                let model = Gbm { rate: inp.rate, div_yield: inp.div_yield, vol };
                for kind in [OptionType::Call, OptionType::Put] {
                    let exact = analytic::price(&inp, kind).unwrap();
                    let cos = price(&model, inp.spot, strike, inp.rate, time, kind, &cfg).unwrap();
                    assert!((exact - cos).abs() < 1e-10, "K={strike} T={time} v={vol}");
                }
            }
        }
    }
}
