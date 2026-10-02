//! Indicators and comparison statistics. Pure functions over bar slices.

use crate::data::Bar;

pub const DAY: i64 = 86_400;

pub fn sma(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if n == 0 || v.len() < n {
        return out;
    }
    let mut sum: f64 = v[..n].iter().sum();
    out[n - 1] = sum / n as f64;
    for i in n..v.len() {
        sum += v[i] - v[i - n];
        out[i] = sum / n as f64;
    }
    out
}

pub fn ema(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if n == 0 || v.is_empty() {
        return out;
    }
    let k = 2.0 / (n as f64 + 1.0);
    let mut prev = v[0];
    out[0] = prev;
    for i in 1..v.len() {
        prev = v[i] * k + prev * (1.0 - k);
        out[i] = prev;
    }
    out
}

/// Returns (mid, upper, lower).
pub fn bollinger(v: &[f64], n: usize, k: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mid = sma(v, n);
    let mut up = vec![f64::NAN; v.len()];
    let mut lo = vec![f64::NAN; v.len()];
    for i in n.saturating_sub(1)..v.len() {
        if v.len() < n {
            break;
        }
        let w = &v[i + 1 - n..=i];
        let m = mid[i];
        let sd = (w.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n as f64).sqrt();
        up[i] = m + k * sd;
        lo[i] = m - k * sd;
    }
    (mid, up, lo)
}

/// Wilder RSI.
pub fn rsi(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if v.len() <= n {
        return out;
    }
    let (mut g, mut l) = (0.0, 0.0);
    for i in 1..=n {
        let d = v[i] - v[i - 1];
        if d >= 0.0 { g += d } else { l -= d }
    }
    g /= n as f64;
    l /= n as f64;
    let f = |g: f64, l: f64| if l == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + g / l) };
    out[n] = f(g, l);
    for i in n + 1..v.len() {
        let d = v[i] - v[i - 1];
        let (dg, dl) = if d >= 0.0 { (d, 0.0) } else { (0.0, -d) };
        g = (g * (n as f64 - 1.0) + dg) / n as f64;
        l = (l * (n as f64 - 1.0) + dl) / n as f64;
        out[i] = f(g, l);
    }
    out
}

/// Returns (macd, signal, histogram).
pub fn macd(v: &[f64], fast: usize, slow: usize, sig: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let ef = ema(v, fast);
    let es = ema(v, slow);
    let m: Vec<f64> = ef.iter().zip(&es).map(|(a, b)| a - b).collect();
    let s = ema(&m, sig);
    let h: Vec<f64> = m.iter().zip(&s).map(|(a, b)| a - b).collect();
    (m, s, h)
}

/// Resample daily bars to weekly / monthly (ISO week / calendar month, UTC).
pub fn resample(bars: &[Bar], unit: Resample) -> Vec<Bar> {
    if unit == Resample::Day {
        return bars.to_vec();
    }
    let key = |t: i64| -> i64 {
        let d = chrono::DateTime::from_timestamp(t, 0).unwrap_or_default().date_naive();
        match unit {
            Resample::Week => {
                use chrono::Datelike;
                let w = d.iso_week();
                w.year() as i64 * 100 + w.week() as i64
            }
            _ => {
                use chrono::Datelike;
                d.year() as i64 * 100 + d.month() as i64
            }
        }
    };
    let mut out: Vec<Bar> = Vec::new();
    let mut last_key = i64::MIN;
    for b in bars {
        let k = key(b.t);
        if k != last_key {
            out.push(*b);
            last_key = k;
        } else if let Some(o) = out.last_mut() {
            o.h = o.h.max(b.h);
            o.l = o.l.min(b.l);
            o.c = b.c;
            o.adj = b.adj;
            o.v += b.v;
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resample {
    Day,
    Week,
    Month,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub total_return: f64,
    pub cagr: f64,
    pub max_drawdown: f64,
    pub volatility: f64,
    pub sharpe: f64,
    pub years: f64,
    pub start_t: i64,
    pub end_t: i64,
}

/// Stats over `bars` within [t0, t1] using the chosen price basis. Returns None when <2 points.
pub fn stats(bars: &[Bar], t0: i64, t1: i64, total_return: bool) -> Option<Stats> {
    let px = |b: &Bar| if total_return { b.adj } else { b.c };
    let w: Vec<&Bar> = bars.iter().filter(|b| b.t >= t0 && b.t <= t1).collect();
    if w.len() < 2 {
        return None;
    }
    let first = px(w[0]);
    let last = px(w[w.len() - 1]);
    let years = ((w[w.len() - 1].t - w[0].t) as f64 / (365.25 * DAY as f64)).max(1.0 / 365.25);
    let total = last / first - 1.0;
    let cagr = if years >= 1.0 { (last / first).powf(1.0 / years) - 1.0 } else { total };
    let mut peak = f64::MIN;
    let mut mdd = 0.0f64;
    let mut rets = Vec::with_capacity(w.len());
    for i in 0..w.len() {
        let p = px(w[i]);
        peak = peak.max(p);
        mdd = mdd.min(p / peak - 1.0);
        if i > 0 {
            rets.push(p / px(w[i - 1]) - 1.0);
        }
    }
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let per_year = n / years;
    let vol = var.sqrt() * per_year.sqrt();
    let sharpe = if vol > 0.0 { (mean * per_year) / vol } else { 0.0 };
    Some(Stats {
        total_return: total,
        cagr,
        max_drawdown: mdd,
        volatility: vol,
        sharpe,
        years,
        start_t: w[0].t,
        end_t: w[w.len() - 1].t,
    })
}
