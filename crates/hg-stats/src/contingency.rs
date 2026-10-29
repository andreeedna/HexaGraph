//! Contingency tables.

use crate::{dist, need, Interval, Result, StatsError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChiSquare {
    pub chi2: f64,
    pub df: f64,
    pub p: f64,
    pub expected: Vec<Vec<f64>>,
    pub yates: bool,
}

/// Pearson chi-square test of independence. Yates' continuity correction is
/// applied to 2x2 tables when `yates` is true.
pub fn chi_square(table: &[Vec<f64>], yates: bool) -> Result<ChiSquare> {
    need(table.len() >= 2 && table[0].len() >= 2, "at least a 2x2 table is required")?;
    let c = table[0].len();
    if table.iter().any(|r| r.len() != c) {
        return Err(StatsError::Invalid("all rows need the same number of columns".into()));
    }
    let rows: Vec<f64> = table.iter().map(|r| r.iter().sum()).collect();
    let cols: Vec<f64> = (0..c).map(|j| table.iter().map(|r| r[j]).sum()).collect();
    let n: f64 = rows.iter().sum();
    need(n > 0.0, "the table is empty")?;
    let yates = yates && table.len() == 2 && c == 2;
    let expected: Vec<Vec<f64>> = rows.iter().map(|ri| cols.iter().map(|cj| ri * cj / n).collect()).collect();
    let mut chi2 = 0.0;
    for (orow, erow) in table.iter().zip(&expected) {
        for (o, e) in orow.iter().zip(erow) {
            let d = if yates { ((o - e).abs() - 0.5).max(0.0) } else { (o - e).abs() };
            chi2 += d * d / e;
        }
    }
    let df = ((table.len() - 1) * (c - 1)) as f64;
    Ok(ChiSquare { chi2, df, p: dist::chi2_sf(chi2, df), expected, yates })
}

fn ln_choose(n: f64, k: f64) -> f64 {
    dist::ln_gamma(n + 1.0) - dist::ln_gamma(k + 1.0) - dist::ln_gamma(n - k + 1.0)
}

/// Two sided Fisher's exact test for a 2x2 table [[a, b], [c, d]].
pub fn fisher_exact(a: u64, b: u64, c: u64, d: u64) -> f64 {
    let (r1, c1, n) = ((a + b) as f64, (a + c) as f64, (a + b + c + d) as f64);
    // Cell `a` ranges over max(0, r1 + c1 - n) ..= min(r1, c1).
    let lo = a.saturating_sub(d);
    let hi = (a + b).min(a + c);
    let prob = |x: f64| (ln_choose(c1, x) + ln_choose(n - c1, r1 - x) - ln_choose(n, r1)).exp();
    let observed = prob(a as f64);
    (lo..=hi).map(|x| prob(x as f64)).filter(|&p| p <= observed * (1.0 + 1e-7)).sum::<f64>().min(1.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoByTwo {
    pub relative_risk: Option<(f64, Interval)>,
    pub odds_ratio: Option<(f64, Interval)>,
    /// Difference in proportions a/(a+b) - c/(c+d).
    pub attributable_risk: (f64, Interval),
    pub fisher_p: f64,
}

/// Effect sizes for a 2x2 table (Katz log RR CI, Woolf logit OR CI, Wald
/// difference) with confidence intervals at `level` (e.g. 0.95).
pub fn two_by_two(a: u64, b: u64, c: u64, d: u64, level: f64) -> Result<TwoByTwo> {
    need(a + b > 0 && c + d > 0, "both rows need observations")?;
    let (af, bf, cf, df) = (a as f64, b as f64, c as f64, d as f64);
    let z = dist::norm_inv(0.5 + level / 2.0);
    let p1 = af / (af + bf);
    let p2 = cf / (cf + df);
    let relative_risk = (a > 0 && c > 0).then(|| {
        let rr = p1 / p2;
        let se = (bf / (af * (af + bf)) + df / (cf * (cf + df))).sqrt();
        (rr, Interval::new((rr.ln() - z * se).exp(), (rr.ln() + z * se).exp()))
    });
    let odds_ratio = (a > 0 && b > 0 && c > 0 && d > 0).then(|| {
        let or = af * df / (bf * cf);
        let se = (1.0 / af + 1.0 / bf + 1.0 / cf + 1.0 / df).sqrt();
        (or, Interval::new((or.ln() - z * se).exp(), (or.ln() + z * se).exp()))
    });
    let diff = p1 - p2;
    let se = (p1 * (1.0 - p1) / (af + bf) + p2 * (1.0 - p2) / (cf + df)).sqrt();
    Ok(TwoByTwo {
        relative_risk,
        odds_ratio,
        attributable_risk: (diff, Interval::new(diff - z * se, diff + z * se)),
        fisher_p: fisher_exact(a, b, c, d),
    })
}

/// Chi-square test for trend (Cochran-Armitage) across ordered rows of a
/// table with two outcome columns.
pub fn chi_square_trend(table: &[Vec<f64>]) -> Result<(f64, f64)> {
    need(table.len() >= 3 && table.iter().all(|r| r.len() == 2), "need at least three rows and two columns")?;
    let n: f64 = table.iter().flatten().sum();
    let r1: f64 = table.iter().map(|r| r[0]).sum();
    let p = r1 / n;
    let (mut t, mut s1, mut s2) = (0.0, 0.0, 0.0);
    for (i, r) in table.iter().enumerate() {
        let score = i as f64 + 1.0;
        let ni = r[0] + r[1];
        t += score * (r[0] - ni * p);
        s1 += ni * score * score;
        s2 += ni * score;
    }
    let var = p * (1.0 - p) * (s1 - s2 * s2 / n);
    let chi2 = t * t / var;
    Ok((chi2, dist::chi2_sf(chi2, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn chi_square_matches_r() {
        // R matrix(c(12,5,7,9), 2) is [[12, 7], [5, 9]]
        let t = vec![vec![12.0, 7.0], vec![5.0, 9.0]];
        let y = chi_square(&t, true).unwrap();
        assert_close(y.chi2, 1.455996379, 1e-8);
        assert_close(y.p, 0.2275682146, 1e-8);
        let c = chi_square(&t, false).unwrap();
        assert_close(c.chi2, 2.43057552, 1e-8);
        let t3 = vec![vec![10.0, 30.0, 25.0], vec![20.0, 15.0, 12.0]];
        let c3 = chi_square(&t3, true).unwrap();
        assert_close(c3.chi2, 10.27339615, 1e-8);
        assert_close(c3.p, 0.005877063345, 1e-8);
    }

    #[test]
    fn fisher_matches_r() {
        assert_close(fisher_exact(12, 7, 5, 9), 0.1663200658, 1e-8);
        assert_close(fisher_exact(3, 1, 1, 3), 0.4857142857, 1e-8);
    }
}
