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

/// Run with `--nocapture` to see the full diagnostic table.
///
/// Interpretation guide when this fails:
///   - grid lines identical across the rate bump  -> mesh is stable, so a rho
///     error is a real discretisation error, not relocation
///   - grid lines differ                          -> the mesh moved; the bump
///     is measuring relocation, not sensitivity
///   - all greeks off by a similar relative amount -> the base price is wrong,
///     not the bumping
#[test]
fn bumped_greeks_match_analytic() {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let cfg = Config::default();
    let exact = optrs_core::analytic::greeks(&req.inputs, req.kind).unwrap();

    // FD has no native greeks, so this exercises the full bump path.
    let bumped = greeks(Engine::FiniteDifference, &req, &cfg).unwrap();

    // Confirm the mesh does not move when `rate` is bumped, which was the
    // original cause of a ~1e-2 rho error.
    let bump = cfg.greeks.rate_abs;
    let mut up = req.inputs;
    up.rate += bump;
    let mut dn = req.inputs;
    dn.rate -= bump;
    let g0 = optrs_fd::grid_report(&req.inputs, &cfg.fd);
    let gu = optrs_fd::grid_report(&up, &cfg.fd);
    let gd = optrs_fd::grid_report(&dn, &cfg.fd);

    println!("\ngrid under rate bump (lo, hi, dx, spot offset within cell)");
    println!("  base {:?}", g0);
    println!("  r+   {:?}", gu);
    println!("  r-   {:?}", gd);
    println!("  mesh stable: {}", g0.0 == gu.0 && g0.0 == gd.0 && g0.2 == gu.2);

    let rows = [
        ("price", bumped.price, exact.price, 1e-3),
        ("delta", bumped.delta, exact.delta, 1e-3),
        ("gamma", bumped.gamma, exact.gamma, 1e-3),
        ("vega", bumped.vega, exact.vega, 1e-2),
        ("theta", bumped.theta, exact.theta, 5e-2),
        ("rho", bumped.rho, exact.rho, 1e-2),
    ];

    println!("\n{:<6} {:>14} {:>14} {:>12} {:>12}", "greek", "bumped", "exact", "abs diff", "tol");
    for (name, got, want, tol) in rows {
        println!("{name:<6} {got:>14.8} {want:>14.8} {:>12.2e} {tol:>12.0e}", (got - want).abs());
    }
    println!();

    for (name, got, want, tol) in rows {
        assert!((got - want).abs() < tol, "{name}: {got} vs {want}");
    }
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
