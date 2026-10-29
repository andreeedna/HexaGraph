//! Analyses: what to compute on a data table and how to present the results.

use crate::format::{interval, num, opt, p, yes_no};
use crate::table::{DataTable, TableKind};
use hg_stats::posthoc::{self, Method};
use hg_stats::{
    anova, contingency, descriptive, diagnostic, dist, nonlinear, nonparam, normality, outliers, p_summary, regression, survival, ttest,
    Interval,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TwoGroupTest {
    Unpaired,
    Welch,
    Paired,
    MannWhitney,
    KolmogorovSmirnov,
    Wilcoxon,
}

impl TwoGroupTest {
    pub const ALL: [TwoGroupTest; 6] = [
        TwoGroupTest::Unpaired,
        TwoGroupTest::Welch,
        TwoGroupTest::Paired,
        TwoGroupTest::MannWhitney,
        TwoGroupTest::KolmogorovSmirnov,
        TwoGroupTest::Wilcoxon,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TwoGroupTest::Unpaired => "Unpaired t test",
            TwoGroupTest::Welch => "Unpaired t test with Welch's correction",
            TwoGroupTest::Paired => "Paired t test",
            TwoGroupTest::MannWhitney => "Mann-Whitney test",
            TwoGroupTest::KolmogorovSmirnov => "Kolmogorov-Smirnov test",
            TwoGroupTest::Wilcoxon => "Wilcoxon matched-pairs signed rank test",
        }
    }

    pub fn paired(self) -> bool {
        matches!(self, TwoGroupTest::Paired | TwoGroupTest::Wilcoxon)
    }

    pub fn parametric(self) -> bool {
        matches!(self, TwoGroupTest::Unpaired | TwoGroupTest::Welch | TwoGroupTest::Paired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OneWayTest {
    Ordinary,
    Welch,
    RepeatedMeasures,
    KruskalWallis,
    Friedman,
}

impl OneWayTest {
    pub const ALL: [OneWayTest; 5] =
        [OneWayTest::Ordinary, OneWayTest::Welch, OneWayTest::RepeatedMeasures, OneWayTest::KruskalWallis, OneWayTest::Friedman];
    pub fn label(self) -> &'static str {
        match self {
            OneWayTest::Ordinary => "Ordinary one-way ANOVA",
            OneWayTest::Welch => "Welch's ANOVA",
            OneWayTest::RepeatedMeasures => "Repeated measures one-way ANOVA",
            OneWayTest::KruskalWallis => "Kruskal-Wallis test",
            OneWayTest::Friedman => "Friedman test",
        }
    }
}

/// Alternative hypothesis for tests that compare a difference with zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tails {
    #[default]
    Two,
    /// One-tailed: B is larger than A (or the mean is above the hypothetical value).
    Greater,
    /// One-tailed: B is smaller than A (or the mean is below the hypothetical value).
    Less,
}

/// Options shared by all analyses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// Significance level used for "significant?" answers.
    pub alpha: f64,
    /// Confidence level of the intervals (0.90, 0.95, 0.99…).
    pub ci_level: f64,
    pub tails: Tails,
}

