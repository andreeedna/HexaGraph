//! Which test to use for comparing groups, and why: the decision path shown
//! in the Analyze dialog.

use crate::analysis::{AnalysisKind, OneWayTest, TwoGroupTest};
use crate::format::p;
use crate::table::{DataTable, TableKind};
use hg_stats::posthoc::Method;
use hg_stats::{anova, descriptive::mean, nonlinear, normality, outliers, regression};

/// One step of the decision. `ok`: Some(true) passed, Some(false) failed,
/// None information only.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub ok: Option<bool>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Advice {
    pub kind: AnalysisKind,
    pub checks: Vec<Check>,
    /// Why this test, in plain words.
    pub reasons: Vec<String>,
    /// The test to use instead if an assumption is in doubt.
    pub alternative: Option<String>,
}

/// Suggests a test to compare `samples` (one per group, in the order of
/// `groups`, the table data set indices). With `matched` the samples are rows
/// of the same subjects and must have equal lengths. `current` keeps the
/// user's comparisons and control.
pub fn suggest(samples: &[Vec<f64>], groups: &[usize], matched: bool, current: &AnalysisKind) -> Option<Advice> {
    let k = samples.len();
    if k < 2 || groups.len() != k || samples.iter().any(|s| s.is_empty()) {
        return None;
    }
    let mut checks = Vec::new();
    let mut reasons = Vec::new();
    checks.push(Check {
        ok: None,
        text: if matched {
            "Matched design: each row is one subject measured in every group (set in Experimental design).".into()
        } else {
            "Independent groups: different subjects in each group (set in Experimental design).".into()
        },
    });

    // Gaussian? Matched designs test the differences (two groups) or the
    // residuals of the subjects × groups model (three or more).
    let tested: Vec<(String, Vec<f64>)> = if matched && k == 2 {
        vec![("the differences".into(), samples[0].iter().zip(&samples[1]).map(|(a, b)| b - a).collect())]
    } else if matched {
        let n = samples[0].len();
        let col: Vec<f64> = samples.iter().map(|s| mean(s)).collect();
        let row: Vec<f64> = (0..n).map(|i| samples.iter().map(|s| s[i]).sum::<f64>() / k as f64).collect();
        let grand = mean(&col);
        let res = (0..n).flat_map(|i| (0..k).map(move |j| (i, j))).map(|(i, j)| samples[j][i] - row[i] - col[j] + grand).collect();
        vec![("the residuals".into(), res)]
    } else {
        samples.iter().enumerate().map(|(j, s)| (format!("group {}", j + 1), s.clone())).collect()
    };
    let shapiro: Vec<(String, usize, Option<f64>)> = tested
        .iter()
        .map(|(name, v)| (name.clone(), v.len(), normality::shapiro_wilk(v).ok().map(|r| r.p).filter(|p| p.is_finite())))
        .collect();
    let failed: Vec<&(String, usize, Option<f64>)> = shapiro.iter().filter(|s| s.2.is_some_and(|p| p < 0.05)).collect();
    let any_tested = shapiro.iter().any(|s| s.2.is_some());
    let gaussian = failed.is_empty();
    let min_n = shapiro.iter().map(|s| s.1).min().unwrap_or(0);
    checks.push(match (any_tested, failed.first()) {
        (false, _) => Check { ok: None, text: "Too few values to test for a Gaussian distribution.".into() },
        (true, None) => {
            Check { ok: Some(true), text: format!("Consistent with a Gaussian distribution (Shapiro-Wilk on {}).", tested_name(&shapiro)) }
        }
        (true, Some(f)) => Check { ok: Some(false), text: format!("Not Gaussian: Shapiro-Wilk P = {} for {}.", p(f.2.unwrap()), f.0) },
    });
    if !any_tested {
        reasons.push(
            "With so few values a normality test is not possible: the parametric test assumes the data come from a Gaussian population."
                .into(),
        );
    } else if gaussian {
        reasons.push("The data are consistent with a Gaussian distribution, so a parametric test (more powerful) is appropriate.".into());
    } else {
        reasons.push(
            "The data do not look Gaussian, so a rank-based (nonparametric) test is safer: it does not assume a distribution.".into(),
        );
    }
    if any_tested && min_n < 8 {
        reasons.push(format!(
            "With only {min_n} values a normality test can miss a non-Gaussian distribution: also consider what is known about this kind of data."
        ));
    }

    // Outliers (Grubbs) in each group.
    if !matched {
        let n_out: usize = samples.iter().map(|s| outliers::grubbs(s, 0.05).map_or(0, |g| g.outliers.len())).sum();
        checks.push(if n_out == 0 {
            Check { ok: Some(true), text: "No outliers (Grubbs, α = 0.05).".into() }
        } else {
            reasons.push(format!("{n_out} possible outlier(s): check them; rank-based tests are less affected by extreme values."));
            Check { ok: Some(false), text: format!("{n_out} possible outlier(s) (Grubbs, α = 0.05).") }
        });
    }

    // Equal spread matters only for independent groups and parametric tests.
    let mut equal_sd = true;
    if !matched && gaussian && samples.iter().all(|s| s.len() >= 2) {
        if let Some((_, bf)) = anova::one_way(samples).ok().and_then(|r| r.brown_forsythe).filter(|b| b.1.is_finite()) {
            equal_sd = bf >= 0.05;
            checks.push(Check {
                ok: Some(equal_sd),
                text: if equal_sd {
                    format!("Similar SDs (Brown-Forsythe P = {}).", p(bf))
                } else {
                    format!("The SDs differ (Brown-Forsythe P = {}).", p(bf))
                },
            });
            if !equal_sd {
                reasons.push("The SDs differ, so the test must not assume equal variances (Welch).".into());
            }
        }
    }

    let (kind, alternative) = if k == 2 {
        let (a, b) = match current {
            AnalysisKind::TwoGroups { a, b, .. } => (*a, *b),
            _ => (groups[0], groups[1]),
        };
        let (test, alt) = match (matched, gaussian, equal_sd) {
            (true, true, _) => (TwoGroupTest::Paired, TwoGroupTest::Wilcoxon),
            (true, false, _) => (TwoGroupTest::Wilcoxon, TwoGroupTest::Paired),
            (false, true, true) => (TwoGroupTest::Unpaired, TwoGroupTest::MannWhitney),
            (false, true, false) => (TwoGroupTest::Welch, TwoGroupTest::MannWhitney),
            (false, false, _) => (TwoGroupTest::MannWhitney, TwoGroupTest::Welch),
        };
        (AnalysisKind::TwoGroups { a, b, test }, alt.label().to_string())
    } else {
        let (comparisons, control) = match current {
            AnalysisKind::OneWay { comparisons, control, .. } => (*comparisons, *control),
            _ => (None, None),
        };
        let (test, alt) = match (matched, gaussian, equal_sd) {
            (true, true, _) => (OneWayTest::RepeatedMeasures, OneWayTest::Friedman),
            (true, false, _) => (OneWayTest::Friedman, OneWayTest::RepeatedMeasures),
            (false, true, true) => (OneWayTest::Ordinary, OneWayTest::KruskalWallis),
            (false, true, false) => (OneWayTest::Welch, OneWayTest::KruskalWallis),
            (false, false, _) => (OneWayTest::KruskalWallis, OneWayTest::Welch),
        };
        let comparisons = comparisons.or(Some(if control.is_some() { Method::Dunnett } else { Method::Tukey }));
        if let Some(then) = follow_up(test, control.is_some()) {
            reasons.push(format!("Then {then} tells which groups differ."));
        }
        (AnalysisKind::OneWay { test, comparisons, control }, alt.label().to_string())
    };
    let doubt = if gaussian { "if you doubt the Gaussian assumption" } else { "if the population is known to be Gaussian" };
    Some(Advice { kind, checks, reasons, alternative: Some(format!("{alternative} {doubt}.")) })
}

