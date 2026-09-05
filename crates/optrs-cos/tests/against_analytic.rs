// crates/optrs-cos/tests/against_analytic.rs
use optrs_core::analytic::{self, BsmInputs, OptionType};
use optrs_core::model::gbm::Gbm;
use optrs_cos::{price, CosConfig};

fn cos_price(inp: &BsmInputs, kind: OptionType, cfg: &CosConfig) -> f64 {
    let model = Gbm { rate: inp.rate, div_yield: inp.div_yield, vol: inp.vol };
    price(&model, inp.spot, inp.strike, inp.rate, inp.time, kind, cfg).unwrap()
}

#[test]
fn cos_matches_black_scholes() {
    let cfg = CosConfig::default();
    for &strike in &[60.0, 90.0, 100.0, 110.0, 160.0] {
        for &time in &[0.05, 1.0, 5.0] {
            for &vol in &[0.05, 0.2, 0.6] {
                let inp = BsmInputs {
                    spot: 100.0, strike, rate: 0.03, div_yield: 0.01, vol, time,
                };
                for kind in [OptionType::Call, OptionType::Put] {
                    let exact = analytic::price(&inp, kind).unwrap();
                    let cos = cos_price(&inp, kind, &cfg);
                    assert!((exact - cos).abs() < 1e-10, "K={strike} T={time} v={vol}");
                }
            }
        }
    }
}

/// Duffy's QuantNet batches. Carry b = r in all four, so div_yield = 0.
/// Published values are quoted to 5 decimals, hence the 1e-4 tolerance here;
/// the agreement against our own closed form is checked separately at 1e-10.
#[test]
fn duffy_batches() {
    struct Batch {
        name: &'static str,
        inp: BsmInputs,
        call: f64,
        put: f64,
    }

    let batches = [
        Batch {
            name: "batch 1",
            inp: BsmInputs { spot: 60.0, strike: 65.0, rate: 0.08, div_yield: 0.0, vol: 0.30, time: 0.25 },
            call: 2.13337,
            put: 5.84628,
        },
        Batch {
            name: "batch 2",
            inp: BsmInputs { spot: 100.0, strike: 100.0, rate: 0.0, div_yield: 0.0, vol: 0.2, time: 1.0 },
            call: 7.96557,
            put: 7.96557,
        },
        Batch {
            name: "batch 3",
            inp: BsmInputs { spot: 5.0, strike: 10.0, rate: 0.12, div_yield: 0.0, vol: 0.50, time: 1.0 },
            call: 0.204058,
            put: 4.07326,
        },
        Batch {
            name: "batch 4",
            inp: BsmInputs { spot: 100.0, strike: 100.0, rate: 0.08, div_yield: 0.0, vol: 0.30, time: 30.0 },
            call: 92.17570,
            put: 1.24750,
        },
    ];

    let cfg = CosConfig::default();
    for b in &batches {
        for (kind, expected) in
            [(OptionType::Call, b.call), (OptionType::Put, b.put)]
        {
            let exact = analytic::price(&b.inp, kind).unwrap();
            let cos = cos_price(&b.inp, kind, &cfg);

            assert!(
                (exact - expected).abs() < 1e-4,
                "{} {kind:?}: analytic {exact} vs published {expected}",
                b.name
            );
            assert!(
                (cos - exact).abs() < 1e-10,
                "{} {kind:?}: cos {cos} vs analytic {exact}",
                b.name
            );
        }
    }
}

/// Batch 3 is the interesting case: spot is half the strike, so the call is
/// deep out of the money and the truncation interval sits well below zero.
/// This is the configuration that broke the original payoff-coefficient ranges.
#[test]
fn deep_out_of_the_money_is_stable() {
    let cfg = CosConfig::default();
    for &strike in &[10.0, 20.0, 50.0, 200.0] {
        let inp = BsmInputs {
            spot: 5.0, strike, rate: 0.12, div_yield: 0.0, vol: 0.50, time: 1.0,
        };
        for kind in [OptionType::Call, OptionType::Put] {
            let exact = analytic::price(&inp, kind).unwrap();
            let cos = cos_price(&inp, kind, &cfg);
            assert!((cos - exact).abs() < 1e-9, "K={strike} {kind:?}: {cos} vs {exact}");
            assert!(cos >= 0.0);
        }
    }
}

#[test]
fn put_call_parity_holds() {
    let cfg = CosConfig::default();
    for &time in &[0.25, 1.0, 30.0] {
        let inp = BsmInputs {
            spot: 100.0, strike: 100.0, rate: 0.08, div_yield: 0.0, vol: 0.30, time,
        };
        let c = cos_price(&inp, OptionType::Call, &cfg);
        let p = cos_price(&inp, OptionType::Put, &cfg);
        let lhs = c - p;
        let rhs = inp.spot * (-inp.div_yield * time).exp()
            - inp.strike * (-inp.rate * time).exp();
        assert!((lhs - rhs).abs() < 1e-9, "T={time}: {lhs} vs {rhs}");
    }
}

/// Spectral convergence: doubling the terms should not move the answer once
/// the series has resolved. If this fails, the truncation range is too narrow
/// rather than the term count being too low.
#[test]
fn converges_in_terms() {
    let inp = BsmInputs {
        spot: 100.0, strike: 100.0, rate: 0.08, div_yield: 0.0, vol: 0.30, time: 30.0,
    };
    let exact = analytic::price(&inp, OptionType::Call).unwrap();
    let mut prev = f64::INFINITY;
    for terms in [64, 128, 256, 512] {
        let cfg = CosConfig { terms, trunc_l: 10.0 };
        let err = (cos_price(&inp, OptionType::Call, &cfg) - exact).abs();
        assert!(err <= prev + 1e-14, "error grew at N={terms}");
        prev = err;
    }
    assert!(prev < 1e-10);
}
