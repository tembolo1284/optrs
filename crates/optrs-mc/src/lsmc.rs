// crates/optrs-mc/src/lsmc.rs
//! Longstaff-Schwartz. Regression is on in-the-money paths only, against a
//! Laguerre basis in moneyness; normal equations solved by Cholesky with a
//! ridge fallback for degenerate exercise dates.

pub fn laguerre_basis(s_over_k: f64, out: &mut [f64]) {
    let x = s_over_k;
    let e = (-x / 2.0).exp();
    out[0] = e;
    if out.len() > 1 { out[1] = e * (1.0 - x); }
    if out.len() > 2 { out[2] = e * (1.0 - 2.0 * x + x * x / 2.0); }
    if out.len() > 3 { out[3] = e * (1.0 - 3.0 * x + 1.5 * x * x - x * x * x / 6.0); }
}

/// Solves (X'X + ridge·I) beta = X'y for small symmetric positive systems.
pub fn cholesky_solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize, ridge: f64) -> Option<Vec<f64>> {
    for i in 0..n {
        a[i * n + i] += ridge;
    }
    for i in 0..n {
        for j in 0..=i {
            let mut sum = a[i * n + j];
            for k in 0..j {
                sum -= a[i * n + k] * a[j * n + k];
            }
            if i == j {
                if sum <= 1e-14 {
                    return None;
                }
                a[i * n + i] = sum.sqrt();
            } else {
                a[i * n + j] = sum / a[j * n + j];
            }
        }
    }
    for i in 0..n {
        let mut sum = b[i];
        for k in 0..i {
            sum -= a[i * n + k] * b[k];
        }
        b[i] = sum / a[i * n + i];
    }
    for i in (0..n).rev() {
        let mut sum = b[i];
        for k in i + 1..n {
            sum -= a[k * n + i] * b[k];
        }
        b[i] = sum / a[i * n + i];
    }
    Some(b)
}