/// An analysis that suits the data, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub kind: AnalysisKind,
    pub title: String,
    pub why: String,
}

impl Pick {
    /// Whether `k` is this analysis (curve fits: the same equation, whatever the constraints).
    pub fn is(&self, k: &AnalysisKind) -> bool {
        match (&self.kind, k) {
            (AnalysisKind::NonlinearFit { model: a, .. }, AnalysisKind::NonlinearFit { model: b, .. }) => a == b,
            (a, b) => a == b,
        }
    }
}

/// What the data look like and which analyses suit them, for any kind of table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    pub facts: Vec<String>,
    /// Best first.
    pub picks: Vec<Pick>,
}

fn pick(kind: AnalysisKind, why: impl Into<String>) -> Pick {
    let title = match &kind {
        AnalysisKind::NonlinearFit { model: nonlinear::Model::DoseResponseVariableSlope, .. } => "Dose-response curve (4PL, EC50)".into(),
        AnalysisKind::NonlinearFit { model: nonlinear::Model::InhibitionVariableSlope, .. } => "Inhibition curve (4PL, IC50)".into(),
        AnalysisKind::TwoWay { repeated: true, .. } => "Two-way ANOVA, repeated measures".into(),
        AnalysisKind::Survival { gehan: true } => "Survival: Gehan-Breslow-Wilcoxon".into(),
        k => k.label(),
    };
    Pick { kind, title, why: why.into() }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Describes the table and suggests analyses. Column tables also get the
/// detailed test advice of [`suggest`] in the Analyze dialog.
pub fn overview(t: &DataTable) -> Overview {
    let sets = t.nonempty_datasets();
    let names: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    let mut o = Overview::default();
    if sets.is_empty() {
        o.facts.push("The table is empty: enter or paste some data first.".into());
        return o;
    }
    match t.kind {
        TableKind::Column => {
            let ns: Vec<usize> = sets.iter().map(|&d| t.column_values(d).len()).collect();
            o.facts.push(format!("{} with {} values.", plural(sets.len(), "group", "groups"), range(&ns)));
            match sets.len() {
                1 => {
                    o.picks.push(pick(AnalysisKind::ColumnStatistics, "One group: describe it (mean, SD, median, CI)."));
                    o.picks.push(pick(
                        AnalysisKind::OneSample { hypothetical: 0.0, wilcoxon: false },
                        "Test whether its mean differs from a known value.",
                    ));
                }
                2 => o.picks.push(pick(
                    AnalysisKind::TwoGroups { a: sets[0], b: sets[1], test: TwoGroupTest::Unpaired },
                    format!(
                        "Two groups ({} and {}): a t test or its nonparametric version; see the recommended test below.",
                        names[0], names[1]
                    ),
                )),
                k => o.picks.push(pick(
                    AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::Tukey), control: None },
                    format!("{k} groups: one-way ANOVA, then multiple comparisons; see the recommended test below."),
                )),
            }
        }
        TableKind::Grouped => {
            let rows: Vec<usize> = (0..t.used_rows()).filter(|&r| sets.iter().any(|&d| !t.cell_values(d, r).is_empty())).collect();
            let reps: Vec<usize> = rows.iter().flat_map(|&r| sets.iter().map(move |&d| t.cell_values(d, r).len())).collect();
            let row_names: Vec<String> = rows.iter().map(|&r| t.row_label(r)).collect();
            o.facts.push(format!(
                "Two factors: {} ({}) × {} ({}).",
                plural(rows.len(), "row", "rows"),
                short_list(&row_names),
                plural(sets.len(), "data set", "data sets"),
                short_list(&names)
            ));
            o.facts.push(format!("{} values per cell.", range(&reps)));
            let min_rep = reps.iter().copied().min().unwrap_or(0);
            if rows.len() >= 2 {
                let (method, how) = if sets.len() == 2 { (Method::Sidak, "Šídák") } else { (Method::Tukey, "Tukey") };
                let mut why = format!(
                    "Tests the effect of the rows, of the data sets and their interaction (does the {} effect change from row to row?). Then {how} compares {} within each row.",
                    short_list(&names),
                    if sets.len() == 2 { format!("{} vs. {}", names[0], names[1]) } else { "the data sets".into() }
                );
                if min_rep < 2 {
                    why.push_str(" With one value per cell the interaction cannot be tested.");
                }
                o.picks.push(pick(AnalysisKind::TwoWay { comparisons: Some(method), within_columns: false, repeated: false }, why));
                if min_rep >= 2 {
                    o.picks.push(pick(
                        AnalysisKind::TwoWay { comparisons: Some(method), within_columns: false, repeated: true },
                        "If each replicate column is the same subject measured in every row (e.g. the same mouse on each day): repeated-measures two-way ANOVA.",
                    ));
                }
            }
            if sets.len() == 2 && min_rep >= 2 {
                o.picks.push(pick(
                    AnalysisKind::MultipleT { a: sets[0], b: sets[1], welch: false, fdr: Some(true) },
                    "One t test per row, corrected for the number of rows: simpler, but ignores that the rows share an error term.",
                ));
            }
            if o.picks.is_empty() {
                o.picks.push(pick(
                    AnalysisKind::ColumnStatistics,
                    "Only one row: describe each data set, or use a Column table to compare them.",
                ));
            }
        }
        TableKind::XY => {
            let pts: Vec<(f64, f64)> = sets.iter().flat_map(|&d| t.xy_points(d)).collect();
            let xs: Vec<f64> = (0..t.rows).filter_map(|r| t.x[r]).collect();
            let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
            o.facts.push(format!(
                "{} with {} points, X from {} to {}.",
                plural(sets.len(), "data set", "data sets"),
                pts.len(),
                crate::format::num(lo),
                crate::format::num(hi)
            ));
            let first = t.xy_points(sets[0]);
            let (x, y): (Vec<f64>, Vec<f64>) = first.iter().copied().unzip();
            let line = regression::linear(&x, &y).ok();
            let sigmoid = nonlinear::fit(nonlinear::Model::DoseResponseVariableSlope, &x, &y, None).ok();
            let r2_line = line.as_ref().map_or(0.0, |l| l.r_squared);
            let r2_sig = sigmoid.as_ref().map_or(0.0, |f| f.r_squared);
            let falling = line.as_ref().is_some_and(|l| l.slope < 0.0);
            if r2_line > 0.0 {
                o.facts.push(format!(
                    "{}: a straight line explains {:.0}% of the variation{}.",
                    names[0],
                    100.0 * r2_line,
                    if sigmoid.is_some() { format!(", a sigmoid curve {:.0}%", 100.0 * r2_sig) } else { String::new() }
                ));
            }
            let curve = x.len() >= 5 && r2_sig > r2_line + 0.02 && r2_sig > 0.9;
            let model = if falling { nonlinear::Model::InhibitionVariableSlope } else { nonlinear::Model::DoseResponseVariableSlope };
            let dose = pick(
                AnalysisKind::NonlinearFit { model, fixed: Vec::new() },
                format!(
                    "The points follow an S-shaped curve: fit a four-parameter {} curve to get the {} and the Hill slope. X must be the log of the concentration.",
                    if falling { "inhibition" } else { "dose-response" },
                    if falling { "IC50" } else { "EC50" }
                ),
            );
            let linear = pick(
                AnalysisKind::LinearRegression,
                "The points lie close to a straight line: linear regression gives the slope, intercept and R².",
            );
            if curve {
                o.picks.extend([dose, linear]);
            } else {
                o.picks.extend([linear, dose]);
            }
            o.picks.push(pick(
                AnalysisKind::Correlation { spearman: false },
                "Only how strongly X and Y move together (Pearson r), without a model.",
            ));
            o.picks.push(pick(AnalysisKind::AreaUnderCurve { baseline: 0.0 }, "One number per curve, e.g. total exposure over time."));
        }
        TableKind::Survival => {
            let subjects: Vec<_> = sets.iter().map(|&d| t.survival_subjects(d)).collect();
            let n: usize = subjects.iter().map(|s| s.times.len()).sum();
            let events: usize = subjects.iter().map(|s| s.events.iter().filter(|e| **e).count()).sum();
            o.facts.push(format!(
                "{}, {} subjects: {} and {} censored.",
                plural(sets.len(), "group", "groups"),
                n,
                plural(events, "event", "events"),
                n - events
            ));
            let why = match sets.len() {
                1 => "Kaplan-Meier curve with the median survival.".to_string(),
                2 => format!("Kaplan-Meier curves, median survival and the log-rank test: does survival differ between {} and {}? Also the hazard ratio.", names[0], names[1]),
                k => format!("Kaplan-Meier curves, the log-rank test across the {k} groups, the test for trend and every pair compared (Bonferroni)."),
            };
            o.picks.push(pick(AnalysisKind::Survival { gehan: false }, why));
            if sets.len() >= 2 {
                o.picks.push(pick(
                    AnalysisKind::Survival { gehan: true },
                    "Gehan-Breslow-Wilcoxon gives more weight to early deaths: use it if the curves differ mostly at the start.",
                ));
            }
        }
        TableKind::Contingency => {
            let counts = t.counts();
            let total: f64 = counts.iter().flatten().sum();
            let (r, c) = (counts.len(), sets.len());
            o.facts.push(format!("{r} × {c} table, {} subjects.", total as u64));
            let row_sums: Vec<f64> = counts.iter().map(|row| row.iter().sum()).collect();
            let col_sums: Vec<f64> = (0..t.datasets.len()).map(|j| counts.iter().map(|row| row[j]).sum()).collect();
            let small = row_sums.iter().any(|a| col_sums.iter().any(|b| total > 0.0 && a * b / total < 5.0 && a * b > 0.0));
            let why = if r == 2 && c == 2 {
                if small {
                    "2 × 2 table with small expected counts: Fisher's exact test, with relative risk and odds ratio.".to_string()
                } else {
                    "2 × 2 table: chi-square and Fisher's exact test, relative risk and odds ratio.".to_string()
                }
            } else {
                let mut w = "Chi-square test: is the distribution across columns the same in every row?".to_string();
                if small {
                    w.push_str(" Some expected counts are below 5, so the P value is approximate.");
                }
                w
            };
            o.picks.push(pick(AnalysisKind::Contingency { yates: false }, why));
        }
        TableKind::PartsOfWhole => {
            o.facts.push(format!("{} parts.", (0..t.used_rows()).filter(|&r| !t.cell_values(sets[0], r).is_empty()).count()));
            o.picks.push(pick(AnalysisKind::FractionOfTotal, "Each value as a fraction and percentage of the total."));
        }
    }
    o
}

