//! Which test to use for comparing groups, and why: the decision path shown
//! in the Analyze dialog.

use crate::analysis::{AnalysisKind, OneWayTest, TwoGroupTest};
use crate::format::p;
use hg_stats::posthoc::Method;
use hg_stats::{anova, descriptive::mean, normality, outliers};

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
    fn three_groups_get_a_follow_up_test() {
        let g: Vec<Vec<f64>> = (0..3).map(|i| GAUSS.iter().map(|v| v + i as f64).collect()).collect();
        let a = suggest(&g, &[0, 1, 2], false, &AnalysisKind::ColumnStatistics).unwrap();
        assert!(matches!(a.kind, AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::Tukey), .. }));
        assert!(a.reasons.iter().any(|r| r.contains("Tukey")));
    }
}
