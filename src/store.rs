//! Persisted user state: watchlist, compare set, view options, drawings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Mode {
    Compare,
    Chart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Range {
    W1,
    M1,
    M3,
    M6,
    Y1,
    Y3,
    Y5,
    Y10,
    Y15,
    Y20,
    Max,
    Custom,
}

impl Range {
    pub const ALL: [Range; 10] =
        [Range::W1, Range::M1, Range::M3, Range::M6, Range::Y1, Range::Y3, Range::Y5, Range::Y10, Range::Y15, Range::Max];
    pub fn label(self) -> &'static str {
        match self {
            Range::W1 => "1週",
            Range::M1 => "1月",
            Range::M3 => "3月",
            Range::M6 => "6月",
            Range::Y1 => "1Y",
            Range::Y3 => "3Y",
            Range::Y5 => "5Y",
            Range::Y10 => "10Y",
            Range::Y15 => "15Y",
            Range::Y20 => "20Y",
            Range::Max => "MAX",
            Range::Custom => "自訂",
        }
    }
    pub fn days(self) -> Option<f64> {
        match self {
            Range::W1 => Some(7.0),
            Range::M1 => Some(30.5),
            Range::M3 => Some(91.5),
            Range::M6 => Some(183.0),
            Range::Y1 => Some(365.25),
            Range::Y3 => Some(3.0 * 365.25),
            Range::Y5 => Some(5.0 * 365.25),
            Range::Y10 => Some(10.0 * 365.25),
            Range::Y15 => Some(15.0 * 365.25),
            Range::Y20 => Some(20.0 * 365.25),
            Range::Max | Range::Custom => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Interval {
    D,
    W,
    M,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Drawing {
    Trend { t1: i64, p1: f64, t2: i64, p2: f64 },
    HLine { p: f64 },
    Fib { t1: i64, p1: f64, t2: i64, p2: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Indicators {
    pub volume: bool,
    pub ma: bool,
    pub ema: bool,
    pub bollinger: bool,
    pub rsi: bool,
    pub macd: bool,
}

impl Default for Indicators {
    fn default() -> Self {
        Self { volume: true, ma: true, ema: false, bollinger: false, rsi: false, macd: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Persisted {
    pub watchlist: Vec<String>,
    pub compare: Vec<String>,
    pub selected: String,
    pub mode: Mode,
    pub range: Range,
    pub custom_from: i64,
    pub custom_to: i64,
    pub interval: Interval,
    pub dark: bool,
    pub red_up: bool,
    pub total_return: bool,
    pub log_scale: bool,
    pub indicators: Indicators,
    pub drawings: HashMap<String, Vec<Drawing>>,
    pub colors: HashMap<String, usize>,
    pub names: HashMap<String, String>,
}

impl Default for Persisted {
    fn default() -> Self {
        let syms = ["2330.TW", "AAPL", "NVDA", "0050.TW", "^GSPC"];
        Self {
            watchlist: ["2330.TW", "2317.TW", "2454.TW", "0050.TW", "AAPL", "NVDA", "MSFT", "TSLA", "^GSPC", "^TWII"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            compare: syms.iter().map(|s| s.to_string()).collect(),
            selected: "2330.TW".into(),
            mode: Mode::Compare,
            range: Range::Y10,
            custom_from: 0,
            custom_to: 0,
            interval: Interval::D,
            dark: false,
            red_up: true,
            total_return: true,
            log_scale: true,
            indicators: Indicators::default(),
            drawings: HashMap::new(),
            colors: HashMap::new(),
            names: HashMap::new(),
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::data::config_dir().join("state.json")
}

impl Persisted {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), t);
        }
    }
}
