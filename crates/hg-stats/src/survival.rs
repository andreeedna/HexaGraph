//! Survival analysis: Kaplan-Meier curves and log-rank tests.

use crate::{dist, linalg, need, Interval, Result, StatsError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Subjects {
    pub times: Vec<f64>,
    /// true = event (death), false = censored.
    pub events: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KmStep {
    pub time: f64,
    pub at_risk: usize,
    pub events: usize,
    pub censored: usize,
    /// Fraction surviving just after `time`.
    pub survival: f64,
    /// Greenwood standard error.
    pub se: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KaplanMeier {
    pub steps: Vec<KmStep>,
    pub median: Option<f64>,
    pub n: usize,
    pub n_events: usize,
}

impl KaplanMeier {
    /// Survival fraction at time `t` (right continuous step function).
    pub fn at(&self, t: f64) -> f64 {
        self.steps.iter().take_while(|s| s.time <= t).last().map_or(1.0, |s| s.survival)
    }

    /// Log-log 95% confidence interval at a step.
    pub fn ci95(step: &KmStep) -> Option<Interval> {
        Self::ci(step, 0.95)
    }

    /// Log-log confidence interval at a step for the given level.
    pub fn ci(step: &KmStep, level: f64) -> Option<Interval> {
        if step.survival <= 0.0 || step.survival >= 1.0 {
            return None;
        }
        let s = step.survival;
        let v = (step.se / s).powi(2);
        let se_ll = v.sqrt() / s.ln().abs();
        let z = dist::norm_inv(0.5 + level / 2.0);
        Some(Interval::new(s.powf((z * se_ll).exp()), s.powf((-z * se_ll).exp())))
    }
}

fn distinct_times(s: &Subjects) -> Vec<f64> {
    let mut t = s.times.clone();
    t.sort_by(f64::total_cmp);
    t.dedup();
    t
}

pub fn kaplan_meier(s: &Subjects) -> Result<KaplanMeier> {
    if s.times.len() != s.events.len() {
        return Err(StatsError::Invalid("times and event codes differ in length".into()));
    }
    need(!s.times.is_empty(), "at least one subject is required")?;
    let mut steps = Vec::new();
    let mut surv = 1.0;
    let mut greenwood = 0.0;
    let mut at_risk = s.times.len();
    for t in distinct_times(s) {
        let (mut d, mut c) = (0, 0);
        for (&ti, &e) in s.times.iter().zip(&s.events) {
            if ti == t {
                if e {
                    d += 1
                } else {
                    c += 1
                }
            }
        }
        if d > 0 {
            let (n, df) = (at_risk as f64, d as f64);
            surv *= 1.0 - df / n;
            if n > df {
                greenwood += df / (n * (n - df));
            }
        }
        steps.push(KmStep { time: t, at_risk, events: d, censored: c, survival: surv, se: surv * greenwood.sqrt() });
        at_risk -= d + c;
    }
    // Like R: when the curve stays at exactly 50%, the median is the middle of that plateau.
    let half = |v: f64| (v - 0.5).abs() < 1e-12;
    let events: Vec<&KmStep> = steps.iter().filter(|st| st.events > 0).collect();
    let median = events.iter().position(|st| st.survival < 0.5 || half(st.survival)).map(|i| match events.get(i + 1) {
        Some(next) if half(events[i].survival) => (events[i].time + next.time) / 2.0,
        _ => events[i].time,
    });
    Ok(KaplanMeier { n: s.times.len(), n_events: s.events.iter().filter(|e| **e).count(), steps, median })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weighting {
    /// Mantel-Cox log-rank.
    LogRank,
    /// Gehan-Breslow-Wilcoxon (weights = number at risk).
    GehanBreslow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub chi2: f64,
    pub df: f64,
    pub p: f64,
    pub observed: Vec<f64>,
    pub expected: Vec<f64>,
    /// Hazard ratio of group 1 vs group 2 (two groups, log-rank method).
    pub hazard_ratio: Option<(f64, Interval)>,
    /// Test for trend across ordered groups (three or more): (chi², P), 1 df.
    #[serde(default)]
    pub trend: Option<(f64, f64)>,
}

/// Compares survival curves of two or more groups.
pub fn compare(groups: &[Subjects], weighting: Weighting) -> Result<Comparison> {
    let k = groups.len();
    need(k >= 2, "at least two groups are required")?;
    let mut times: Vec<f64> = groups.iter().flat_map(|g| g.times.iter().copied()).collect();
    times.sort_by(f64::total_cmp);
    times.dedup();
    let mut o = vec![0.0; k];
    let mut e = vec![0.0; k];
    let mut v = vec![vec![0.0; k]; k];
    let mut uo = vec![0.0; k];
    for &t in &times {
        let n: Vec<f64> = groups.iter().map(|g| g.times.iter().filter(|&&x| x >= t).count() as f64).collect();
        let d: Vec<f64> = groups.iter().map(|g| g.times.iter().zip(&g.events).filter(|(x, ev)| **x == t && **ev).count() as f64).collect();
        let (nt, dt): (f64, f64) = (n.iter().sum(), d.iter().sum());
        if dt == 0.0 || nt == 0.0 {
            continue;
        }
        let w = match weighting {
            Weighting::LogRank => 1.0,
            Weighting::GehanBreslow => nt,
        };
        for j in 0..k {
            let ej = dt * n[j] / nt;
            o[j] += d[j];
            e[j] += ej;
            uo[j] += w * (d[j] - ej);
            if nt > 1.0 {
                for l in 0..k {
                    let delta = if j == l { 1.0 } else { 0.0 };
                    v[j][l] += w * w * dt * (nt - dt) / (nt - 1.0) * (n[j] / nt) * (delta - n[l] / nt);
                }
            }
        }
    }
    let m = k - 1;
    let vr: Vec<Vec<f64>> = v[..m].iter().map(|r| r[..m].to_vec()).collect();
    let x = linalg::solve(&vr, &uo[..m]).ok_or_else(|| StatsError::Invalid("no events to compare".into()))?;
    let chi2: f64 = x.iter().zip(&uo[..m]).map(|(a, b)| a * b).sum();
    let df = m as f64;
    let hazard_ratio = (k == 2 && e[0] > 0.0 && e[1] > 0.0 && o[1] > 0.0 && o[0] > 0.0).then(|| {
        let hr = (o[0] / e[0]) / (o[1] / e[1]);
        let se = (1.0 / e[0] + 1.0 / e[1]).sqrt();
        let z = dist::norm_inv(0.975);
        (hr, Interval::new((hr.ln() - z * se).exp(), (hr.ln() + z * se).exp()))
    });
    // Trend: scores 1, 2, … k for the groups in their order.
    let trend = (k >= 3).then(|| {
        let w: Vec<f64> = (1..=k).map(|i| i as f64).collect();
        let u: f64 = w.iter().zip(&uo).map(|(a, b)| a * b).sum();
        let var: f64 = (0..k).flat_map(|i| (0..k).map(move |j| (i, j))).map(|(i, j)| w[i] * v[i][j] * w[j]).sum();
        let c = u * u / var;
        (c, dist::chi2_sf(c, 1.0))
    });
    Ok(Comparison { chi2, df, p: dist::chi2_sf(chi2, df), observed: o, expected: e, hazard_ratio, trend })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn groups() -> Vec<Subjects> {
        let t = [5.0, 8.0, 12.0, 12.0, 15.0, 20.0, 22.0, 30.0, 3.0, 4.0, 7.0, 9.0, 10.0, 14.0, 16.0, 18.0];
        let e = [1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1];
        (0..2)
            .map(|g| Subjects { times: t[g * 8..g * 8 + 8].to_vec(), events: e[g * 8..g * 8 + 8].iter().map(|&v| v == 1).collect() })
            .collect()
    }

    #[test]
    fn kaplan_meier_matches_r() {
        let km = kaplan_meier(&groups()[0]).unwrap();
        let ev: Vec<&KmStep> = km.steps.iter().filter(|s| s.events > 0).collect();
        assert_close(ev[3].survival, 0.46875, 1e-12);
        assert_close(ev[4].survival, 0.234375, 1e-12);
        assert_close(ev[3].se, 0.186521, 1e-5);
        assert_eq!(km.median, Some(15.0));
        assert_eq!(kaplan_meier(&groups()[1]).unwrap().median, Some(10.0));
        // R: survfit(Surv(1:4, rep(1, 4)) ~ 1) has median 2.5.
        let all_die = Subjects { times: vec![1.0, 2.0, 3.0, 4.0], events: vec![true; 4] };
        assert_eq!(kaplan_meier(&all_die).unwrap().median, Some(2.5));
    }

    #[test]
    fn log_rank_matches_r() {
        let c = compare(&groups(), Weighting::LogRank).unwrap();
        assert_close(c.chi2, 2.570477568, 1e-8);
        assert_close(c.p, 0.1088748589, 1e-8);
        assert_close(c.expected[0], 7.581318681, 1e-8);
        // Weighted log-rank with weight = number at risk, computed in R from
        // survival's risk sets (the same code gives survdiff's chi² with weight 1).
        let g = compare(&groups(), Weighting::GehanBreslow).unwrap();
        assert_close(g.chi2, 1.67405063291, 1e-9);
        assert_close(g.p, 0.195716862836, 1e-9);
    }
}
