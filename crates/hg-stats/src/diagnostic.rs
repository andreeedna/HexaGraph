//! Method comparison and diagnostic accuracy: Bland-Altman and ROC curves.

use crate::descriptive::{mean, sd};
use crate::nonparam::mann_whitney;
use crate::{dist, need, Interval, Result, StatsError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlandAltman {
    /// (average, difference B - A) for each pair.
    pub points: Vec<(f64, f64)>,
    pub bias: f64,
    pub sd_difference: f64,
    pub limits_of_agreement: Interval,
}

pub fn bland_altman(a: &[f64], b: &[f64]) -> Result<BlandAltman> {
    if a.len() != b.len() {
        return Err(StatsError::Invalid("both methods need the same number of values".into()));
    }
    need(a.len() >= 2, "at least two pairs are required")?;
    let points: Vec<(f64, f64)> = a.iter().zip(b).map(|(x, y)| ((x + y) / 2.0, y - x)).collect();
    let d: Vec<f64> = points.iter().map(|p| p.1).collect();
    let (bias, s) = (mean(&d), sd(&d));
    Ok(BlandAltman { points, bias, sd_difference: s, limits_of_agreement: Interval::new(bias - 1.96 * s, bias + 1.96 * s) })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RocPoint {
    pub cutoff: f64,
    pub sensitivity: f64,
    pub specificity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Roc {
    pub points: Vec<RocPoint>,
    pub auc: f64,
    pub se: f64,
    pub ci95: Interval,
    pub p: f64,
}

/// ROC curve where larger values indicate disease (patients).
/// AUC SE by Hanley & McNeil (1982).
pub fn roc(controls: &[f64], patients: &[f64]) -> Result<Roc> {
    need(!controls.is_empty() && !patients.is_empty(), "both groups need values")?;
    let mut cuts: Vec<f64> = controls.iter().chain(patients).copied().collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let (nc, np) = (controls.len() as f64, patients.len() as f64);
    let mut points: Vec<RocPoint> = cuts
        .windows(2)
        .map(|w| {
            let c = (w[0] + w[1]) / 2.0;
            RocPoint {
                cutoff: c,
                sensitivity: patients.iter().filter(|&&v| v > c).count() as f64 / np,
                specificity: controls.iter().filter(|&&v| v < c).count() as f64 / nc,
            }
        })
        .collect();
    points.insert(0, RocPoint { cutoff: cuts[0] - 1.0, sensitivity: 1.0, specificity: 0.0 });
    points.push(RocPoint { cutoff: cuts[cuts.len() - 1] + 1.0, sensitivity: 0.0, specificity: 1.0 });
    let mw = mann_whitney(patients, controls)?;
    let a = mw.u / (nc * np);
    let q1 = a / (2.0 - a);
    let q2 = 2.0 * a * a / (1.0 + a);
    let se = ((a * (1.0 - a) + (np - 1.0) * (q1 - a * a) + (nc - 1.0) * (q2 - a * a)) / (nc * np)).sqrt();
    let z = dist::norm_inv(0.975);
    Ok(Roc {
        points,
        auc: a,
        se,
        ci95: Interval::new((a - z * se).max(0.0), (a + z * se).min(1.0)),
        // AUC = 0.5 is tested by Mann-Whitney: the Hanley-McNeil SE assumes the
        // alternative and is 0 when the groups do not overlap.
        p: mw.p,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn roc_perfect_and_bland_altman() {
        let r = roc(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap();
        assert_close(r.auc, 1.0, 1e-12);
        // R: wilcox.test(4:6, 1:3)$p.value
        assert_close(r.p, 0.1, 1e-12);
        let b = bland_altman(&[1.0, 2.0, 3.0], &[1.5, 2.5, 3.5]).unwrap();
        assert_close(b.bias, 0.5, 1e-12);
    }
}
