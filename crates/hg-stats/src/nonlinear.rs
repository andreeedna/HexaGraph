//! Nonlinear regression (curve fitting) with the Levenberg-Marquardt method.

use crate::descriptive::mean;
use crate::{dist, linalg, need, Interval, Result, StatsError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    StraightLine,
    Polynomial2,
    Polynomial3,
    /// log(agonist) vs. response, variable slope (four parameters).
    DoseResponseVariableSlope,
    /// log(agonist) vs. response, Hill slope fixed at 1 (three parameters).
    DoseResponseStandardSlope,
    /// log(inhibitor) vs. response, variable slope: IC50 parameterisation.
    InhibitionVariableSlope,
    OnePhaseDecay,
    TwoPhaseDecay,
    OnePhaseAssociation,
    ExponentialGrowth,
    MichaelisMenten,
    OneSiteSpecificBinding,
    OneSiteTotalBinding,
    Gaussian,
    LogNormal,
    BoltzmannSigmoid,
    PowerSeries,
}

impl Model {
    pub const ALL: [Model; 17] = [
        Model::StraightLine,
        Model::Polynomial2,
        Model::Polynomial3,
        Model::DoseResponseVariableSlope,
        Model::DoseResponseStandardSlope,
        Model::InhibitionVariableSlope,
        Model::OnePhaseDecay,
        Model::TwoPhaseDecay,
        Model::OnePhaseAssociation,
        Model::ExponentialGrowth,
        Model::MichaelisMenten,
        Model::OneSiteSpecificBinding,
        Model::OneSiteTotalBinding,
        Model::Gaussian,
        Model::LogNormal,
        Model::BoltzmannSigmoid,
        Model::PowerSeries,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Model::StraightLine => "Straight line",
            Model::Polynomial2 => "Second order polynomial (quadratic)",
            Model::Polynomial3 => "Third order polynomial (cubic)",
            Model::DoseResponseVariableSlope => "log(agonist) vs. response — variable slope (4 parameters)",
            Model::DoseResponseStandardSlope => "log(agonist) vs. response — standard slope (3 parameters)",
            Model::InhibitionVariableSlope => "log(inhibitor) vs. response — variable slope (4 parameters)",
            Model::OnePhaseDecay => "One phase exponential decay",
            Model::TwoPhaseDecay => "Two phase exponential decay",
            Model::OnePhaseAssociation => "One phase association",
            Model::ExponentialGrowth => "Exponential (Malthusian) growth",
            Model::MichaelisMenten => "Michaelis-Menten",
            Model::OneSiteSpecificBinding => "One site — specific binding",
            Model::OneSiteTotalBinding => "One site — total binding",
            Model::Gaussian => "Gaussian",
            Model::LogNormal => "Lognormal",
            Model::BoltzmannSigmoid => "Boltzmann sigmoid",
            Model::PowerSeries => "Power series (Y = A·X^B)",
        }
    }

    pub fn equation(self) -> &'static str {
        match self {
            Model::StraightLine => "Y = B0 + B1*X",
            Model::Polynomial2 => "Y = B0 + B1*X + B2*X^2",
            Model::Polynomial3 => "Y = B0 + B1*X + B2*X^2 + B3*X^3",
            Model::DoseResponseVariableSlope => "Y = Bottom + (Top-Bottom)/(1+10^((LogEC50-X)*HillSlope))",
            Model::DoseResponseStandardSlope => "Y = Bottom + (Top-Bottom)/(1+10^(LogEC50-X))",
            Model::InhibitionVariableSlope => "Y = Bottom + (Top-Bottom)/(1+10^((X-LogIC50)*HillSlope))",
            Model::OnePhaseDecay => "Y = (Y0-Plateau)*exp(-K*X) + Plateau",
            Model::TwoPhaseDecay => "Y = Plateau + SpanFast*exp(-KFast*X) + SpanSlow*exp(-KSlow*X)",
            Model::OnePhaseAssociation => "Y = Y0 + (Plateau-Y0)*(1-exp(-K*X))",
            Model::ExponentialGrowth => "Y = Y0*exp(k*X)",
            Model::MichaelisMenten => "Y = Vmax*X/(Km+X)",
            Model::OneSiteSpecificBinding => "Y = Bmax*X/(Kd+X)",
            Model::OneSiteTotalBinding => "Y = Bmax*X/(Kd+X) + NS*X + Background",
            Model::Gaussian => "Y = Amplitude*exp(-0.5*((X-Mean)/SD)^2)",
            Model::LogNormal => "Y = (A/X)*exp(-0.5*(ln(X/GeoMean)/ln(GeoSD))^2)",
            Model::BoltzmannSigmoid => "Y = Bottom + (Top-Bottom)/(1+exp((V50-X)/Slope))",
            Model::PowerSeries => "Y = A*X^B",
        }
    }

    pub fn parameters(self) -> &'static [&'static str] {
        match self {
            Model::StraightLine => &["B0", "B1"],
            Model::Polynomial2 => &["B0", "B1", "B2"],
            Model::Polynomial3 => &["B0", "B1", "B2", "B3"],
            Model::DoseResponseVariableSlope => &["Bottom", "Top", "LogEC50", "HillSlope"],
            Model::DoseResponseStandardSlope => &["Bottom", "Top", "LogEC50"],
            Model::InhibitionVariableSlope => &["Bottom", "Top", "LogIC50", "HillSlope"],
            Model::OnePhaseDecay => &["Y0", "Plateau", "K"],
            Model::TwoPhaseDecay => &["Plateau", "SpanFast", "KFast", "SpanSlow", "KSlow"],
            Model::OnePhaseAssociation => &["Y0", "Plateau", "K"],
            Model::ExponentialGrowth => &["Y0", "k"],
            Model::MichaelisMenten => &["Vmax", "Km"],
            Model::OneSiteSpecificBinding => &["Bmax", "Kd"],
            Model::OneSiteTotalBinding => &["Bmax", "Kd", "NS", "Background"],
            Model::Gaussian => &["Amplitude", "Mean", "SD"],
            Model::LogNormal => &["A", "GeoMean", "GeoSD"],
            Model::BoltzmannSigmoid => &["Bottom", "Top", "V50", "Slope"],
            Model::PowerSeries => &["A", "B"],
        }
    }

    pub fn eval(self, x: f64, p: &[f64]) -> f64 {
        match self {
            Model::StraightLine => p[0] + p[1] * x,
            Model::Polynomial2 => p[0] + x * (p[1] + x * p[2]),
            Model::Polynomial3 => p[0] + x * (p[1] + x * (p[2] + x * p[3])),
            Model::DoseResponseVariableSlope => p[0] + (p[1] - p[0]) / (1.0 + 10f64.powf((p[2] - x) * p[3])),
            Model::DoseResponseStandardSlope => p[0] + (p[1] - p[0]) / (1.0 + 10f64.powf(p[2] - x)),
            Model::InhibitionVariableSlope => p[0] + (p[1] - p[0]) / (1.0 + 10f64.powf((x - p[2]) * p[3])),
            Model::OnePhaseDecay => (p[0] - p[1]) * (-p[2] * x).exp() + p[1],
            Model::TwoPhaseDecay => p[0] + p[1] * (-p[2] * x).exp() + p[3] * (-p[4] * x).exp(),
            Model::OnePhaseAssociation => p[0] + (p[1] - p[0]) * (1.0 - (-p[2] * x).exp()),
            Model::ExponentialGrowth => p[0] * (p[1] * x).exp(),
            Model::MichaelisMenten | Model::OneSiteSpecificBinding => p[0] * x / (p[1] + x),
            Model::OneSiteTotalBinding => p[0] * x / (p[1] + x) + p[2] * x + p[3],
            Model::Gaussian => p[0] * (-0.5 * ((x - p[1]) / p[2]).powi(2)).exp(),
            Model::LogNormal => (p[0] / x) * (-0.5 * ((x / p[1]).ln() / p[2].ln()).powi(2)).exp(),
            Model::BoltzmannSigmoid => p[0] + (p[1] - p[0]) / (1.0 + ((p[2] - x) / p[3]).exp()),
            Model::PowerSeries => p[0] * x.powf(p[1]),
        }
    }

    /// Data driven starting values.
    pub fn initial(self, x: &[f64], y: &[f64]) -> Vec<f64> {
        let (ymin, ymax) = min_max(y);
        let (xmin, xmax) = min_max(x);
        let xr = (xmax - xmin).max(1e-12);
        let at_min_x = y[argmin(x)];
        let at_max_x = y[argmax(x)];
        let half_x = |target: f64| {
            let i = (0..y.len()).min_by(|&a, &b| (y[a] - target).abs().total_cmp(&(y[b] - target).abs())).unwrap();
            x[i]
        };
        let rising = at_max_x >= at_min_x;
        match self {
            Model::StraightLine => vec![mean(y), 0.0],
            Model::Polynomial2 => vec![mean(y), 0.0, 0.0],
            Model::Polynomial3 => vec![mean(y), 0.0, 0.0, 0.0],
            Model::DoseResponseVariableSlope => {
                vec![ymin, ymax, half_x((ymin + ymax) / 2.0), if rising { 1.0 } else { -1.0 }]
            }
            Model::DoseResponseStandardSlope => vec![ymin, ymax, half_x((ymin + ymax) / 2.0)],
            Model::InhibitionVariableSlope => {
                vec![ymin, ymax, half_x((ymin + ymax) / 2.0), if rising { -1.0 } else { 1.0 }]
            }
            Model::OnePhaseDecay => vec![at_min_x, at_max_x, 3.0 / xr],
            Model::TwoPhaseDecay => {
                let span = at_min_x - at_max_x;
                vec![at_max_x, span * 0.5, 10.0 / xr, span * 0.5, 1.0 / xr]
            }
            Model::OnePhaseAssociation => vec![at_min_x, at_max_x, 3.0 / xr],
            Model::ExponentialGrowth => {
                let y0 = if at_min_x != 0.0 { at_min_x } else { 1.0 };
                let k = if at_min_x > 0.0 && at_max_x > 0.0 { (at_max_x / at_min_x).ln() / xr } else { 1.0 / xr };
                vec![y0, k]
            }
            Model::MichaelisMenten | Model::OneSiteSpecificBinding => vec![ymax, half_x(ymax / 2.0).max(xr * 0.1)],
            Model::OneSiteTotalBinding => vec![ymax, half_x(ymax / 2.0).max(xr * 0.1), 0.0, ymin],
            Model::Gaussian => vec![ymax, x[argmax(y)], xr / 6.0],
            Model::LogNormal => {
                let peak = x[argmax(y)].max(1e-9);
                vec![ymax * peak, peak, 2.0]
            }
            Model::BoltzmannSigmoid => {
                vec![ymin, ymax, half_x((ymin + ymax) / 2.0), if rising { xr / 10.0 } else { -xr / 10.0 }]
            }
            Model::PowerSeries => vec![if ymax != 0.0 { ymax / xmax.abs().max(1e-12) } else { 1.0 }, 1.0],
        }
    }
}

