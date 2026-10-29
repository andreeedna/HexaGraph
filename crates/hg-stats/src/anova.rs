//! Analysis of variance: one-way, Welch, repeated measures and two-way.

use crate::descriptive::{mean, median, variance};
use crate::{dist, linalg, need, Result, StatsError};
use serde::{Deserialize, Serialize};

/// One row of an ANOVA table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub ss: f64,
    pub df: f64,
    pub ms: f64,
    pub f: Option<f64>,
    pub p: Option<f64>,
    pub percent_of_total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OneWay {
    pub sources: Vec<Source>,
    pub f: f64,
    pub df_between: f64,
    pub df_within: f64,
    pub p: f64,
    pub r_squared: f64,
    pub ms_within: f64,
    pub means: Vec<f64>,
    pub ns: Vec<f64>,
    /// Brown-Forsythe test for equal variances (F, P).
    pub brown_forsythe: Option<(f64, f64)>,
    /// Bartlett's test for equal variances (chi², P).
    pub bartlett: Option<(f64, f64)>,
}

fn source(name: &str, ss: f64, df: f64, err: Option<(f64, f64)>, total: f64) -> Source {
    let ms = ss / df;
    let (f, p) = match err {
        Some((ms_err, df_err)) => {
            let f = ms / ms_err;
            (Some(f), Some(dist::f_sf(f, df, df_err)))
        }
        None => (None, None),
    };
    Source { name: name.into(), ss, df, ms, f, p, percent_of_total: 100.0 * ss / total }
}

fn f_only(groups: &[Vec<f64>]) -> (f64, f64, f64, f64, f64) {
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let grand = mean(&all);
    let ssb: f64 = groups.iter().map(|g| g.len() as f64 * (mean(g) - grand).powi(2)).sum();
    let ssw: f64 = groups
        .iter()
        .map(|g| {
            let m = mean(g);
            g.iter().map(|v| (v - m).powi(2)).sum::<f64>()
        })
        .sum();
    let dfb = (groups.len() - 1) as f64;
    let dfw = (all.len() - groups.len()) as f64;
    let f = (ssb / dfb) / (ssw / dfw);
    (f, dfb, dfw, ssb, ssw)
}

/// Ordinary one-way ANOVA.
pub fn one_way(groups: &[Vec<f64>]) -> Result<OneWay> {
    need(groups.len() >= 2, "at least two groups are required")?;
    need(groups.iter().all(|g| !g.is_empty()), "every group needs at least one value")?;
    let n: usize = groups.iter().map(Vec::len).sum();
    need(n > groups.len(), "at least one group needs replicates")?;
    let (f, dfb, dfw, ssb, ssw) = f_only(groups);
    let total = ssb + ssw;
    let ms_within = ssw / dfw;
    let sources = vec![
        source("Treatment (between columns)", ssb, dfb, Some((ms_within, dfw)), total),
        source("Residual (within columns)", ssw, dfw, None, total),
        Source { name: "Total".into(), ss: total, df: dfb + dfw, ms: f64::NAN, f: None, p: None, percent_of_total: 100.0 },
    ];
    let enough = groups.iter().all(|g| g.len() >= 2);
    let brown_forsythe = enough.then(|| {
        let dev: Vec<Vec<f64>> = groups
            .iter()
            .map(|g| {
                let md = median(g);
                g.iter().map(|v| (v - md).abs()).collect()
            })
            .collect();
        let (f, d1, d2, _, _) = f_only(&dev);
        (f, dist::f_sf(f, d1, d2))
    });
    let bartlett = enough.then(|| bartlett(groups));
    Ok(OneWay {
        sources,
        f,
        df_between: dfb,
        df_within: dfw,
        p: dist::f_sf(f, dfb, dfw),
        r_squared: ssb / total,
        ms_within,
        means: groups.iter().map(|g| mean(g)).collect(),
        ns: groups.iter().map(|g| g.len() as f64).collect(),
        brown_forsythe,
        bartlett,
    })
}

