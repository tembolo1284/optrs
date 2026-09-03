// tests/cross_engine.rs
use optrs_core::analytic::{self, BsmInputs, OptionType};
use optrs_core::exercise::Exercise;
use optrs_core::model::gbm::Gbm;
use optrs_fd::FdConfig;
use optrs_mc::McConfig;
use optrs_tree::lattice::LeisenReimer;
use optrs_tree::TreeConfig;

fn base() -> BsmInputs {
    BsmInputs { spot: 100.0, strike: 100.0, rate: 0.05, div_yield: 0.02, vol: 0.25, time: 1.0 }
}

#[test]
fn european_all_engines_agree() {
    let inp = base();
    let ex = Exercise::European { expiry: inp.time };
    let model = Gbm { rate: inp.rate, div_yield: inp.div_yield, vol: inp.vol };

    for kind in [OptionType::Call, OptionType::Put] {
        let truth = analytic::price(&inp, kind).unwrap();

        let cos = optrs_cos::price(
            &model, inp.spot, inp.strike, inp.rate, inp.time, kind, &Default::default(),
        ).unwrap();
        assert!((cos - truth).abs() < 1e-10, "cos");

        let tree = optrs_tree::price(&inp, kind, &ex, &LeisenReimer, &TreeConfig { steps: 1001 }).unwrap();
        assert!((tree - truth).abs() < 1e-4, "tree: {tree} vs {truth}");

        let fd = optrs_fd::price(&inp, kind, &ex, &FdConfig::default()).unwrap();
        assert!((fd - truth).abs() < 1e-3, "fd: {fd} vs {truth}");

        let mc = optrs_mc::price(&inp, kind, &ex, &McConfig::default()).unwrap();
        assert!((mc.price - truth).abs() < 4.0 * mc.std_error.max(1e-4), "mc");
    }
}

#[test]
fn american_put_premium_is_positive_and_consistent() {
    let inp = base();
    let ex = Exercise::American { expiry: inp.time };
    let euro = analytic::price(&inp, OptionType::Put).unwrap();

    let tree = optrs_tree::price(&inp, OptionType::Put, &ex, &LeisenReimer, &TreeConfig { steps: 2001 }).unwrap();
    let fd = optrs_fd::price(&inp, OptionType::Put, &ex, &FdConfig::default()).unwrap();

    assert!(tree > euro);
    assert!((tree - fd).abs() < 2e-3, "tree {tree} vs fd {fd}");
}

#[test]
fn bermudan_is_bracketed_by_european_and_american() {
    let inp = base();
    let dates: Vec<f64> = (1..=4).map(|i| i as f64 * 0.25).collect();
    let berm = Exercise::Bermudan { expiry: 1.0, dates };
    let amer = Exercise::American { expiry: 1.0 };
    let cfg = TreeConfig { steps: 2001 };

    let e = analytic::price(&inp, OptionType::Put).unwrap();
    let b = optrs_tree::price(&inp, OptionType::Put, &berm, &LeisenReimer, &cfg).unwrap();
    let a = optrs_tree::price(&inp, OptionType::Put, &amer, &LeisenReimer, &cfg).unwrap();

    assert!(e <= b + 1e-9 && b <= a + 1e-9, "{e} <= {b} <= {a}");
}
