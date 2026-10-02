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

// ---------------------------------------------------------------- more indicators

fn roll(v: &[f64], n: usize, f: impl Fn(&[f64]) -> f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if n == 0 || v.len() < n {
        return out;
    }
    for i in n - 1..v.len() {
        out[i] = f(&v[i + 1 - n..=i]);
    }
    out
}

pub fn roll_max(v: &[f64], n: usize) -> Vec<f64> {
    roll(v, n, |w| w.iter().cloned().fold(f64::MIN, f64::max))
}

pub fn roll_min(v: &[f64], n: usize) -> Vec<f64> {
    roll(v, n, |w| w.iter().cloned().fold(f64::MAX, f64::min))
}

pub fn wma(v: &[f64], n: usize) -> Vec<f64> {
    let denom = (n * (n + 1)) as f64 / 2.0;
    roll(v, n, |w| w.iter().enumerate().map(|(k, x)| x * (k + 1) as f64).sum::<f64>() / denom)
}

/// Hull moving average.
pub fn hma(v: &[f64], n: usize) -> Vec<f64> {
    let half = wma(v, (n / 2).max(1));
    let full = wma(v, n);
    let raw: Vec<f64> = half.iter().zip(&full).map(|(a, b)| 2.0 * a - b).collect();
    wma(&raw, ((n as f64).sqrt().round() as usize).max(1))
}

/// Taiwan-style stochastic: K = (K·(ks-1) + RSV)/ks, D likewise over K. Returns (K, D, J).
pub fn kd(h: &[f64], l: &[f64], c: &[f64], n: usize, ks: usize, ds: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let len = c.len();
    let (hh, ll) = (roll_max(h, n), roll_min(l, n));
    let (mut k, mut d) = (50.0, 50.0);
    let (ks, ds) = (ks.max(1) as f64, ds.max(1) as f64);
    let mut ko = vec![f64::NAN; len];
    let mut dout = vec![f64::NAN; len];
    let mut jo = vec![f64::NAN; len];
    for i in 0..len {
        if hh[i].is_nan() {
            continue;
        }
        let rsv = if hh[i] > ll[i] { (c[i] - ll[i]) / (hh[i] - ll[i]) * 100.0 } else { k };
        k = (k * (ks - 1.0) + rsv) / ks;
        d = (d * (ds - 1.0) + k) / ds;
        ko[i] = k;
        dout[i] = d;
        jo[i] = 3.0 * k - 2.0 * d;
    }
    (ko, dout, jo)
}

pub fn cci(h: &[f64], l: &[f64], c: &[f64], n: usize) -> Vec<f64> {
    let tp: Vec<f64> = (0..c.len()).map(|i| (h[i] + l[i] + c[i]) / 3.0).collect();
    roll(&tp, n, |w| {
        let m = w.iter().sum::<f64>() / w.len() as f64;
        let md = w.iter().map(|x| (x - m).abs()).sum::<f64>() / w.len() as f64;
        if md == 0.0 { 0.0 } else { (w[w.len() - 1] - m) / (0.015 * md) }
    })
}

pub fn willr(h: &[f64], l: &[f64], c: &[f64], n: usize) -> Vec<f64> {
    let (hh, ll) = (roll_max(h, n), roll_min(l, n));
    (0..c.len()).map(|i| if hh[i] > ll[i] { -100.0 * (hh[i] - c[i]) / (hh[i] - ll[i]) } else { f64::NAN }).collect()
}

fn true_range(h: &[f64], l: &[f64], c: &[f64]) -> Vec<f64> {
    (0..c.len())
        .map(|i| {
            if i == 0 {
                h[0] - l[0]
            } else {
                (h[i] - l[i]).max((h[i] - c[i - 1]).abs()).max((l[i] - c[i - 1]).abs())
            }
        })
        .collect()
}

pub fn atr(h: &[f64], l: &[f64], c: &[f64], n: usize) -> Vec<f64> {
    let tr = true_range(h, l, c);
    let mut out = vec![f64::NAN; c.len()];
    if n == 0 || c.len() < n {
        return out;
    }
    let mut a = tr[..n].iter().sum::<f64>() / n as f64;
    out[n - 1] = a;
    for i in n..c.len() {
        a = (a * (n as f64 - 1.0) + tr[i]) / n as f64;
        out[i] = a;
    }
    out
}

pub fn obv(c: &[f64], v: &[f64]) -> Vec<f64> {
    let mut acc = 0.0;
    (0..c.len())
        .map(|i| {
            if i > 0 {
                acc += if c[i] > c[i - 1] { v[i] } else if c[i] < c[i - 1] { -v[i] } else { 0.0 };
            }
            acc
        })
        .collect()
}

pub fn roc(c: &[f64], n: usize) -> Vec<f64> {
    (0..c.len()).map(|i| if i >= n && c[i - n] != 0.0 { (c[i] / c[i - n] - 1.0) * 100.0 } else { f64::NAN }).collect()
}

pub fn mfi(h: &[f64], l: &[f64], c: &[f64], v: &[f64], n: usize) -> Vec<f64> {
    let len = c.len();
    let tp: Vec<f64> = (0..len).map(|i| (h[i] + l[i] + c[i]) / 3.0).collect();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let (mut pos, mut neg) = (0.0, 0.0);
        for k in i + 1 - n..=i {
            let flow = tp[k] * v[k];
            if tp[k] > tp[k - 1] { pos += flow } else if tp[k] < tp[k - 1] { neg += flow }
        }
        out[i] = if neg == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + pos / neg) };
    }
    out
}

