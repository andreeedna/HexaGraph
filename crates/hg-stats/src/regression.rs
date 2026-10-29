//! Linear regression and correlation.

use crate::descriptive::mean;
use crate::nonparam::rank;
use crate::{dist, need, Interval, Result, StatsError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearFit {
    pub n: usize,
    pub slope: f64,
    pub intercept: f64,
    pub se_slope: f64,
    pub se_intercept: f64,
    pub slope_ci95: Interval,
    pub intercept_ci95: Interval,
    pub x_intercept: f64,
    pub r_squared: f64,
    pub sy_x: f64,
    pub f: f64,
    pub df: f64,
    /// P value testing slope = 0.
    pub p: f64,
    mean_x: f64,
    sxx: f64,
}

impl LinearFit {
    pub fn predict(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }

    /// Half width of the confidence band of the line at `x` (`level` e.g. 0.95).
    pub fn confidence_half_width(&self, x: f64, level: f64) -> f64 {
        let t = dist::t_crit(level, self.df);
        t * self.sy_x * (1.0 / self.n as f64 + (x - self.mean_x).powi(2) / self.sxx).sqrt()
    }
}

fn check_pairs(x: &[f64], y: &[f64], min: usize) -> Result<()> {
    if x.len() != y.len() {
        return Err(StatsError::Invalid("X and Y must have the same length".into()));
    }
    need(x.len() >= min, &format!("at least {min} XY pairs are required"))
}

/// Least squares straight line.
pub fn linear(x: &[f64], y: &[f64]) -> Result<LinearFit> {
    check_pairs(x, y, 3)?;
    let n = x.len() as f64;
    let (mx, my) = (mean(x), mean(y));
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    need(sxx > 0.0, "X values must not all be identical")?;
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let syy: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let sse: f64 = x.iter().zip(y).map(|(a, b)| (b - intercept - slope * a).powi(2)).sum();
    let df = n - 2.0;
    let sy_x = (sse / df).sqrt();
    let se_slope = sy_x / sxx.sqrt();
    let se_intercept = sy_x * (1.0 / n + mx * mx / sxx).sqrt();
    let t = dist::t_crit(0.95, df);
    let f = (syy - sse) / (sse / df);
    Ok(LinearFit {
        n: x.len(),
        slope,
        intercept,
        se_slope,
        se_intercept,
        slope_ci95: Interval::new(slope - t * se_slope, slope + t * se_slope),
        intercept_ci95: Interval::new(intercept - t * se_intercept, intercept + t * se_intercept),
        x_intercept: -intercept / slope,
        r_squared: if syy > 0.0 { 1.0 - sse / syy } else { f64::NAN },
        sy_x,
        f,
        df,
        p: dist::t_p2(slope / se_slope, df),
        mean_x: mx,
        sxx,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correlation {
    pub n: usize,
    pub r: f64,
    pub r_squared: f64,
    pub p: f64,
    pub ci95: Option<Interval>,
}

pub fn pearson(x: &[f64], y: &[f64]) -> Result<Correlation> {
    check_pairs(x, y, 3)?;
    let r = pearson_r(x, y);
    let n = x.len() as f64;
    let df = n - 2.0;
    let t = r * (df / (1.0 - r * r)).sqrt();
    let ci95 = (x.len() > 3).then(|| {
        let z = r.atanh();
        let h = dist::norm_inv(0.975) / (n - 3.0).sqrt();
        Interval::new((z - h).tanh(), (z + h).tanh())
    });
    Ok(Correlation { n: x.len(), r, r_squared: r * r, p: if r.abs() >= 1.0 { 0.0 } else { dist::t_p2(t, df) }, ci95 })
}

fn pearson_r(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my) = (mean(x), mean(y));
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    let syy: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    sxy / (sxx * syy).sqrt()
}

/// Spearman rank correlation (t approximation for the P value, CI from
/// Fisher's z with the Bonett-Wright standard error).
pub fn spearman(x: &[f64], y: &[f64]) -> Result<Correlation> {
    check_pairs(x, y, 3)?;
    let (rx, _) = rank(x);
    let (ry, _) = rank(y);
    let r = pearson_r(&rx, &ry);
    let n = x.len() as f64;
    let df = n - 2.0;
    let t = r * (df / (1.0 - r * r)).sqrt();
    let ci95 = (x.len() > 3).then(|| {
        let z = r.atanh();
        let h = dist::norm_inv(0.975) * ((1.0 + r * r / 2.0) / (n - 3.0)).sqrt();
        Interval::new((z - h).tanh(), (z + h).tanh())
    });
    Ok(Correlation { n: x.len(), r, r_squared: r * r, p: if r.abs() >= 1.0 { 0.0 } else { dist::t_p2(t, df) }, ci95 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use crate::ttest::tests::{A, B};

    #[test]
    fn linear_matches_r() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let y = [2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1];
        let f = linear(&x, &y).unwrap();
        assert_close(f.slope, 1.99761904762, 1e-10);
        assert_close(f.intercept, 0.03571428571, 1e-9);
        assert_close(f.se_slope, 0.02780044427, 1e-9);
        assert_close(f.se_intercept, 0.14038536208, 1e-9);
        assert_close(f.r_squared, 0.9988392866, 1e-9);
        assert_close(f.sy_x, 0.1801674706, 1e-9);
        assert_close(f.intercept_ci95.low, -0.3077963205, 1e-8);
        assert_close(f.slope_ci95.high, 2.065644284, 1e-8);
        assert_close(f.p, 4.888933613e-10, 1e-6);
    }

    #[test]
    fn correlations_match_r() {
        let c = pearson(&A, &B).unwrap();
        assert_close(c.r, 0.3223740385, 1e-9);
        assert_close(c.p, 0.4361214241, 1e-8);
        let ci = c.ci95.unwrap();
        assert_close(ci.low, -0.4946728622, 1e-8);
        assert_close(ci.high, 0.8369243259, 1e-8);
        let s = spearman(&A, &B).unwrap();
        assert_close(s.r, 0.2619047619, 1e-9);
        assert_close(s.p, 0.5309228616, 1e-8);
    }
}
