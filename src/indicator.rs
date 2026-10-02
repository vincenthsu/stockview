//! User-configurable technical indicators: catalogue, parameter specs and evaluation.

use crate::calc;
use crate::data::Bar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Sma,
    Ema,
    Wma,
    Hma,
    Boll,
    Envelope,
    Donchian,
    Keltner,
    Sar,
    Ichimoku,
    Rsi,
    Macd,
    Kd,
    Kdj,
    Cci,
    Willr,
    Atr,
    Obv,
    Roc,
    Mfi,
    Adx,
    Bias,
}

pub struct Param {
    pub name: &'static str,
    pub def: f64,
    pub min: f64,
    pub max: f64,
    pub int: bool,
}

// macros (not const fns) so the slices below are promoted to 'static
macro_rules! pi {
    ($name:expr, $def:expr) => {
        Param { name: $name, def: $def, min: 1.0, max: 1000.0, int: true }
    };
}
macro_rules! pf {
    ($name:expr, $def:expr, $min:expr, $max:expr) => {
        Param { name: $name, def: $def, min: $min, max: $max, int: false }
    };
}

impl Kind {
    pub const OVERLAYS: [Kind; 10] = [
        Kind::Sma,
        Kind::Ema,
        Kind::Wma,
        Kind::Hma,
        Kind::Boll,
        Kind::Envelope,
        Kind::Donchian,
        Kind::Keltner,
        Kind::Sar,
        Kind::Ichimoku,
    ];
    pub const PANES: [Kind; 12] = [
        Kind::Rsi,
        Kind::Macd,
        Kind::Kd,
        Kind::Kdj,
        Kind::Cci,
        Kind::Willr,
        Kind::Atr,
        Kind::Obv,
        Kind::Roc,
        Kind::Mfi,
        Kind::Adx,
        Kind::Bias,
    ];

    pub fn overlay(self) -> bool {
        Self::OVERLAYS.contains(&self)
    }

    /// Overlays whose envelope should stay inside the price scale.
    pub fn is_band(self) -> bool {
        matches!(self, Kind::Boll | Kind::Envelope | Kind::Donchian | Kind::Keltner)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Sma => "SMA 簡單移動平均",
            Kind::Ema => "EMA 指數移動平均",
            Kind::Wma => "WMA 加權移動平均",
            Kind::Hma => "HMA 赫爾移動平均",
            Kind::Boll => "布林通道",
            Kind::Envelope => "包絡線",
            Kind::Donchian => "唐奇安通道",
            Kind::Keltner => "肯特納通道",
            Kind::Sar => "拋物線 SAR",
            Kind::Ichimoku => "一目均衡表",
            Kind::Rsi => "RSI 相對強弱",
            Kind::Macd => "MACD",
            Kind::Kd => "KD 隨機指標",
            Kind::Kdj => "KDJ",
            Kind::Cci => "CCI 順勢指標",
            Kind::Willr => "威廉 %R",
            Kind::Atr => "ATR 真實波幅",
            Kind::Obv => "OBV 能量潮",
            Kind::Roc => "ROC 變動率",
            Kind::Mfi => "MFI 資金流量",
            Kind::Adx => "DMI / ADX 趨向",
            Kind::Bias => "BIAS 乖離率",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Kind::Sma => "SMA",
            Kind::Ema => "EMA",
            Kind::Wma => "WMA",
            Kind::Hma => "HMA",
            Kind::Boll => "BB",
            Kind::Envelope => "ENV",
            Kind::Donchian => "DC",
            Kind::Keltner => "KC",
            Kind::Sar => "SAR",
            Kind::Ichimoku => "ICH",
            Kind::Rsi => "RSI",
            Kind::Macd => "MACD",
            Kind::Kd => "KD",
            Kind::Kdj => "KDJ",
            Kind::Cci => "CCI",
            Kind::Willr => "%R",
            Kind::Atr => "ATR",
            Kind::Obv => "OBV",
            Kind::Roc => "ROC",
            Kind::Mfi => "MFI",
            Kind::Adx => "ADX",
            Kind::Bias => "BIAS",
        }
    }

    pub fn params(self) -> &'static [Param] {
        match self {
            Kind::Sma | Kind::Wma | Kind::Hma => &[pi!("週期", 20.0)],
            Kind::Ema => &[pi!("週期", 50.0)],
            Kind::Boll => &[pi!("週期", 20.0), pf!("倍數", 2.0, 0.1, 10.0)],
            Kind::Envelope => &[pi!("週期", 20.0), pf!("幅度%", 2.5, 0.1, 50.0)],
            Kind::Donchian => &[pi!("週期", 20.0)],
            Kind::Keltner => &[pi!("EMA", 20.0), pi!("ATR", 10.0), pf!("倍數", 2.0, 0.1, 10.0)],
            Kind::Sar => &[pf!("步長", 0.02, 0.001, 1.0), pf!("上限", 0.2, 0.01, 1.0)],
            Kind::Ichimoku => &[pi!("轉換", 9.0), pi!("基準", 26.0), pi!("先行B", 52.0)],
            Kind::Rsi => &[pi!("週期", 14.0)],
            Kind::Macd => &[pi!("快線", 12.0), pi!("慢線", 26.0), pi!("訊號", 9.0)],
            Kind::Kd | Kind::Kdj => &[pi!("週期", 9.0), pi!("K平滑", 3.0), pi!("D平滑", 3.0)],
            Kind::Cci => &[pi!("週期", 20.0)],
            Kind::Willr => &[pi!("週期", 14.0)],
            Kind::Atr => &[pi!("週期", 14.0)],
            Kind::Obv => &[],
            Kind::Roc => &[pi!("週期", 12.0)],
            Kind::Mfi => &[pi!("週期", 14.0)],
            Kind::Adx => &[pi!("週期", 14.0)],
            Kind::Bias => &[pi!("週期", 20.0)],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Solid,
    Soft,
    Dots,
}

