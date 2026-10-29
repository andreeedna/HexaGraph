//! t tests. Differences are reported as mean(B) - mean(A).

use crate::descriptive::{mean, variance};
use crate::{dist, need, Interval, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TTest {
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub mean_a: f64,
    pub mean_b: f64,
    /// mean(B) - mean(A) (or mean - hypothetical value for one sample tests).
    pub difference: f64,
    pub se_difference: f64,
    pub ci95: Interval,
    /// Effect size eta squared: t² / (t² + df).
    pub r_squared: f64,
    /// F test comparing variances (unpaired tests only).
    pub f_test: Option<FTest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FTest {
    pub f: f64,
    pub df_num: f64,
    pub df_den: f64,
    pub p: f64,
}

fn finish(t: f64, df: f64, mean_a: f64, mean_b: f64, diff: f64, se: f64, f_test: Option<FTest>) -> TTest {
    let tc = dist::t_crit(0.95, df);
    TTest {
        t,
        df,
        p: dist::t_p2(t, df),
        mean_a,
        mean_b,
        difference: diff,
        se_difference: se,
        ci95: Interval::new(diff - tc * se, diff + tc * se),
        r_squared: t * t / (t * t + df),
        f_test,
    }
}

/// Unpaired t test. With `welch = true` variances are not assumed equal.
pub fn unpaired(a: &[f64], b: &[f64], welch: bool) -> Result<TTest> {
    need(a.len() >= 2 && b.len() >= 2, "each group needs at least two values")?;
    let (na, nb) = (a.len() as f64, b.len() as f64);
    let (ma, mb) = (mean(a), mean(b));
    let (va, vb) = (variance(a), variance(b));
    let diff = mb - ma;
    let (se, df) = if welch {
        let (qa, qb) = (va / na, vb / nb);
        let se = (qa + qb).sqrt();
        let df = (qa + qb).powi(2) / (qa * qa / (na - 1.0) + qb * qb / (nb - 1.0));
        (se, df)
    } else {
        let pooled = ((na - 1.0) * va + (nb - 1.0) * vb) / (na + nb - 2.0);
        ((pooled * (1.0 / na + 1.0 / nb)).sqrt(), na + nb - 2.0)
    };
    let f_test = variance_f_test(a, b);
    Ok(finish(diff / se, df, ma, mb, diff, se, Some(f_test)))
}

/// F test for equal variances, two tailed. F = var(A) / var(B) (as in R's var.test).
pub fn variance_f_test(a: &[f64], b: &[f64]) -> FTest {
    let (va, vb) = (variance(a), variance(b));
    let (d1, d2) = (a.len() as f64 - 1.0, b.len() as f64 - 1.0);
    let f = va / vb;
    let upper = dist::f_sf(f, d1, d2);
    FTest { f, df_num: d1, df_den: d2, p: (2.0 * upper.min(1.0 - upper)).clamp(0.0, 1.0) }
}

/// Paired t test on B - A.
pub fn paired(a: &[f64], b: &[f64]) -> Result<TTest> {
    if a.len() != b.len() {
        return Err(crate::StatsError::Invalid("paired data must have the same number of values".into()));
    }
    need(a.len() >= 2, "at least two pairs are required")?;
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    let n = d.len() as f64;
    let md = mean(&d);
    let se = (variance(&d) / n).sqrt();
    Ok(finish(md / se, n - 1.0, mean(a), mean(b), md, se, None))
}

/// One sample t test against a hypothetical mean.
pub fn one_sample(a: &[f64], hypothetical: f64) -> Result<TTest> {
    need(a.len() >= 2, "at least two values are required")?;
    let n = a.len() as f64;
    let m = mean(a);
    let se = (variance(a) / n).sqrt();
    Ok(finish((m - hypothetical) / se, n - 1.0, hypothetical, m, m - hypothetical, se, None))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::assert_close;

    pub const A: [f64; 8] = [23.1, 25.4, 21.8, 27.2, 24.5, 26.1, 22.9, 25.0];
    pub const B: [f64; 8] = [28.4, 30.1, 27.5, 31.6, 29.2, 26.8, 30.9, 29.7];
    pub const C: [f64; 6] = [24.0, 26.5, 25.1, 27.9, 23.8, 26.2];

    // R reports A - B; we report B - A, so signs flip.
    #[test]
    fn student() {
        let r = unpaired(&A, &B, false).unwrap();
        assert_close(r.t, 5.535152429, 1e-8);
        assert_close(r.df, 14.0, 1e-12);
        assert_close(r.p, 7.346987618e-05, 1e-6);
        assert_close(r.ci95.low, 2.924760722, 1e-8);
        assert_close(r.ci95.high, 6.625239278, 1e-8);
        let f = r.f_test.unwrap();
        assert_close(f.f, 1.19862833, 1e-8);
        assert_close(f.p, 0.8172040835, 1e-7);
    }

    #[test]
    fn welch() {
        let r = unpaired(&A, &B, true).unwrap();
        assert_close(r.df, 13.88666172, 1e-8);
        assert_close(r.p, 7.567668123e-05, 1e-6);
        assert_close(r.ci95.high, 6.626657006, 1e-8);
    }

    #[test]
    fn paired_and_one_sample() {
        let r = paired(&A, &B).unwrap();
        assert_close(r.t, 6.717575253, 1e-8);
        assert_close(r.p, 0.0002729968023, 1e-6);
        assert_close(r.ci95.low, 3.094173158, 1e-8);
        let o = one_sample(&A, 25.0).unwrap();
        assert_close(o.t, -0.784982282, 1e-8);
        assert_close(o.p, 0.4582028926, 1e-7);
    }
}
