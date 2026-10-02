//! Axis tick generation and number/date formatting shared by both chart kinds.

use crate::calc::DAY;
use chrono::{Datelike, NaiveDate};

pub fn ts_of(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
}

pub fn date_of(t: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp(t, 0).unwrap_or_default().date_naive()
}

pub fn fmt_date(t: i64) -> String {
    let d = date_of(t);
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

/// 1,234.5 style with thousands separators.
pub fn group(n: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, n.abs());
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s, None),
    };
    let mut out = String::new();
    for (i, ch) in int.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let mut out: String = out.chars().rev().collect();
    if let Some(f) = frac {
        out.push('.');
        out.push_str(&f);
    }
    if n < 0.0 && out.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.insert(0, '-');
    }
    out
}

pub fn fmt_price(p: f64) -> String {
    let d = if p.abs() >= 1000.0 { 0 } else if p.abs() >= 100.0 { 1 } else { 2 };
    group(p, d)
}

pub fn fmt_pct(r: f64) -> String {
    let p = r * 100.0;
    let d = if p.abs() >= 1000.0 { 0 } else if p.abs() >= 100.0 { 0 } else { 1 };
    format!("{}{}%", if p >= 0.0 { "+" } else { "" }, group(p, d))
}

pub fn fmt_pct_plain(r: f64) -> String {
    format!("{}%", group(r * 100.0, 1))
}

pub fn fmt_vol(v: f64) -> String {
    if v >= 1e9 {
        format!("{:.2}B", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.2}M", v / 1e6)
    } else if v >= 1e3 {
        format!("{:.1}K", v / 1e3)
    } else {
        format!("{v:.0}")
    }
}

/// Linear "nice" ticks (1-2-5 steps).
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !(max > min) {
        return vec![min];
    }
    let raw = (max - min) / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = mag
        * if norm < 1.5 { 1.0 } else if norm < 3.0 { 2.0 } else if norm < 7.0 { 5.0 } else { 10.0 };
    let mut v = (min / step).ceil() * step;
    let mut out = Vec::new();
    while v <= max + step * 1e-9 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    out
}

/// Ticks for a multiplicative scale: 1, 1.5, 2, 3, 5, 7.5 x 10^k within [min, max] (> 0).
pub fn log_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !(min > 0.0 && max > min) {
        return vec![];
    }
    let mut all = Vec::new();
    let k0 = min.log10().floor() as i32 - 1;
    let k1 = max.log10().ceil() as i32 + 1;
    for k in k0..=k1 {
        for m in [1.0, 1.5, 2.0, 3.0, 5.0, 7.5] {
            let v = m * 10f64.powi(k);
            if v >= min && v <= max {
                all.push(v);
            }
        }
    }
    if all.len() > target {
        // Thin: prefer the 1-2-5 members first, then drop to a regular stride.
        let strong: Vec<f64> = all
            .iter()
            .copied()
            .filter(|v| {
                let m = v / 10f64.powf(v.log10().floor());
                (m - 1.0).abs() < 1e-9 || (m - 2.0).abs() < 1e-9 || (m - 5.0).abs() < 1e-9
            })
            .collect();
        all = strong;
        while all.len() > target {
            all = all.iter().step_by(2).copied().collect();
        }
    }
    all
}

pub struct TimeTick {
    pub t: i64,
    pub label: String,
    pub major: bool,
}

/// Calendar-aligned time ticks between t0 and t1 (unix seconds) for `width_px` of axis.
pub fn time_ticks(t0: i64, t1: i64, width_px: f32) -> Vec<TimeTick> {
    let mut out = Vec::new();
    if t1 <= t0 {
        return out;
    }
    let max_labels = ((width_px / 78.0).floor() as i64).max(2);
    let span_days = (t1 - t0) as f64 / DAY as f64;
    let d0 = date_of(t0);
    let d1 = date_of(t1);
    if span_days > 365.0 * 1.6 {
        let years = span_days / 365.25;
        let mut step = 1;
        for s in [1, 2, 5, 10, 20, 25, 50] {
            step = s;
            if years / (s as f64) <= max_labels as f64 {
                break;
            }
        }
        let mut y = (d0.year() / step) * step;
        while y <= d1.year() {
            if let Some(d) = NaiveDate::from_ymd_opt(y, 1, 1) {
                let t = ts_of(d);
                if t >= t0 && t <= t1 {
                    out.push(TimeTick { t, label: format!("{y}"), major: true });
                }
            }
            y += step;
        }
    } else if span_days > 100.0 {
        let months = span_days / 30.4;
        let mut step = 1;
        for s in [1, 2, 3, 6] {
            step = s;
            if months / (s as f64) <= max_labels as f64 {
                break;
            }
        }
        let mut y = d0.year();
        let mut m = d0.month() as i32;
        loop {
            if let Some(d) = NaiveDate::from_ymd_opt(y, m as u32, 1) {
                let t = ts_of(d);
                if t > t1 {
                    break;
                }
                if t >= t0 && (m - 1) % step == 0 {
                    let jan = m == 1;
                    out.push(TimeTick {
                        t,
                        label: if jan { format!("{y}") } else { format!("{m}月") },
                        major: jan,
                    });
                }
            }
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
            if y > d1.year() + 1 {
                break;
            }
        }
    } else {
        let mut step = 1;
        for s in [1, 2, 7, 14, 30] {
            step = s;
            if span_days / (s as f64) <= max_labels as f64 {
                break;
            }
        }
        let mut t = (t0 / DAY) * DAY;
        while t <= t1 {
            if t >= t0 {
                let d = date_of(t);
                if (t / DAY) % step == 0 {
                    out.push(TimeTick {
                        t,
                        label: format!("{}/{}", d.month(), d.day()),
                        major: d.day() == 1,
                    });
                }
            }
            t += DAY;
        }
    }
    out
}

/// Spread labels vertically so none closer than `gap` px; keeps order, returns adjusted ys.
pub fn spread(ys: &[f32], gap: f32, lo: f32, hi: f32) -> Vec<f32> {
    let n = ys.len();
    if n == 0 {
        return vec![];
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| ys[a].partial_cmp(&ys[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut pos: Vec<f32> = idx.iter().map(|&i| ys[i].clamp(lo, hi)).collect();
    for _ in 0..40 {
        let mut moved = false;
        for i in 1..n {
            let d = pos[i] - pos[i - 1];
            if d < gap {
                let push = (gap - d) / 2.0;
                pos[i - 1] -= push;
                pos[i] += push;
                moved = true;
            }
        }
        for p in pos.iter_mut() {
            *p = p.clamp(lo, hi);
        }
        if !moved {
            break;
        }
    }
    let mut out = vec![0.0; n];
    for (k, &i) in idx.iter().enumerate() {
        out[i] = pos[k];
    }
    out
}
