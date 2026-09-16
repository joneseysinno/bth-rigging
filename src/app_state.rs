//! Shared app context and formatting helpers.

use crate::db::RiggingStore;

#[derive(Clone)]
pub struct AppCtx {
    pub store: Option<RiggingStore>,
}

pub fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
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
    if n < 0 {
        format!("-{digits}")
    } else {
        digits
    }
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
