//! Persisted user state: watchlist, compare set, view options, drawings.

use serde::{Deserialize, Serialize};
use crate::indicator::{self, Cfg};
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
#[serde(default)]
pub struct Indicators {
    pub volume: bool,
    pub list: Vec<Cfg>,
}

impl Default for Indicators {
    fn default() -> Self {
        Self { volume: true, list: indicator::defaults() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub name: String,
    pub symbols: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Persisted {
    /// Legacy single watchlist; migrated into `groups` on load and never written back.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub watchlist: Vec<String>,
    pub groups: Vec<Group>,
    pub active_group: usize,
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
            watchlist: Vec::new(),
            groups: vec![Group {
                name: "預設".into(),
                symbols: ["2330.TW", "2317.TW", "2454.TW", "0050.TW", "AAPL", "NVDA", "MSFT", "TSLA", "^GSPC", "^TWII"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            }],
            active_group: 0,
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

pub const EXPORT_APP: &str = "stockview";
pub const EXPORT_VERSION: u32 = 1;

impl Persisted {
    pub fn load() -> Self {
        let mut st: Self = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        st.normalize();
        st
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), t);
        }
    }

    /// Migrate old files and repair anything a hand-edited import could break.
    pub fn normalize(&mut self) {
        if !self.watchlist.is_empty() {
            self.groups = vec![Group { name: "自選".into(), symbols: std::mem::take(&mut self.watchlist) }];
            self.active_group = 0;
        }
        if self.groups.is_empty() {
            self.groups.push(Group { name: "預設".into(), symbols: Vec::new() });
        }
        self.active_group = self.active_group.min(self.groups.len() - 1);
        for g in &mut self.groups {
            let mut seen = std::collections::HashSet::new();
            g.symbols.retain(|s| !s.trim().is_empty() && seen.insert(s.clone()));
        }
        for c in &mut self.indicators.list {
            c.sanitize();
        }
    }

    pub fn watch(&self) -> &Vec<String> {
        &self.groups[self.active_group].symbols
    }

    pub fn watch_mut(&mut self) -> &mut Vec<String> {
        &mut self.groups[self.active_group].symbols
    }

    pub fn in_any_group(&self, sym: &str) -> bool {
        self.groups.iter().any(|g| g.symbols.iter().any(|s| s == sym))
    }

    /// Whole-state snapshot as a self-describing JSON document.
    pub fn export_json(&self) -> String {
        let doc = serde_json::json!({ "app": EXPORT_APP, "version": EXPORT_VERSION, "settings": self });
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    }

    /// Accepts an export document, or a bare state.json.
    pub fn import_json(text: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("不是有效的 JSON:{e}"))?;
        let body = match v.get("settings") {
            Some(s) => s.clone(),
            None => v,
        };
        if !body.is_object() {
            return Err("檔案內容不是 StockView 設定".into());
        }
        let mut st: Self = serde_json::from_value(body).map_err(|e| format!("設定格式錯誤:{e}"))?;
        st.normalize();
        Ok(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip_keeps_everything() {
        let mut st = Persisted::default();
        st.groups.push(Group { name: "半導體".into(), symbols: vec!["2330.TW".into(), "NVDA".into()] });
        st.active_group = 1;
        st.indicators.list.push(Cfg::new(indicator::Kind::Kdj, 4));
        st.indicators.list[0].params[0] = 7.0;
        st.drawings.insert("AAPL".into(), vec![Drawing::HLine { p: 123.5 }]);
        let back = Persisted::import_json(&st.export_json()).unwrap();
        assert_eq!(back.groups, st.groups);
        assert_eq!(back.active_group, 1);
        assert_eq!(back.indicators.list, st.indicators.list);
        assert_eq!(back.drawings, st.drawings);
    }

    #[test]
    fn legacy_state_with_watchlist_and_old_indicators_loads() {
        let old = r#"{"watchlist":["AAPL","MSFT","AAPL"],"compare":["AAPL"],
            "indicators":{"volume":false,"ma":true,"ema":false,"bollinger":true,"rsi":false,"macd":false}}"#;
        let st = Persisted::import_json(old).unwrap();
        assert_eq!(st.groups.len(), 1);
        assert_eq!(st.groups[0].symbols, vec!["AAPL", "MSFT"]);
        assert!(!st.indicators.volume);
        assert!(!st.indicators.list.is_empty());
        assert!(!st.export_json().contains("\"watchlist\""));
    }

    #[test]
    fn bad_input_is_rejected_and_bad_indices_repaired() {
        assert!(Persisted::import_json("nope").is_err());
        assert!(Persisted::import_json("[1,2]").is_err());
        let st = Persisted::import_json(r#"{"groups":[],"active_group":9}"#).unwrap();
        assert_eq!((st.groups.len(), st.active_group), (1, 0));
        let st = Persisted::import_json(r#"{"indicators":{"list":[{"kind":"Rsi","params":[],"enabled":true,"color":99}]}}"#).unwrap();
        assert_eq!(st.indicators.list[0].params, vec![14.0]);
    }
}
