//! Long-only, all-in strategy backtest. Signals on bar close, fills at the next bar's open.

use crate::calc;
use crate::data::Bar;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strat {
    Kd,
    Rsi,
    Ma,
}

pub struct Param {
    pub name: &'static str,
    pub def: f64,
    pub min: f64,
    pub max: f64,
}

const fn p(name: &'static str, def: f64, min: f64, max: f64) -> Param {
    Param { name, def, min, max }
}

impl Strat {
    pub const ALL: [Strat; 3] = [Strat::Kd, Strat::Rsi, Strat::Ma];

    pub fn label(self) -> &'static str {
        match self {
            Strat::Kd => "KD 交叉",
            Strat::Rsi => "RSI 穿越",
            Strat::Ma => "均線交叉",
        }
    }

    pub fn rule(self) -> &'static str {
        match self {
            Strat::Kd => "買:K 上穿 D 且 K < 買進線;賣:K 下穿 D 且 K > 賣出線",
            Strat::Rsi => "買:RSI 由下往上穿越買進線;賣:RSI 由上往下穿越賣出線",
            Strat::Ma => "買:快線上穿慢線(黃金交叉);賣:快線下穿慢線(死亡交叉)",
        }
    }

    pub fn params(self) -> &'static [Param] {
        const KD: [Param; 5] = [p("週期", 9.0, 2.0, 100.0), p("K 平滑", 3.0, 1.0, 20.0), p("D 平滑", 3.0, 1.0, 20.0), p("買進線", 20.0, 0.0, 100.0), p("賣出線", 80.0, 0.0, 100.0)];
        const RSI: [Param; 3] = [p("週期", 14.0, 2.0, 100.0), p("買進線", 30.0, 0.0, 100.0), p("賣出線", 70.0, 0.0, 100.0)];
        const MA: [Param; 2] = [p("快線", 20.0, 2.0, 400.0), p("慢線", 60.0, 3.0, 800.0)];
        match self {
            Strat::Kd => &KD,
            Strat::Rsi => &RSI,
            Strat::Ma => &MA,
        }
    }
}

pub struct Trade {
    pub entry_t: i64,
    pub exit_t: i64,
    pub entry: f64,
    pub exit: f64,
    pub ret: f64,
    /// Still held at the end of the data (marked at the last close).
    pub open: bool,
}

pub struct Report {
    pub trades: Vec<Trade>,
    /// (time, equity) per bar, starting at 1.0.
    pub equity: Vec<(i64, f64)>,
    pub total: f64,
    pub cagr: f64,
    pub bh_total: f64,
    pub bh_cagr: f64,
    pub max_dd: f64,
    pub win_rate: f64,
    pub years: f64,
}

/// Buy (+1) / sell (-1) / none (0) per bar, decided at that bar's close.
fn signals(s: Strat, par: &[f64], bars: &[Bar]) -> Vec<i8> {
    let h: Vec<f64> = bars.iter().map(|b| b.h).collect();
    let l: Vec<f64> = bars.iter().map(|b| b.l).collect();
    let c: Vec<f64> = bars.iter().map(|b| b.c).collect();
    let n = |i: usize| par[i].max(1.0) as usize;
    let mut out = vec![0i8; bars.len()];
    let both = |a: f64, b: f64| !a.is_nan() && !b.is_nan();
    match s {
        Strat::Kd => {
            let (k, d, _) = calc::kd(&h, &l, &c, n(0), n(1), n(2));
            for i in 1..c.len() {
                if !both(k[i - 1], d[i - 1]) || !both(k[i], d[i]) {
                    continue;
                }
                if k[i - 1] <= d[i - 1] && k[i] > d[i] && k[i] < par[3] {
                    out[i] = 1;
                } else if k[i - 1] >= d[i - 1] && k[i] < d[i] && k[i] > par[4] {
                    out[i] = -1;
                }
            }
        }
        Strat::Rsi => {
            let r = calc::rsi(&c, n(0));
            for i in 1..c.len() {
                if !both(r[i - 1], r[i]) {
                    continue;
                }
                if r[i - 1] < par[1] && r[i] >= par[1] {
                    out[i] = 1;
                } else if r[i - 1] > par[2] && r[i] <= par[2] {
                    out[i] = -1;
                }
            }
        }
        Strat::Ma => {
            let (f, sl) = (calc::sma(&c, n(0)), calc::sma(&c, n(1)));
            for i in 1..c.len() {
                if !both(f[i - 1], sl[i - 1]) || !both(f[i], sl[i]) {
                    continue;
                }
                if f[i - 1] <= sl[i - 1] && f[i] > sl[i] {
                    out[i] = 1;
                } else if f[i - 1] >= sl[i - 1] && f[i] < sl[i] {
                    out[i] = -1;
                }
            }
        }
    }
    out
}