fn bartlett(groups: &[Vec<f64>]) -> (f64, f64) {
    let k = groups.len() as f64;
    let n: f64 = groups.iter().map(|g| g.len() as f64).sum();
    let pooled = groups.iter().map(|g| (g.len() as f64 - 1.0) * variance(g)).sum::<f64>() / (n - k);
    let num = (n - k) * pooled.ln() - groups.iter().map(|g| (g.len() as f64 - 1.0) * variance(g).ln()).sum::<f64>();
    let den = 1.0 + (groups.iter().map(|g| 1.0 / (g.len() as f64 - 1.0)).sum::<f64>() - 1.0 / (n - k)) / (3.0 * (k - 1.0));
    let stat = num / den;
    (stat, dist::chi2_sf(stat, k - 1.0))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welch {
    pub f: f64,
    pub df_num: f64,
    pub df_den: f64,
    pub p: f64,
}

/// Welch's ANOVA (unequal variances).
pub fn welch(groups: &[Vec<f64>]) -> Result<Welch> {
    need(groups.len() >= 2, "at least two groups are required")?;
    need(groups.iter().all(|g| g.len() >= 2), "every group needs at least two values")?;
    let k = groups.len() as f64;
    let w: Vec<f64> = groups.iter().map(|g| g.len() as f64 / variance(g)).collect();
    let m: Vec<f64> = groups.iter().map(|g| mean(g)).collect();
    let sw: f64 = w.iter().sum();
    let mw: f64 = w.iter().zip(&m).map(|(a, b)| a * b).sum::<f64>() / sw;
    let num: f64 = w.iter().zip(&m).map(|(wi, mi)| wi * (mi - mw).powi(2)).sum::<f64>() / (k - 1.0);
    let lambda: f64 = groups.iter().zip(&w).map(|(g, wi)| (1.0 - wi / sw).powi(2) / (g.len() as f64 - 1.0)).sum();
    let den = 1.0 + 2.0 * (k - 2.0) / (k * k - 1.0) * lambda;
    let f = num / den;
    let df_den = (k * k - 1.0) / (3.0 * lambda);
    Ok(Welch { f, df_num: k - 1.0, df_den, p: dist::f_sf(f, k - 1.0, df_den) })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatedMeasures {
    pub sources: Vec<Source>,
    pub f: f64,
    pub p: f64,
    /// Geisser-Greenhouse epsilon and the corrected P value.
    pub epsilon: f64,
    pub p_corrected: f64,
    pub ms_residual: f64,
    pub df_residual: f64,
    pub means: Vec<f64>,
    pub n_subjects: usize,
}

/// One-way repeated measures ANOVA. `rows` are subjects, columns treatments.
pub fn repeated_measures(rows: &[Vec<f64>]) -> Result<RepeatedMeasures> {
    need(rows.len() >= 2, "at least two subjects are required")?;
    let k = rows[0].len();
    need(k >= 2, "at least two treatments are required")?;
    if rows.iter().any(|r| r.len() != k) {
        return Err(StatsError::Invalid("every subject needs a value for every treatment".into()));
    }
    let n = rows.len();
    let (nf, kf) = (n as f64, k as f64);
    let all: Vec<f64> = rows.iter().flatten().copied().collect();
    let grand = mean(&all);
    let col_means: Vec<f64> = (0..k).map(|j| rows.iter().map(|r| r[j]).sum::<f64>() / nf).collect();
    let row_means: Vec<f64> = rows.iter().map(|r| mean(r)).collect();
    let ss_total: f64 = all.iter().map(|v| (v - grand).powi(2)).sum();
    let ss_treat: f64 = nf * col_means.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let ss_subj: f64 = kf * row_means.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let ss_res = ss_total - ss_treat - ss_subj;
    let (df_t, df_s) = (kf - 1.0, nf - 1.0);
    let df_r = df_t * df_s;
    let ms_r = ss_res / df_r;
    let f = (ss_treat / df_t) / ms_r;
    // Geisser-Greenhouse epsilon from the double centered covariance matrix.
    let cov: Vec<Vec<f64>> = (0..k)
        .map(|a| (0..k).map(|b| rows.iter().map(|r| (r[a] - col_means[a]) * (r[b] - col_means[b])).sum::<f64>() / (nf - 1.0)).collect())
        .collect();
    let row_avg: Vec<f64> = cov.iter().map(|r| mean(r)).collect();
    let all_avg = mean(&row_avg);
    let c: Vec<Vec<f64>> = (0..k).map(|a| (0..k).map(|b| cov[a][b] - row_avg[a] - row_avg[b] + all_avg).collect()).collect();
    let trace: f64 = (0..k).map(|a| c[a][a]).sum();
    let trace_sq: f64 = c.iter().flatten().map(|v| v * v).sum();
    let epsilon = (trace * trace / (df_t * trace_sq)).clamp(1.0 / df_t, 1.0);
    let sources = vec![
        source("Treatment (between columns)", ss_treat, df_t, Some((ms_r, df_r)), ss_total),
        source("Individual (between rows)", ss_subj, df_s, Some((ms_r, df_r)), ss_total),
        source("Residual (random)", ss_res, df_r, None, ss_total),
        Source { name: "Total".into(), ss: ss_total, df: nf * kf - 1.0, ms: f64::NAN, f: None, p: None, percent_of_total: 100.0 },
    ];
    Ok(RepeatedMeasures {
        sources,
        f,
        p: dist::f_sf(f, df_t, df_r),
        epsilon,
        p_corrected: dist::f_sf(f, epsilon * df_t, epsilon * df_r),
        ms_residual: ms_r,
        df_residual: df_r,
        means: col_means,
        n_subjects: n,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoWay {
    pub sources: Vec<Source>,
    pub ms_residual: f64,
    pub df_residual: f64,
    pub interaction_fitted: bool,
    /// cell_means[row][col]
    pub cell_means: Vec<Vec<f64>>,
    pub cell_ns: Vec<Vec<f64>>,
}

/// Two-way ANOVA with Type III sums of squares (effect coding).
/// `cells[row][col]` holds the replicates of one cell. When every cell has a
/// single value the interaction cannot be estimated and an additive model is used.
pub fn two_way(cells: &[Vec<Vec<f64>>]) -> Result<TwoWay> {
    let a = cells.len();
    need(a >= 2, "at least two rows are required")?;
    let b = cells[0].len();
    need(b >= 2, "at least two columns are required")?;
    if cells.iter().any(|r| r.len() != b) || cells.iter().flatten().any(Vec::is_empty) {
        return Err(StatsError::Invalid("every cell needs at least one value".into()));
    }
    let obs: Vec<(usize, usize, f64)> = cells
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.iter().enumerate().flat_map(move |(j, c)| c.iter().map(move |&v| (i, j, v))))
        .collect();
    let n = obs.len();
    let interaction = n > a * b;
    let code = |level: usize, levels: usize| -> Vec<f64> {
        (0..levels - 1)
            .map(|c| {
                if level == levels - 1 {
                    -1.0
                } else if level == c {
                    1.0
                } else {
                    0.0
                }
            })
            .collect()
    };
    let y: Vec<f64> = obs.iter().map(|o| o.2).collect();
    let design = |with_a: bool, with_b: bool, with_ab: bool| -> Vec<Vec<f64>> {
        obs.iter()
            .map(|&(i, j, _)| {
                let (ca, cb) = (code(i, a), code(j, b));
                let mut row = vec![1.0];
                if with_a {
                    row.extend(&ca);
                }
                if with_b {
                    row.extend(&cb);
                }
                if with_ab {
                    for x in &ca {
                        for z in &cb {
                            row.push(x * z);
                        }
                    }
                }
                row
            })
            .collect()
    };
    let sse = |x: Vec<Vec<f64>>| -> Result<f64> {
        linalg::least_squares(&x, &y).map(|(_, s)| s).ok_or_else(|| StatsError::Invalid("the design is singular".into()))
    };
    let full = sse(design(true, true, interaction))?;
    let ss_a = sse(design(false, true, interaction))? - full;
    let ss_b = sse(design(true, false, interaction))? - full;
    let ss_ab = if interaction { sse(design(true, true, false))? - full } else { 0.0 };
    let grand = mean(&y);
    let ss_total: f64 = y.iter().map(|v| (v - grand).powi(2)).sum();
    let (df_a, df_b) = ((a - 1) as f64, (b - 1) as f64);
    let df_ab = df_a * df_b;
    let df_res = if interaction { (n - a * b) as f64 } else { df_ab };
    let ms_res = full / df_res;
    let err = Some((ms_res, df_res));
    let mut sources = Vec::new();
    if interaction {
        sources.push(source("Interaction", ss_ab, df_ab, err, ss_total));
    }
    sources.push(source("Row factor", ss_a, df_a, err, ss_total));
    sources.push(source("Column factor", ss_b, df_b, err, ss_total));
    sources.push(source("Residual", full, df_res, None, ss_total));
    Ok(TwoWay {
        sources,
        ms_residual: ms_res,
        df_residual: df_res,
        interaction_fitted: interaction,
        cell_means: cells.iter().map(|r| r.iter().map(|c| mean(c)).collect()).collect(),
        cell_ns: cells.iter().map(|r| r.iter().map(|c| c.len() as f64).collect()).collect(),
    })
}

/// Two-way ANOVA with repeated measures across rows (split plot): each subject
/// belongs to one column group and is measured at every row. `groups[g][s][r]`
/// is subject `s` of group `g` at row `r`. Within-subject effects use Type III
/// sums of squares; their Geisser-Greenhouse epsilon comes from the pooled
/// within-group covariance matrix.
pub fn two_way_repeated(groups: &[Vec<Vec<f64>>]) -> Result<TwoWayRepeated> {
    let k = groups.len();
    need(k >= 2, "at least two data sets are required")?;
    let r = groups.iter().flatten().map(Vec::len).next().unwrap_or(0);
    need(r >= 2, "at least two rows are required")?;
    if groups.iter().flatten().any(|s| s.len() != r) {
        return Err(StatsError::Invalid("every subject needs a value in every row".into()));
    }
    need(groups.iter().all(|g| !g.is_empty()), "every data set needs at least one subject")?;
    let n_subj: usize = groups.iter().map(Vec::len).sum();
    need(n_subj > k, "at least one data set needs two or more subjects")?;
    let (kf, rf, nf) = (k as f64, r as f64, n_subj as f64);
    // Between subjects: a one-way ANOVA of the subject means (scaled by r).
    let subj_means: Vec<Vec<f64>> = groups.iter().map(|g| g.iter().map(|s| mean(s)).collect()).collect();
    let all: Vec<f64> = groups.iter().flatten().flatten().copied().collect();
    let grand = mean(&all);
    let ss_total: f64 = all.iter().map(|v| (v - grand).powi(2)).sum();
    let group_means: Vec<f64> = subj_means.iter().map(|g| mean(g)).collect();
    let ss_col: f64 = subj_means.iter().zip(&group_means).map(|(g, m)| rf * g.len() as f64 * (m - grand).powi(2)).sum();
    let ss_subj: f64 = subj_means.iter().zip(&group_means).map(|(g, m)| rf * g.iter().map(|s| (s - m).powi(2)).sum::<f64>()).sum();
    let (df_col, df_subj) = (kf - 1.0, nf - kf);
    // Within subjects: Type III fit of the subject centered values.
    let mut obs = Vec::new();
    for (g, subjects) in groups.iter().enumerate() {
        for (s, m) in subjects.iter().zip(&subj_means[g]) {
            for (row, v) in s.iter().enumerate() {
                obs.push((row, g, v - m));
            }
        }
    }
    let y: Vec<f64> = obs.iter().map(|o| o.2).collect();
    let code = |level: usize, levels: usize| -> Vec<f64> {
        (0..levels - 1).map(|c| if level == levels - 1 { -1.0 } else if level == c { 1.0 } else { 0.0 }).collect()
    };
    let design = |with_a: bool, with_ab: bool| -> Vec<Vec<f64>> {
        obs.iter()
            .map(|&(i, j, _)| {
                let (ca, cb) = (code(i, r), code(j, k));
                let mut row = vec![1.0];
                row.extend(&cb);
                if with_a {
                    row.extend(&ca);
                }
                if with_ab {
                    for x in &ca {
                        for z in &cb {
                            row.push(x * z);
                        }
                    }
                }
                row
            })
            .collect()
    };
    let sse = |x: Vec<Vec<f64>>| -> Result<f64> {
        linalg::least_squares(&x, &y).map(|(_, s)| s).ok_or_else(|| StatsError::Invalid("the design is singular".into()))
    };
    let ss_res = sse(design(true, true))?;
    let ss_row = sse(design(false, true))? - ss_res;
    let ss_int = sse(design(true, false))? - ss_res;
    let df_row = rf - 1.0;
    let df_int = df_row * df_col;
    let df_res = df_row * df_subj;
    let ms_res = ss_res / df_res;
    // Pooled within-group covariance of the repeated measurements.
    let cell_means: Vec<Vec<f64>> =
        (0..r).map(|row| groups.iter().map(|g| g.iter().map(|s| s[row]).sum::<f64>() / g.len() as f64).collect()).collect();
    let cov: Vec<Vec<f64>> = (0..r)
        .map(|a| {
            (0..r)
                .map(|b| {
                    groups
                        .iter()
                        .enumerate()
                        .map(|(g, sub)| sub.iter().map(|s| (s[a] - cell_means[a][g]) * (s[b] - cell_means[b][g])).sum::<f64>())
                        .sum::<f64>()
                        / df_subj
                })
                .collect()
        })
        .collect();
    let row_avg: Vec<f64> = cov.iter().map(|c| mean(c)).collect();
    let all_avg = mean(&row_avg);
    let c: Vec<Vec<f64>> = (0..r).map(|a| (0..r).map(|b| cov[a][b] - row_avg[a] - row_avg[b] + all_avg).collect()).collect();
    let trace: f64 = (0..r).map(|a| c[a][a]).sum();
    let trace_sq: f64 = c.iter().flatten().map(|v| v * v).sum();
    let epsilon = if trace_sq > 0.0 { (trace * trace / (df_row * trace_sq)).clamp(1.0 / df_row, 1.0) } else { 1.0 };
    let corrected = |name: &str, ss: f64, df: f64| {
        let mut src = source(name, ss, df, Some((ms_res, df_res)), ss_total);
        if let Some(f) = src.f {
            src.p = Some(dist::f_sf(f, epsilon * df, epsilon * df_res));
        }
        src
    };
    let sources = vec![
        corrected("Interaction", ss_int, df_int),
        corrected("Row factor", ss_row, df_row),
        source("Column factor", ss_col, df_col, Some((ss_subj / df_subj, df_subj)), ss_total),
        source("Subjects (matching)", ss_subj, df_subj, None, ss_total),
        source("Residual", ss_res, df_res, None, ss_total),
    ];
    Ok(TwoWayRepeated { sources, epsilon, ms_residual: ms_res, df_residual: df_res, cell_means, n_subjects: n_subj })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoWayRepeated {
    pub sources: Vec<Source>,
    /// Geisser-Greenhouse epsilon applied to the row factor and the interaction.
    pub epsilon: f64,
    pub ms_residual: f64,
    pub df_residual: f64,
    /// cell_means[row][col]
    pub cell_means: Vec<Vec<f64>>,
    pub n_subjects: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use crate::posthoc::{all_pairs, compare_means, Method};
    use crate::ttest::tests::{A, B, C};

    fn groups() -> Vec<Vec<f64>> {
        vec![A.to_vec(), B.to_vec(), C.to_vec()]
    }

    #[test]
    fn one_way_matches_r() {
        let r = one_way(&groups()).unwrap();
        assert_close(r.f, 17.29188082, 1e-8);
        assert_close(r.p, 5.276474276e-05, 1e-6);
        assert_close(r.sources[0].ss, 98.62439394, 1e-8);
        assert_close(r.sources[1].ss, 54.18333333, 1e-8);
        let (b, bp) = r.bartlett.unwrap();
        assert_close(b, 0.1040086155, 1e-8);
        assert_close(bp, 0.9493247774, 1e-8);
        let w = welch(&groups()).unwrap();
        assert_close(w.f, 16.34681156, 1e-8);
        assert_close(w.df_den, 12.21190659, 1e-8);
        assert_close(w.p, 0.0003523575475, 1e-6);
    }

    #[test]
    fn tukey_matches_r() {
        let r = one_way(&groups()).unwrap();
        // R reports B-A, C-A, C-B, i.e. mean[j] - mean[i] for our pairs.
        let c = compare_means(&r.means, &r.ns, r.ms_within, r.df_within, &all_pairs(3), Method::Tukey, 3, 0.95);
        assert_close(-c[0].difference, 4.775, 1e-12);
        assert_close(-c[0].ci95.unwrap().low, 6.920046527, 1e-6);
        assert_close(c[0].p_adjusted, 0.0000536640, 1e-4);
        assert_close(c[1].p_adjusted, 0.4744676658, 1e-6);
        assert_close(c[2].p_adjusted, 0.0018948754, 1e-5);
    }

    #[test]
    fn two_way_balanced_matches_classic_formulas() {
        // Balanced 2x3 design with 2 replicates.
        let cells = vec![vec![vec![4.0, 5.0], vec![6.0, 7.0], vec![8.0, 9.5]], vec![vec![3.0, 4.5], vec![7.0, 6.0], vec![12.0, 11.0]]];
        let r = two_way(&cells).unwrap();
        assert!(r.interaction_fitted);
        let ss: Vec<f64> = r.sources.iter().map(|s| s.ss).collect();
        assert_close(ss[0], REF_AB, 1e-8);
        assert_close(ss[1], REF_A, 1e-8);
        assert_close(ss[2], REF_B, 1e-8);
        assert_close(ss[3], REF_E, 1e-8);
    }

    // R: anova(lm(y ~ a * b))
    const REF_AB: f64 = 6.791666666667;
    const REF_A: f64 = 1.333333333333;
    const REF_B: f64 = 73.041666666667;
    const REF_E: f64 = 4.25;

    #[test]
    fn two_way_unbalanced_type_iii_matches_r() {
        // R: drop1(lm(y ~ a * b), test = "F") with sum contrasts
        let cells =
            vec![vec![vec![4.0, 5.0, 5.5], vec![6.0, 7.0], vec![8.0, 9.5]], vec![vec![3.0, 4.5], vec![7.0, 6.0], vec![12.0, 11.0, 13.0]]];
        let r = two_way(&cells).unwrap();
        let ss: Vec<f64> = r.sources.iter().map(|s| s.ss).collect();
        assert_close(ss[0], 12.095, 1e-3);
        assert_close(ss[1], 1.760, 1e-3);
        assert_close(ss[2], 90.777, 1e-4);
        assert_close(ss[3], 6.417, 1e-3);
    }

    #[test]
    fn two_way_repeated_split_plot() {
        // Two groups of subjects measured at three times.
        let groups = vec![
            vec![vec![1.0, 3.0, 6.0], vec![2.0, 3.5, 7.0], vec![1.5, 4.0, 6.5]],
            vec![vec![2.0, 5.0, 9.0], vec![2.5, 6.0, 9.5], vec![1.0, 5.5, 8.0], vec![2.2, 4.8, 10.0]],
        ];
        let r = two_way_repeated(&groups).unwrap();
        let src = |n: &str| r.sources.iter().find(|s| s.name == n).unwrap().clone();
        // The between subjects test is a one-way ANOVA of the subject means.
        let means: Vec<Vec<f64>> = groups.iter().map(|g| g.iter().map(|s| mean(s)).collect()).collect();
        let one = one_way(&means).unwrap();
        assert_close(src("Column factor").f.unwrap(), one.f, 1e-9);
        assert_close(src("Column factor").p.unwrap(), one.p, 1e-9);
        assert_eq!(src("Residual").df, 2.0 * 5.0);
        // Sums of squares add up when the design is balanced within subjects.
        let all: Vec<f64> = groups.iter().flatten().flatten().copied().collect();
        let grand = mean(&all);
        let total: f64 = all.iter().map(|v| (v - grand).powi(2)).sum();
        let parts: f64 = r.sources.iter().map(|s| s.ss).sum();
        assert!(parts <= total + 1e-9);
        assert!(r.epsilon > 0.5 - 1e-12 && r.epsilon <= 1.0);
    }

    #[test]
    fn repeated_measures_matches_r() {
        let rows: Vec<Vec<f64>> = (0..6).map(|i| vec![A[i], B[i], C[i]]).collect();
        let r = repeated_measures(&rows).unwrap();
        assert_close(r.f, 23.01, 1e-3);
        assert_close(r.p, 0.000181, 1e-5);
        assert_close(r.sources[0].ss, 60.19, 1e-3);
        assert_close(r.sources[1].ss, 34.57, 1e-3);
        assert!(r.epsilon > 0.5 && r.epsilon <= 1.0);
    }
}