fn min_max(v: &[f64]) -> (f64, f64) {
    v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)))
}

fn argmin(v: &[f64]) -> usize {
    (0..v.len()).min_by(|&a, &b| v[a].total_cmp(&v[b])).unwrap()
}

fn argmax(v: &[f64]) -> usize {
    (0..v.len()).max_by(|&a, &b| v[a].total_cmp(&v[b])).unwrap()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    pub model: Model,
    pub parameters: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub ci95: Vec<Interval>,
    pub df: f64,
    pub ss: f64,
    pub sy_x: f64,
    pub r_squared: f64,
    pub n: usize,
    pub iterations: usize,
}

impl Fit {
    pub fn predict(&self, x: f64) -> f64 {
        self.model.eval(x, &self.parameters)
    }
}

/// The model restricted to its free parameters: fixed ones keep a constant value.
struct Free<'a> {
    model: Model,
    fixed: &'a [Option<f64>],
}

impl Free<'_> {
    fn full(&self, free: &[f64]) -> Vec<f64> {
        let mut it = free.iter();
        (0..self.model.parameters().len()).map(|i| self.fixed.get(i).copied().flatten().unwrap_or_else(|| *it.next().unwrap())).collect()
    }

    fn eval(&self, x: f64, free: &[f64]) -> f64 {
        self.model.eval(x, &self.full(free))
    }
}

