//! Rank based (nonparametric) tests.

use crate::descriptive::{median, sorted};
use crate::{dist, need, Result, StatsError};
use serde::{Deserialize, Serialize};

/// Average ranks (1 based) and the tie correction term Σ(t³ - t).
pub fn rank(values: &[f64]) -> (Vec<f64>, f64) {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&i, &j| values[i].total_cmp(&values[j]));
    let mut ranks = vec![0.0; values.len()];
    let mut ties = 0.0;
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && values[idx[j + 1]] == values[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for &k in &idx[i..=j] {
            ranks[k] = avg;
        }
        let t = (j - i + 1) as f64;
        ties += t * t * t - t;
        i = j + 1;
    }
    (ranks, ties)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MannWhitney {
    /// U statistic for group A (R's W).
    pub u: f64,
    pub sum_ranks_a: f64,
    pub sum_ranks_b: f64,
    pub p: f64,
    pub exact: bool,
    pub median_a: f64,
    pub median_b: f64,
    /// Hodges-Lehmann estimate of the difference B - A.
    pub hodges_lehmann: f64,
}

/// Two sided Mann-Whitney test. Exact (ties included) when N <= 50,
/// otherwise the normal approximation with continuity and tie corrections.
pub fn mann_whitney(a: &[f64], b: &[f64]) -> Result<MannWhitney> {
    need(!a.is_empty() && !b.is_empty(), "each group needs at least one value")?;
    let (n1, n2) = (a.len(), b.len());
    let all: Vec<f64> = a.iter().chain(b).copied().collect();
    let (ranks, ties) = rank(&all);
    let ra: f64 = ranks[..n1].iter().sum();
    let rb: f64 = ranks[n1..].iter().sum();
    let u = ra - (n1 * (n1 + 1)) as f64 / 2.0;
    let mn = (n1 * n2) as f64;
    let exact = n1 + n2 <= 50;
    let p = if exact {
        // Exact permutation distribution of the rank sum of A (midranks doubled
        // so they are integers); valid with and without ties.
        let doubled: Vec<usize> = ranks.iter().map(|r| (2.0 * r).round() as usize).collect();
        let max: usize = doubled.iter().sum();
        let mut counts = vec![vec![0.0f64; max + 1]; n1 + 1];
        counts[0][0] = 1.0;
        for (seen, &r) in doubled.iter().enumerate() {
            for k in (1..=n1.min(seen + 1)).rev() {
                for s in (r..=max).rev() {
                    counts[k][s] += counts[k - 1][s - r];
                }
            }
        }
        let dist = &counts[n1];
        let total: f64 = dist.iter().sum();
        let obs = (2.0 * ra).round() as usize;
        let lower: f64 = dist[..=obs].iter().sum::<f64>() / total;
        let upper: f64 = dist[obs..].iter().sum::<f64>() / total;
        (2.0 * lower.min(upper)).min(1.0)
    } else {
        let nt = (n1 + n2) as f64;
        let sigma = (mn / 12.0 * ((nt + 1.0) - ties / (nt * (nt - 1.0)))).sqrt();
        let dev = (u - mn / 2.0).abs();
        let z = ((dev - 0.5).max(0.0)) / sigma;
        dist::z_p2(z)
    };
    let mut diffs: Vec<f64> = b.iter().flat_map(|y| a.iter().map(move |x| y - x)).collect();
    diffs.sort_by(f64::total_cmp);
    Ok(MannWhitney {
        u,
        sum_ranks_a: ra,
        sum_ranks_b: rb,
        p,
        exact,
        median_a: median(a),
        median_b: median(b),
        hodges_lehmann: crate::descriptive::percentile_sorted(&diffs, 0.5),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedRank {
    /// Sum of positive ranks (R's V).
    pub sum_positive: f64,
    pub sum_negative: f64,
    /// Sum of signed ranks W = sum_positive - sum_negative.
    pub w: f64,
    pub n_used: usize,
    pub n_zero: usize,
    pub p: f64,
    pub exact: bool,
    pub median_difference: f64,
}

/// Wilcoxon signed rank test of values against `hypothetical`. Zero
/// differences are dropped.
pub fn wilcoxon_signed_rank(values: &[f64], hypothetical: f64) -> Result<SignedRank> {
    let d_all: Vec<f64> = values.iter().map(|v| v - hypothetical).collect();
    let d: Vec<f64> = d_all.iter().copied().filter(|v| *v != 0.0).collect();
    need(!d.is_empty(), "at least one nonzero difference is required")?;
    let n = d.len();
    let abs: Vec<f64> = d.iter().map(|v| v.abs()).collect();
    let (ranks, ties) = rank(&abs);
    let pos: f64 = ranks.iter().zip(&d).filter(|(_, v)| **v > 0.0).map(|(r, _)| r).sum();
    let total = (n * (n + 1)) as f64 / 2.0;
    let neg = total - pos;
    let n_zero = d_all.len() - n;
    let exact = n < 50;
    let p = if exact {
        // Exact sign-flip distribution over (doubled) midranks.
        let doubled: Vec<usize> = ranks.iter().map(|r| (2.0 * r).round() as usize).collect();
        let max: usize = doubled.iter().sum();
        let mut counts = vec![0.0f64; max + 1];
        counts[0] = 1.0;
        for &r in &doubled {
            for s in (r..=max).rev() {
                counts[s] += counts[s - r];
            }
        }
        let total_count = 2f64.powi(n as i32);
        let k = (2.0 * pos).round() as usize;
        let lower: f64 = counts[..=k].iter().sum::<f64>() / total_count;
        let upper: f64 = counts[k..].iter().sum::<f64>() / total_count;
        (2.0 * lower.min(upper)).min(1.0)
    } else {
        let nf = n as f64;
        let sigma = (nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - ties / 48.0).sqrt();
        let z = ((pos - total / 2.0).abs() - 0.5).max(0.0) / sigma;
        dist::z_p2(z)
    };
    Ok(SignedRank { sum_positive: pos, sum_negative: -neg, w: pos - neg, n_used: n, n_zero, p, exact, median_difference: median(&d_all) })
}

/// Wilcoxon matched pairs signed rank test on B - A.
pub fn wilcoxon_paired(a: &[f64], b: &[f64]) -> Result<SignedRank> {
    if a.len() != b.len() {
        return Err(StatsError::Invalid("paired data must have the same number of values".into()));
    }
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    wilcoxon_signed_rank(&d, 0.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KruskalWallis {
    pub h: f64,
    pub df: f64,
    pub p: f64,
    pub mean_ranks: Vec<f64>,
    pub ns: Vec<usize>,
    pub n_total: usize,
    pub tie_sum: f64,
}

pub fn kruskal_wallis(groups: &[Vec<f64>]) -> Result<KruskalWallis> {
    need(groups.len() >= 2, "at least two groups are required")?;
    need(groups.iter().all(|g| !g.is_empty()), "every group needs at least one value")?;
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let n = all.len() as f64;
    let (ranks, ties) = rank(&all);
    let mut offset = 0;
    let mut stat = 0.0;
    let mut mean_ranks = Vec::new();
    for g in groups {
        let r: f64 = ranks[offset..offset + g.len()].iter().sum();
        stat += r * r / g.len() as f64;
        mean_ranks.push(r / g.len() as f64);
        offset += g.len();
    }
    let mut h = 12.0 / (n * (n + 1.0)) * stat - 3.0 * (n + 1.0);
    h /= 1.0 - ties / (n * n * n - n);
    let df = (groups.len() - 1) as f64;
    Ok(KruskalWallis {
        h,
        df,
        p: dist::chi2_sf(h, df),
        mean_ranks,
        ns: groups.iter().map(Vec::len).collect(),
        n_total: all.len(),
        tie_sum: ties,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Friedman {
    pub statistic: f64,
    pub df: f64,
    pub p: f64,
    pub rank_sums: Vec<f64>,
}

/// Friedman test. `rows` are subjects (blocks), columns are treatments.
pub fn friedman(rows: &[Vec<f64>]) -> Result<Friedman> {
    need(rows.len() >= 2, "at least two subjects are required")?;
    let k = rows[0].len();
    need(k >= 2, "at least two treatments are required")?;
    if rows.iter().any(|r| r.len() != k) {
        return Err(StatsError::Invalid("every subject needs a value for every treatment".into()));
    }
    let n = rows.len() as f64;
    let kf = k as f64;
    let mut rank_sums = vec![0.0; k];
    let mut ties = 0.0;
    for r in rows {
        let (rk, t) = rank(r);
        ties += t;
        for (s, v) in rank_sums.iter_mut().zip(rk) {
            *s += v;
        }
    }
    let ss: f64 = rank_sums.iter().map(|r| r * r).sum();
    let mut q = 12.0 / (n * kf * (kf + 1.0)) * ss - 3.0 * n * (kf + 1.0);
    q /= 1.0 - ties / (n * (kf * kf * kf - kf));
    let df = kf - 1.0;
    Ok(Friedman { statistic: q, df, p: dist::chi2_sf(q, df), rank_sums })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DunnComparison {
    pub i: usize,
    pub j: usize,
    pub mean_rank_diff: f64,
    pub z: f64,
    pub p_adjusted: f64,
}

/// Dunn's multiple comparisons after Kruskal-Wallis, Bonferroni adjusted
/// for the number of `pairs` compared.
pub fn dunn(kw: &KruskalWallis, pairs: &[(usize, usize)]) -> Vec<DunnComparison> {
    let n = kw.n_total as f64;
    let m = pairs.len() as f64;
    let base = n * (n + 1.0) / 12.0 - kw.tie_sum / (12.0 * (n - 1.0));
    pairs
        .iter()
        .map(|&(i, j)| {
            let diff = kw.mean_ranks[i] - kw.mean_ranks[j];
            let se = (base * (1.0 / kw.ns[i] as f64 + 1.0 / kw.ns[j] as f64)).sqrt();
            let z = diff / se;
            DunnComparison { i, j, mean_rank_diff: diff, z, p_adjusted: (dist::z_p2(z) * m).clamp(0.0, 1.0) }
        })
        .collect()
}

/// Kolmogorov-Smirnov two sample statistic D (asymptotic P value).
pub fn kolmogorov_smirnov_2(a: &[f64], b: &[f64]) -> Result<(f64, f64)> {
    need(!a.is_empty() && !b.is_empty(), "each group needs at least one value")?;
    let (sa, sb) = (sorted(a), sorted(b));
    let (na, nb) = (sa.len() as f64, sb.len() as f64);
    let mut d: f64 = 0.0;
    for x in sa.iter().chain(&sb) {
        let fa = sa.partition_point(|v| v <= x) as f64 / na;
        let fb = sb.partition_point(|v| v <= x) as f64 / nb;
        d = d.max((fa - fb).abs());
    }
    let en = (na * nb / (na + nb)).sqrt();
    let lambda = (en + 0.12 + 0.11 / en) * d;
    // The series does not converge for tiny lambda (D near 0): the distributions agree, P = 1.
    let mut p = 1.0;
    let mut sum = 0.0;
    for j in 1..=100 {
        let jf = j as f64;
        let term = 2.0 * (-1f64).powi(j - 1) * (-2.0 * jf * jf * lambda * lambda).exp();
        sum += term;
        if term.abs() < 1e-12 {
            p = sum;
            break;
        }
    }
    Ok((d, p.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use crate::ttest::tests::{A, B, C};

    #[test]
    fn mann_whitney_matches_r() {
        let r = mann_whitney(&A, &C).unwrap();
        assert!(r.exact);
        assert_close(r.u, 15.0, 1e-12);
        assert_close(r.p, 0.2823842824, 1e-9);
        let mut a2 = A.to_vec();
        a2.push(1.5);
        let mut c2 = C.to_vec();
        c2.extend([1.5, 30.0]);
        let r = mann_whitney(&a2, &c2).unwrap();
        assert!(r.exact); // R >= 4.4 also computes exact P values with ties
        assert_close(r.u, 23.5, 1e-12);
        assert_close(r.p, 0.2441793501, 1e-8);
    }

    #[test]
    fn signed_rank_matches_r() {
        let r = wilcoxon_paired(&B, &A).unwrap();
        assert_close(r.sum_positive, 0.0, 1e-12);
        assert_close(r.p, 0.0078125, 1e-12);
        let d = [1.2, -0.4, 2.5, 0.8, 1.2, 3.1, -1.5, 0.8, 2.2, 0.4];
        let r = wilcoxon_signed_rank(&d, 0.0).unwrap();
        assert_close(r.sum_positive, 46.5, 1e-12);
        assert_close(r.p, 0.052734375, 1e-10);
    }

    #[test]
    fn ks_two_sample_of_identical_groups_is_not_significant() {
        assert_eq!(kolmogorov_smirnov_2(&A, &A).unwrap(), (0.0, 1.0));
    }

    #[test]
    fn kruskal_and_friedman_match_r() {
        let kw = kruskal_wallis(&[A.to_vec(), B.to_vec(), C.to_vec()]).unwrap();
        assert_close(kw.h, 13.73814229, 1e-8);
        assert_close(kw.p, 0.001039442106, 1e-8);
        let rows: Vec<Vec<f64>> = (0..6).map(|i| vec![A[i], B[i], C[i]]).collect();
        let f = friedman(&rows).unwrap();
        assert_close(f.statistic, 10.33333333, 1e-8);
        assert_close(f.p, 0.005703548998, 1e-8);
    }
}