pub struct Ln {
    pub name: &'static str,
    pub vals: Vec<f64>,
    pub slot: usize,
    pub style: Style,
}

#[derive(Default)]
pub struct Out {
    pub lines: Vec<Ln>,
    pub hist: Option<Vec<f64>>,
    pub levels: &'static [f64],
    pub fixed: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cfg {
    pub kind: Kind,
    pub params: Vec<f64>,
    pub enabled: bool,
    pub color: usize,
}

impl Cfg {
    pub fn new(kind: Kind, color: usize) -> Self {
        Self { kind, params: kind.params().iter().map(|p| p.def).collect(), enabled: true, color }
    }

    fn with(kind: Kind, params: &[f64], color: usize, enabled: bool) -> Self {
        Self { kind, params: params.to_vec(), enabled, color }
    }

    /// Repair a config coming from an old or hand-edited file.
    pub fn sanitize(&mut self) {
        let specs = self.kind.params();
        self.params.resize(specs.len(), 0.0);
        for (v, s) in self.params.iter_mut().zip(specs) {
            if !v.is_finite() || *v == 0.0 && s.min > 0.0 {
                *v = s.def;
            }
            *v = v.clamp(s.min, s.max);
            if s.int {
                *v = v.round();
            }
        }
        self.color %= 8;
    }

    fn p(&self, i: usize) -> f64 {
        self.params.get(i).copied().unwrap_or_else(|| self.kind.params()[i].def)
    }
    fn n(&self, i: usize) -> usize {
        self.p(i).round().max(1.0) as usize
    }

    pub fn title(&self) -> String {
        if self.kind.params().is_empty() {
            return self.kind.short().to_string();
        }
        let ps: Vec<String> = (0..self.kind.params().len()).map(|i| format!("{}", self.p(i))).collect();
        format!("{}({})", self.kind.short(), ps.join(","))
    }

