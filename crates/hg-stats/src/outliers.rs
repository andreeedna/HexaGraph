//! Outlier identification.

use crate::descriptive::{mean, sd};
use crate::{dist, need, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grubbs {
    /// Indices (into the input) of the values flagged as outliers.
    pub outliers: Vec<usize>,
    pub cleaned: Vec<f64>,
}

/// Iterative two-sided Grubbs test at significance `alpha`.
pub fn grubbs(values: &[f64], alpha: f64) -> Result<Grubbs> {
    need(values.len() >= 3, "Grubbs' test needs at least 3 values")?;
    let mut idx: Vec<usize> = (0..values.len()).collect();
    let mut outliers = Vec::new();
    while idx.len() >= 3 {
        let v: Vec<f64> = idx.iter().map(|&i| values[i]).collect();
        let (m, s) = (mean(&v), sd(&v));
        if s == 0.0 {
            break;
        }
        let (pos, g) = v.iter().enumerate().map(|(k, x)| (k, (x - m).abs() / s)).max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        let n = v.len() as f64;
        let t = dist::t_inv(1.0 - alpha / (2.0 * n), n - 2.0);
        let crit = (n - 1.0) / n.sqrt() * (t * t / (n - 2.0 + t * t)).sqrt();
        if g > crit {
            outliers.push(idx.remove(pos));
        } else {
            break;
        }
    }
    Ok(Grubbs { cleaned: idx.iter().map(|&i| values[i]).collect(), outliers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grubbs_finds_obvious_outlier() {
        let g = grubbs(&[10.1, 9.8, 10.3, 10.0, 9.9, 10.2, 25.0], 0.05).unwrap();
        assert_eq!(g.outliers, vec![6]);
    }
}
