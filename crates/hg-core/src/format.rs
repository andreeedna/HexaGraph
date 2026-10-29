//! Number formatting for result sheets.

/// Formats a number with about four significant digits.
pub fn num(v: f64) -> String {
    if v.is_nan() {
        return "—".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-4..1e6).contains(&a) {
        return format!("{v:.4e}");
    }
    let decimals = (3 - a.log10().floor() as i32).clamp(0, 6) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

pub fn opt(v: Option<f64>) -> String {
    v.map_or("—".into(), num)
}

pub fn interval(i: hg_stats::Interval) -> String {
    format!("{} to {}", num(i.low), num(i.high))
}

pub fn p(v: f64) -> String {
    hg_stats::format_p(v)
}

pub fn yes_no(b: bool) -> String {
    if b {
        "Yes".into()
    } else {
        "No".into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        assert_eq!(super::num(12.34567), "12.35");
        assert_eq!(super::num(0.012345), "0.01235");
        assert_eq!(super::num(1500.0), "1500");
        assert_eq!(super::num(2.0), "2");
    }
}
