//! Descriptive statistics ("column statistics").

use crate::{dist, need, Interval, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Descriptive {
    pub n: usize,
    pub sum: f64,
    pub mean: f64,
    pub sd: f64,
    pub sem: f64,
    pub variance: f64,
    pub cv_percent: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub range: f64,
    /// 95% confidence interval of the mean.
    pub mean_ci95: Option<Interval>,
    /// Only defined when every value is positive.
    pub geometric_mean: Option<f64>,
    pub geometric_sd_factor: Option<f64>,
    pub harmonic_mean: Option<f64>,
    pub skewness: Option<f64>,
    pub kurtosis: Option<f64>,
}

pub fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

/// Sample variance (n - 1 denominator).
pub fn variance(x: &[f64]) -> f64 {
    let m = mean(x);
    x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (x.len() as f64 - 1.0)
}

pub fn sd(x: &[f64]) -> f64 {
    variance(x).sqrt()
}

pub fn sem(x: &[f64]) -> f64 {
    sd(x) / (x.len() as f64).sqrt()
}

pub fn sorted(x: &[f64]) -> Vec<f64> {
    let mut v = x.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

pub fn median(x: &[f64]) -> f64 {
    percentile_sorted(&sorted(x), 0.5)
}

/// Percentile of sorted data using the (n + 1)p rank definition
/// (Hyndman & Fan type 6), clamped to the data range.
pub fn percentile_sorted(s: &[f64], p: f64) -> f64 {
    let n = s.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = ((n as f64 + 1.0) * p).clamp(1.0, n as f64);
    let lo = h.floor() as usize;
    let frac = h - lo as f64;
    if lo >= n {
        s[n - 1]
    } else {
        s[lo - 1] + frac * (s[lo] - s[lo - 1])
    }
}

pub fn describe(x: &[f64]) -> Result<Descriptive> {
    need(!x.is_empty(), "at least one value is required")?;
    let n = x.len();
    let nf = n as f64;
    let s = sorted(x);
    let sum: f64 = x.iter().sum();
    let mean = sum / nf;
    let (variance, sd, sem) = if n > 1 {
        let v = variance(x);
        (v, v.sqrt(), v.sqrt() / nf.sqrt())
    } else {
        (f64::NAN, f64::NAN, f64::NAN)
    };
    let mean_ci95 = (n > 1).then(|| {
        let t = dist::t_crit(0.95, nf - 1.0);
        Interval::new(mean - t * sem, mean + t * sem)
    });
    let all_positive = x.iter().all(|&v| v > 0.0);
    let (geometric_mean, geometric_sd_factor, harmonic_mean) = if all_positive {
        let logs: Vec<f64> = x.iter().map(|v| v.ln()).collect();
        let gsd = (n > 1).then(|| sd_of(&logs).exp());
        (Some(mean_of(&logs).exp()), gsd, Some(nf / x.iter().map(|v| 1.0 / v).sum::<f64>()))
    } else {
        (None, None, None)
    };
    let skewness = (n > 2 && sd > 0.0).then(|| {
        let s3: f64 = x.iter().map(|v| ((v - mean) / sd).powi(3)).sum();
        nf / ((nf - 1.0) * (nf - 2.0)) * s3
    });
    let kurtosis = (n > 3 && sd > 0.0).then(|| {
        let s4: f64 = x.iter().map(|v| ((v - mean) / sd).powi(4)).sum();
        nf * (nf + 1.0) / ((nf - 1.0) * (nf - 2.0) * (nf - 3.0)) * s4 - 3.0 * (nf - 1.0).powi(2) / ((nf - 2.0) * (nf - 3.0))
    });
    Ok(Descriptive {
        n,
        sum,
        mean,
        sd,
        sem,
        variance,
        cv_percent: 100.0 * sd / mean.abs(),
        min: s[0],
        q1: percentile_sorted(&s, 0.25),
        median: percentile_sorted(&s, 0.5),
        q3: percentile_sorted(&s, 0.75),
        max: s[n - 1],
        range: s[n - 1] - s[0],
        mean_ci95,
        geometric_mean,
        geometric_sd_factor,
        harmonic_mean,
        skewness,
        kurtosis,
    })
}

fn mean_of(x: &[f64]) -> f64 {
    mean(x)
}

fn sd_of(x: &[f64]) -> f64 {
    sd(x)
}

/// Area under the curve by the trapezoid rule, measured from `baseline`.
/// Points must be sorted by X.
pub fn area_under_curve(x: &[f64], y: &[f64], baseline: f64) -> f64 {
    x.windows(2).zip(y.windows(2)).map(|(xs, ys)| (xs[1] - xs[0]) * ((ys[0] - baseline) + (ys[1] - baseline)) / 2.0).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    pub(crate) const A: [f64; 8] = [23.1, 25.4, 21.8, 27.2, 24.5, 26.1, 22.9, 25.0];

    #[test]
    fn column_statistics_match_r() {
        let d = describe(&A).unwrap();
        assert_close(d.sd, 1.801586602, 1e-9);
        assert_close(d.q1, 22.95, 1e-12);
        assert_close(d.median, 24.75, 1e-12);
        assert_close(d.q3, 25.925, 1e-12);
        assert_close(d.skewness.unwrap(), -0.0615654517, 1e-8);
        assert_close(d.kurtosis.unwrap(), -0.8657874182, 1e-8);
    }

    #[test]
    fn auc_trapezoid() {
        assert_close(area_under_curve(&[0.0, 1.0, 2.0], &[0.0, 2.0, 0.0], 0.0), 2.0, 1e-12);
    }
}
