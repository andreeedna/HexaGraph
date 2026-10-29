//! Axis scaling and tick generation.

use hg_core::Scale;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub min: f64,
    pub max: f64,
    pub scale: Scale,
    /// Pixel positions of min and max.
    pub p0: f32,
    pub p1: f32,
}

impl Axis {
    fn t(&self, v: f64) -> f64 {
        match self.scale {
            Scale::Linear => v,
            Scale::Log10 => v.max(1e-300).log10(),
        }
    }

    pub fn map(&self, v: f64) -> f32 {
        let (a, b) = (self.t(self.min), self.t(self.max));
        let f = if b == a { 0.5 } else { (self.t(v) - a) / (b - a) };
        self.p0 + (self.p1 - self.p0) * f as f32
    }

    pub fn ticks(&self) -> Vec<f64> {
        match self.scale {
            Scale::Linear => nice_ticks(self.min, self.max, 6),
            Scale::Log10 => {
                let (lo, hi) = (self.min.log10().floor() as i32, self.max.log10().ceil() as i32);
                (lo..=hi).map(|e| 10f64.powi(e)).filter(|v| *v >= self.min * 0.999 && *v <= self.max * 1.001).collect()
            }
        }
    }
}

pub fn nice_step(range: f64, target: usize) -> f64 {
    let raw = range / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let nice = if f < 1.5 {
        1.0
    } else if f < 3.0 {
        2.0
    } else if f < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !(max > min) {
        return vec![min];
    }
    let step = nice_step(max - min, target);
    let start = (min / step).ceil() * step;
    let mut v = Vec::new();
    let mut x = start;
    while x <= max + step * 1e-9 {
        v.push(if x.abs() < step * 1e-9 { 0.0 } else { x });
        x += step;
    }
    v
}

/// Expands a data range to "nice" axis limits.
pub fn nice_range(min: f64, max: f64, scale: Scale, include_zero: bool) -> (f64, f64) {
    match scale {
        Scale::Log10 => {
            let lo = if min > 0.0 { 10f64.powf(min.log10().floor()) } else { 1e-3 };
            let hi = if max > 0.0 { 10f64.powf(max.log10().ceil()) } else { 1.0 };
            (lo, if hi <= lo { lo * 10.0 } else { hi })
        }
        Scale::Linear => {
            let (mut lo, mut hi) = (min, max);
            if include_zero {
                lo = lo.min(0.0);
                hi = hi.max(0.0);
            }
            if !(hi > lo) {
                let d = if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
                lo -= d;
                hi += d;
            }
            let step = nice_step(hi - lo, 6);
            let nlo = (lo / step).floor() * step;
            let nhi = (hi / step).ceil() * step;
            (if include_zero && min >= 0.0 { nlo.max(0.0) } else { nlo }, nhi)
        }
    }
}

pub fn tick_label(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && !(1e-3..1e5).contains(&a) {
        let e = a.log10().round() as i32;
        if (v / 10f64.powi(e) - 1.0).abs() < 1e-9 {
            return format!("10^{e}");
        }
        return format!("{v:.1e}");
    }
    hg_core::format::num(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_nice() {
        assert_eq!(nice_ticks(0.0, 10.0, 5), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(nice_range(1.3, 97.0, Scale::Linear, true), (0.0, 100.0));
    }
}
