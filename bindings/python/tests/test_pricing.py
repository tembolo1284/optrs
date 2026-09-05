# bindings/python/tests/test_pricing.py
"""Tests against the C ABI. Mirrors the Rust suite so a divergence between the
two points at the binding rather than the maths."""

from __future__ import annotations

import math

import pytest

import optrs
from optrs import Engine, OptrsError, Pricer

# Duffy's QuantNet batches: (spot, strike, rate, vol, time, call, put)
BATCHES = [
    pytest.param(60.0, 65.0, 0.08, 0.30, 0.25, 2.13337, 5.84628, id="batch1"),
    pytest.param(100.0, 100.0, 0.0, 0.20, 1.0, 7.96557, 7.96557, id="batch2"),
    pytest.param(5.0, 10.0, 0.12, 0.50, 1.0, 0.204058, 4.07326, id="batch3"),
    pytest.param(100.0, 100.0, 0.08, 0.30, 30.0, 92.17570, 1.24750, id="batch4"),
]


@pytest.fixture
def pricer():
    with Pricer() as p:
        yield p


def test_library_loads_and_reports_version():
    assert optrs.version()
    assert Engine.TREE_LR.label == "tree-lr"


@pytest.mark.parametrize("spot,strike,rate,vol,time,call,put", BATCHES)
def test_duffy_batches(pricer, spot, strike, rate, vol, time, call, put):
    for kind, expected in (("call", call), ("put", put)):
        q = pricer.price(
            spot=spot, strike=strike, rate=rate, vol=vol, time=time,
            kind=kind, engine=Engine.ANALYTIC,
        )
        assert q.price == pytest.approx(expected, abs=1e-4)


@pytest.mark.parametrize("spot,strike,rate,vol,time,call,put", BATCHES)
def test_cos_matches_analytic(pricer, spot, strike, rate, vol, time, call, put):
    for kind in ("call", "put"):
        common = dict(spot=spot, strike=strike, rate=rate, vol=vol, time=time, kind=kind)
        a = pricer.price(engine=Engine.ANALYTIC, **common).price
        c = pricer.price(engine=Engine.COS, **common).price
        assert c == pytest.approx(a, abs=1e-10)


def test_auto_engine_picks_analytic_for_european(pricer):
    q = pricer.price(spot=100, strike=100, rate=0.05, vol=0.25, time=1.0)
    assert q.engine is Engine.ANALYTIC
    assert q.std_error is None


def test_compare_returns_every_supporting_engine(pricer):
    quotes = pricer.compare(spot=100, strike=100, rate=0.05, div_yield=0.02, vol=0.25, time=1.0)
    engines = {q.engine for q in quotes}
    assert Engine.ANALYTIC in engines
    assert Engine.COS in engines

    truth = next(q.price for q in quotes if q.engine is Engine.ANALYTIC)
    for q in quotes:
        tol = 4 * q.std_error if q.std_error else 5e-3
        assert q.price == pytest.approx(truth, abs=tol), q.engine.label


def test_american_put_carries_early_exercise_premium(pricer):
    common = dict(spot=100, strike=100, rate=0.05, div_yield=0.02, vol=0.25, time=1.0, kind="put")
    euro = pricer.price(engine=Engine.ANALYTIC, **common).price
    amer = pricer.configure(tree_steps=1001).price(
        engine=Engine.TREE_LR, american=True, **common
    ).price
    assert amer > euro


def test_bermudan_is_bracketed(pricer):
    common = dict(spot=100, strike=100, rate=0.05, vol=0.25, time=1.0, kind="put")
    pricer.configure(tree_steps=1001)
    euro = pricer.price(engine=Engine.ANALYTIC, **common).price
    berm = pricer.price(engine=Engine.TREE_LR, bermudan=[0.25, 0.5, 0.75, 1.0], **common).price
    amer = pricer.price(engine=Engine.TREE_LR, american=True, **common).price
    assert euro <= berm + 1e-9 <= amer + 1e-9


