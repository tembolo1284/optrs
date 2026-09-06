#!/usr/bin/env python3
# bindings/python/examples/duffy.py
"""Duffy's QuantNet test batches, priced through every engine.

Published reference values come from the course text. All four batches have
carry b = r, so the dividend yield is zero throughout.

Exit code is non-zero if any engine falls outside its tolerance, which makes
this usable as a smoke test as well as a demonstration.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass

import optrs
from optrs import Engine, Pricer


@dataclass(frozen=True)
class Batch:
    name: str
    spot: float
    strike: float
    rate: float
    vol: float
    time: float
    call: float
    put: float

    @property
    def args(self) -> dict:
        return dict(
            spot=self.spot, strike=self.strike, rate=self.rate,
            vol=self.vol, time=self.time,
        )


BATCHES = [
    Batch("batch 1", 60.0, 65.0, 0.08, 0.30, 0.25, 2.13337, 5.84628),
    Batch("batch 2", 100.0, 100.0, 0.00, 0.20, 1.00, 7.96557, 7.96557),
    Batch("batch 3", 5.0, 10.0, 0.12, 0.50, 1.00, 0.204058, 4.07326),
    Batch("batch 4", 100.0, 100.0, 0.08, 0.30, 30.00, 92.17570, 1.24750),
]


def fd_steps_for(batch: Batch) -> int:
    """Grid resolution scaled to the maturity.

    The finite-difference domain half-width is `width * sig * sqrt(T)`, so with
    a fixed step count dx grows as sqrt(T). Crank-Nicolson error is O(dx^2),
    which is why batch 4 (T=30) is roughly 30x less accurate than the one-year
    batches at the same 512 steps. Scaling the count by sqrt(T) holds dx
    roughly constant across the set.
    """
    return max(512, int(512 * batch.time**0.5))


def tolerance_for(engine: Engine, batch: Batch) -> float:
    """Per-engine tolerance against the closed form.

    FD and the CRR tree degrade with maturity — the lattice because its error
    is O(1/N) with N fixed, the PDE because dx grows as sqrt(T) even after the
    step scaling above. Scaling the tolerance keeps the check meaningful at
    T=30 without hiding a genuine regression at T=0.25. Monte Carlo is absent:
    it is judged against its own standard error instead.
    """
    scale = max(1.0, batch.time**0.5)
    return {
        Engine.ANALYTIC: 1e-12,
        Engine.COS: 1e-9,
        Engine.TREE_CRR: 5e-2 * scale,
        Engine.TREE_LR: 5e-3 * scale,
        Engine.FINITE_DIFFERENCE: 5e-3 * scale,
    }.get(engine, 1e-2)


GREEN, RED, DIM, RESET = "\033[32m", "\033[31m", "\033[2m", "\033[0m"


def main() -> int:
    print(f"optrs {optrs.version()}\n")
    failures = 0

    with Pricer(tree_steps=1001, mc_paths=200_000, mc_seed=7) as p:
        for batch in BATCHES:
            steps = fd_steps_for(batch)
            p.configure(fd_space_steps=steps, fd_time_steps=steps)

            print(f"{batch.name}: S={batch.spot} K={batch.strike} r={batch.rate} "
                  f"sig={batch.vol} T={batch.time}  {DIM}(fd grid {steps}){RESET}")

            for kind, published in (("call", batch.call), ("put", batch.put)):
                truth = p.price(kind=kind, engine=Engine.ANALYTIC, **batch.args).price
                delta = abs(truth - published)
                mark = f"{GREEN}ok{RESET}" if delta < 1e-4 else f"{RED}MISMATCH{RESET}"
                if delta >= 1e-4:
                    failures += 1
                print(f"  {kind:<4} published {published:>12.6f}   "
                      f"closed form {truth:>12.6f}   {mark}")

                for quote in p.compare(kind=kind, **batch.args):
                    if quote.engine is Engine.ANALYTIC:
                        continue
                    diff = abs(quote.price - truth)
                    if quote.std_error is not None:
                        # Stochastic: judge against four standard errors.
                        limit = 4 * quote.std_error
                        note = f"se {quote.std_error:.2e}"
                    else:
                        limit = tolerance_for(quote.engine, batch)
                        note = ""
                    ok = diff < limit
                    if not ok:
                        failures += 1
                    mark = f"{GREEN}ok{RESET}" if ok else f"{RED}FAIL{RESET}"
                    print(f"       {DIM}{quote.engine.label:<18}{RESET} "
                          f"{quote.price:>12.6f}   diff {diff:>8.2e}   {mark} {note}")
            print()

        # Greeks on batch 1, closed form against the bumped finite-difference
        # engine — the same comparison the Rust facade test makes.
        b = BATCHES[0]
        steps = fd_steps_for(b)
        p.configure(fd_space_steps=steps, fd_time_steps=steps)
        print(f"greeks, {b.name} call")
        exact = p.greeks(engine=Engine.ANALYTIC, **b.args)
        bumped = p.greeks(engine=Engine.FINITE_DIFFERENCE, **b.args)
        print(f"  {'':<8}{'closed form':>14}{'bumped fd':>14}{'diff':>12}")
        for field in ("price", "delta", "gamma", "vega", "theta", "rho"):
            a, c = getattr(exact, field), getattr(bumped, field)
            print(f"  {field:<8}{a:>14.6f}{c:>14.6f}{abs(a - c):>12.2e}")
        print()

        # Implied vol round trip: recover the input vol from the price.
        print("implied vol round trip")
        for b in BATCHES:
            target = p.price(kind="call", engine=Engine.ANALYTIC, **b.args).price
            args = {k: v for k, v in b.args.items() if k != "vol"}
            recovered = optrs.implied_vol(target, kind="call", **args)
            diff = abs(recovered - b.vol)
            ok = diff < 1e-8
            if not ok:
                failures += 1
            mark = f"{GREEN}ok{RESET}" if ok else f"{RED}FAIL{RESET}"
            print(f"  {b.name}: input {b.vol:.4f}  recovered {recovered:.10f}  "
                  f"diff {diff:.2e}  {mark}")

    print()
    if failures:
        print(f"{RED}{failures} check(s) failed{RESET}")
        return 1
    print(f"{GREEN}all checks passed{RESET}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