    pub fn compute(&self, bars: &[Bar]) -> Out {
        let c: Vec<f64> = bars.iter().map(|b| b.c).collect();
        let h: Vec<f64> = bars.iter().map(|b| b.h).collect();
        let l: Vec<f64> = bars.iter().map(|b| b.l).collect();
        let v: Vec<f64> = bars.iter().map(|b| b.v).collect();
        let ln = |name, vals, slot, style| Ln { name, vals, slot, style };
        use Style::*;
        let mut o = Out::default();
        match self.kind {
            Kind::Sma => o.lines.push(ln("", calc::sma(&c, self.n(0)), 0, Solid)),
            Kind::Ema => o.lines.push(ln("", calc::ema(&c, self.n(0)), 0, Solid)),
            Kind::Wma => o.lines.push(ln("", calc::wma(&c, self.n(0)), 0, Solid)),
            Kind::Hma => o.lines.push(ln("", calc::hma(&c, self.n(0)), 0, Solid)),
            Kind::Boll => {
                let (mid, up, lo) = calc::bollinger(&c, self.n(0), self.p(1));
                o.lines = vec![ln("上", up, 0, Solid), ln("中", mid, 0, Soft), ln("下", lo, 0, Solid)];
            }
            Kind::Envelope => {
                let mid = calc::sma(&c, self.n(0));
                let k = self.p(1) / 100.0;
                let up = mid.iter().map(|m| m * (1.0 + k)).collect();
                let lo = mid.iter().map(|m| m * (1.0 - k)).collect();
                o.lines = vec![ln("上", up, 0, Solid), ln("中", mid, 0, Soft), ln("下", lo, 0, Solid)];
            }
            Kind::Donchian => {
                let (up, lo) = (calc::roll_max(&h, self.n(0)), calc::roll_min(&l, self.n(0)));
                let mid = up.iter().zip(&lo).map(|(a, b)| (a + b) / 2.0).collect();
                o.lines = vec![ln("上", up, 0, Solid), ln("中", mid, 0, Soft), ln("下", lo, 0, Solid)];
            }
            Kind::Keltner => {
                let mid = calc::ema(&c, self.n(0));
                let a = calc::atr(&h, &l, &c, self.n(1));
                let k = self.p(2);
                let up = mid.iter().zip(&a).map(|(m, a)| m + k * a).collect();
                let lo = mid.iter().zip(&a).map(|(m, a)| m - k * a).collect();
                o.lines = vec![ln("上", up, 0, Solid), ln("中", mid, 0, Soft), ln("下", lo, 0, Solid)];
            }
            Kind::Sar => o.lines.push(ln("", calc::sar(&h, &l, self.p(0), self.p(1)), 0, Dots)),
            Kind::Ichimoku => {
                let mid = |n: usize| -> Vec<f64> {
                    let (a, b) = (calc::roll_max(&h, n), calc::roll_min(&l, n));
                    a.iter().zip(&b).map(|(x, y)| (x + y) / 2.0).collect()
                };
                let (tk, kj) = (mid(self.n(0)), mid(self.n(1)));
                let sh = self.n(1) as i64;
                let sa: Vec<f64> = tk.iter().zip(&kj).map(|(a, b)| (a + b) / 2.0).collect();
                o.lines = vec![
                    ln("轉換", tk, 0, Solid),
                    ln("基準", kj, 1, Solid),
                    ln("先行A", calc::shift(&sa, sh), 2, Soft),
                    ln("先行B", calc::shift(&mid(self.n(2)), sh), 3, Soft),
                    ln("遲行", calc::shift(&c, -sh), 4, Soft),
                ];
            }
            Kind::Rsi => {
                o.lines.push(ln("", calc::rsi(&c, self.n(0)), 0, Solid));
                o.levels = &[30.0, 50.0, 70.0];
                o.fixed = Some((0.0, 100.0));
            }
            Kind::Macd => {
                let (m, s, hist) = calc::macd(&c, self.n(0), self.n(1), self.n(2));
                o.lines = vec![ln("DIF", m, 0, Solid), ln("MACD", s, 1, Solid)];
                o.hist = Some(hist);
            }
            Kind::Kd | Kind::Kdj => {
                let (k, d, j) = calc::kd(&h, &l, &c, self.n(0), self.n(1), self.n(2));
                o.lines = vec![ln("K", k, 0, Solid), ln("D", d, 1, Solid)];
                if self.kind == Kind::Kdj {
                    o.lines.push(ln("J", j, 2, Solid));
                } else {
                    o.fixed = Some((0.0, 100.0));
                }
                o.levels = &[20.0, 80.0];
            }
            Kind::Cci => {
                o.lines.push(ln("", calc::cci(&h, &l, &c, self.n(0)), 0, Solid));
                o.levels = &[-100.0, 100.0];
            }
            Kind::Willr => {
                o.lines.push(ln("", calc::willr(&h, &l, &c, self.n(0)), 0, Solid));
                o.levels = &[-80.0, -20.0];
                o.fixed = Some((-100.0, 0.0));
            }
            Kind::Atr => o.lines.push(ln("", calc::atr(&h, &l, &c, self.n(0)), 0, Solid)),
            Kind::Obv => o.lines.push(ln("", calc::obv(&c, &v), 0, Solid)),
            Kind::Roc => {
                o.lines.push(ln("", calc::roc(&c, self.n(0)), 0, Solid));
                o.levels = &[0.0];
            }
            Kind::Mfi => {
                o.lines.push(ln("", calc::mfi(&h, &l, &c, &v, self.n(0)), 0, Solid));
                o.levels = &[20.0, 80.0];
                o.fixed = Some((0.0, 100.0));
            }
            Kind::Adx => {
                let (p, m, a) = calc::adx(&h, &l, &c, self.n(0));
                o.lines = vec![ln("+DI", p, 0, Solid), ln("-DI", m, 1, Solid), ln("ADX", a, 2, Solid)];
                o.levels = &[25.0];
            }
            Kind::Bias => {
                o.lines.push(ln("", calc::bias(&c, self.n(0)), 0, Solid));
                o.levels = &[0.0];
            }
        }
        o
    }
}

pub fn defaults() -> Vec<Cfg> {
    vec![
        Cfg::with(Kind::Sma, &[5.0], 1, true),
        Cfg::with(Kind::Sma, &[20.0], 0, true),
        Cfg::with(Kind::Sma, &[60.0], 2, true),
        Cfg::with(Kind::Ema, &[50.0], 3, false),
        Cfg::with(Kind::Boll, &[20.0, 2.0], 5, false),
        Cfg::with(Kind::Rsi, &[14.0], 5, false),
        Cfg::with(Kind::Macd, &[12.0, 26.0, 9.0], 0, false),
        Cfg::with(Kind::Kd, &[9.0, 3.0, 3.0], 0, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let c = 100.0 + (i as f64 * 0.3).sin() * 5.0 + i as f64 * 0.1;
                Bar { t: i as i64 * 86_400, o: c - 0.2, h: c + 1.0, l: c - 1.0, c, adj: c, v: 1000.0 + i as f64 }
            })
            .collect()
    }

    #[test]
    fn every_kind_computes_with_defaults_and_odd_params() {
        let b = bars(300);
        for k in Kind::OVERLAYS.iter().chain(Kind::PANES.iter()) {
            let mut cfg = Cfg::new(*k, 0);
            let o = cfg.compute(&b);
            assert!(!o.lines.is_empty(), "{k:?}");
            assert!(o.lines.iter().all(|l| l.vals.len() == 300), "{k:?}");
            // garbage params get repaired and never panic
            cfg.params = vec![f64::NAN, 0.0, -5.0, 1e9];
            cfg.sanitize();
            cfg.compute(&b);
            cfg.compute(&b[..3]);
            cfg.compute(&[]);
        }
    }

    #[test]
    fn title_trims_float_noise() {
        assert_eq!(Cfg::new(Kind::Boll, 0).title(), "BB(20,2)");
        assert_eq!(Cfg::new(Kind::Obv, 0).title(), "OBV");
    }
}
