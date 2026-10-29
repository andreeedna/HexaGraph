//! HexaGraph statistics engine.
//!
//! Every routine in this crate is implemented from published statistical
//! literature and checked against reference values computed with R and SciPy
//! (see the tests in each module).

pub mod anova;
pub mod contingency;
pub mod descriptive;
pub mod diagnostic;
pub mod dist;
pub mod linalg;
pub mod nonlinear;
pub mod nonparam;
pub mod normality;
pub mod outliers;
pub mod posthoc;
pub mod regression;
pub mod survival;
pub mod ttest;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StatsError {
    #[error("not enough data: {0}")]
    NotEnoughData(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the fit did not converge")]
    NoConvergence,
}

pub type Result<T> = std::result::Result<T, StatsError>;

pub(crate) fn need(cond: bool, msg: &str) -> Result<()> {
    if cond {
        Ok(())
    } else {
        Err(StatsError::NotEnoughData(msg.to_string()))
    }
}

/// Asterisk summary of a P value: ns, *, **, ***, ****.
pub fn p_summary(p: f64) -> &'static str {
    if p < 0.0001 {
        "****"
    } else if p < 0.001 {
        "***"
    } else if p < 0.01 {
        "**"
    } else if p < 0.05 {
        "*"
    } else {
        "ns"
    }
}

/// Human friendly P value formatting, e.g. "0.0123" or "<0.0001".
pub fn format_p(p: f64) -> String {
    if !p.is_finite() {
        "—".into()
    } else if p < 0.0001 {
        "<0.0001".into()
    } else if p >= 0.9999 {
        ">0.9999".into()
    } else {
        format!("{p:.4}")
    }
}

/// A closed interval, typically a confidence interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

impl Interval {
    pub fn new(low: f64, high: f64) -> Self {
        Self { low, high }
    }
}

#[cfg(test)]
pub(crate) fn assert_close(a: f64, b: f64, tol: f64) {
    let scale = b.abs().max(1.0);
    assert!((a - b).abs() <= tol * scale, "expected {b}, got {a} (tol {tol})");
}
