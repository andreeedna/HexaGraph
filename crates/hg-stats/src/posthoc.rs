//! Multiple comparisons and the studentized range distribution.

// Quadrature nodes and weights are kept exactly as published.
#![allow(clippy::excessive_precision)]

use crate::{dist, Interval};
use serde::{Deserialize, Serialize};
use std::f64::consts::LN_2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Method {
    #[default]
    Tukey,
    Bonferroni,
    Sidak,
    HolmSidak,
    FisherLsd,
    /// Each group against a control (multivariate t).
    Dunnett,
}

impl Method {
    pub const ALL: [Method; 5] = [Method::Tukey, Method::Bonferroni, Method::Sidak, Method::HolmSidak, Method::FisherLsd];

    pub fn label(self) -> &'static str {
        match self {
            Method::Tukey => "Tukey",
            Method::Bonferroni => "Bonferroni",
            Method::Sidak => "Šídák",
            Method::HolmSidak => "Holm-Šídák",
            Method::FisherLsd => "Fisher's LSD (uncorrected)",
            Method::Dunnett => "Dunnett",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub i: usize,
    pub j: usize,
    /// mean[i] - mean[j]
    pub difference: f64,
    pub ci95: Option<Interval>,
    pub statistic: f64,
    pub p_adjusted: f64,
}

/// Pairwise comparisons of group means sharing a pooled error term
/// (mean square `mse` with `df` degrees of freedom).
/// `k` is the number of means in the family (used by Tukey) and `level` the
/// family-wise confidence level of the intervals (e.g. 0.95).
pub fn compare_means(
    means: &[f64],
    ns: &[f64],
    mse: f64,
    df: f64,
    pairs: &[(usize, usize)],
    method: Method,
    k: usize,
    level: f64,
) -> Vec<Comparison> {
    let alpha = 1.0 - level;
    // Dunnett: pairs are (group, control); the critical value is shared by the family.
    let dunnett = (method == Method::Dunnett).then(|| {
        let lambdas: Vec<f64> = pairs.iter().map(|&(g, c)| (ns[g] / (ns[g] + ns[c])).sqrt()).collect();
        let q = dunnett_quantile(level, &lambdas, df);
        (lambdas, q)
    });
    let m = pairs.len() as f64;
    let mut out: Vec<Comparison> = pairs
        .iter()
        .map(|&(i, j)| {
            let diff = means[i] - means[j];
            let se = (mse * (1.0 / ns[i] + 1.0 / ns[j])).sqrt();
            let t = diff / se;
            let p_raw = dist::t_p2(t, df);
            let (stat, p, ci) = match method {
                Method::Tukey => {
                    let q = t.abs() * std::f64::consts::SQRT_2;
                    let qc = qtukey(level, k as f64, df) / std::f64::consts::SQRT_2;
                    (q, 1.0 - ptukey(q, k as f64, df), Some(Interval::new(diff - qc * se, diff + qc * se)))
                }
                Method::Bonferroni => {
                    let tc = dist::t_inv(1.0 - alpha / (2.0 * m), df);
                    (t, (p_raw * m).clamp(0.0, 1.0), Some(Interval::new(diff - tc * se, diff + tc * se)))
                }
                Method::Sidak => {
                    let a = 1.0 - level.powf(1.0 / m);
                    let tc = dist::t_inv(1.0 - a / 2.0, df);
                    (t, 1.0 - (1.0 - p_raw).powf(m), Some(Interval::new(diff - tc * se, diff + tc * se)))
                }
                Method::HolmSidak => (t, p_raw, None),
                Method::FisherLsd => {
                    let tc = dist::t_crit(level, df);
                    (t, p_raw, Some(Interval::new(diff - tc * se, diff + tc * se)))
                }
                Method::Dunnett => {
                    let (lambdas, qc) = dunnett.as_ref().unwrap();
                    let p = 1.0 - dunnett_cdf(t.abs(), lambdas, df);
                    (t, p, Some(Interval::new(diff - qc * se, diff + qc * se)))
                }
            };
            Comparison { i, j, difference: diff, ci95: ci, statistic: stat, p_adjusted: p.clamp(0.0, 1.0) }
        })
        .collect();
    if method == Method::HolmSidak {
        let raw: Vec<f64> = out.iter().map(|c| c.p_adjusted).collect();
        for (c, p) in out.iter_mut().zip(holm_sidak(&raw)) {
            c.p_adjusted = p;
        }
    }
    out
}

/// All pairs (i, j) with i < j.
pub fn all_pairs(k: usize) -> Vec<(usize, usize)> {
    (0..k).flat_map(|i| (i + 1..k).map(move |j| (i, j))).collect()
}

fn step_down(p: &[f64], f: impl Fn(f64, usize) -> f64) -> Vec<f64> {
    let m = p.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    let mut out = vec![0.0; m];
    let mut running: f64 = 0.0;
    for (rank, &idx) in order.iter().enumerate() {
        running = running.max(f(p[idx], m - rank).min(1.0));
        out[idx] = running;
    }
    out
}

/// Holm (Bonferroni) step-down adjusted P values.
pub fn holm(p: &[f64]) -> Vec<f64> {
    step_down(p, |p, remaining| p * remaining as f64)
}

/// Holm-Šídák step-down adjusted P values.
pub fn holm_sidak(p: &[f64]) -> Vec<f64> {
    step_down(p, |p, remaining| 1.0 - (1.0 - p).powi(remaining as i32))
}

/// Benjamini-Hochberg false discovery rate (q values).
pub fn benjamini_hochberg(p: &[f64]) -> Vec<f64> {
    let m = p.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p[b].total_cmp(&p[a]));
    let mut out = vec![0.0; m];
    let mut running: f64 = 1.0;
    for (k, &idx) in order.iter().enumerate() {
        let rank = (m - k) as f64;
        running = running.min(p[idx] * m as f64 / rank);
        out[idx] = running;
    }
    out
}