/// Backtest over the last `years` of `bars` (indicators warm up on all earlier history).
/// `cost` is the per-side fee as a fraction; `total_return` uses dividend-adjusted prices.
pub fn run(s: Strat, par: &[f64], bars: &[Bar], years: f64, cost: f64, total_return: bool) -> Option<Report> {
    if bars.len() < 3 || par.len() < s.params().len() {
        return None;
    }
    let px: Vec<Bar> = if total_return {
        bars.iter()
            .map(|b| {
                let f = if b.c != 0.0 { b.adj / b.c } else { 1.0 };
                Bar { t: b.t, o: b.o * f, h: b.h * f, l: b.l * f, c: b.adj, adj: b.adj, v: b.v }
            })
            .collect()
    } else {
        bars.to_vec()
    };
    let last_t = px.last()?.t;
    let from_t = last_t - (years * 365.25 * calc::DAY as f64) as i64;
    let start = px.iter().position(|b| b.t >= from_t).unwrap_or(0).min(px.len() - 2);
    let sig = signals(s, par, &px);

    let (mut cash, mut shares) = (1.0f64, 0.0f64);
    let mut entry: Option<(i64, f64)> = None;
    let mut trades = Vec::new();
    let mut equity = Vec::with_capacity(px.len() - start);
    for i in start..px.len() {
        // execute the previous bar's signal at this bar's open
        if i > start {
            match sig[i - 1] {
                1 if shares == 0.0 && px[i].o > 0.0 => {
                    shares = cash / (px[i].o * (1.0 + cost));
                    cash = 0.0;
                    entry = Some((px[i].t, px[i].o));
                }
                -1 if shares > 0.0 => {
                    cash = shares * px[i].o * (1.0 - cost);
                    shares = 0.0;
                    let (et, ep) = entry.take()?;
                    trades.push(Trade { entry_t: et, exit_t: px[i].t, entry: ep, exit: px[i].o, ret: px[i].o * (1.0 - cost) / (ep * (1.0 + cost)) - 1.0, open: false });
                }
                _ => {}
            }
        }
        equity.push((px[i].t, cash + shares * px[i].c));
    }
    if let Some((et, ep)) = entry {
        let b = px.last()?;
        trades.push(Trade { entry_t: et, exit_t: b.t, entry: ep, exit: b.c, ret: b.c * (1.0 - cost) / (ep * (1.0 + cost)) - 1.0, open: true });
    }

    let yrs = ((last_t - px[start].t) as f64 / (365.25 * calc::DAY as f64)).max(1e-9);
    let total = equity.last()?.1 - 1.0;
    let bh_total = px.last()?.c / px[start].c - 1.0;
    let cagr = |t: f64| if t > -1.0 && yrs >= 0.1 { (1.0 + t).powf(1.0 / yrs) - 1.0 } else { f64::NAN };
    let (mut peak, mut max_dd) = (1.0f64, 0.0f64);
    for &(_, e) in &equity {
        peak = peak.max(e);
        max_dd = max_dd.min(e / peak - 1.0);
    }
    let closed: Vec<&Trade> = trades.iter().filter(|t| !t.open).collect();
    let win_rate = if closed.is_empty() { f64::NAN } else { closed.iter().filter(|t| t.ret > 0.0).count() as f64 / closed.len() as f64 };
    Some(Report { trades, equity, total, cagr: cagr(total), bh_total, bh_cagr: cagr(bh_total), max_dd, win_rate, years: yrs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar { t: i as i64 * calc::DAY, o: c, h: c * 1.01, l: c * 0.99, c, adj: c, v: 1.0 })
            .collect()
    }

    fn defaults(s: Strat) -> Vec<f64> {
        s.params().iter().map(|p| p.def).collect()
    }

    #[test]
    fn ma_cross_trades_a_zigzag_and_accounts_consistently() {
        let c: Vec<f64> = (0..400).map(|i| 100.0 + 20.0 * (i as f64 / 20.0).sin() + i as f64 * 0.1).collect();
        let r = run(Strat::Ma, &[5.0, 20.0], &bars(&c), 100.0, 0.0, false).unwrap();
        assert!(!r.trades.is_empty());
        let prod: f64 = r.trades.iter().map(|t| 1.0 + t.ret).product();
        assert!((prod - 1.0 - r.total).abs() < 1e-9, "{prod} vs {}", r.total);
        assert!(r.max_dd <= 0.0);
    }

    #[test]
    fn costs_reduce_return() {
        let c: Vec<f64> = (0..400).map(|i| 100.0 + 20.0 * (i as f64 / 20.0).sin()).collect();
        let a = run(Strat::Ma, &[5.0, 20.0], &bars(&c), 100.0, 0.0, false).unwrap();
        let b = run(Strat::Ma, &[5.0, 20.0], &bars(&c), 100.0, 0.005, false).unwrap();
        assert!(b.total < a.total);
    }

    #[test]
    fn every_strategy_runs_with_defaults() {
        let c: Vec<f64> = (0..600).map(|i| 100.0 + 30.0 * (i as f64 / 15.0).sin()).collect();
        for s in Strat::ALL {
            let r = run(s, &defaults(s), &bars(&c), 10.0, 0.0, false).unwrap();
            assert_eq!(r.equity.len() > 1, true);
        }
        assert!(run(Strat::Kd, &defaults(Strat::Kd), &bars(&c[..2]), 10.0, 0.0, false).is_none());
    }

    #[test]
    fn window_limits_start() {
        let c: Vec<f64> = (0..1000).map(|i| 100.0 + i as f64).collect();
        let r = run(Strat::Ma, &[5.0, 20.0], &bars(&c), 1.0, 0.0, false).unwrap();
        assert!(r.years < 1.01 && r.years > 0.99);
    }
}
