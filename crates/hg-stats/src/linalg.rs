//! Tiny dense linear algebra helpers (the systems we solve are small).

pub type Matrix = Vec<Vec<f64>>;

/// Solves `a * x = b` with Gaussian elimination and partial pivoting.
pub fn solve(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut m: Matrix = a.to_vec();
    let mut v = b.to_vec();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() < 1e-300 {
            return None;
        }
        m.swap(col, pivot);
        v.swap(col, pivot);
        for row in col + 1..n {
            let f = m[row][col] / m[col][col];
            if f != 0.0 {
                for k in col..n {
                    m[row][k] -= f * m[col][k];
                }
                v[row] -= f * v[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| m[i][k] * x[k]).sum();
        x[i] = (v[i] - s) / m[i][i];
    }
    x.iter().all(|x| x.is_finite()).then_some(x)
}

/// Inverts a square matrix with Gauss-Jordan elimination.
pub fn invert(a: &Matrix) -> Option<Matrix> {
    let n = a.len();
    let mut m: Matrix = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            r
        })
        .collect();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() < 1e-300 {
            return None;
        }
        m.swap(col, pivot);
        let p = m[col][col];
        for k in 0..2 * n {
            m[col][k] /= p;
        }
        for row in 0..n {
            if row != col {
                let f = m[row][col];
                if f != 0.0 {
                    for k in 0..2 * n {
                        m[row][k] -= f * m[col][k];
                    }
                }
            }
        }
    }
    Some(m.into_iter().map(|r| r[n..].to_vec()).collect())
}

/// Ordinary least squares via the normal equations.
/// Returns the coefficients and the residual sum of squares.
pub fn least_squares(x: &[Vec<f64>], y: &[f64]) -> Option<(Vec<f64>, f64)> {
    let p = x.first()?.len();
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for (row, &yi) in x.iter().zip(y) {
        for i in 0..p {
            xty[i] += row[i] * yi;
            for j in 0..p {
                xtx[i][j] += row[i] * row[j];
            }
        }
    }
    let beta = solve(&xtx, &xty)?;
    let sse = x
        .iter()
        .zip(y)
        .map(|(row, &yi)| {
            let fit: f64 = row.iter().zip(&beta).map(|(a, b)| a * b).sum();
            (yi - fit).powi(2)
        })
        .sum();
    Some((beta, sse))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_and_invert() {
        let a = vec![vec![4.0, 1.0], vec![2.0, 3.0]];
        let x = solve(&a, &[1.0, 2.0]).unwrap();
        crate::assert_close(x[0], 0.1, 1e-12);
        crate::assert_close(x[1], 0.6, 1e-12);
        let inv = invert(&a).unwrap();
        crate::assert_close(inv[0][0], 0.3, 1e-12);
        crate::assert_close(inv[1][0], -0.2, 1e-12);
    }
}
