// crates/optrs-engine/tests/facade.rs
use optrs_core::analytic::{BsmInputs, OptionType};
use optrs_core::instrument::PriceRequest;
use optrs_engine::{greeks, price_all, price_converged, Config, Engine};

fn inputs() -> BsmInputs {
    BsmInputs { spot: 100.0, strike: 95.0, rate: 0.04, div_yield: 0.015, vol: 0.3, time: 0.75 }
}

#[test]
fn every_supporting_engine_agrees_on_european() {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let cfg = Config::default();
    let truth = optrs_core::analytic::price(&req.inputs, req.kind).unwrap();

    for (engine, res) in price_all(&req, &cfg) {
        let r = res.unwrap_or_else(|e| panic!("{} failed: {e}", engine.name()));
        let tol = match engine {
            Engine::Analytic | Engine::Cos => 1e-10,
            Engine::TreeCrr => 5e-3,
            Engine::TreeLr => 1e-4,
            Engine::FiniteDifference => 1e-3,
            Engine::MonteCarlo => 4.0 * r.std_error.unwrap(),
        };
        assert!((r.price - truth).abs() < tol, "{}: {} vs {truth}", engine.name(), r.price);
    }
}

#[test]
fn support_matrix_rejects_early_exercise_where_expected() {
    let req = PriceRequest::american(inputs(), OptionType::Put);
    assert!(!Engine::Analytic.supports(&req));
    assert!(!Engine::Cos.supports(&req));
    assert!(Engine::TreeLr.supports(&req));
    assert!(Engine::FiniteDifference.supports(&req));
    assert!(Engine::MonteCarlo.supports(&req));
    assert!(Engine::Analytic.price_raw(&req, &Config::default()).is_err());
}

#[test]
fn convergence_beats_a_single_coarse_run() {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let mut cfg = Config::default();
    cfg.tree.steps = 25;
    cfg.convergence.tolerance = 1e-5;

    let truth = optrs_core::analytic::price(&req.inputs, req.kind).unwrap();
    let coarse = Engine::TreeLr.price_raw(&req, &cfg).unwrap().price;
    let report = price_converged(Engine::TreeLr, &req, &cfg).unwrap();

    assert!(report.refinements > 0);
    assert!((report.result.price - truth).abs() <= (coarse - truth).abs());
    assert!((report.result.price - truth).abs() < 1e-5);
}

#[test]
fn bumped_greeks_match_analytic() {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let cfg = Config::default();
    let exact = optrs_core::analytic::greeks(&req.inputs, req.kind).unwrap();

    // FD has no native greeks, so this exercises the full bump path.
    let bumped = greeks(Engine::FiniteDifference, &req, &cfg).unwrap();
    assert!((bumped.delta - exact.delta).abs() < 1e-3, "delta");
    assert!((bumped.gamma - exact.gamma).abs() < 1e-3, "gamma");
    assert!((bumped.vega - exact.vega).abs() < 1e-2, "vega");
    assert!((bumped.rho - exact.rho).abs() < 1e-2, "rho");
    assert!((bumped.theta - exact.theta).abs() < 5e-2, "theta");
}

#[test]
fn american_engines_agree_through_the_facade() {
    let req = PriceRequest::american(inputs(), OptionType::Put);
    let mut cfg = Config::default();
    cfg.tree.steps = 2001;

    let tree = Engine::TreeLr.price_raw(&req, &cfg).unwrap().price;
    let fd = Engine::FiniteDifference.price_raw(&req, &cfg).unwrap().price;
    let euro = optrs_core::analytic::price(&req.inputs, req.kind).unwrap();

    assert!(tree > euro, "american put must carry early-exercise premium");
    assert!((tree - fd).abs() < 3e-3, "tree {tree} vs fd {fd}");
}
