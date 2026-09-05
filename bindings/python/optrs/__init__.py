# bindings/python/optrs/__init__.py
"""Option pricing across four numerical methods.

    >>> import optrs
    >>> optrs.price(spot=60, strike=65, rate=0.08, vol=0.30, time=0.25)
    2.133...
"""

from __future__ import annotations

import weakref
from dataclasses import dataclass
from enum import IntEnum
from typing import Iterable, Sequence

from ._ffi import ffi, lib

__all__ = [
    "Engine",
    "Kind",
    "Style",
    "Greeks",
    "Quote",
    "OptrsError",
    "Pricer",
    "price",
    "greeks",
    "compare",
    "implied_vol",
    "version",
]


def version() -> str:
    return ffi.string(lib.opt_version()).decode()


class Engine(IntEnum):
    ANALYTIC = 0
    COS = 1
    TREE_CRR = 2
    TREE_LR = 3
    FINITE_DIFFERENCE = 4
    MONTE_CARLO = 5

    @property
    def label(self) -> str:
        return ffi.string(lib.opt_engine_name(int(self))).decode()


class Kind(IntEnum):
    CALL = 0
    PUT = 1


class Style(IntEnum):
    EUROPEAN = 0
    AMERICAN = 1
    BERMUDAN = 2


class OptrsError(RuntimeError):
    """Raised when the native library returns a non-zero status."""

    def __init__(self, status: int, message: str) -> None:
        super().__init__(message or f"optrs error {status}")
        self.status = status


def _check(status: int) -> None:
    if status == lib.OPT_STATUS_OK:
        return
    raw = lib.opt_last_error_message()
    message = ffi.string(raw).decode() if raw != ffi.NULL else ""
    raise OptrsError(int(status), message)


@dataclass(frozen=True)
class Greeks:
    price: float
    delta: float
    gamma: float
    vega: float
    theta: float
    rho: float


@dataclass(frozen=True)
class Quote:
    """A single engine's answer. `std_error` is None for deterministic engines."""

    engine: Engine
    price: float
    std_error: float | None = None
    refinements: int = 0
    extrapolated: bool = False


def _coerce_kind(kind: Kind | str) -> int:
    if isinstance(kind, str):
        try:
            return int(Kind[kind.strip().upper()])
        except KeyError:
            raise ValueError(f"unknown option kind {kind!r}; use 'call' or 'put'") from None
    return int(kind)


def _coerce_engine(engine: Engine | str) -> int:
    if isinstance(engine, str):
        key = engine.strip().upper().replace("-", "_")
        aliases = {"FD": "FINITE_DIFFERENCE", "MC": "MONTE_CARLO"}
        key = aliases.get(key, key)
        try:
            return int(Engine[key])
        except KeyError:
            raise ValueError(f"unknown engine {engine!r}") from None
    return int(engine)


def _build_option(
    *,
    spot: float,
    strike: float,
    vol: float,
    time: float,
    rate: float,
    div_yield: float,
    kind: Kind | str,
    american: bool,
    bermudan: Sequence[float] | None,
):
    """Returns (option, keepalive). The keepalive holds the dates buffer alive
    for as long as the option struct points at it."""
    if american and bermudan:
        raise ValueError("specify either american=True or bermudan=[...], not both")

    option = ffi.new("opt_option_t *")
    _check(lib.opt_option_init(option))

    option.kind = _coerce_kind(kind)
    option.spot = float(spot)
    option.strike = float(strike)
    option.rate = float(rate)
    option.div_yield = float(div_yield)
    option.vol = float(vol)
    option.time = float(time)

    keepalive = None
    if bermudan:
        dates = [float(d) for d in bermudan]
        keepalive = ffi.new("double[]", dates)
        option.style = int(Style.BERMUDAN)
        option.dates = keepalive
        option.n_dates = len(dates)
    elif american:
        option.style = int(Style.AMERICAN)
    else:
        option.style = int(Style.EUROPEAN)

    return option, keepalive


