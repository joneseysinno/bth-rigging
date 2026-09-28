//! Display formatters shared by report and UI.
//!
//! Pure string helpers with no UI or store dependency. Roadmap: Step 0 foundation.

pub fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

pub fn format_ft_in(ft: f64, denominator: u32) -> String {
    if !ft.is_finite() {
        return "—".into();
    }
    let denominator = u64::from(denominator.max(1));
    let units_per_foot = 12 * denominator;
    let total_units = (ft.abs() * units_per_foot as f64).round() as u64;
    let feet = total_units / units_per_foot;
    let inch_units = total_units % units_per_foot;
    let inches = inch_units / denominator;
    let fraction = inch_units % denominator;
    let sign = if ft.is_sign_negative() && total_units > 0 {
        "-"
    } else {
        ""
    };

    let inch_text = if fraction == 0 {
        inches.to_string()
    } else {
        let divisor = gcd(fraction, denominator);
        let numerator = fraction / divisor;
        let reduced_denominator = denominator / divisor;
        if inches == 0 {
            format!("{numerator}/{reduced_denominator}")
        } else {
            format!("{inches} {numerator}/{reduced_denominator}")
        }
    };
    format!("{sign}{feet}'-{inch_text}\"")
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.max(1)
}

pub fn format_lbs(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let digits: String = out.chars().rev().collect();
    if n < 0 { format!("-{digits}") } else { digits }
}

pub fn format_updated(ms: u64) -> String {
    if ms == 0 {
        return "—".into();
    }
    let secs = ms / 1000;
    // Simple local-ish display without chrono dep: epoch days approximation
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hours = rem / 3600;
    let mins = (rem % 3600) / 60;
    format!("day {days} · {hours:02}:{mins:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::format_ft_in;

    #[test]
    fn formats_feet_inches_to_sixteenths() {
        assert_eq!(format_ft_in(8.5, 16), "8'-6\"");
        assert_eq!(format_ft_in(0.0520833, 16), "0'-5/8\"");
        assert_eq!(format_ft_in(12.99999, 16), "13'-0\"");
        assert_eq!(format_ft_in(-1.1041666666666667, 16), "-1'-1 1/4\"");
        assert_eq!(format_ft_in(-0.0520833, 16), "-0'-5/8\"");
    }

    #[test]
    fn formatter_handles_integer_inches_and_non_finite_values() {
        assert_eq!(format_ft_in(1.25, 16), "1'-3\"");
        assert_eq!(format_ft_in(1.0 / 12.0, 16), "0'-1\"");
        assert_eq!(format_ft_in(f64::NAN, 16), "—");
    }
}
