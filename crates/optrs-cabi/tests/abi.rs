// crates/optrs-cabi/tests/abi.rs
//! Exercises the exported functions from Rust so `cargo test` covers the ABI
//! without needing a C compiler in CI.

use optrs_cabi::types::{OptKind, OptOption, OptResult, OptStatus, OptStyle};
use optrs_cabi::*;

fn european_call() -> OptOption {
    let mut o = OptOption {
        size: std::mem::size_of::<OptOption>() as u32,
        kind: OptKind::Call,
        style: OptStyle::European,
        _pad: 0,
        spot: 100.0,
        strike: 100.0,
        rate: 0.05,
        div_yield: 0.02,
        vol: 0.25,
        time: 1.0,
        dates: std::ptr::null(),
        n_dates: 0,
    };
    o.size = std::mem::size_of::<OptOption>() as u32;
    o
}

#[test]
fn layout_handshake() {
    assert_eq!(opt_sizeof_option(), std::mem::size_of::<OptOption>());
    assert_eq!(opt_sizeof_result(), std::mem::size_of::<OptResult>());
    assert_eq!(opt_abi_version(), 1);
}

#[test]
fn price_round_trip() {
    unsafe {
        let p = opt_pricer_new();
        let o = european_call();
        let mut r = OptResult::init();
        assert_eq!(opt_price(p, 0, &o, &mut r), OptStatus::Ok);

        let truth = optrs_core::analytic::price(
            &optrs_core::analytic::BsmInputs {
                spot: o.spot, strike: o.strike, rate: o.rate,
                div_yield: o.div_yield, vol: o.vol, time: o.time,
            },
            optrs_core::analytic::OptionType::Call,
        )
        .unwrap();
        assert!((r.price - truth).abs() < 1e-12);
        opt_pricer_free(p);
    }
}

#[test]
fn rejects_stale_struct_size() {
    unsafe {
        let p = opt_pricer_new();
        let mut o = european_call();
        o.size = 8; // pretend an ancient caller
        let mut r = OptResult::init();
        assert_eq!(opt_price(p, 0, &o, &mut r), OptStatus::Domain);
        assert!(!opt_last_error_message().is_null());
        opt_pricer_free(p);
    }
}

#[test]
fn buffer_too_small_reports_required_size() {
    unsafe {
        let p = opt_pricer_new();
        let o = european_call();
        let mut one = [OptResult::init()];
        let mut n = 0usize;
        let st = opt_price_all(p, &o, one.as_mut_ptr(), 1, &mut n);
        assert_eq!(st, OptStatus::BufferTooSmall);
        assert!(n > 1, "should report how many are needed");
        opt_pricer_free(p);
    }
}

#[test]
fn setter_validation_rejects_nonsense() {
    unsafe {
        let p = opt_pricer_new();
        assert_eq!(opt_set_tree_steps(p, 1), OptStatus::Domain);
        assert_eq!(opt_set_tolerance(p, -1.0), OptStatus::Domain);
        assert_eq!(opt_set_tree_steps(p, 501), OptStatus::Ok);
        opt_pricer_free(p);
    }
}

#[test]
fn null_pointers_never_crash() {
    unsafe {
        let mut r = OptResult::init();
        assert_eq!(opt_price(std::ptr::null(), 0, std::ptr::null(), &mut r), OptStatus::NullPointer);
        assert_eq!(opt_set_tree_steps(std::ptr::null_mut(), 100), OptStatus::NullPointer);
    }
}