// --- Studentized range distribution (Copenhaver & Holland 1988, as in R's ptukey.c) ---

fn wprob(w: f64, rr: f64, cc: f64) -> f64 {
    const NLEG: usize = 12;
    const IHALF: usize = 6;
    const C1: f64 = -30.0;
    const C2: f64 = -50.0;
    const C3: f64 = 60.0;
    const BB: f64 = 8.0;
    const WLAR: f64 = 3.0;
    const XLEG: [f64; IHALF] = [
        0.981560634246719250690549090149,
        0.904117256370474856678465866119,
        0.769902674194304687036893833213,
        0.587317954286617447296702418941,
        0.367831498998180193752691536644,
        0.125233408511468915472441369464,
    ];
    const ALEG: [f64; IHALF] = [
        0.047175336386511827194615961485,
        0.106939325995318430960254718194,
        0.160078328543346226334652529543,
        0.203167426723065921749064455810,
        0.233492536538354808760849898925,
        0.249147045813402785000562436043,
    ];
    let qsqz = w * 0.5;
    if qsqz >= BB {
        return 1.0;
    }
    let mut pr_w = 2.0 * dist::norm_cdf(qsqz) - 1.0;
    pr_w = if pr_w >= (C2 / cc).exp() { pr_w.powf(cc) } else { 0.0 };
    let wincr = if w > WLAR { 2.0 } else { 3.0 };
    let mut blb = qsqz;
    let binc = (BB - qsqz) / wincr;
    let mut bub = blb + binc;
    let mut einsum = 0.0;
    let cc1 = cc - 1.0;
    let mut wi = 1.0;
    while wi <= wincr {
        let mut elsum = 0.0;
        let a = 0.5 * (bub + blb);
        let b = 0.5 * (bub - blb);
        for jj in 1..=NLEG {
            let (j, xx) = if IHALF < jj {
                let j = NLEG - jj + 1;
                (j, XLEG[j - 1])
            } else {
                (jj, -XLEG[jj - 1])
            };
            let c = b * xx;
            let ac = a + c;
            let qexpo = ac * ac;
            if qexpo > C3 {
                break;
            }
            let pplus = 2.0 * dist::norm_cdf(ac);
            let pminus = 2.0 * dist::norm_cdf(ac - w);
            let mut rinsum = pplus * 0.5 - pminus * 0.5;
            if rinsum >= (C1 / cc1).exp() {
                rinsum = ALEG[j - 1] * (-(0.5 * qexpo)).exp() * rinsum.powf(cc1);
                elsum += rinsum;
            }
        }
        elsum *= 2.0 * b * cc / (2.0 * std::f64::consts::PI).sqrt();
        einsum += elsum;
        blb = bub;
        bub += binc;
        wi += 1.0;
    }
    pr_w += einsum;
    if pr_w <= (C1 / rr).exp() {
        return 0.0;
    }
    pr_w = pr_w.powf(rr);
    pr_w.min(1.0)
}

