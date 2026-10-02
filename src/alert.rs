//! Price / indicator alerts: conditions, evaluation over bars, firing rules.

use crate::calc;
use crate::data::Bar;
use crate::indicator::{pf, pi, Param};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CondKind {
    Price,
    PctMove,
    MaCross,
    KdCross,
    MacdCross,
    RsiCross,
    BollBreak,
    Extreme,
    VolSpike,
}

impl CondKind {
    pub const ALL: [CondKind; 9] = [
        CondKind::Price,
        CondKind::PctMove,
        CondKind::MaCross,
        CondKind::KdCross,
        CondKind::MacdCross,
        CondKind::RsiCross,
        CondKind::BollBreak,
        CondKind::Extreme,
        CondKind::VolSpike,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CondKind::Price => "價格觸價",
            CondKind::PctMove => "單日漲跌幅",
            CondKind::MaCross => "均線交叉",
            CondKind::KdCross => "KD 交叉",
            CondKind::MacdCross => "MACD 交叉",
            CondKind::RsiCross => "RSI 穿越水位",
            CondKind::BollBreak => "布林通道突破",
            CondKind::Extreme => "創 N 日新高/新低",
            CondKind::VolSpike => "成交量爆量",
        }
    }

    /// Wording of the (up, down) direction; None when the kind has no direction.
    pub fn dirs(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            CondKind::Price => ("向上觸及 ≥", "向下觸及 ≤"),
            CondKind::PctMove => ("漲幅達", "跌幅達"),
            CondKind::MaCross => ("黃金交叉", "死亡交叉"),
            CondKind::KdCross => ("K 上穿 D", "K 下穿 D"),
            CondKind::MacdCross => ("DIF 上穿訊號線", "DIF 下穿訊號線"),
            CondKind::RsiCross => ("上穿", "下穿"),
            CondKind::BollBreak => ("突破上軌", "跌破下軌"),
            CondKind::Extreme => ("新高", "新低"),
            CondKind::VolSpike => return None,
        })
    }

    pub fn params(self) -> &'static [Param] {
        match self {
            CondKind::Price => &[Param { name: "價位", def: 100.0, min: 0.0001, max: 1e9, int: false }],
            CondKind::PctMove => &[pf!("幅度%", 3.0, 0.1, 100.0)],
            CondKind::MaCross => &[pi!("快線", 5.0), pi!("慢線", 20.0)],
            CondKind::KdCross => &[pi!("週期", 9.0), pi!("K平滑", 3.0), pi!("D平滑", 3.0)],
            CondKind::MacdCross => &[pi!("快線", 12.0), pi!("慢線", 26.0), pi!("訊號", 9.0)],
            CondKind::RsiCross => &[pi!("週期", 14.0), pf!("水位", 70.0, 1.0, 99.0)],
            CondKind::BollBreak => &[pi!("週期", 20.0), pf!("倍數", 2.0, 0.1, 10.0)],
            CondKind::Extreme => &[pi!("天數", 20.0)],
            CondKind::VolSpike => &[pi!("均量天數", 20.0), pf!("倍數", 2.0, 0.1, 100.0)],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cond {
    pub kind: CondKind,
    pub up: bool,
    pub params: Vec<f64>,
}

fn fnum(v: f64) -> String {
    format!("{}", (v * 1e4).round() / 1e4)
}

impl Cond {
    pub fn new(kind: CondKind, up: bool, price_hint: f64) -> Self {
        let mut params: Vec<f64> = kind.params().iter().map(|p| p.def).collect();
        if kind == CondKind::Price && price_hint.is_finite() && price_hint > 0.0 {
            params[0] = price_hint;
        }
        if kind == CondKind::RsiCross {
            params[1] = if up { 70.0 } else { 30.0 };
        }
        Self { kind, up, params }
    }

    pub fn sanitize(&mut self) {
        let specs = self.kind.params();
        self.params.resize(specs.len(), 0.0);
        for (v, s) in self.params.iter_mut().zip(specs) {
            if !v.is_finite() {
                *v = s.def;
            }
            *v = v.clamp(s.min, s.max);
            if s.int {
                *v = v.round();
            }
        }
    }

    fn p(&self, i: usize) -> f64 {
        self.params.get(i).copied().unwrap_or_else(|| self.kind.params()[i].def)
    }
    fn n(&self, i: usize) -> usize {
        self.p(i).round().max(1.0) as usize
    }

    pub fn title(&self) -> String {
        let (u, d) = self.kind.dirs().unwrap_or(("", ""));
        let dir = if self.up { u } else { d };
        match self.kind {
            CondKind::Price => format!("價格{} {}", dir, fnum(self.p(0))),
            CondKind::PctMove => format!("單日{} {}%", dir, fnum(self.p(0))),
            CondKind::MaCross => format!("MA{}/{} {}", self.n(0), self.n(1), dir),
            CondKind::KdCross => format!("KD({},{},{}) {}", self.n(0), self.n(1), self.n(2), dir),
            CondKind::MacdCross => format!("MACD({},{},{}) {}", self.n(0), self.n(1), self.n(2), dir),
            CondKind::RsiCross => format!("RSI({}) {} {}", self.n(0), dir, fnum(self.p(1))),
            CondKind::BollBreak => format!("收盤{}布林({},{})", dir, self.n(0), fnum(self.p(1))),
            CondKind::Extreme => format!("創 {} 日{}", self.n(0), dir),
            CondKind::VolSpike => format!("成交量 ≥ {} 倍 {} 日均量", fnum(self.p(1)), self.n(0)),
        }
    }

    /// Whether the condition holds at each bar (state for level-type conditions, event for crosses).
    pub fn series(&self, bars: &[Bar]) -> Vec<bool> {
        let len = bars.len();
        let c: Vec<f64> = bars.iter().map(|b| b.c).collect();
        let h: Vec<f64> = bars.iter().map(|b| b.h).collect();
        let l: Vec<f64> = bars.iter().map(|b| b.l).collect();
        let v: Vec<f64> = bars.iter().map(|b| b.v).collect();
        let up = self.up;
        let cross = |a: &[f64], b: &[f64]| -> Vec<bool> {
            (0..len)
                .map(|i| {
                    i > 0
                        && [a[i], b[i], a[i - 1], b[i - 1]].iter().all(|x| x.is_finite())
                        && if up { a[i - 1] <= b[i - 1] && a[i] > b[i] } else { a[i - 1] >= b[i - 1] && a[i] < b[i] }
                })
                .collect()
        };
        match self.kind {
            CondKind::Price => {
                let lv = self.p(0);
                c.iter().map(|x| if up { *x >= lv } else { *x <= lv }).collect()
            }
            CondKind::PctMove => {
                let t = self.p(0);
                (0..len)
                    .map(|i| {
                        if i == 0 || c[i - 1] == 0.0 {
                            return false;
                        }
                        let ch = (c[i] / c[i - 1] - 1.0) * 100.0;
                        if up { ch >= t } else { ch <= -t }
                    })
                    .collect()
            }
            CondKind::MaCross => cross(&calc::sma(&c, self.n(0)), &calc::sma(&c, self.n(1))),
            CondKind::KdCross => {
                let (k, d, _) = calc::kd(&h, &l, &c, self.n(0), self.n(1), self.n(2));
                cross(&k, &d)
            }
            CondKind::MacdCross => {
                let (m, s, _) = calc::macd(&c, self.n(0), self.n(1), self.n(2));
                // the first bars of an EMA are not meaningful; ignore the warm-up
                let warm = self.n(1) + self.n(2);
                let mut r = cross(&m, &s);
                for x in r.iter_mut().take(warm) {
                    *x = false;
                }
                r
            }
            CondKind::RsiCross => {
                let r = calc::rsi(&c, self.n(0));
                cross(&r, &vec![self.p(1); len])
            }
            CondKind::BollBreak => {
                let (_, u, lo) = calc::bollinger(&c, self.n(0), self.p(1));
                (0..len).map(|i| if up { u[i].is_finite() && c[i] > u[i] } else { lo[i].is_finite() && c[i] < lo[i] }).collect()
            }
            CondKind::Extreme => {
                let n = self.n(0);
                (0..len)
                    .map(|i| {
                        i >= n
                            && if up {
                                h[i] > h[i - n..i].iter().cloned().fold(f64::MIN, f64::max)
                            } else {
                                l[i] < l[i - n..i].iter().cloned().fold(f64::MAX, f64::min)
                            }
                    })
                    .collect()
            }
            CondKind::VolSpike => {
                let avg = calc::sma(&v, self.n(0));
                let m = self.p(1);
                (0..len).map(|i| i > 0 && avg[i - 1].is_finite() && avg[i - 1] > 0.0 && v[i] >= m * avg[i - 1]).collect()
            }
        }
    }
}

/// Indices where the series turns true (history markers).
pub fn rising_edges(s: &[bool]) -> Vec<usize> {
    (1..s.len()).filter(|&i| s[i] && !s[i - 1]).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Repeat {
    Once,
    EveryMinute,
}

impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => "只提醒一次",
            Repeat::EveryMinute => "每分鐘一次",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Alert {
    pub id: u64,
    pub symbol: String,
    pub cond: Cond,
    pub repeat: Repeat,
    pub toast: bool,
    pub system: bool,
    pub email: bool,
    pub enabled: bool,
    pub chart: bool,
    pub last_fired: i64,
    pub fired: u32,
}

impl Default for Alert {
    fn default() -> Self {
        Self {
            id: 0,
            symbol: String::new(),
            cond: Cond::new(CondKind::Price, true, 100.0),
            repeat: Repeat::Once,
            toast: true,
            system: false,
            email: false,
            enabled: true,
            chart: true,
            last_fired: 0,
            fired: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlertEvent {
    pub alert: u64,
    pub symbol: String,
    pub t: i64,
    pub price: f64,
    pub text: String,
    pub bull: bool,
}

/// Once: fire on the false→true transition (the first look only sets the baseline).
/// EveryMinute: fire whenever the condition holds, at most once per 60 s.
pub fn should_fire(repeat: Repeat, met: bool, prev_met: Option<bool>, now: i64, last_fired: i64) -> bool {
    match repeat {
        Repeat::Once => met && prev_met == Some(false),
        Repeat::EveryMinute => met && now - last_fired >= 60,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Quote {
    pub price: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub t: i64,
}

/// Fold a live quote into the daily history (replace today's bar or append a new one).
pub fn merge_live(bars: &[Bar], q: &Quote) -> Vec<Bar> {
    let mut v = bars.to_vec();
    let Some(last) = v.last().copied() else { return v };
    let ratio = if last.c != 0.0 { last.adj / last.c } else { 1.0 };
    if q.t - last.t < 16 * 3600 {
        if q.t >= last.t {
            let b = v.last_mut().unwrap();
            b.c = q.price;
            b.h = b.h.max(q.high).max(q.price);
            b.l = b.l.min(q.low).min(q.price);
            if q.volume > 0.0 {
                b.v = q.volume;
            }
            b.adj = q.price * ratio;
        }
    } else {
        v.push(Bar {
            t: q.t,
            o: q.price,
            h: q.high.max(q.price),
            l: if q.low > 0.0 { q.low.min(q.price) } else { q.price },
            c: q.price,
            adj: q.price * ratio,
            v: q.volume,
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(c: &[f64]) -> Vec<Bar> {
        c.iter()
            .enumerate()
            .map(|(i, &c)| Bar { t: i as i64 * 86_400, o: c, h: c + 1.0, l: c - 1.0, c, adj: c, v: 100.0 })
            .collect()
    }

    #[test]
    fn price_level_both_directions() {
        let b = bars(&[10.0, 11.0, 12.0, 11.0]);
        assert_eq!(Cond::new(CondKind::Price, true, 11.5).series(&b), vec![false, false, true, false]);
        assert_eq!(Cond::new(CondKind::Price, false, 10.5).series(&b), vec![true, false, false, false]);
    }

    #[test]
    fn ma_cross_marks_only_the_crossing_bar() {
        let mut c = vec![10.0; 30];
        c.extend((0..10).map(|i| 10.0 + i as f64 * 2.0));
        let mut cond = Cond::new(CondKind::MaCross, true, 0.0);
        cond.params = vec![3.0, 10.0];
        let s = cond.series(&bars(&c));
        let edges = rising_edges(&s);
        assert_eq!(s.iter().filter(|x| **x).count(), 1);
        assert_eq!(edges.len(), 1);
        assert!(edges[0] >= 30);
        cond.up = false;
        assert!(cond.series(&bars(&c)).iter().all(|x| !x));
    }

    #[test]
    fn extreme_pct_and_volume() {
        let mut b = bars(&[10.0, 10.0, 10.0, 10.0, 20.0]);
        b[4].v = 1000.0;
        let mut c = Cond::new(CondKind::Extreme, true, 0.0);
        c.params = vec![3.0];
        assert_eq!(c.series(&b)[4], true);
        assert_eq!(Cond::new(CondKind::PctMove, true, 0.0).series(&b)[4], true);
        assert_eq!(Cond::new(CondKind::PctMove, false, 0.0).series(&b)[4], false);
        let mut v = Cond::new(CondKind::VolSpike, true, 0.0);
        v.params = vec![3.0, 2.0];
        assert_eq!(v.series(&b), vec![false, false, false, false, true]);
    }

    #[test]
    fn fire_rules() {
        assert!(!should_fire(Repeat::Once, true, None, 1000, 0));
        assert!(!should_fire(Repeat::Once, true, Some(true), 1000, 0));
        assert!(should_fire(Repeat::Once, true, Some(false), 1000, 0));
        assert!(!should_fire(Repeat::Once, false, Some(false), 1000, 0));
        assert!(should_fire(Repeat::EveryMinute, true, None, 1000, 0));
        assert!(!should_fire(Repeat::EveryMinute, true, Some(true), 1030, 1000));
        assert!(should_fire(Repeat::EveryMinute, true, Some(true), 1060, 1000));
        assert!(!should_fire(Repeat::EveryMinute, false, Some(true), 5000, 0));
    }

    #[test]
    fn live_quote_replaces_or_appends() {
        let b = bars(&[10.0, 11.0]);
        let same = merge_live(&b, &Quote { price: 13.0, high: 14.0, low: 9.0, volume: 500.0, t: b[1].t + 3600 });
        assert_eq!(same.len(), 2);
        assert_eq!((same[1].c, same[1].h, same[1].l, same[1].v), (13.0, 14.0, 9.0, 500.0));
        let next = merge_live(&b, &Quote { price: 12.0, high: 12.5, low: 11.5, volume: 5.0, t: b[1].t + 86_400 });
        assert_eq!(next.len(), 3);
        assert_eq!(next[2].c, 12.0);
    }

    #[test]
    fn every_condition_survives_garbage_params_and_tiny_inputs() {
        for k in CondKind::ALL {
            for up in [true, false] {
                let mut c = Cond::new(k, up, 50.0);
                c.params = vec![f64::NAN, -3.0, 1e12, 0.0];
                c.sanitize();
                assert_eq!(c.params.len(), k.params().len());
                for n in [0usize, 1, 5, 80] {
                    let b = bars(&(0..n).map(|i| 10.0 + (i as f64).sin()).collect::<Vec<_>>());
                    assert_eq!(c.series(&b).len(), n);
                }
                assert!(!c.title().is_empty());
            }
        }
    }
}
