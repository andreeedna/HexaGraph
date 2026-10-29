//! Normality tests.

use crate::descriptive::{mean, sd, sorted};
use crate::{dist, need, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalityTest {
    pub statistic: f64,
    pub p: f64,
}

fn poly(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x + k)
}

/// Shapiro-Wilk W test (Royston 1995, algorithm AS R94). 3 <= n <= 5000.
pub fn shapiro_wilk(x: &[f64]) -> Result<NormalityTest> {
    let n = x.len();
    need(n >= 3, "Shapiro-Wilk needs at least 3 values")?;
    let x = sorted(x);
    let range = x[n - 1] - x[0];
    need(range > 1e-19, "all values are identical")?;
    let nn2 = n / 2;
    let an = n as f64;
    let mut a = vec![0.0; nn2 + 1]; // 1 based
    if n == 3 {
        a[1] = std::f64::consts::FRAC_1_SQRT_2;
    } else {
        const C1: [f64; 6] = [0.0, 0.221157, -0.147981, -2.07119, 4.434685, -2.706056];
        const C2: [f64; 6] = [0.0, 0.042981, -0.293762, -1.752461, 5.682633, -3.582633];
        let an25 = an + 0.25;
        let mut summ2 = 0.0;
        for (i, ai) in a.iter_mut().enumerate().skip(1) {
            *ai = dist::norm_inv((i as f64 - 0.375) / an25);
            summ2 += *ai * *ai;
        }
        summ2 *= 2.0;
        let ssumm2 = summ2.sqrt();
        let rsn = 1.0 / an.sqrt();
        let a1 = poly(&C1, rsn) - a[1] / ssumm2;
        let (i1, fac) = if n > 5 {
            let a2 = -a[2] / ssumm2 + poly(&C2, rsn);
            let fac = ((summ2 - 2.0 * a[1] * a[1] - 2.0 * a[2] * a[2]) / (1.0 - 2.0 * a1 * a1 - 2.0 * a2 * a2)).sqrt();
            a[2] = a2;
            (3, fac)
        } else {
            (2, ((summ2 - 2.0 * a[1] * a[1]) / (1.0 - 2.0 * a1 * a1)).sqrt())
        };
        a[1] = a1;
        for ai in a.iter_mut().skip(i1) {
            *ai /= -fac;
        }
    }
    // Full coefficient vector, antisymmetric around the middle.
    let coef: Vec<f64> = (0..n)
        .map(|i| {
            let j = n - 1 - i;
            match i.cmp(&j) {
                std::cmp::Ordering::Less => -a[i + 1],
                std::cmp::Ordering::Greater => a[j + 1],
                std::cmp::Ordering::Equal => 0.0,
            }
        })
        .collect();
    let xs: Vec<f64> = x.iter().map(|v| v / range).collect();
    let (ma, mx) = (mean(&coef), mean(&xs));
    let (mut ssa, mut ssx, mut sax) = (0.0, 0.0, 0.0);
    for (c, v) in coef.iter().zip(&xs) {
        let (da, dx) = (c - ma, v - mx);
        ssa += da * da;
        ssx += dx * dx;
        sax += da * dx;
    }
    let ssassx = (ssa * ssx).sqrt();
    let w1 = (ssassx - sax) * (ssassx + sax) / (ssa * ssx);
    let w = 1.0 - w1;
    if n == 3 {
        let p = (1.909_859_317_102_74 * (w.sqrt().asin() - std::f64::consts::FRAC_PI_3)).max(0.0);
        return Ok(NormalityTest { statistic: w, p });
    }
    let y = w1.ln();
    let lxx = an.ln();
    let (y, m, s) = if n <= 11 {
        let gamma = poly(&[-2.273, 0.459], an);
        if y >= gamma {
            return Ok(NormalityTest { statistic: w, p: 1e-99 });
        }
        (-(gamma - y).ln(), poly(&[0.544, -0.39978, 0.025054, -6.714e-4], an), poly(&[1.3822, -0.77857, 0.062767, -0.0020322], an).exp())
    } else {
        (y, poly(&[-1.5861, -0.31082, -0.083751, 0.0038915], lxx), poly(&[-0.4803, -0.082676, 0.0030302], lxx).exp())
    };
    Ok(NormalityTest { statistic: w, p: dist::norm_sf((y - m) / s) })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DAgostino {
    pub k2: f64,
    pub p: f64,
    pub z_skewness: f64,
    pub z_kurtosis: f64,
}

/// D'Agostino-Pearson omnibus K² test. Needs n >= 8.
pub fn dagostino_pearson(x: &[f64]) -> Result<DAgostino> {
    let n = x.len() as f64;
    need(x.len() >= 8, "D'Agostino-Pearson needs at least 8 values")?;
    let m = mean(x);
    let m2 = x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n;
    let m3 = x.iter().map(|v| (v - m).powi(3)).sum::<f64>() / n;
    let m4 = x.iter().map(|v| (v - m).powi(4)).sum::<f64>() / n;
    need(m2 > 0.0, "all values are identical")?;
    let b1 = m3 / m2.powf(1.5);
    let b2 = m4 / (m2 * m2);

    let y = b1 * ((n + 1.0) * (n + 3.0) / (6.0 * (n - 2.0))).sqrt();
    let beta2 = 3.0 * (n * n + 27.0 * n - 70.0) * (n + 1.0) * (n + 3.0) / ((n - 2.0) * (n + 5.0) * (n + 7.0) * (n + 9.0));
    let w2 = -1.0 + (2.0 * (beta2 - 1.0)).sqrt();
    let delta = 1.0 / (0.5 * w2.ln()).sqrt();
    let alpha = (2.0 / (w2 - 1.0)).sqrt();
    let y = if y == 0.0 { 1.0 } else { y };
    let zs = delta * (y / alpha + ((y / alpha).powi(2) + 1.0).sqrt()).ln();

    let e = 3.0 * (n - 1.0) / (n + 1.0);
    let varb2 = 24.0 * n * (n - 2.0) * (n - 3.0) / ((n + 1.0).powi(2) * (n + 3.0) * (n + 5.0));
    let xk = (b2 - e) / varb2.sqrt();
    let sqrtbeta1 =
        6.0 * (n * n - 5.0 * n + 2.0) / ((n + 7.0) * (n + 9.0)) * (6.0 * (n + 3.0) * (n + 5.0) / (n * (n - 2.0) * (n - 3.0))).sqrt();
    let a = 6.0 + 8.0 / sqrtbeta1 * (2.0 / sqrtbeta1 + (1.0 + 4.0 / (sqrtbeta1 * sqrtbeta1)).sqrt());
    let term1 = 1.0 - 2.0 / (9.0 * a);
    let denom = 1.0 + xk * (2.0 / (a - 4.0)).sqrt();
    let term2 = denom.signum() * ((1.0 - 2.0 / a) / denom.abs()).cbrt();
    let zk = (term1 - term2) / (2.0 / (9.0 * a)).sqrt();

    let k2 = zs * zs + zk * zk;
    Ok(DAgostino { k2, p: dist::chi2_sf(k2, 2.0), z_skewness: zs, z_kurtosis: zk })
}

/// Anderson-Darling test with estimated mean and SD (Stephens' modification).
pub fn anderson_darling(x: &[f64]) -> Result<NormalityTest> {
    let n = x.len();
    need(n >= 8, "Anderson-Darling needs at least 8 values")?;
    let s = sorted(x);
    let (m, sdv) = (mean(&s), sd(&s));
    need(sdv > 0.0, "all values are identical")?;
    let nf = n as f64;
    let p: Vec<f64> = s.iter().map(|v| dist::norm_cdf((v - m) / sdv).clamp(1e-300, 1.0 - 1e-16)).collect();
    let sum: f64 = (0..n).map(|i| (2.0 * i as f64 + 1.0) * (p[i].ln() + (1.0 - p[n - 1 - i]).ln())).sum();
    let a = -nf - sum / nf;
    let aa = a * (1.0 + 0.75 / nf + 2.25 / (nf * nf));
    let pv = if aa < 0.2 {
        1.0 - (-13.436 + 101.14 * aa - 223.73 * aa * aa).exp()
    } else if aa < 0.34 {
        1.0 - (-8.318 + 42.796 * aa - 59.938 * aa * aa).exp()
    } else if aa < 0.6 {
        (0.9177 - 4.279 * aa - 1.38 * aa * aa).exp()
    } else {
        (1.2937 - 5.709 * aa + 0.0186 * aa * aa).exp()
    };
    Ok(NormalityTest { statistic: a, p: pv.clamp(0.0, 1.0) })
}

/// Kolmogorov-Smirnov test with Lilliefors correction
/// (Dallal-Wilkinson approximation for the P value).
pub fn kolmogorov_smirnov(x: &[f64]) -> Result<NormalityTest> {
    let n = x.len();
    need(n >= 5, "Kolmogorov-Smirnov needs at least 5 values")?;
    let s = sorted(x);
    let (m, sdv) = (mean(&s), sd(&s));
    need(sdv > 0.0, "all values are identical")?;
    let nf = n as f64;
    let mut d: f64 = 0.0;
    for (i, v) in s.iter().enumerate() {
        let f = dist::norm_cdf((v - m) / sdv);
        d = d.max((i as f64 + 1.0) / nf - f).max(f - i as f64 / nf);
    }
    let (kd, nd) = if n <= 100 { (d, nf) } else { (d * (nf / 100.0).powf(0.49), 100.0) };
    let mut p =
        (-7.01256 * kd * kd * (nd + 2.78019) + 2.99587 * kd * (nd + 2.78019).sqrt() - 0.122119 + 0.974598 / nd.sqrt() + 1.67997 / nd).exp();
    if p > 0.1 {
        let kk = (nf.sqrt() - 0.01 + 0.85 / nf.sqrt()) * d;
        p = if kk <= 0.302 {
            1.0
        } else if kk <= 0.5 {
            2.76773 - 19.828315 * kk + 80.709644 * kk.powi(2) - 138.55152 * kk.powi(3) + 81.218052 * kk.powi(4)
        } else if kk <= 0.9 {
            -4.901232 + 40.662806 * kk - 97.490286 * kk.powi(2) + 94.029866 * kk.powi(3) - 32.355711 * kk.powi(4)
        } else if kk <= 1.31 {
            6.198765 - 19.558097 * kk + 23.186922 * kk.powi(2) - 12.234627 * kk.powi(3) + 2.423045 * kk.powi(4)
        } else {
            0.0
        };
    }
    Ok(NormalityTest { statistic: d, p: p.clamp(0.0, 1.0) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use crate::ttest::tests::{A, B, C};

    fn abc() -> Vec<f64> {
        A.iter().chain(&B).chain(&C).copied().collect()
    }

    #[test]
    fn shapiro_matches_r() {
        let r = shapiro_wilk(&A).unwrap();
        assert_close(r.statistic, 0.9766168888, 1e-8);
        assert_close(r.p, 0.9442726347, 1e-6);
        let r = shapiro_wilk(&abc()).unwrap();
        assert_close(r.statistic, 0.9806618265, 1e-8);
        assert_close(r.p, 0.9260309642, 1e-6);
    }

    #[test]
    fn other_tests_match_scipy() {
        let r = dagostino_pearson(&abc()).unwrap();
        assert_close(r.k2, REF_K2, 1e-8);
        assert_close(r.p, REF_K2_P, 1e-8);
        let r = anderson_darling(&abc()).unwrap();
        assert_close(r.statistic, REF_AD, 1e-8);
    }

    const REF_K2: f64 = 0.8423487501643326;
    const REF_K2_P: f64 = 0.6562756533100431;
    const REF_AD: f64 = 0.13623245194517963;
}