/// CDF of the studentized range for `k` means and `df` degrees of freedom.
pub fn ptukey(q: f64, k: f64, df: f64) -> f64 {
    const NLEGQ: usize = 16;
    const IHALFQ: usize = 8;
    const XLEGQ: [f64; IHALFQ] = [
        0.989400934991649932596154173450,
        0.944575023073232576077988415535,
        0.865631202387831743880467897712,
        0.755404408355003033895101194847,
        0.617876244402643748446671764049,
        0.458016777657227386342419442984,
        0.281603550779258913230460501460,
        0.950125098376374401853193354250e-1,
    ];
    const ALEGQ: [f64; IHALFQ] = [
        0.271524594117540948517805724560e-1,
        0.622535239386478928628438369944e-1,
        0.951585116824927848099251076022e-1,
        0.124628971255533872052476282192,
        0.149595988816576732081501730547,
        0.169156519395002538189312079030,
        0.182603415044923588866763667969,
        0.189450610455068496285396723208,
    ];
    let rr = 1.0;
    if q <= 0.0 {
        return 0.0;
    }
    if !q.is_finite() {
        return 1.0;
    }
    if df > 25000.0 {
        return wprob(q, rr, k);
    }
    let f2 = df * 0.5;
    let mut f2lf = f2 * df.ln() - df * LN_2 - dist::ln_gamma(f2);
    let f21 = f2 - 1.0;
    let ff4 = df * 0.25;
    let ulen: f64 = if df <= 100.0 {
        1.0
    } else if df <= 800.0 {
        0.5
    } else if df <= 5000.0 {
        0.25
    } else {
        0.125
    };
    f2lf += ulen.ln();
    let mut ans = 0.0;
    for i in 1..=50 {
        let mut otsum = 0.0;
        let twa1 = (2 * i - 1) as f64 * ulen;
        for jj in 1..=NLEGQ {
            let (j, t1) = if IHALFQ < jj {
                let j = jj - IHALFQ - 1;
                (j, f2lf + f21 * (twa1 + XLEGQ[j] * ulen).ln() - (XLEGQ[j] * ulen + twa1) * ff4)
            } else {
                let j = jj - 1;
                (j, f2lf + f21 * (twa1 - XLEGQ[j] * ulen).ln() + (XLEGQ[j] * ulen - twa1) * ff4)
            };
            if t1 >= -30.0 {
                let qsqz =
                    if IHALFQ < jj { q * ((XLEGQ[j] * ulen + twa1) * 0.5).sqrt() } else { q * ((-(XLEGQ[j] * ulen) + twa1) * 0.5).sqrt() };
                otsum += wprob(qsqz, rr, k) * ALEGQ[j] * t1.exp();
            }
        }
        if i as f64 * ulen >= 1.0 && otsum <= 1e-14 {
            break;
        }
        ans += otsum;
    }
    ans.min(1.0)
}