fn sum_sq(m: &Free, x: &[f64], y: &[f64], p: &[f64]) -> f64 {
    x.iter().zip(y).map(|(&xi, &yi)| (yi - m.eval(xi, p)).powi(2)).sum()
}

fn jacobian(m: &Free, x: &[f64], p: &[f64]) -> Vec<Vec<f64>> {
    x.iter()
        .map(|&xi| {
            (0..p.len())
                .map(|k| {
                    let h = 1e-7 * p[k].abs().max(1e-4);
                    let mut up = p.to_vec();
                    let mut dn = p.to_vec();
                    up[k] += h;
                    dn[k] -= h;
                    (m.eval(xi, &up) - m.eval(xi, &dn)) / (2.0 * h)
                })
                .collect()
        })
        .collect()
}

/// Least squares fit of `model` to the points, starting from `initial`
/// (or data driven defaults when `None`).
pub fn fit(model: Model, x: &[f64], y: &[f64], initial: Option<Vec<f64>>) -> Result<Fit> {
    fit_constrained(model, x, y, initial, &[])
}

/// Like [`fit`], but parameters with `Some(value)` in `fixed` are held
/// constant (e.g. Bottom = 0 and Top = 100 for a normalized dose-response).
pub fn fit_constrained(model: Model, x: &[f64], y: &[f64], initial: Option<Vec<f64>>, fixed: &[Option<f64>]) -> Result<Fit> {
    if x.len() != y.len() {
        return Err(StatsError::Invalid("X and Y must have the same length".into()));
    }
    let m = Free { model, fixed };
    let all = model.parameters().len();
    let is_fixed = |i: usize| fixed.get(i).copied().flatten().is_some();
    let k = (0..all).filter(|&i| !is_fixed(i)).count();
    need(k > 0, "at least one parameter must be free")?;
    need(x.len() > k, "more data points than parameters are required")?;
    let start = initial.unwrap_or_else(|| model.initial(x, y));
    let mut p: Vec<f64> = (0..all).filter(|&i| !is_fixed(i)).map(|i| start[i]).collect();
    let model = &m;
    let mut ss = sum_sq(model, x, y, &p);
    if !ss.is_finite() {
        return Err(StatsError::Invalid("the initial values give an undefined curve".into()));
    }
    let mut lambda = 1e-3;
    let mut iterations = 0;
    let mut converged = false;
    for it in 0..2000 {
        iterations = it + 1;
        let j = jacobian(model, x, &p);
        let r: Vec<f64> = x.iter().zip(y).map(|(&xi, &yi)| yi - model.eval(xi, &p)).collect();
        let mut jtj = vec![vec![0.0; k]; k];
        let mut jtr = vec![0.0; k];
        for (row, ri) in j.iter().zip(&r) {
            for a in 0..k {
                jtr[a] += row[a] * ri;
                for b in 0..k {
                    jtj[a][b] += row[a] * row[b];
                }
            }
        }
        let mut improved = false;
        while lambda < 1e12 {
            let mut mm = jtj.clone();
            for (a, row) in mm.iter_mut().enumerate() {
                row[a] += lambda * jtj[a][a].max(1e-12);
            }
            if let Some(delta) = linalg::solve(&mm, &jtr) {
                let trial: Vec<f64> = p.iter().zip(&delta).map(|(a, d)| a + d).collect();
                let ss_new = sum_sq(model, x, y, &trial);
                if ss_new.is_finite() && ss_new <= ss {
                    let rel = (ss - ss_new) / ss.max(1e-300);
                    let step: f64 = delta.iter().zip(&p).map(|(d, a)| (d / a.abs().max(1e-12)).abs()).fold(0.0, f64::max);
                    p = trial;
                    ss = ss_new;
                    lambda = (lambda / 10.0).max(1e-12);
                    improved = true;
                    if rel < 1e-12 || step < 1e-10 {
                        converged = true;
                    }
                    break;
                }
            }
            lambda *= 10.0;
        }
        if !improved {
            converged = true; // no further descent possible: at a minimum
        }
        if converged {
            break;
        }
    }
    if !converged {
        return Err(StatsError::NoConvergence);
    }
    let n = x.len();
    let df = (n - k) as f64;
    let s2 = ss / df;
    let j = jacobian(model, x, &p);
    let mut jtj = vec![vec![0.0; k]; k];
    for row in &j {
        for a in 0..k {
            for b in 0..k {
                jtj[a][b] += row[a] * row[b];
            }
        }
    }
    let cov = linalg::invert(&jtj).ok_or_else(|| StatsError::Invalid("the fit is ambiguous (parameters are not identifiable)".into()))?;
    let free_se: Vec<f64> = (0..k).map(|a| (s2 * cov[a][a]).max(0.0).sqrt()).collect();
    let mut it = free_se.iter();
    let se: Vec<f64> = (0..all).map(|i| if is_fixed(i) { 0.0 } else { *it.next().unwrap() }).collect();
    let p = m.full(&p);
    let t = dist::t_crit(0.95, df);
    let my = mean(y);
    let sst: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    Ok(Fit {
        model: m.model,
        ci95: p.iter().zip(&se).map(|(v, s)| Interval::new(v - t * s, v + t * s)).collect(),
        parameters: p,
        std_errors: se,
        df,
        ss,
        sy_x: s2.sqrt(),
        r_squared: 1.0 - ss / sst,
        n,
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn dose_response_matches_r_nls() {
        let x = [-9.0, -8.5, -8.0, -7.5, -7.0, -6.5, -6.0, -5.5, -5.0];
        let y = [2.1, 4.5, 10.2, 24.8, 51.3, 76.9, 90.4, 96.8, 98.9];
        let f = fit(Model::DoseResponseVariableSlope, &x, &y, None).unwrap();
        assert_close(f.parameters[0], 1.277673046, 1e-5);
        assert_close(f.parameters[1], 99.520549661, 1e-6);
        assert_close(f.parameters[2], -7.011864092, 1e-6);
        assert_close(f.parameters[3], 1.012502437, 1e-5);
        assert_close(f.std_errors[2], 0.00686596375, 1e-3);
        assert_close(f.ss, 0.7766432149, 1e-6);
    }

    #[test]
    fn decay_matches_r_nls() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0];
        let y = [100.0, 62.0, 40.0, 27.0, 19.0, 12.0, 10.0, 9.5];
        let f = fit(Model::OnePhaseDecay, &x, &y, None).unwrap();
        assert_close(f.parameters[0], 100.0417468754, 1e-6);
        assert_close(f.parameters[1], 8.7415554717, 1e-6);
        assert_close(f.parameters[2], 0.5389584748, 1e-6);
        assert_close(f.std_errors[2], 0.004219919921, 1e-3);
    }

    #[test]
    fn fixed_parameters_are_held() {
        let x = [-9.0, -8.5, -8.0, -7.5, -7.0, -6.5, -6.0, -5.5, -5.0];
        let y = [2.1, 4.5, 10.2, 24.8, 51.3, 76.9, 90.4, 96.8, 98.9];
        let f = fit_constrained(Model::DoseResponseVariableSlope, &x, &y, None, &[Some(0.0), Some(100.0), None, None]).unwrap();
        assert_eq!(f.parameters[0], 0.0);
        assert_eq!(f.parameters[1], 100.0);
        assert_eq!(f.std_errors[0], 0.0);
        assert!((f.parameters[2] + 7.0).abs() < 0.05);
        assert_eq!(f.df, 7.0);
    }

    #[test]
    fn line_is_exact() {
        let x = [1.0, 2.0, 3.0, 4.0];
        let y = [3.0, 5.0, 7.1, 8.9];
        let f = fit(Model::StraightLine, &x, &y, None).unwrap();
        let l = crate::regression::linear(&x, &y).unwrap();
        assert_close(f.parameters[1], l.slope, 1e-8);
        assert_close(f.parameters[0], l.intercept, 1e-8);
    }
}