def test_unsupported_engine_raises_with_a_useful_message(pricer):
    with pytest.raises(OptrsError) as exc:
        pricer.price(
            spot=100, strike=100, vol=0.25, time=1.0, kind="put",
            american=True, engine=Engine.ANALYTIC,
        )
    assert exc.value.status == 3
    assert "unsupported" in str(exc.value).lower()


def test_supports_matches_the_engine_matrix(pricer):
    common = dict(spot=100, strike=100, vol=0.25, time=1.0, kind="put")
    assert pricer.supports(Engine.ANALYTIC, **common)
    assert not pricer.supports(Engine.ANALYTIC, american=True, **common)
    assert pricer.supports(Engine.TREE_LR, american=True, **common)


def test_domain_errors_are_raised_not_returned(pricer):
    with pytest.raises(OptrsError):
        pricer.price(spot=-1, strike=100, vol=0.25, time=1.0)
    with pytest.raises(OptrsError):
        pricer.price(spot=100, strike=100, vol=0.25, time=0.0)


def test_monte_carlo_reports_a_standard_error(pricer):
    pricer.configure(mc_paths=50_000, mc_seed=12345)
    q = pricer.price(spot=100, strike=100, rate=0.05, vol=0.25, time=1.0, engine="mc")
    assert q.std_error is not None and q.std_error > 0
    truth = pricer.price(spot=100, strike=100, rate=0.05, vol=0.25, time=1.0).price
    assert abs(q.price - truth) < 4 * q.std_error


def test_seeded_monte_carlo_is_reproducible(pricer):
    args = dict(spot=100, strike=100, rate=0.05, vol=0.25, time=1.0, engine="mc")
    a = pricer.configure(mc_seed=42, mc_paths=20_000).price(**args).price
    b = pricer.configure(mc_seed=42, mc_paths=20_000).price(**args).price
    assert a == b


def test_greeks_match_closed_form(pricer):
    common = dict(spot=100, strike=95, rate=0.04, div_yield=0.015, vol=0.30, time=0.75)
    exact = pricer.greeks(engine=Engine.ANALYTIC, **common)
    bumped = pricer.greeks(engine=Engine.FINITE_DIFFERENCE, **common)
    assert bumped.delta == pytest.approx(exact.delta, abs=1e-3)
    assert bumped.gamma == pytest.approx(exact.gamma, abs=1e-3)
    assert bumped.vega == pytest.approx(exact.vega, abs=1e-2)
    assert bumped.rho == pytest.approx(exact.rho, abs=1e-2)


def test_convergence_reports_refinements(pricer):
    pricer.configure(tree_steps=25, tolerance=1e-5)
    q = pricer.price(
        spot=100, strike=95, rate=0.04, vol=0.30, time=0.75,
        engine=Engine.TREE_LR, converged=True,
    )
    assert q.refinements > 0
    truth = pricer.price(spot=100, strike=95, rate=0.04, vol=0.30, time=0.75).price
    assert q.price == pytest.approx(truth, abs=1e-5)


def test_implied_vol_round_trips():
    common = dict(spot=100, strike=110, rate=0.05, time=1.0, kind="call")
    target = optrs.price(vol=0.28, **common)
    assert optrs.implied_vol(target, **common) == pytest.approx(0.28, abs=1e-8)


def test_bermudan_dates_survive_the_call():
    """Regression: the dates buffer must outlive the option struct. If the
    keepalive is dropped early this reads freed memory and the price drifts."""
    dates = [0.2, 0.4, 0.6, 0.8, 1.0]
    with Pricer(tree_steps=501) as p:
        first = p.price(
            spot=100, strike=100, rate=0.05, vol=0.25, time=1.0,
            kind="put", bermudan=dates, engine=Engine.TREE_LR,
        ).price
        for _ in range(20):
            again = p.price(
                spot=100, strike=100, rate=0.05, vol=0.25, time=1.0,
                kind="put", bermudan=dates, engine=Engine.TREE_LR,
            ).price
            assert again == first


def test_closed_pricer_raises_rather_than_segfaulting():
    p = Pricer()
    p.close()
    with pytest.raises(RuntimeError):
        p.price(spot=100, strike=100, vol=0.25, time=1.0)