fn range(v: &[usize]) -> String {
    let (lo, hi) = (v.iter().copied().min().unwrap_or(0), v.iter().copied().max().unwrap_or(0));
    if lo == hi {
        lo.to_string()
    } else {
        format!("{lo}–{hi}")
    }
}

fn short_list(v: &[String]) -> String {
    if v.len() <= 3 {
        v.join(", ")
    } else {
        format!("{}, … {}", v[..2].join(", "), v[v.len() - 1])
    }
}

fn tested_name(s: &[(String, usize, Option<f64>)]) -> String {
    match s {
        [one] => format!("{}, P = {}", one.0, p(one.2.unwrap_or(f64::NAN))),
        _ => format!("each group, lowest P = {}", p(s.iter().filter_map(|x| x.2).fold(1.0, f64::min))),
    }
}

fn follow_up(test: OneWayTest, control: bool) -> Option<&'static str> {
    match test {
        OneWayTest::Ordinary | OneWayTest::RepeatedMeasures if control => Some("Dunnett's test against the control"),
        OneWayTest::Ordinary | OneWayTest::RepeatedMeasures => Some("Tukey's test of all pairs"),
        OneWayTest::Welch => Some("the Games-Howell test"),
        OneWayTest::KruskalWallis => Some("Dunn's test"),
        OneWayTest::Friedman => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAUSS: [f64; 10] = [4.8, 5.1, 5.0, 4.6, 5.3, 4.9, 5.2, 5.5, 4.7, 5.0];

    fn two(advice: Advice) -> TwoGroupTest {
        match advice.kind {
            AnalysisKind::TwoGroups { test, .. } => test,
            k => panic!("unexpected {k:?}"),
        }
    }

    #[test]
    fn picks_the_test_from_design_distribution_and_spread() {
        let shifted: Vec<f64> = GAUSS.iter().map(|v| v + 1.0).collect();
        let cur = AnalysisKind::ColumnStatistics;
        assert_eq!(two(suggest(&[GAUSS.to_vec(), shifted.clone()], &[0, 1], false, &cur).unwrap()), TwoGroupTest::Unpaired);
        assert_eq!(two(suggest(&[GAUSS.to_vec(), shifted], &[0, 1], true, &cur).unwrap()), TwoGroupTest::Paired);
        let wide: Vec<f64> = GAUSS.iter().map(|v| 10.0 * (v - 5.0) + 6.0).collect();
        assert_eq!(two(suggest(&[GAUSS.to_vec(), wide], &[0, 1], false, &cur).unwrap()), TwoGroupTest::Welch);
        let skewed = vec![1.0, 1.1, 1.2, 1.1, 1.3, 1.2, 1.0, 9.0, 15.0, 30.0];
        let advice = suggest(&[GAUSS.to_vec(), skewed], &[0, 1], false, &cur).unwrap();
        assert!(advice.checks.iter().any(|c| c.ok == Some(false)));
        assert_eq!(two(advice), TwoGroupTest::MannWhitney);
    }

    #[test]
    fn overview_fits_every_kind_of_table() {
        use crate::sample::sample_table;
        for kind in TableKind::ALL {
            let o = overview(&sample_table(kind));
            assert!(!o.facts.is_empty() && !o.picks.is_empty(), "{kind:?}");
            assert!(o.picks[0].kind.suitability(kind).is_ok(), "{kind:?}");
        }
        // The sample dose-response data are S-shaped: the curve fit comes first.
        assert!(matches!(overview(&sample_table(TableKind::XY)).picks[0].kind, AnalysisKind::NonlinearFit { .. }));
        let g = overview(&sample_table(TableKind::Grouped));
        assert!(matches!(g.picks[0].kind, AnalysisKind::TwoWay { repeated: false, .. }));
    }

    #[test]
    fn three_groups_get_a_follow_up_test() {
        let g: Vec<Vec<f64>> = (0..3).map(|i| GAUSS.iter().map(|v| v + i as f64).collect()).collect();
        let a = suggest(&g, &[0, 1, 2], false, &AnalysisKind::ColumnStatistics).unwrap();
        assert!(matches!(a.kind, AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::Tukey), .. }));
        assert!(a.reasons.iter().any(|r| r.contains("Tukey")));
    }
}