/// Quantile of the studentized range (bisection on `ptukey`).
pub fn qtukey(p: f64, k: f64, df: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    while ptukey(hi, k, df) < p {
        hi *= 2.0;
    }
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if ptukey(mid, k, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-10 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Density of s = sqrt(chi²(df) / df).
fn chi_scaled_pdf(s: f64, df: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    let ln = (df / 2.0) * df.ln() + (df - 1.0) * s.ln()
        - df * s * s / 2.0
        - (df / 2.0 - 1.0) * std::f64::consts::LN_2
        - dist::ln_gamma(df / 2.0);
    ln.exp()
}

/// P(max |T_i| < c) for Dunnett's statistics. The correlations between the
/// comparisons with a common control are ρ_ij = λ_i λ_j, which reduces the
/// multivariate t integral to two dimensions (Dunnett 1955).
pub fn dunnett_cdf(c: f64, lambdas: &[f64], df: f64) -> f64 {
    if c <= 0.0 {
        return 0.0;
    }
    let inner = |cs: f64| -> f64 {
        // ∫ φ(z) Π_i [Φ((cs − λz)/√(1−λ²)) − Φ((−cs − λz)/√(1−λ²))] dz, Simpson on [−8, 8].
        let n = 160;
        let (a, b) = (-8.0, 8.0);
        let h = (b - a) / n as f64;
        let f = |z: f64| {
            let phi = (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt();
            let prod: f64 = lambdas
                .iter()
                .map(|&l| {
                    let r = (1.0 - l * l).max(1e-12).sqrt();
                    dist::norm_cdf((cs - l * z) / r) - dist::norm_cdf((-cs - l * z) / r)
                })
                .product();
            phi * prod
        };
        let mut sum = f(a) + f(b);
        for i in 1..n {
            sum += f(a + i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        sum * h / 3.0
    };
    if df > 2000.0 {
        return inner(c).min(1.0);
    }
    // Integrate over the distribution of s = σ̂/σ.
    let sd = (1.0 / (2.0 * df)).sqrt();
    let (lo, hi) = ((1.0 - 9.0 * sd).max(0.0), 1.0 + 9.0 * sd + 2.0 / df);
    let n = 120;
    let h = (hi - lo) / n as f64;
    let g = |s: f64| chi_scaled_pdf(s, df) * inner(c * s);
    let mut sum = g(lo) + g(hi);
    for i in 1..n {
        sum += g(lo + i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    (sum * h / 3.0).clamp(0.0, 1.0)
}

/// Two-sided critical value of Dunnett's test.
pub fn dunnett_quantile(level: f64, lambdas: &[f64], df: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 10.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if dunnett_cdf(mid, lambdas, df) < level {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Games-Howell comparisons (unequal variances): Welch standard errors and
/// degrees of freedom, studentized range for the family of `k` groups.
pub fn games_howell(means: &[f64], vars: &[f64], ns: &[f64], pairs: &[(usize, usize)], k: usize, level: f64) -> Vec<Comparison> {
    pairs
        .iter()
        .map(|&(i, j)| {
            let (qi, qj) = (vars[i] / ns[i], vars[j] / ns[j]);
            let se = (qi + qj).sqrt();
            let df = (qi + qj).powi(2) / (qi * qi / (ns[i] - 1.0) + qj * qj / (ns[j] - 1.0));
            let diff = means[i] - means[j];
            let q = (diff / se).abs() * std::f64::consts::SQRT_2;
            let qc = qtukey(level, k as f64, df) / std::f64::consts::SQRT_2;
            Comparison {
                i,
                j,
                difference: diff,
                ci95: Some(Interval::new(diff - qc * se, diff + qc * se)),
                statistic: q,
                p_adjusted: (1.0 - ptukey(q, k as f64, df)).clamp(0.0, 1.0),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn studentized_range_matches_r() {
        assert_close(ptukey(3.5, 3.0, 20.0), 0.9441081509, 1e-8);
        assert_close(ptukey(2.1, 5.0, 12.0), 0.4100710518, 1e-8);
        assert_close(qtukey(0.95, 3.0, 20.0), 3.577934582, 1e-7);
    }

    #[test]
    fn dunnett_matches_published_critical_values() {
        // Dunnett (1964) two-sided 5% critical values, equal n, df = 20.
        let l = 0.5f64.sqrt();
        assert!((dunnett_quantile(0.95, &[l, l], 20.0) - 2.38).abs() < 0.01);
        assert!((dunnett_quantile(0.95, &[l, l, l], 20.0) - 2.54).abs() < 0.01);
        // One comparison is just a t test.
        assert!((dunnett_quantile(0.95, &[l], 20.0) - crate::dist::t_crit(0.95, 20.0)).abs() < 1e-3);
    }

    #[test]
    fn adjustments() {
        let p = [0.01, 0.04, 0.03, 0.005];
        let h = holm(&p);
        assert_close(h[3], 0.02, 1e-12);
        assert_close(h[0], 0.03, 1e-12);
        assert_close(h[1], 0.06, 1e-12);
        let bh = benjamini_hochberg(&p);
        assert_close(bh[1], 0.04, 1e-12);
        assert_close(bh[3], 0.02, 1e-12);
    }
}