class Pricer:
    """Holds engine configuration. Reusable across many valuations.

    Not thread-safe: the native error channel is per-thread, but a single
    handle's configuration is not guarded. Use one Pricer per thread.
    """

    def __init__(self, **settings) -> None:
        handle = lib.opt_pricer_new()
        if handle == ffi.NULL:
            raise MemoryError("opt_pricer_new returned NULL")
        self._handle = handle
        self._finalizer = weakref.finalize(self, lib.opt_pricer_free, handle)
        if settings:
            self.configure(**settings)

    def close(self) -> None:
        self._finalizer()

    def __enter__(self) -> "Pricer":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    @property
    def _ptr(self):
        if not self._finalizer.alive:
            raise RuntimeError("Pricer has been closed")
        return self._handle

    _SETTERS = {
        "tree_steps": (lib.opt_set_tree_steps, int),
        "fd_space_steps": (lib.opt_set_fd_space_steps, int),
        "fd_time_steps": (lib.opt_set_fd_time_steps, int),
        "fd_width": (lib.opt_set_fd_width, float),
        "mc_paths": (lib.opt_set_mc_paths, int),
        "mc_steps": (lib.opt_set_mc_steps, int),
        "mc_seed": (lib.opt_set_mc_seed, int),
        "cos_terms": (lib.opt_set_cos_terms, int),
        "tolerance": (lib.opt_set_tolerance, float),
        "max_refinements": (lib.opt_set_max_refinements, int),
        "spot_bump": (lib.opt_set_spot_bump, float),
        "vol_bump": (lib.opt_set_vol_bump, float),
        "mc_antithetic": (lib.opt_set_mc_antithetic, lambda v: int(bool(v))),
        "mc_control_variate": (lib.opt_set_mc_control_variate, lambda v: int(bool(v))),
        "richardson": (lib.opt_set_richardson, lambda v: int(bool(v))),
    }

    def configure(self, **settings) -> "Pricer":
        for name, value in settings.items():
            try:
                setter, cast = self._SETTERS[name]
            except KeyError:
                known = ", ".join(sorted(self._SETTERS))
                raise ValueError(f"unknown setting {name!r}; known: {known}") from None
            _check(setter(self._ptr, cast(value)))
        return self

    def reset(self) -> "Pricer":
        _check(lib.opt_pricer_reset(self._ptr))
        return self

    def price(
        self,
        *,
        spot: float,
        strike: float,
        vol: float,
        time: float,
        rate: float = 0.0,
        div_yield: float = 0.0,
        kind: Kind | str = Kind.CALL,
        american: bool = False,
        bermudan: Sequence[float] | None = None,
        engine: Engine | str | None = None,
        converged: bool = False,
    ) -> Quote:
        option, _keep = _build_option(
            spot=spot, strike=strike, vol=vol, time=time, rate=rate,
            div_yield=div_yield, kind=kind, american=american, bermudan=bermudan,
        )
        out = ffi.new("opt_result_t *")
        _check(lib.opt_result_init(out))

        if engine is None:
            if converged:
                raise ValueError("converged=True requires an explicit engine")
            _check(lib.opt_price_auto(self._ptr, option, out))
        else:
            eng = _coerce_engine(engine)
            fn = lib.opt_price_converged if converged else lib.opt_price
            _check(fn(self._ptr, eng, option, out))

        return Quote(
            engine=Engine(out.engine),
            price=out.price,
            std_error=out.std_error if out.has_std_error else None,
            refinements=int(out.refinements),
            extrapolated=bool(out.extrapolated),
        )

    def greeks(
        self,
        *,
        engine: Engine | str = Engine.TREE_LR,
        **option_kwargs,
    ) -> Greeks:
        option, _keep = _build_option(**_option_defaults(option_kwargs))
        out = ffi.new("opt_result_t *")
        _check(lib.opt_result_init(out))
        _check(lib.opt_greeks(self._ptr, _coerce_engine(engine), option, out))
        return Greeks(
            price=out.price, delta=out.delta, gamma=out.gamma,
            vega=out.vega, theta=out.theta, rho=out.rho,
        )

    def compare(self, **option_kwargs) -> list[Quote]:
        """Price under every engine supporting the option.

        Engines that failed are omitted rather than raising, so one
        misconfigured engine does not lose the others.
        """
        option, _keep = _build_option(**_option_defaults(option_kwargs))
        capacity = lib.opt_engine_count()
        results = ffi.new("opt_result_t[]", capacity)
        for i in range(capacity):
            _check(lib.opt_result_init(ffi.addressof(results, i)))

        written = ffi.new("size_t *")
        _check(lib.opt_price_all(self._ptr, option, results, capacity, written))

        quotes = []
        for i in range(written[0]):
            r = results[i]
            if r.engine < 0:  # engine failed; see opt_price_all docs
                continue
            quotes.append(
                Quote(
                    engine=Engine(r.engine),
                    price=r.price,
                    std_error=r.std_error if r.has_std_error else None,
                )
            )
        return quotes

    def supports(self, engine: Engine | str, **option_kwargs) -> bool:
        option, _keep = _build_option(**_option_defaults(option_kwargs))
        out = ffi.new("int32_t *")
        _check(lib.opt_engine_supports(_coerce_engine(engine), option, out))
        return bool(out[0])


def _option_defaults(kwargs: dict) -> dict:
    defaults = {
        "rate": 0.0, "div_yield": 0.0, "kind": Kind.CALL,
        "american": False, "bermudan": None,
    }
    required = {"spot", "strike", "vol", "time"}
    missing = required - kwargs.keys()
    if missing:
        raise TypeError(f"missing required arguments: {', '.join(sorted(missing))}")
    return {**defaults, **kwargs}


# Module-level convenience wrappers. Each creates a short-lived Pricer, so for
# tight loops build one Pricer and call its methods instead.

def price(**kwargs) -> float:
    with Pricer() as p:
        return p.price(**kwargs).price


def greeks(**kwargs) -> Greeks:
    with Pricer() as p:
        return p.greeks(**kwargs)


def compare(**kwargs) -> list[Quote]:
    with Pricer() as p:
        return p.compare(**kwargs)


def implied_vol(
    target_price: float,
    *,
    spot: float,
    strike: float,
    time: float,
    rate: float = 0.0,
    div_yield: float = 0.0,
    kind: Kind | str = Kind.CALL,
) -> float:
    option, _keep = _build_option(
        spot=spot, strike=strike, vol=0.2, time=time, rate=rate,
        div_yield=div_yield, kind=kind, american=False, bermudan=None,
    )
    out = ffi.new("double *")
    _check(lib.opt_implied_vol(option, float(target_price), out))
    return out[0]
