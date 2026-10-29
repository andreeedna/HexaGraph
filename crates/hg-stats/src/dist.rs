//! Thin wrappers around the probability distributions we need.

use statrs::distribution::{ChiSquared, ContinuousCDF, FisherSnedecor, Normal, StudentsT};

fn student(df: f64) -> StudentsT {
    StudentsT::new(0.0, 1.0, df).expect("df must be positive")
}

/// statrs panics on NaN input or non-positive df (e.g. 0/0 from constant data).
fn undefined(x: f64, df: f64) -> bool {
    x.is_nan() || !(df > 0.0)
}

/// Upper tail probability P(T > t).
pub fn t_sf(t: f64, df: f64) -> f64 {
    if undefined(t, df) {
        return f64::NAN;
    }
    student(df).sf(t)
}

/// Two tailed P value for a t statistic (`clamp`, unlike `min`, keeps NaN).
pub fn t_p2(t: f64, df: f64) -> f64 {
    (2.0 * t_sf(t.abs(), df)).clamp(0.0, 1.0)
}

/// Quantile of Student's t.
pub fn t_inv(p: f64, df: f64) -> f64 {
    if undefined(p, df) {
        return f64::NAN;
    }
    student(df).inverse_cdf(p)
}

/// Critical t value for a two sided confidence interval with the given level.
pub fn t_crit(level: f64, df: f64) -> f64 {
    t_inv(1.0 - (1.0 - level) / 2.0, df)
}

pub fn f_sf(f: f64, df1: f64, df2: f64) -> f64 {
    if undefined(f, df1) || undefined(f, df2) {
        return f64::NAN;
    }
    if f.is_infinite() {
        return 0.0;
    }
    if f <= 0.0 {
        return 1.0;
    }
    FisherSnedecor::new(df1, df2).expect("df must be positive").sf(f)
}

pub fn chi2_sf(x: f64, df: f64) -> f64 {
    if undefined(x, df) {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 1.0;
    }
    ChiSquared::new(df).expect("df must be positive").sf(x)
}

fn std_normal() -> Normal {
    Normal::new(0.0, 1.0).unwrap()
}

pub fn norm_cdf(z: f64) -> f64 {
    std_normal().cdf(z)
}

pub fn norm_sf(z: f64) -> f64 {
    std_normal().sf(z)
}

pub fn norm_inv(p: f64) -> f64 {
    std_normal().inverse_cdf(p)
}

/// Two tailed P value for a z statistic.
pub fn z_p2(z: f64) -> f64 {
    (2.0 * norm_sf(z.abs())).clamp(0.0, 1.0)
}

pub fn ln_gamma(x: f64) -> f64 {
    statrs::function::gamma::ln_gamma(x)
}

#[cfg(test)]
mod tests {
    #[test]
    fn constant_groups_give_undefined_p_not_a_panic() {
        let same = [3.0, 3.0, 3.0];
        for welch in [false, true] {
            assert!(crate::ttest::unpaired(&same, &same, welch).unwrap().p.is_nan());
        }
        assert!(crate::ttest::variance_f_test(&same, &same).p.is_nan());
        assert!(super::chi2_sf(f64::NAN, 1.0).is_nan());
    }
}