impl Default for Options {
    fn default() -> Self {
        Options { alpha: 0.05, ci_level: 0.95, tails: Tails::Two }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnalysisKind {
    ColumnStatistics,
    OneSample {
        hypothetical: f64,
        wilcoxon: bool,
    },
    Normality,
    Outliers {
        alpha: f64,
    },
    TwoGroups {
        a: usize,
        b: usize,
        test: TwoGroupTest,
    },
    OneWay {
        test: OneWayTest,
        comparisons: Option<Method>,
        /// Compare every group with this one instead of all pairs.
        #[serde(default)]
        control: Option<usize>,
    },
    /// One t test per row of a grouped table, with a correction for multiple comparisons.
    MultipleT {
        a: usize,
        b: usize,
        welch: bool,
        /// None = no correction, Some(true) = false discovery rate (Benjamini-Hochberg), Some(false) = Holm-Šídák.
        fdr: Option<bool>,
    },
    TwoWay {
        comparisons: Option<Method>,
        /// Compare rows within each data set instead of data sets within each row.
        #[serde(default)]
        within_columns: bool,
        /// Each replicate column is one subject measured in every row.
        #[serde(default)]
        repeated: bool,
    },
    Correlation {
        spearman: bool,
    },
    LinearRegression,
    NonlinearFit {
        model: nonlinear::Model,
        /// Parameters held at a constant value (None = fitted).
        #[serde(default)]
        fixed: Vec<Option<f64>>,
    },
    AreaUnderCurve {
        baseline: f64,
    },
    Survival {
        gehan: bool,
    },
    Contingency {
        yates: bool,
    },
    BlandAltman {
        a: usize,
        b: usize,
    },
    Roc {
        controls: usize,
        patients: usize,
    },
    FractionOfTotal,
}

impl AnalysisKind {
    pub fn label(&self) -> String {
        match self {
            AnalysisKind::ColumnStatistics => "Descriptive statistics".into(),
            AnalysisKind::OneSample { wilcoxon, .. } => {
                if *wilcoxon {
                    "Wilcoxon signed rank test".into()
                } else {
                    "One sample t test".into()
                }
            }
            AnalysisKind::Normality => "Normality tests".into(),
            AnalysisKind::Outliers { .. } => "Identify outliers (Grubbs)".into(),
            AnalysisKind::TwoGroups { test, .. } => test.label().into(),
            AnalysisKind::OneWay { test, .. } => test.label().into(),
            AnalysisKind::TwoWay { .. } => "Two-way ANOVA".into(),
            AnalysisKind::MultipleT { .. } => "Multiple t tests (one per row)".into(),
            AnalysisKind::Correlation { spearman } => {
                if *spearman {
                    "Spearman correlation".into()
                } else {
                    "Pearson correlation".into()
                }
            }
            AnalysisKind::LinearRegression => "Simple linear regression".into(),
            AnalysisKind::NonlinearFit { model, .. } => format!("Nonlinear fit: {}", model.name()),
            AnalysisKind::AreaUnderCurve { .. } => "Area under curve".into(),
            AnalysisKind::Survival { .. } => "Survival analysis".into(),
            AnalysisKind::Contingency { .. } => "Contingency table analysis".into(),
            AnalysisKind::BlandAltman { .. } => "Bland-Altman".into(),
            AnalysisKind::Roc { .. } => "ROC curve".into(),
            AnalysisKind::FractionOfTotal => "Fraction of total".into(),
        }
    }

    /// Analyses offered for a table kind, grouped in categories, with default settings.
    pub fn catalog(kind: TableKind) -> Vec<(&'static str, Vec<AnalysisKind>)> {
        use AnalysisKind::*;
        match kind {
            TableKind::Column => vec![
                ("Compare two groups", TwoGroupTest::ALL.map(|test| TwoGroups { a: 0, b: 1, test }).to_vec()),
                (
                    "Compare three or more groups",
                    OneWayTest::ALL.map(|test| OneWay { test, comparisons: Some(Method::Tukey), control: None }).to_vec(),
                ),
                (
                    "Describe and test each column",
                    vec![
                        ColumnStatistics,
                        Normality,
                        OneSample { hypothetical: 0.0, wilcoxon: false },
                        OneSample { hypothetical: 0.0, wilcoxon: true },
                        Outliers { alpha: 0.05 },
                    ],
                ),
                (
                    "Relationships and diagnostics",
                    vec![
                        Correlation { spearman: false },
                        Correlation { spearman: true },
                        BlandAltman { a: 0, b: 1 },
                        Roc { controls: 0, patients: 1 },
                    ],
                ),
            ],
            TableKind::XY => vec![
                (
                    "Curves and regression",
                    vec![NonlinearFit { model: nonlinear::Model::DoseResponseVariableSlope, fixed: Vec::new() }, LinearRegression],
                ),
                ("Correlation", vec![Correlation { spearman: false }, Correlation { spearman: true }]),
                ("Other", vec![AreaUnderCurve { baseline: 0.0 }]),
            ],
            TableKind::Grouped => vec![
                (
                    "Grouped analyses",
                    vec![
                        TwoWay { comparisons: Some(Method::Sidak), within_columns: false, repeated: false },
                        MultipleT { a: 0, b: 1, welch: false, fdr: Some(true) },
                    ],
                ),
                ("Describe", vec![ColumnStatistics]),
            ],
            TableKind::Contingency => vec![("Contingency analyses", vec![Contingency { yates: false }])],
            TableKind::Survival => vec![("Survival analyses", vec![Survival { gehan: false }])],
            TableKind::PartsOfWhole => vec![("Parts of whole", vec![FractionOfTotal])],
        }
    }

    /// Every analysis, whatever the table: first those designed for this kind
    /// of table, then all the others (they may need adapting, see
    /// [`AnalysisKind::suitability`]).
    pub fn catalog_all(kind: TableKind) -> Vec<(String, Vec<AnalysisKind>)> {
        let mut out: Vec<(String, Vec<AnalysisKind>)> = Self::catalog(kind).into_iter().map(|(c, v)| (c.to_string(), v)).collect();
        let mut seen: Vec<String> = out.iter().flat_map(|(_, v)| v.iter().map(AnalysisKind::label)).collect();
        for other in TableKind::ALL.into_iter().filter(|k| *k != kind) {
            for (cat, items) in Self::catalog(other) {
                let fresh: Vec<AnalysisKind> = items.into_iter().filter(|k| !seen.contains(&k.label())).collect();
                if fresh.is_empty() {
                    continue;
                }
                seen.extend(fresh.iter().map(AnalysisKind::label));
                out.push((format!("{cat} ({} tables)", other.label()), fresh));
            }
        }
        out
    }

    /// The kind of table an analysis is designed for.
    pub fn native_table(&self) -> TableKind {
        TableKind::ALL
            .into_iter()
            .find(|k| Self::catalog(*k).iter().any(|(_, v)| v.iter().any(|a| std::mem::discriminant(a) == std::mem::discriminant(self))))
            .unwrap_or(TableKind::Column)
    }

    /// `Err` when the analysis cannot run on this kind of table, `Ok(Some)`
    /// with a warning when it runs on adapted data, `Ok(None)` when it fits.
    pub fn suitability(&self, kind: TableKind) -> Result<Option<String>, String> {
        use AnalysisKind::*;
        let native = self.native_table();
        if native == kind || matches!(self, ColumnStatistics) {
            return Ok(None);
        }
        match self {
            LinearRegression | NonlinearFit { .. } | AreaUnderCurve { .. } if !kind.has_x() => {
                Err("this analysis needs X values: use an XY table".into())
            }
            Survival { .. } if !kind.has_x() => Err("survival analysis needs times in X and 1/0 outcomes: use a Survival table".into()),
            Correlation { .. } => Ok(None),
            TwoWay { .. } if kind == TableKind::Column => Ok(Some("Each row is treated as a level of the second factor.".into())),
            TwoWay { .. } if kind == TableKind::XY => Ok(Some("Each X value (row) is treated as a level of the row factor.".into())),
            _ => Ok(Some(match kind {
                TableKind::Grouped => {
                    "This is a grouped table: for this test all the rows of each data set are pooled into one group.".into()
                }
                TableKind::XY => "This is an XY table: the X values are ignored and the Y values of each data set form one group.".into(),
                TableKind::Survival => "This is a survival table: the outcome codes (1/0) are analyzed as numbers.".into(),
                TableKind::Contingency => "This is a contingency table: the counts are analyzed as numbers.".into(),
                _ => format!("This analysis is designed for {} tables; check that the result makes sense.", native.label()),
            })),
        }
    }

    /// All analyses offered for a table kind.
    pub fn available(kind: TableKind) -> Vec<AnalysisKind> {
        Self::catalog(kind).into_iter().flat_map(|(_, v)| v).collect()
    }

    /// Short explanation shown in the Analyze dialog.
    pub fn help(&self) -> &'static str {
        use AnalysisKind::*;
        match self {
            ColumnStatistics => "Mean, SD, SEM, confidence interval, median, quartiles and more for each data set.",
            OneSample { wilcoxon: false, .. } => "Does the mean differ from a hypothetical value?",
            OneSample { wilcoxon: true, .. } => "Does the median differ from a hypothetical value? (nonparametric)",
            Normality => "Are the values consistent with a Gaussian distribution?",
            Outliers { .. } => "Flags values that are far from the rest (Grubbs' test).",
            TwoGroups { test, .. } => match test {
                TwoGroupTest::Unpaired => "Compare the means of two unmatched groups, assuming equal SDs.",
                TwoGroupTest::Welch => "Compare the means of two unmatched groups without assuming equal SDs.",
                TwoGroupTest::Paired => "Compare two matched measurements on the same subjects (row by row).",
                TwoGroupTest::MannWhitney => "Compare the ranks of two unmatched groups (nonparametric).",
                TwoGroupTest::KolmogorovSmirnov => "Compare the cumulative distributions of two unmatched groups.",
                TwoGroupTest::Wilcoxon => "Compare two matched groups without assuming Gaussian distributions.",
            },
            OneWay { test, .. } => match test {
                OneWayTest::Ordinary => "Compare the means of three or more unmatched groups.",
                OneWayTest::Welch => "Compare three or more means without assuming equal SDs.",
                OneWayTest::RepeatedMeasures => "Compare three or more matched measurements (each row is a subject).",
                OneWayTest::KruskalWallis => "Nonparametric comparison of three or more unmatched groups.",
                OneWayTest::Friedman => "Nonparametric comparison of three or more matched groups.",
            },
            TwoWay { .. } => "How do two factors (rows and data sets) and their interaction affect the result?",
            MultipleT { .. } => "Compare two data sets row by row (e.g. many genes or time points), correcting for the number of tests.",
            Correlation { .. } => "How strongly do two variables vary together?",
            LinearRegression => "Fit a straight line through the points.",
            NonlinearFit { .. } => "Fit a curve (dose-response, exponential, binding, Gaussian, …) to the points.",
            AreaUnderCurve { .. } => "Total area under each curve (trapezoid rule).",
            Survival { .. } => "Kaplan-Meier survival curves, median survival and a comparison of the curves.",
            Contingency { .. } => "Is the outcome associated with the group? Chi-square, Fisher's exact test, relative risk.",
            BlandAltman { .. } => "Agreement between two methods of measurement: bias and limits of agreement.",
            Roc { .. } => "How well does a test separate patients from controls?",
            FractionOfTotal => "Each value as a fraction and a percentage of the total.",
        }
    }

    /// Whether the analysis works on a chosen subset of data sets.
    pub fn uses_dataset_selection(&self) -> bool {
        use AnalysisKind::*;
        matches!(
            self,
            ColumnStatistics
                | OneSample { .. }
                | Normality
                | Outliers { .. }
                | OneWay { .. }
                | Correlation { .. }
                | LinearRegression
                | NonlinearFit { .. }
                | AreaUnderCurve { .. }
                | Survival { .. }
                | FractionOfTotal
        )
    }

    /// Whether a one-tailed P value makes sense for this analysis.
    pub fn supports_tails(&self) -> bool {
        match self {
            AnalysisKind::TwoGroups { test, .. } => *test != TwoGroupTest::KolmogorovSmirnov,
            AnalysisKind::OneSample { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub id: u64,
    pub table_id: u64,
    pub kind: AnalysisKind,
    /// Data sets to analyze (None = all of them).
    #[serde(default)]
    pub datasets: Option<Vec<usize>>,
    #[serde(default)]
    pub options: Options,
    /// Values left out of this analysis (e.g. outliers): (data set, row, replicate).
    #[serde(default)]
    pub excluded: Vec<(usize, usize, usize)>,
    /// Remarks shown with the results (e.g. the values reported by Prism when imported).
    #[serde(default)]
    pub notes: Vec<String>,
    /// Name shown in the navigator (None = "<analysis> of <table>").
    #[serde(default)]
    pub name: Option<String>,
}

impl Analysis {
    /// "Unpaired t test of GDF15", or the name the user gave.
    pub fn display_name(&self, table: &str) -> String {
        self.name.clone().unwrap_or_else(|| format!("{} of {table}", self.kind.label()))
    }
}

/// A titled grid of strings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ResultTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl ResultTable {
    fn new(title: impl Into<String>, columns: &[&str]) -> Self {
        Self { title: title.into(), columns: columns.iter().map(|s| s.to_string()).collect(), rows: Vec::new() }
    }
    fn kv(title: impl Into<String>) -> Self {
        Self::new(title, &["", "Value"])
    }
    fn row(&mut self, cells: Vec<String>) -> &mut Self {
        self.rows.push(cells);
        self
    }
    fn add(&mut self, k: &str, v: String) -> &mut Self {
        self.row(vec![k.to_string(), v])
    }

    /// Two column "label: value" table without a header row.
    pub fn is_key_value(&self) -> bool {
        self.columns.len() == 2 && self.columns[0].is_empty()
    }

    pub fn to_tsv(&self) -> String {
        let mut s = format!("{}\n{}\n", self.title, self.columns.join("\t"));
        for r in &self.rows {
            s.push_str(&r.join("\t"));
            s.push('\n');
        }
        s
    }
}

/// Curves produced by an analysis that graphs can overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub dataset: usize,
    pub points: Vec<(f64, f64)>,
    /// Drawn dashed, e.g. the confidence band of a regression line.
    #[serde(default)]
    pub dashed: bool,
}

/// One pairwise comparison, used by graphs to draw significance brackets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Significance {
    /// For grouped data: the row (comparisons within rows) or the data set
    /// (comparisons within columns) the comparison belongs to.
    pub group: Option<usize>,
    /// For grouped data, whether `group` is a data set (and `a`, `b` are rows).
    pub group_is_dataset: bool,
    pub a: usize,
    pub b: usize,
    pub p: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnalysisOutput {
    pub tables: Vec<ResultTable>,
    pub curves: Vec<Curve>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub significance: Vec<Significance>,
    /// Main effects of an ANOVA (factor name, P value), shown on graphs.
    #[serde(default)]
    pub effects: Vec<(String, f64)>,
}

impl AnalysisOutput {
    fn sig(&mut self, a: usize, b: usize, p: f64) {
        self.significance.push(Significance { group: None, group_is_dataset: false, a, b, p });
    }
}

type R<T> = Result<T, String>;

fn e<T>(r: hg_stats::Result<T>) -> R<T> {
    r.map_err(|e| e.to_string())
}

fn title(t: &DataTable, d: usize) -> String {
    t.datasets.get(d).map_or_else(|| format!("#{d}"), |d| d.title.clone())
}

fn check_ds(t: &DataTable, d: usize) -> R<()> {
    if d < t.datasets.len() {
        Ok(())
    } else {
        Err(format!("data set {} does not exist", d + 1))
    }
}

/// "95%" for 0.95.
pub fn pct(level: f64) -> String {
    format!("{}%", num(100.0 * level))
}

/// Converts a two-tailed P value to the requested alternative, given the
/// sign of the observed effect.
fn tailed(p2: f64, effect: f64, tails: Tails) -> f64 {
    let agrees = match tails {
        Tails::Two => return p2,
        Tails::Greater => effect >= 0.0,
        Tails::Less => effect <= 0.0,
    };
    if agrees {
        p2 / 2.0
    } else {
        1.0 - p2 / 2.0
    }
}

fn tails_label(tails: Tails, a: &str, b: &str) -> String {
    match tails {
        Tails::Two => "Two-tailed".into(),
        Tails::Greater => format!("One-tailed ({b} > {a})"),
        Tails::Less => format!("One-tailed ({b} < {a})"),
    }
}

fn t_interval(center: f64, se: f64, df: f64, level: f64) -> Interval {
    let t = dist::t_crit(level, df);
    Interval::new(center - t * se, center + t * se)
}

/// Runs an analysis. `selection` limits it to some data sets (None = all).
pub fn run(kind: &AnalysisKind, t: &DataTable, selection: Option<&[usize]>, o: &Options) -> R<AnalysisOutput> {
    let mut out = AnalysisOutput::default();
    if let Some(w) = kind.suitability(t.kind)? {
        out.warnings.push(w);
    }
    let groups: Vec<usize> = t.nonempty_datasets().into_iter().filter(|d| selection.is_none_or(|s| s.contains(d))).collect();
    let level = o.ci_level;
    let ci = pct(level);
    match kind {
        AnalysisKind::ColumnStatistics => {
            let mut cols = vec![""];
            let names: Vec<String> = groups.iter().map(|&d| title(t, d)).collect();
            cols.extend(names.iter().map(String::as_str));
            let mut rt = ResultTable::new("Descriptive statistics", &cols);
            let stats: Vec<descriptive::Descriptive> =
                groups.iter().map(|&d| e(descriptive::describe(&t.column_values(d)))).collect::<R<_>>()?;
            let mean_ci = |d: &descriptive::Descriptive| (d.n > 1).then(|| t_interval(d.mean, d.sem, d.n as f64 - 1.0, level));
            let lower = format!("Lower {ci} CI of mean");
            let upper = format!("Upper {ci} CI of mean");
            type Row<'a> = (&'a str, Box<dyn Fn(&descriptive::Descriptive) -> String + 'a>);
            let rows: Vec<Row> = vec![
                ("Number of values", Box::new(|d| d.n.to_string())),
                ("Minimum", Box::new(|d| num(d.min))),
                ("25% percentile", Box::new(|d| num(d.q1))),
                ("Median", Box::new(|d| num(d.median))),
                ("75% percentile", Box::new(|d| num(d.q3))),
                ("Maximum", Box::new(|d| num(d.max))),
                ("Range", Box::new(|d| num(d.range))),
                ("Mean", Box::new(|d| num(d.mean))),
                ("Std. deviation", Box::new(|d| num(d.sd))),
                ("Std. error of mean", Box::new(|d| num(d.sem))),
                (&lower, Box::new(move |d| opt(mean_ci(d).map(|i| i.low)))),
                (&upper, Box::new(move |d| opt(mean_ci(d).map(|i| i.high)))),
                ("Coefficient of variation", Box::new(|d| format!("{}%", num(d.cv_percent)))),
                ("Geometric mean", Box::new(|d| opt(d.geometric_mean))),
                ("Geometric SD factor", Box::new(|d| opt(d.geometric_sd_factor))),
                ("Harmonic mean", Box::new(|d| opt(d.harmonic_mean))),
                ("Skewness", Box::new(|d| opt(d.skewness))),
                ("Kurtosis", Box::new(|d| opt(d.kurtosis))),
                ("Sum", Box::new(|d| num(d.sum))),
                ("Variance", Box::new(|d| num(d.variance))),
            ];
            for (label, f) in &rows {
                let mut r = vec![label.to_string()];
                r.extend(stats.iter().map(f));
                rt.row(r);
            }
            out.tables.push(rt);
        }
        AnalysisKind::OneSample { hypothetical, wilcoxon } => {
            let mut rt = ResultTable::new(
                kind.label(),
                &["Data set", "Statistic", "P value", "Summary", "Significant?", "Discrepancy", &format!("{ci} CI")],
            );
            for &d in &groups {
                let v = t.column_values(d);
                if *wilcoxon {
                    let r = e(nonparam::wilcoxon_signed_rank(&v, *hypothetical))?;
                    let pv = tailed(r.p, r.w, o.tails);
                    rt.row(vec![
                        title(t, d),
                        format!("W = {}", num(r.w)),
                        p(pv),
                        p_summary(pv).into(),
                        yes_no(pv < o.alpha),
                        num(r.median_difference),
                        "—".into(),
                    ]);
                } else {
                    let r = e(ttest::one_sample(&v, *hypothetical))?;
                    let pv = tailed(r.p, r.difference, o.tails);
                    rt.row(vec![
                        title(t, d),
                        format!("t = {}, df = {}", num(r.t), num(r.df)),
                        p(pv),
                        p_summary(pv).into(),
                        yes_no(pv < o.alpha),
                        num(r.difference),
                        interval(t_interval(r.difference, r.se_difference, r.df, level)),
                    ]);
                }
            }
            out.tables.push(rt);
            if o.tails != Tails::Two {
                out.warnings.push(format!(
                    "One-tailed P values: {}.",
                    if o.tails == Tails::Greater { "mean above the hypothetical value" } else { "mean below the hypothetical value" }
                ));
            }
        }
        AnalysisKind::Normality => {
            let mut rt = ResultTable::new(
                "Normality tests",
                &["Data set", "Test", "Statistic", "P value", &format!("Passed (α = {})?", num(o.alpha))],
            );
            for &d in &groups {
                let v = t.column_values(d);
                let tests: [(&str, hg_stats::Result<(f64, f64)>); 4] = [
                    ("D'Agostino-Pearson (K2)", normality::dagostino_pearson(&v).map(|r| (r.k2, r.p))),
                    ("Anderson-Darling (A2*)", normality::anderson_darling(&v).map(|r| (r.statistic, r.p))),
                    ("Shapiro-Wilk (W)", normality::shapiro_wilk(&v).map(|r| (r.statistic, r.p))),
                    ("Kolmogorov-Smirnov (KS distance)", normality::kolmogorov_smirnov(&v).map(|r| (r.statistic, r.p))),
                ];
                for (name, r) in tests {
                    match r {
                        Ok((s, pv)) => rt.row(vec![title(t, d), name.into(), num(s), p(pv), yes_no(pv >= o.alpha)]),
                        Err(err) => rt.row(vec![title(t, d), name.into(), "—".into(), "—".into(), err.to_string()]),
                    };
                }
            }
            out.tables.push(rt);
        }
        AnalysisKind::Outliers { alpha } => {
            let mut rt = ResultTable::new(format!("Grubbs' test, α = {alpha}"), &["Data set", "Outliers found", "Outlier values"]);
            for &d in &groups {
                let v = t.column_values(d);
                let g = e(outliers::grubbs(&v, *alpha))?;
                let vals: Vec<String> = g.outliers.iter().map(|&i| num(v[i])).collect();
                rt.row(vec![title(t, d), g.outliers.len().to_string(), vals.join(", ")]);
            }
            out.tables.push(rt);
        }
        AnalysisKind::TwoGroups { a, b, test } => {
            check_ds(t, *a)?;
            check_ds(t, *b)?;
            if a == b {
                return Err("choose two different data sets".into());
            }
            let (na, nb) = (title(t, *a), title(t, *b));
            let (va, vb) = (t.column_values(*a), t.column_values(*b));
            let mut rt = ResultTable::kv(format!("{}: {na} vs. {nb}", test.label()));
            let tails = if *test == TwoGroupTest::KolmogorovSmirnov { Tails::Two } else { o.tails };
            let summary = |rt: &mut ResultTable, out: &mut AnalysisOutput, pv: f64| {
                out.sig(*a, *b, pv);
                rt.add("P value", p(pv)).add("P value summary", p_summary(pv).into());
                rt.add(&format!("Significantly different (P < {})?", num(o.alpha)), yes_no(pv < o.alpha));
                rt.add("One- or two-tailed P value?", tails_label(tails, &na, &nb));
            };
            match test {
                TwoGroupTest::Unpaired | TwoGroupTest::Welch | TwoGroupTest::Paired => {
                    let r = if *test == TwoGroupTest::Paired {
                        let rows = t.complete_rows(&[*a, *b]);
                        let (pa, pb): (Vec<f64>, Vec<f64>) = rows.iter().map(|r| (r[0], r[1])).unzip();
                        e(ttest::paired(&pa, &pb))?
                    } else {
                        e(ttest::unpaired(&va, &vb, *test == TwoGroupTest::Welch))?
                    };
                    let pv = tailed(r.p, r.difference, tails);
                    summary(&mut rt, &mut out, pv);
                    // Like Prism, t is reported as a positive number; the sign is in the difference.
                    rt.add("t, df", format!("t = {}, df = {}", num(r.t.abs()), num(r.df)));
                    rt.add(&format!("Mean of {na}"), num(r.mean_a)).add(&format!("Mean of {nb}"), num(r.mean_b));
                    rt.add(&format!("Difference ({nb} − {na})"), num(r.difference));
                    rt.add("SE of difference", num(r.se_difference));
                    rt.add(&format!("{ci} confidence interval"), interval(t_interval(r.difference, r.se_difference, r.df, level)));
                    rt.add("R squared (eta squared)", num(r.r_squared));
                    if let Some(f) = r.f_test {
                        rt.add(
                            "F test to compare variances",
                            format!("F = {}, DFn = {}, DFd = {}", num(f.f), num(f.df_num), num(f.df_den)),
                        );
                        rt.add("F test P value", p(f.p));
                        if f.p < o.alpha && *test == TwoGroupTest::Unpaired {
                            out.warnings.push("The variances differ significantly; consider Welch's correction.".into());
                        }
                    }
                    if *test == TwoGroupTest::Paired {
                        rt.add("Number of pairs", t.complete_rows(&[*a, *b]).len().to_string());
                    }
                }
                TwoGroupTest::MannWhitney => {
                    let r = e(nonparam::mann_whitney(&va, &vb))?;
                    let pv = tailed(r.p, r.hodges_lehmann, tails);
                    summary(&mut rt, &mut out, pv);
                    rt.add("Exact or approximate P value?", if r.exact { "Exact".into() } else { "Approximate".into() });
                    rt.add("Sum of ranks in A, B", format!("{}, {}", num(r.sum_ranks_a), num(r.sum_ranks_b)));
                    rt.add("Mann-Whitney U", num(r.u.min(va.len() as f64 * vb.len() as f64 - r.u)));
                    rt.add(&format!("Median of {na}"), num(r.median_a)).add(&format!("Median of {nb}"), num(r.median_b));
                    rt.add("Hodges-Lehmann difference", num(r.hodges_lehmann));
                }
                TwoGroupTest::KolmogorovSmirnov => {
                    let (d, pv) = e(nonparam::kolmogorov_smirnov_2(&va, &vb))?;
                    summary(&mut rt, &mut out, pv);
                    rt.add("Kolmogorov-Smirnov D", num(d));
                    if o.tails != Tails::Two {
                        out.warnings.push("The Kolmogorov-Smirnov test is always two-tailed.".into());
                    }
                }
                TwoGroupTest::Wilcoxon => {
                    let rows = t.complete_rows(&[*a, *b]);
                    let (pa, pb): (Vec<f64>, Vec<f64>) = rows.iter().map(|r| (r[0], r[1])).unzip();
                    let r = e(nonparam::wilcoxon_paired(&pa, &pb))?;
                    let pv = tailed(r.p, r.w, tails);
                    summary(&mut rt, &mut out, pv);
                    rt.add("Exact or approximate P value?", if r.exact { "Exact".into() } else { "Approximate".into() });
                    rt.add("Sum of positive, negative ranks", format!("{}, {}", num(r.sum_positive), num(r.sum_negative)));
                    rt.add("Sum of signed ranks (W)", num(r.w)).add("Median of differences", num(r.median_difference));
                    rt.add("Number of pairs", r.n_used.to_string());
                    if r.n_zero > 0 {
                        out.warnings.push(format!("{} pairs with zero difference were excluded.", r.n_zero));
                    }
                }
            }
            out.tables.push(rt);
        }
        AnalysisKind::OneWay { test, comparisons, control } => run_one_way(t, &groups, *test, *comparisons, *control, o, &mut out)?,
        AnalysisKind::TwoWay { comparisons, within_columns, repeated } => {
            // Data sets without values are ignored; rows where a data set has no
            // value cannot enter the model and are left out with a note.
            let sets = groups.clone();
            if sets.len() < 2 {
                return Err("two-way ANOVA needs at least two data sets with values".into());
            }
            let candidate: Vec<usize> =
                (0..t.used_rows()).filter(|&row| sets.iter().any(|&d| !t.cell_values(d, row).is_empty())).collect();
            let (used_rows, incomplete): (Vec<usize>, Vec<usize>) =
                candidate.into_iter().partition(|&row| sets.iter().all(|&d| !t.cell_values(d, row).is_empty()));
            if !incomplete.is_empty() {
                let names: Vec<String> = incomplete.iter().map(|&r| t.row_label(r)).collect();
                out.warnings.push(format!(
                    "Rows where some data set has no value were left out of the ANOVA: {}.",
                    names.join(", ")
                ));
            }
            if used_rows.len() < 2 {
                return Err("two-way ANOVA needs at least two rows with a value in every data set".into());
            }
            // (cell means, cell ns, MS and df for comparing data sets, MS and df for comparing rows)
            let (cell_means, cell_ns, between, within) = if *repeated {
                let mut dropped = 0;
                let groups: Vec<Vec<Vec<f64>>> = sets
                    .iter()
                    .map(|&d| {
                        (0..t.replicates)
                            .filter_map(|k| {
                                let s: Vec<Option<f64>> = used_rows
                                    .iter()
                                    .map(|&row| t.get(row, crate::table::GridColumn::Value { dataset: d, replicate: k }).filter(|v| v.is_finite()))
                                    .collect();
                                if s.iter().all(Option::is_none) {
                                    None
                                } else if s.iter().any(Option::is_none) {
                                    dropped += 1;
                                    None
                                } else {
                                    Some(s.into_iter().flatten().collect())
                                }
                            })
                            .collect()
                    })
                    .collect();
                if dropped > 0 {
                    out.warnings.push(format!(
                        "{dropped} subjects (replicate columns) with missing values were left out: repeated measures need every row."
                    ));
                }
                let r = e(anova::two_way_repeated(&groups))?;
                out.tables.push(anova_table("Two-way repeated measures ANOVA (matching across rows)", &r.sources, o.alpha));
                out.effects = effects(t, &r.sources);
                out.warnings.push(format!(
                    "Geisser-Greenhouse correction applied to the row factor and the interaction (ε = {}); {} subjects.",
                    num(r.epsilon),
                    r.n_subjects
                ));
                let src = |name: &str| r.sources.iter().find(|s| s.name == name).cloned();
                let (subj, res) = (src("Subjects (matching)"), src("Residual"));
                let (ss_s, df_s) = subj.map_or((0.0, 0.0), |s| (s.ss, s.df));
                let (ss_r, df_r) = res.map_or((0.0, 0.0), |s| (s.ss, s.df));
                let ns: Vec<Vec<f64>> = used_rows.iter().map(|_| groups.iter().map(|g| g.len() as f64).collect()).collect();
                // Data sets within a row: the pooled within-cell error.
                ((r.cell_means), ns, ((ss_s + ss_r) / (df_s + df_r), df_s + df_r), (r.ms_residual, r.df_residual))
            } else {
                let cells: Vec<Vec<Vec<f64>>> =
                    used_rows.iter().map(|&row| sets.iter().map(|&d| t.cell_values(d, row)).collect()).collect();
                let r = e(anova::two_way(&cells))?;
                out.tables.push(anova_table("Two-way ANOVA (Type III sums of squares)", &r.sources, o.alpha));
                out.effects = effects(t, &r.sources);
                if !r.interaction_fitted {
                    out.warnings.push("Only one value per cell: the interaction cannot be tested (additive model).".into());
                }
                (r.cell_means, r.cell_ns, (r.ms_residual, r.df_residual), (r.ms_residual, r.df_residual))
            };
            if let Some(method) = comparisons {
                let (ms_err, df_err) = if *within_columns { within } else { between };
                // One family per group: (label, group index in the table, means, ns).
                let families: Vec<(String, usize, Vec<f64>, Vec<f64>)> = if *within_columns {
                    sets.iter()
                        .enumerate()
                        .map(|(si, &d)| {
                            (title(t, d), d, cell_means.iter().map(|row| row[si]).collect(), cell_ns.iter().map(|row| row[si]).collect())
                        })
                        .collect()
                } else {
                    used_rows
                        .iter()
                        .enumerate()
                        .map(|(ri, &row)| (t.row_label(row), row, cell_means[ri].clone(), cell_ns[ri].clone()))
                        .collect()
                };
                let member_name = |i: usize| if *within_columns { t.row_label(used_rows[i]) } else { title(t, sets[i]) };
                let member_index = |i: usize| if *within_columns { used_rows[i] } else { sets[i] };
                let k = families.first().map_or(0, |f| f.2.len());
                let pairs = posthoc::all_pairs(k);
                let family = (pairs.len() * families.len()) as f64;
                let heading = if *within_columns { "Compare rows within each data set" } else { "Compare data sets within each row" };
                let mut rt = ResultTable::new(
                    format!("{heading} ({})", method.label()),
                    &["Group", "Comparison", "Mean diff.", &format!("{ci} CI"), "Adjusted P", "Summary"],
                );
                for (label, gidx, means, ns) in &families {
                    let mut res = posthoc::compare_means(means, ns, ms_err, df_err, &pairs, *method, k, level);
                    if matches!(method, Method::Bonferroni | Method::Sidak) {
                        // Re-adjust for the whole family of comparisons.
                        let raw = posthoc::compare_means(means, ns, ms_err, df_err, &pairs, Method::FisherLsd, k, level);
                        for (c, raw) in res.iter_mut().zip(raw) {
                            c.p_adjusted = match method {
                                Method::Bonferroni => (raw.p_adjusted * family).clamp(0.0, 1.0),
                                _ => 1.0 - (1.0 - raw.p_adjusted).powf(family),
                            };
                        }
                    }
                    for c in res {
                        out.significance.push(Significance {
                            group: Some(*gidx),
                            group_is_dataset: *within_columns,
                            a: member_index(c.i),
                            b: member_index(c.j),
                            p: c.p_adjusted,
                        });
                        rt.row(vec![
                            label.clone(),
                            format!("{} vs. {}", member_name(c.i), member_name(c.j)),
                            num(c.difference),
                            c.ci95.map_or("—".into(), interval),
                            p(c.p_adjusted),
                            p_summary(c.p_adjusted).into(),
                        ]);
                    }
                }
                out.tables.push(rt);
            }
        }
        AnalysisKind::MultipleT { a, b, welch, fdr } => {
            check_ds(t, *a)?;
            check_ds(t, *b)?;
            let (na, nb) = (title(t, *a), title(t, *b));
            let rows: Vec<usize> =
                (0..t.used_rows()).filter(|&r| t.cell_values(*a, r).len() >= 2 && t.cell_values(*b, r).len() >= 2).collect();
            if rows.is_empty() {
                return Err("each row needs at least two values in both data sets (use replicates)".into());
            }
            let tests: Vec<ttest::TTest> =
                rows.iter().map(|&r| e(ttest::unpaired(&t.cell_values(*a, r), &t.cell_values(*b, r), *welch))).collect::<R<_>>()?;
            let raw: Vec<f64> = tests.iter().map(|x| x.p).collect();
            let adj = match fdr {
                Some(true) => posthoc::benjamini_hochberg(&raw),
                Some(false) => posthoc::holm_sidak(&raw),
                None => raw.clone(),
            };
            let adj_name = match fdr {
                Some(true) => "q value (FDR)",
                Some(false) => "Adjusted P (Holm-Šídák)",
                None => "P value",
            };
            let mut rt = ResultTable::new(
                format!("{}: {nb} vs. {na}, row by row", if *welch { "Welch t tests" } else { "Unpaired t tests" }),
                &["Row", &format!("Mean {na}"), &format!("Mean {nb}"), "Difference", "t", "P value", adj_name, "Summary", "Discovery?"],
            );
            for ((&r, x), &q) in rows.iter().zip(&tests).zip(&adj) {
                out.significance.push(Significance { group: Some(r), group_is_dataset: false, a: *a, b: *b, p: q });
                rt.row(vec![
                    t.row_label(r),
                    num(x.mean_a),
                    num(x.mean_b),
                    num(x.difference),
                    num(x.t),
                    p(x.p),
                    p(q),
                    p_summary(q).into(),
                    yes_no(q < o.alpha),
                ]);
            }
            let found = adj.iter().filter(|q| **q < o.alpha).count();
            out.warnings.push(format!("{found} of {} rows are significant after correction (threshold {}).", rows.len(), num(o.alpha)));
            out.tables.push(rt);
        }
        AnalysisKind::Correlation { spearman } => {
            let mut rt = ResultTable::new(kind.label(), &["Pair", "r", &format!("{ci} CI"), "R squared", "P value", "Summary", "n"]);
            let pairs: Vec<(String, Vec<f64>, Vec<f64>)> = if t.kind == TableKind::XY {
                groups
                    .iter()
                    .map(|&d| {
                        let (x, y): (Vec<f64>, Vec<f64>) = t.xy_points(d).into_iter().unzip();
                        (format!("X vs. {}", title(t, d)), x, y)
                    })
                    .collect()
            } else {
                posthoc::all_pairs(groups.len())
                    .into_iter()
                    .map(|(i, j)| {
                        let rows = t.complete_rows(&[groups[i], groups[j]]);
                        let (x, y): (Vec<f64>, Vec<f64>) = rows.iter().map(|r| (r[0], r[1])).unzip();
                        (format!("{} vs. {}", title(t, groups[i]), title(t, groups[j])), x, y)
                    })
                    .collect()
            };
            let z = dist::norm_inv(0.5 + level / 2.0);
            for (name, x, y) in pairs {
                let c = e(if *spearman { regression::spearman(&x, &y) } else { regression::pearson(&x, &y) })?;
                let n = c.n as f64;
                let ci_r = (c.n > 3 && c.r.abs() < 1.0).then(|| {
                    let zr = c.r.atanh();
                    let se = if *spearman { ((1.0 + c.r * c.r / 2.0) / (n - 3.0)).sqrt() } else { 1.0 / (n - 3.0).sqrt() };
                    Interval::new((zr - z * se).tanh(), (zr + z * se).tanh())
                });
                rt.row(vec![
                    name,
                    num(c.r),
                    ci_r.map_or("—".into(), interval),
                    num(c.r_squared),
                    p(c.p),
                    p_summary(c.p).into(),
                    c.n.to_string(),
                ]);
            }
            out.tables.push(rt);
        }
        AnalysisKind::LinearRegression => {
            for &d in &groups {
                let (x, y): (Vec<f64>, Vec<f64>) = t.xy_points(d).into_iter().unzip();
                let f = e(regression::linear(&x, &y))?;
                let mut rt = ResultTable::kv(format!("Linear regression: {}", title(t, d)));
                rt.add("Slope", format!("{} ± {}", num(f.slope), num(f.se_slope)))
                    .add("Y-intercept", format!("{} ± {}", num(f.intercept), num(f.se_intercept)));
                rt.add("X-intercept", num(f.x_intercept));
                rt.add(&format!("Slope {ci} CI"), interval(t_interval(f.slope, f.se_slope, f.df, level)));
                rt.add(&format!("Y-intercept {ci} CI"), interval(t_interval(f.intercept, f.se_intercept, f.df, level)));
                rt.add("R squared", num(f.r_squared))
                    .add("Sy.x", num(f.sy_x))
                    .add("F, DFn, DFd", format!("{}, 1, {}", num(f.f), num(f.df)));
                rt.add("P value (slope ≠ 0)", p(f.p))
                    .add("Deviation from zero?", if f.p < o.alpha { "Significant".into() } else { "Not significant".into() });
                rt.add("Equation", format!("Y = {}*X + {}", num(f.slope), num(f.intercept)));
                out.tables.push(rt);
                out.curves.push(Curve { dataset: d, points: sample_curve(&x, |v| f.predict(v)), dashed: false });
                for sign in [-1.0, 1.0] {
                    let band = |v: f64| f.predict(v) + sign * f.confidence_half_width(v, level);
                    out.curves.push(Curve { dataset: d, points: sample_curve(&x, band), dashed: true });
                }
            }
        }
        AnalysisKind::NonlinearFit { model, fixed } => {
            for &d in &groups {
                let (x, y): (Vec<f64>, Vec<f64>) = t.xy_points(d).into_iter().unzip();
                match nonlinear::fit_constrained(*model, &x, &y, None, fixed) {
                    Ok(f) => {
                        let mut rt = ResultTable::new(
                            format!("{} — {}", model.name(), title(t, d)),
                            &["Parameter", "Best-fit value", "Std. error", &format!("{ci} CI")],
                        );
                        let cis: Vec<Interval> =
                            f.parameters.iter().zip(&f.std_errors).map(|(v, s)| t_interval(*v, *s, f.df, level)).collect();
                        for (i, name) in model.parameters().iter().enumerate() {
                            let is_fixed = fixed.get(i).copied().flatten().is_some();
                            rt.row(vec![
                                name.to_string(),
                                if is_fixed { format!("= {} (constant)", num(f.parameters[i])) } else { num(f.parameters[i]) },
                                if is_fixed { "—".into() } else { num(f.std_errors[i]) },
                                if is_fixed { "—".into() } else { interval(cis[i]) },
                            ]);
                        }
                        let pow10 = |i: Interval| interval(Interval::new(10f64.powf(i.low), 10f64.powf(i.high)));
                        match model {
                            nonlinear::Model::DoseResponseVariableSlope | nonlinear::Model::DoseResponseStandardSlope => {
                                rt.row(vec!["EC50".into(), num(10f64.powf(f.parameters[2])), "—".into(), pow10(cis[2])]);
                            }
                            nonlinear::Model::InhibitionVariableSlope => {
                                rt.row(vec!["IC50".into(), num(10f64.powf(f.parameters[2])), "—".into(), pow10(cis[2])]);
                            }
                            nonlinear::Model::OnePhaseDecay | nonlinear::Model::OnePhaseAssociation => {
                                rt.row(vec!["Half-life".into(), num(std::f64::consts::LN_2 / f.parameters[2]), "—".into(), "—".into()]);
                                rt.row(vec!["Tau".into(), num(1.0 / f.parameters[2]), "—".into(), "—".into()]);
                            }
                            nonlinear::Model::ExponentialGrowth => {
                                rt.row(vec!["Doubling time".into(), num(std::f64::consts::LN_2 / f.parameters[1]), "—".into(), "—".into()]);
                            }
                            _ => {}
                        }
                        out.tables.push(rt);
                        let mut gof = ResultTable::kv(format!("Goodness of fit — {}", title(t, d)));
                        gof.add("Equation", model.equation().into())
                            .add("Degrees of freedom", num(f.df))
                            .add("R squared", num(f.r_squared));
                        gof.add("Sum of squares", num(f.ss)).add("Sy.x", num(f.sy_x)).add("Number of points analyzed", f.n.to_string());
                        out.tables.push(gof);
                        out.curves.push(Curve { dataset: d, points: sample_curve(&x, |v| f.predict(v)), dashed: false });
                    }
                    Err(err) => out.warnings.push(format!("{}: {}", title(t, d), err)),
                }
            }
        }
        AnalysisKind::AreaUnderCurve { baseline } => {
            let mut rt = ResultTable::new(
                format!("Area under curve (baseline Y = {})", num(*baseline)),
                &["Data set", "Total area", "Number of points"],
            );
            for &d in &groups {
                let mut pts: Vec<(f64, f64)> = (0..t.rows)
                    .filter_map(|r| {
                        let v = t.cell_values(d, r);
                        Some((t.x[r]?, v)).filter(|(_, v)| !v.is_empty()).map(|(x, v)| (x, descriptive::mean(&v)))
                    })
                    .collect();
                pts.sort_by(|a, b| a.0.total_cmp(&b.0));
                let (x, y): (Vec<f64>, Vec<f64>) = pts.iter().copied().unzip();
                rt.row(vec![title(t, d), num(descriptive::area_under_curve(&x, &y, *baseline)), x.len().to_string()]);
            }
            out.tables.push(rt);
        }
        AnalysisKind::Survival { gehan } => {
            let subjects: Vec<survival::Subjects> = groups.iter().map(|&d| t.survival_subjects(d)).collect();
            let mut med = ResultTable::new("Survival summary", &["Group", "Subjects", "Events", "Median survival"]);
            for (s, &d) in subjects.iter().zip(&groups) {
                let km = e(survival::kaplan_meier(s))?;
                med.row(vec![title(t, d), km.n.to_string(), km.n_events.to_string(), km.median.map_or("Undefined".into(), num)]);
                let mut rt = ResultTable::new(
                    format!("Kaplan-Meier: {}", title(t, d)),
                    &["Time", "At risk", "Events", "Censored", "% survival", "SE (%)", &format!("{ci} CI (%)")],
                );
                for st in &km.steps {
                    let cis = survival::KaplanMeier::ci(st, level)
                        .map_or("—".into(), |i| format!("{} to {}", num(100.0 * i.low), num(100.0 * i.high)));
                    rt.row(vec![
                        num(st.time),
                        st.at_risk.to_string(),
                        st.events.to_string(),
                        st.censored.to_string(),
                        num(100.0 * st.survival),
                        num(100.0 * st.se),
                        cis,
                    ]);
                }
                out.tables.push(rt);
            }
            out.tables.insert(0, med);
            if subjects.len() >= 2 {
                let w = if *gehan { survival::Weighting::GehanBreslow } else { survival::Weighting::LogRank };
                let c = e(survival::compare(&subjects, w))?;
                let mut rt = ResultTable::kv(if *gehan { "Gehan-Breslow-Wilcoxon test" } else { "Log-rank (Mantel-Cox) test" });
                rt.add("Chi square", num(c.chi2)).add("df", num(c.df)).add("P value", p(c.p)).add("P value summary", p_summary(c.p).into());
                rt.add("Are the survival curves significantly different?", yes_no(c.p < o.alpha));
                if let Some((chi, pv)) = c.trend {
                    rt.add("Log-rank test for trend: chi square", num(chi)).add("Test for trend: P value", p(pv));
                }
                if c.observed.len() == 2 && c.expected.iter().chain(&c.observed).all(|v| *v > 0.0) {
                    let hr = (c.observed[0] / c.expected[0]) / (c.observed[1] / c.expected[1]);
                    let se = (1.0 / c.expected[0] + 1.0 / c.expected[1]).sqrt();
                    let z = dist::norm_inv(0.5 + level / 2.0);
                    rt.add("Hazard ratio (log-rank method)", num(hr));
                    rt.add(&format!("{ci} CI of ratio"), interval(Interval::new((hr.ln() - z * se).exp(), (hr.ln() + z * se).exp())));
                }
                out.tables.insert(1, rt);
            }
        }
        AnalysisKind::Contingency { yates } => {
            let counts = t.counts();
            let c = e(contingency::chi_square(&counts, *yates))?;
            let mut rt = ResultTable::kv(if c.yates { "Chi-square test with Yates' correction" } else { "Chi-square test" });
            rt.add("Chi-square, df", format!("{}, {}", num(c.chi2), num(c.df)))
                .add("P value", p(c.p))
                .add("P value summary", p_summary(c.p).into());
            rt.add(&format!("Significant association (P < {})?", num(o.alpha)), yes_no(c.p < o.alpha));
            if c.expected.iter().flatten().any(|&v| v < 5.0) {
                out.warnings.push("Some expected counts are below 5; the chi-square approximation may be inaccurate.".into());
            }
            out.tables.push(rt);
            if counts.len() == 2 && counts[0].len() == 2 {
                let u = |v: f64| v.round().max(0.0) as u64;
                let tb = e(contingency::two_by_two(u(counts[0][0]), u(counts[0][1]), u(counts[1][0]), u(counts[1][1]), level))?;
                let mut rt = ResultTable::kv("2×2 table");
                rt.add("Fisher's exact test P value", p(tb.fisher_p));
                if let Some((v, i)) = tb.relative_risk {
                    rt.add("Relative risk", num(v)).add(&format!("Relative risk {ci} CI (Katz)"), interval(i));
                }
                if let Some((v, i)) = tb.odds_ratio {
                    rt.add("Odds ratio", num(v)).add(&format!("Odds ratio {ci} CI (Woolf)"), interval(i));
                }
                rt.add("Difference of proportions", num(tb.attributable_risk.0))
                    .add(&format!("Difference {ci} CI"), interval(tb.attributable_risk.1));
                out.tables.push(rt);
            } else if counts.len() >= 3 && counts[0].len() == 2 {
                let (chi, pv) = e(contingency::chi_square_trend(&counts))?;
                let mut rt = ResultTable::kv("Chi-square test for trend");
                rt.add("Chi-square, df", format!("{}, 1", num(chi))).add("P value", p(pv));
                out.tables.push(rt);
            }
        }
        AnalysisKind::BlandAltman { a, b } => {
            check_ds(t, *a)?;
            check_ds(t, *b)?;
            let rows = t.complete_rows(&[*a, *b]);
            let (pa, pb): (Vec<f64>, Vec<f64>) = rows.iter().map(|r| (r[0], r[1])).unzip();
            let r = e(diagnostic::bland_altman(&pa, &pb))?;
            let mut rt = ResultTable::kv(format!("Bland-Altman: {} vs. {}", title(t, *a), title(t, *b)));
            rt.add("Bias", num(r.bias))
                .add("SD of bias", num(r.sd_difference))
                .add("95% limits of agreement", interval(r.limits_of_agreement));
            out.tables.push(rt);
        }
        AnalysisKind::Roc { controls, patients } => {
            check_ds(t, *controls)?;
            check_ds(t, *patients)?;
            let r = e(diagnostic::roc(&t.column_values(*controls), &t.column_values(*patients)))?;
            let z = dist::norm_inv(0.5 + level / 2.0);
            let mut rt = ResultTable::kv(format!("ROC curve: {} (controls) vs. {} (patients)", title(t, *controls), title(t, *patients)));
            rt.add("Area under the ROC curve", num(r.auc)).add("Std. error", num(r.se));
            rt.add(&format!("{ci} CI"), interval(Interval::new((r.auc - z * r.se).max(0.0), (r.auc + z * r.se).min(1.0))))
                .add("P value", p(r.p));
            out.tables.push(rt);
            let mut pts =
                ResultTable::new("Sensitivity and specificity", &["Cutoff", "Sensitivity %", "Specificity %", "Likelihood ratio"]);
            for pt in &r.points[1..r.points.len() - 1] {
                let lr = pt.sensitivity / (1.0 - pt.specificity);
                pts.row(vec![format!("> {}", num(pt.cutoff)), num(100.0 * pt.sensitivity), num(100.0 * pt.specificity), num(lr)]);
            }
            out.tables.push(pts);
            out.curves.push(Curve {
                dataset: 0,
                points: r.points.iter().map(|p| (100.0 * (1.0 - p.specificity), 100.0 * p.sensitivity)).rev().collect(),
                dashed: false,
            });
        }
        AnalysisKind::FractionOfTotal => {
            for &d in &groups {
                let vals: Vec<(usize, f64)> = (0..t.used_rows()).filter_map(|r| t.datasets[d].values[r][0].map(|v| (r, v))).collect();
                let total: f64 = vals.iter().map(|v| v.1).sum();
                let mut rt = ResultTable::new(format!("Fraction of total: {}", title(t, d)), &["Row", "Value", "Fraction", "Percent"]);
                for (r, v) in vals {
                    rt.row(vec![t.row_label(r), num(v), num(v / total), format!("{}%", num(100.0 * v / total))]);
                }
                rt.row(vec!["Total".into(), num(total), "1".into(), "100%".into()]);
                out.tables.push(rt);
            }
        }
    }
    Ok(out)
}

/// The tested effects of a two-way ANOVA, named after the table layout.
fn effects(t: &DataTable, sources: &[anova::Source]) -> Vec<(String, f64)> {
    let rows = match t.x_title.trim() {
        title if t.kind.has_x() && !title.is_empty() && title != "X" => title.to_string(),
        _ if t.kind.has_x() => "X (rows)".to_string(),
        _ => "Rows".to_string(),
    };
    let mut v: Vec<(String, f64)> = sources
        .iter()
        .filter_map(|s| {
            let name = match s.name.as_str() {
                "Row factor" => rows.clone(),
                "Column factor" => "Groups".to_string(),
                "Interaction" => "Interaction".to_string(),
                _ => return None,
            };
            Some((name, s.p?))
        })
        .collect();
    // Groups first, then rows, then the interaction.
    v.reverse();
    v
}

fn anova_table(title: &str, sources: &[anova::Source], alpha: f64) -> ResultTable {
    let mut rt = ResultTable::new(
        title,
        &["Source of variation", "SS", "DF", "MS", "F", "P value", "Summary", "Significant?", "% of total variation"],
    );
    for s in sources {
        rt.row(vec![
            s.name.clone(),
            num(s.ss),
            num(s.df),
            if s.ms.is_nan() { String::new() } else { num(s.ms) },
            s.f.map_or(String::new(), num),
            s.p.map_or(String::new(), p),
            s.p.map_or(String::new(), |v| p_summary(v).into()),
            s.p.map_or(String::new(), |v| yes_no(v < alpha)),
            format!("{}%", num(s.percent_of_total)),
        ]);
    }
    rt
}

fn run_one_way(
    t: &DataTable,
    groups: &[usize],
    test: OneWayTest,
    comparisons: Option<Method>,
    control: Option<usize>,
    o: &Options,
    out: &mut AnalysisOutput,
) -> R<()> {
    let names: Vec<String> = groups.iter().map(|&d| title(t, d)).collect();
    let data: Vec<Vec<f64>> = groups.iter().map(|&d| t.column_values(d)).collect();
    let k = groups.len();
    let ci = pct(o.ci_level);
    // The control is a table data set index; it must be one of the analyzed groups.
    let control = control.and_then(|c| groups.iter().position(|&g| g == c));
    // Dunnett always compares with a control: the first group unless chosen.
    let control = if comparisons == Some(Method::Dunnett) { control.or(Some(0)) } else { control };
    let pairs: Vec<(usize, usize)> = match control {
        Some(c) => (0..k).filter(|&j| j != c).map(|j| (j, c)).collect(),
        None => posthoc::all_pairs(k),
    };
    // Tukey is for all pairs; against a control Dunnett's test is the equivalent.
    let method = comparisons.map(|m| if control.is_some() && m == Method::Tukey { Method::Dunnett } else { m });
    let posthoc_table = |means: &[f64], ns: &[f64], mse: f64, df: f64, out: &mut AnalysisOutput| {
        if let Some(method) = method {
            let res = posthoc::compare_means(means, ns, mse, df, &pairs, method, k, o.ci_level);
            let family = control.map_or(String::new(), |c| format!(", each vs. {}", names[c]));
            let mut rt = ResultTable::new(
                format!("{} multiple comparisons{family}", method.label()),
                &["Comparison", "Mean diff.", &format!("{ci} CI of diff."), "Adjusted P", "Summary", "Significant?"],
            );
            for c in res {
                out.sig(groups[c.i], groups[c.j], c.p_adjusted);
                rt.row(vec![
                    format!("{} vs. {}", names[c.i], names[c.j]),
                    num(c.difference),
                    c.ci95.map_or("—".into(), interval),
                    p(c.p_adjusted),
                    p_summary(c.p_adjusted).into(),
                    yes_no(c.p_adjusted < o.alpha),
                ]);
            }
            out.tables.push(rt);
        }
    };
    match test {
        OneWayTest::Ordinary => {
            let r = e(anova::one_way(&data))?;
            let mut s = ResultTable::kv("One-way ANOVA summary");
            s.add("F", format!("F({}, {}) = {}", num(r.df_between), num(r.df_within), num(r.f)))
                .add("P value", p(r.p))
                .add("P value summary", p_summary(r.p).into());
            s.add(&format!("Significant difference among means (P < {})?", num(o.alpha)), yes_no(r.p < o.alpha))
                .add("R squared", num(r.r_squared));
            if let Some((f, pv)) = r.brown_forsythe {
                s.add("Brown-Forsythe test", format!("F = {}, P = {}", num(f), p(pv)));
            }
            if let Some((b, pv)) = r.bartlett {
                s.add("Bartlett's test", format!("χ² = {}, P = {}", num(b), p(pv)));
            }
            out.tables.push(s);
            out.tables.push(anova_table("ANOVA table", &r.sources, o.alpha));
            posthoc_table(&r.means, &r.ns, r.ms_within, r.df_within, out);
        }
        OneWayTest::Welch => {
            let r = e(anova::welch(&data))?;
            let mut s = ResultTable::kv("Welch's ANOVA");
            s.add("W (DFn, DFd)", format!("{} ({}, {})", num(r.f), num(r.df_num), num(r.df_den)))
                .add("P value", p(r.p))
                .add("P value summary", p_summary(r.p).into());
            s.add(&format!("Significant difference among means (P < {})?", num(o.alpha)), yes_no(r.p < o.alpha));
            out.tables.push(s);
            if comparisons.is_some() {
                let means: Vec<f64> = data.iter().map(|v| descriptive::mean(v)).collect();
                let vars: Vec<f64> = data.iter().map(|v| descriptive::variance(v)).collect();
                let ns: Vec<f64> = data.iter().map(|v| v.len() as f64).collect();
                let mut rt = ResultTable::new(
                    "Games-Howell multiple comparisons (unequal SDs)",
                    &["Comparison", "Mean diff.", &format!("{ci} CI of diff."), "Adjusted P", "Summary", "Significant?"],
                );
                for c in posthoc::games_howell(&means, &vars, &ns, &pairs, k, o.ci_level) {
                    out.sig(groups[c.i], groups[c.j], c.p_adjusted);
                    rt.row(vec![
                        format!("{} vs. {}", names[c.i], names[c.j]),
                        num(c.difference),
                        c.ci95.map_or("—".into(), interval),
                        p(c.p_adjusted),
                        p_summary(c.p_adjusted).into(),
                        yes_no(c.p_adjusted < o.alpha),
                    ]);
                }
                out.tables.push(rt);
            }
        }
        OneWayTest::RepeatedMeasures => {
            let rows = t.complete_rows(groups);
            let r = e(anova::repeated_measures(&rows))?;
            let mut s = ResultTable::kv("Repeated measures ANOVA summary");
            s.add("F", format!("F({}, {}) = {}", num(r.sources[0].df), num(r.df_residual), num(r.f))).add("P value", p(r.p));
            s.add("Geisser-Greenhouse ε", num(r.epsilon))
                .add("P value (Geisser-Greenhouse corrected)", p(r.p_corrected))
                .add("Number of subjects", r.n_subjects.to_string());
            out.tables.push(s);
            out.tables.push(anova_table("ANOVA table", &r.sources, o.alpha));
            let ns = vec![r.n_subjects as f64; k];
            posthoc_table(&r.means, &ns, r.ms_residual, r.df_residual, out);
        }
        OneWayTest::KruskalWallis => {
            let r = e(nonparam::kruskal_wallis(&data))?;
            let mut s = ResultTable::kv("Kruskal-Wallis test");
            s.add("Kruskal-Wallis statistic", num(r.h)).add("P value", p(r.p)).add("P value summary", p_summary(r.p).into());
            s.add(&format!("Do the medians vary significantly (P < {})?", num(o.alpha)), yes_no(r.p < o.alpha))
                .add("Number of groups", k.to_string());
            out.tables.push(s);
            if comparisons.is_some() {
                let mut rt =
                    ResultTable::new("Dunn's multiple comparisons", &["Comparison", "Mean rank diff.", "Z", "Adjusted P", "Summary"]);
                for c in nonparam::dunn(&r, &pairs) {
                    out.sig(groups[c.i], groups[c.j], c.p_adjusted);
                    rt.row(vec![
                        format!("{} vs. {}", names[c.i], names[c.j]),
                        num(c.mean_rank_diff),
                        num(c.z),
                        p(c.p_adjusted),
                        p_summary(c.p_adjusted).into(),
                    ]);
                }
                out.tables.push(rt);
            }
        }
        OneWayTest::Friedman => {
            let rows = t.complete_rows(groups);
            let r = e(nonparam::friedman(&rows))?;
            let mut s = ResultTable::kv("Friedman test");
            s.add("Friedman statistic", num(r.statistic)).add("P value", p(r.p)).add("P value summary", p_summary(r.p).into());
            for (n, rs) in names.iter().zip(&r.rank_sums) {
                s.add(&format!("Sum of ranks: {n}"), num(*rs));
            }
            out.tables.push(s);
        }
    }
    Ok(())
}

/// Samples a function at 200 points spanning the X range.
fn sample_curve(x: &[f64], f: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let (lo, hi) = x.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
    (0..=200).map(|i| lo + (hi - lo) * i as f64 / 200.0).map(|v| (v, f(v))).filter(|p| p.1.is_finite()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::sample_table;

    #[test]
    fn one_tailed_halves_p_in_the_predicted_direction() {
        let t = sample_table(TableKind::Column);
        let kind = AnalysisKind::TwoGroups { a: 0, b: 1, test: TwoGroupTest::Unpaired };
        let two = run(&kind, &t, None, &Options::default()).unwrap().significance[0].p;
        let greater = run(&kind, &t, None, &Options { tails: Tails::Greater, ..Default::default() }).unwrap().significance[0].p;
        let less = run(&kind, &t, None, &Options { tails: Tails::Less, ..Default::default() }).unwrap().significance[0].p;
        assert!((greater - two / 2.0).abs() < 1e-12);
        assert!((less - (1.0 - two / 2.0)).abs() < 1e-12);
    }

    #[test]
    fn regression_line_has_a_dashed_confidence_band_around_it() {
        let out = run(&AnalysisKind::LinearRegression, &sample_table(TableKind::XY), Some(&[0]), &Options::default()).unwrap();
        let [line, lo, hi] = &out.curves[..] else { panic!("expected a line and two band edges") };
        assert!(!line.dashed && lo.dashed && hi.dashed);
        for ((l, a), b) in line.points.iter().zip(&lo.points).zip(&hi.points) {
            assert!(a.1 < l.1 && l.1 < b.1);
        }
    }

    #[test]
    fn comparisons_against_a_control() {
        let t = sample_table(TableKind::Column);
        let kind = AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::HolmSidak), control: Some(0) };
        let out = run(&kind, &t, None, &Options::default()).unwrap();
        assert_eq!(out.significance.len(), 2);
        assert!(out.significance.iter().all(|s| s.b == 0));
    }

    #[test]
    fn any_test_on_any_table() {
        let grouped = sample_table(TableKind::Grouped);
        let t_test = AnalysisKind::TwoGroups { a: 0, b: 1, test: TwoGroupTest::Unpaired };
        let out = run(&t_test, &grouped, None, &Options::default()).unwrap();
        assert!(out.warnings.iter().any(|w| w.contains("pooled")));
        let column = sample_table(TableKind::Column);
        let fit = AnalysisKind::NonlinearFit { model: nonlinear::Model::StraightLine, fixed: Vec::new() };
        assert!(run(&fit, &column, None, &Options::default()).is_err());
        let all = AnalysisKind::catalog_all(TableKind::Grouped);
        assert!(all.iter().any(|(_, v)| v.iter().any(|k| matches!(k, AnalysisKind::OneWay { .. }))));
    }

    #[test]
    fn dunnett_games_howell_and_multiple_t() {
        let t = sample_table(TableKind::Column);
        let d = AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::Tukey), control: Some(0) };
        let out = run(&d, &t, None, &Options::default()).unwrap();
        assert!(out.tables.iter().any(|x| x.title.starts_with("Dunnett")));
        let w = AnalysisKind::OneWay { test: OneWayTest::Welch, comparisons: Some(Method::Tukey), control: None };
        let out = run(&w, &t, None, &Options::default()).unwrap();
        assert!(out.tables.iter().any(|x| x.title.starts_with("Games-Howell")));
        let g = sample_table(TableKind::Grouped);
        let m = AnalysisKind::MultipleT { a: 0, b: 1, welch: false, fdr: Some(true) };
        let out = run(&m, &g, None, &Options::default()).unwrap();
        assert_eq!(out.significance.len(), 3);
    }

    #[test]
    fn two_way_skips_empty_data_sets_and_incomplete_rows() {
        use crate::table::GridColumn;
        let mut t = sample_table(TableKind::Grouped);
        let full = run(&AnalysisKind::TwoWay { comparisons: None, within_columns: false, repeated: false }, &t, None, &Options::default())
            .unwrap();
        // An extra titled data set without values, and a row with one empty cell.
        let empty = t.datasets.len();
        t.set(0, GridColumn::Value { dataset: empty, replicate: 0 }, Some(1.0));
        t.set(0, GridColumn::Value { dataset: empty, replicate: 0 }, None);
        t.datasets[empty].title = "Not measured".into();
        let extra = t.used_rows();
        t.set(extra, GridColumn::Value { dataset: 0, replicate: 0 }, Some(5.0));
        let kind = AnalysisKind::TwoWay { comparisons: Some(Method::Sidak), within_columns: false, repeated: false };
        let out = run(&kind, &t, None, &Options::default()).unwrap();
        assert!(out.warnings.iter().any(|w| w.contains("left out")));
        assert_eq!(out.effects, full.effects);
        assert!(out.significance.iter().all(|s| s.a != empty && s.b != empty));
    }

    #[test]
    fn two_way_within_columns_tags_groups() {
        let t = sample_table(TableKind::Grouped);
        let kind = AnalysisKind::TwoWay { comparisons: Some(Method::Sidak), within_columns: true, repeated: false };
        let out = run(&kind, &t, None, &Options::default()).unwrap();
        assert_eq!(out.significance.len(), 2 * 3);
        assert!(out.significance.iter().all(|s| s.group_is_dataset));
    }
}
