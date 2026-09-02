// crates/optrs-fd/src/solver.rs
//! Thomas algorithm and PSOR. Tridiagonal systems are stored as three vectors;
//! the American case needs PSOR because projection alone breaks Crank-Nicolson.

/// Solves a·x[i-1] + b·x[i] + c·x[i+1] = d in place. `b` is consumed as scratch.
pub fn thomas(a: &[f64], b: &mut [f64], c: &[f64], d: &mut [f64], x: &mut [f64]) {
    let n = x.len();
    for i in 1..n {
        let m = a[i] / b[i - 1];
        b[i] -= m * c[i - 1];
        d[i] -= m * d[i - 1];
    }
    x[n - 1] = d[n - 1] / b[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = (d[i] - c[i] * x[i + 1]) / b[i];
    }
}

/// Projected SOR for the linear complementarity problem Ax >= d, x >= payoff.
pub fn psor(
    a: &[f64],
    b: &[f64],
    c: &[f64],
    d: &[f64],
    payoff: &[f64],
    x: &mut [f64],
    omega: f64,
    tol: f64,
    max_iter: usize,
) -> usize {
    let n = x.len();
    for iter in 0..max_iter {
        let mut err = 0.0_f64;
        for i in 0..n {
            let lo = if i > 0 { a[i] * x[i - 1] } else { 0.0 };
            let hi = if i + 1 < n { c[i] * x[i + 1] } else { 0.0 };
            let gs = (d[i] - lo - hi) / b[i];
            let next = (x[i] + omega * (gs - x[i])).max(payoff[i]);
            err = err.max((next - x[i]).abs());
            x[i] = next;
        }
        if err < tol {
            return iter + 1;
        }
    }
    max_iter
}