/// Wilder DMI. Returns (+DI, -DI, ADX).
pub fn adx(h: &[f64], l: &[f64], c: &[f64], n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let len = c.len();
    let nan = || vec![f64::NAN; len];
    let (mut pdi, mut mdi, mut adx) = (nan(), nan(), nan());
    if n == 0 || len <= n {
        return (pdi, mdi, adx);
    }
    let tr = true_range(h, l, c);
    let nf = n as f64;
    let (mut str_, mut spdm, mut smdm) = (0.0, 0.0, 0.0);
    let mut dx = nan();
    for i in 1..len {
        let (up, dn) = (h[i] - h[i - 1], l[i - 1] - l[i]);
        let pdm = if up > dn && up > 0.0 { up } else { 0.0 };
        let mdm = if dn > up && dn > 0.0 { dn } else { 0.0 };
        if i <= n {
            str_ += tr[i];
            spdm += pdm;
            smdm += mdm;
        } else {
            str_ = str_ - str_ / nf + tr[i];
            spdm = spdm - spdm / nf + pdm;
            smdm = smdm - smdm / nf + mdm;
        }
        if i >= n && str_ > 0.0 {
            pdi[i] = 100.0 * spdm / str_;
            mdi[i] = 100.0 * smdm / str_;
            let s = pdi[i] + mdi[i];
            dx[i] = if s > 0.0 { 100.0 * (pdi[i] - mdi[i]).abs() / s } else { 0.0 };
        }
    }
    let first = 2 * n - 1;
    if len > first {
        let mut a = dx[n..=first].iter().filter(|x| x.is_finite()).sum::<f64>() / nf;
        adx[first] = a;
        for i in first + 1..len {
            a = (a * (nf - 1.0) + dx[i]) / nf;
            adx[i] = a;
        }
    }
    (pdi, mdi, adx)
}

pub fn bias(c: &[f64], n: usize) -> Vec<f64> {
    let m = sma(c, n);
    (0..c.len()).map(|i| if m[i].is_finite() && m[i] != 0.0 { (c[i] / m[i] - 1.0) * 100.0 } else { f64::NAN }).collect()
}

/// Parabolic SAR.
pub fn sar(h: &[f64], l: &[f64], step: f64, max: f64) -> Vec<f64> {
    let len = h.len();
    let mut out = vec![f64::NAN; len];
    if len < 2 {
        return out;
    }
    let mut up = true;
    let mut s = l[0];
    let mut ep = h[0];
    let mut af = step;
    for i in 1..len {
        s += af * (ep - s);
        if up {
            s = s.min(l[i - 1]);
            if i >= 2 {
                s = s.min(l[i - 2]);
            }
            if l[i] < s {
                up = false;
                s = ep;
                ep = l[i];
                af = step;
            } else if h[i] > ep {
                ep = h[i];
                af = (af + step).min(max);
            }
        } else {
            s = s.max(h[i - 1]);
            if i >= 2 {
                s = s.max(h[i - 2]);
            }
            if h[i] > s {
                up = true;
                s = ep;
                ep = h[i];
                af = step;
            } else if l[i] < ep {
                ep = l[i];
                af = (af + step).min(max);
            }
        }
        out[i] = s;
    }
    out
}

/// Move values `by` bars forward (positive) or backward (negative), padding with NaN.
pub fn shift(v: &[f64], by: i64) -> Vec<f64> {
    let len = v.len() as i64;
    (0..len)
        .map(|i| {
            let j = i - by;
            if j >= 0 && j < len { v[j as usize] } else { f64::NAN }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kd_stays_in_range_and_starts_at_fifty() {
        let h: Vec<f64> = (0..40).map(|i| 10.0 + (i as f64 * 0.5).sin() + 1.0).collect();
        let l: Vec<f64> = h.iter().map(|x| x - 2.0).collect();
        let c: Vec<f64> = h.iter().map(|x| x - 1.0).collect();
        let (k, d, _) = kd(&h, &l, &c, 9, 3, 3);
        assert!(k[7].is_nan());
        for i in 8..40 {
            assert!((0.0..=100.0).contains(&k[i]) && (0.0..=100.0).contains(&d[i]));
        }
    }

    #[test]
    fn adx_rising_series_is_trending() {
        let c: Vec<f64> = (0..80).map(|i| 100.0 + i as f64).collect();
        let h: Vec<f64> = c.iter().map(|x| x + 1.0).collect();
        let l: Vec<f64> = c.iter().map(|x| x - 1.0).collect();
        let (p, m, a) = adx(&h, &l, &c, 14);
        assert!(p[79] > m[79]);
        assert!(a[79] > 50.0);
    }

    #[test]
    fn shift_moves_forward() {
        assert_eq!(shift(&[1.0, 2.0, 3.0], 1)[1..], [1.0, 2.0]);
        assert_eq!(shift(&[1.0, 2.0, 3.0], -1)[..2], [2.0, 3.0]);
    }

    #[test]
    fn sar_flips_on_reversal() {
        let c: Vec<f64> = (0..30).map(|i| if i < 15 { 100.0 + i as f64 } else { 114.0 - (i - 15) as f64 * 2.0 }).collect();
        let h: Vec<f64> = c.iter().map(|x| x + 0.5).collect();
        let l: Vec<f64> = c.iter().map(|x| x - 0.5).collect();
        let s = sar(&h, &l, 0.02, 0.2);
        assert!(s[10] < c[10]);
        assert!(s[29] > c[29]);
    }
}
