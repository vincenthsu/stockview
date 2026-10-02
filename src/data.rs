//! Market data: Yahoo Finance public chart endpoint (no API token) + on-disk cache.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Bar {
    /// Unix seconds (UTC) of the session open.
    pub t: i64,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    /// Dividend/split adjusted close (total return basis).
    pub adj: f64,
    pub v: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Series {
    pub symbol: String,
    pub name: String,
    pub currency: String,
    pub exchange: String,
    pub bars: Vec<Bar>,
    pub fetched_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub kind: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .user_agent("Mozilla/5.0 (compatible; StockView/0.1)")
        .build()
        .into()
}

pub fn cache_dir() -> PathBuf {
    let base = directories::ProjectDirs::from("dev", "stockview", "StockView")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("stockview"));
    let _ = std::fs::create_dir_all(&base);
    base
}

pub fn config_dir() -> PathBuf {
    let base = directories::ProjectDirs::from("dev", "stockview", "StockView")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("stockview-config"));
    let _ = std::fs::create_dir_all(&base);
    base
}

fn cache_path(symbol: &str) -> PathBuf {
    let safe: String = symbol
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    cache_dir().join(format!("{safe}.json"))
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn load_cached(symbol: &str) -> Option<Series> {
    let txt = std::fs::read_to_string(cache_path(symbol)).ok()?;
    serde_json::from_str(&txt).ok()
}

fn save_cache(s: &Series) {
    if let Ok(txt) = serde_json::to_string(s) {
        let _ = std::fs::write(cache_path(&s.symbol), txt);
    }
}

/// A cached series is "fresh" for 6 hours; daily bars don't change faster than that.
pub fn is_fresh(s: &Series) -> bool {
    now_secs() - s.fetched_at < 6 * 3600
}

fn num(v: &serde_json::Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

/// Download the full daily history. Network errors become readable strings.
pub fn fetch_series(symbol: &str) -> Result<Series, String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1=0&period2={}&interval=1d&events=div%7Csplit&includeAdjustedClose=true",
        urlenc(symbol),
        now_secs()
    );
    let mut resp = agent().get(&url).call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => format!("找不到代號 {symbol}"),
        ureq::Error::StatusCode(429) => "請求過於頻繁(429),稍後再試".to_string(),
        other => format!("連線失敗:{other}"),
    })?;
    let body: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("資料解析失敗:{e}"))?;
    let res = body["chart"]["result"]
        .get(0)
        .ok_or_else(|| format!("找不到代號 {symbol}"))?;
    let meta = &res["meta"];
    let ts = res["timestamp"].as_array().ok_or("沒有歷史資料")?;
    let q = &res["indicators"]["quote"][0];
    let adjs = res["indicators"]["adjclose"][0]["adjclose"].as_array();
    let mut bars = Vec::with_capacity(ts.len());
    for (i, t) in ts.iter().enumerate() {
        let (Some(t), Some(o), Some(h), Some(l), Some(c)) = (
            t.as_i64(),
            num(&q["open"][i]),
            num(&q["high"][i]),
            num(&q["low"][i]),
            num(&q["close"][i]),
        ) else {
            continue;
        };
        let adj = adjs.and_then(|a| num(&a[i])).unwrap_or(c);
        let v = num(&q["volume"][i]).unwrap_or(0.0);
        bars.push(Bar { t, o, h, l, c, adj, v });
    }
    if bars.len() < 2 {
        return Err(format!("{symbol} 沒有足夠的歷史資料"));
    }
    let name = meta["longName"]
        .as_str()
        .or_else(|| meta["shortName"].as_str())
        .unwrap_or(symbol)
        .to_string();
    let s = Series {
        symbol: symbol.to_string(),
        name,
        currency: meta["currency"].as_str().unwrap_or("").to_string(),
        exchange: meta["fullExchangeName"]
            .as_str()
            .or_else(|| meta["exchangeName"].as_str())
            .unwrap_or("")
            .to_string(),
        bars,
        fetched_at: now_secs(),
    };
    save_cache(&s);
    Ok(s)
}

pub fn search(query: &str) -> Result<Vec<SearchHit>, String> {
    let url = format!(
        "https://query2.finance.yahoo.com/v1/finance/search?q={}&quotesCount=10&newsCount=0&lang=zh-Hant-TW&region=TW",
        urlenc(query)
    );
    let mut resp = agent().get(&url).call().map_err(|e| format!("搜尋失敗:{e}"))?;
    let body: serde_json::Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for q in body["quotes"].as_array().into_iter().flatten() {
        let Some(sym) = q["symbol"].as_str() else { continue };
        let kind = q["quoteType"].as_str().unwrap_or("");
        if !matches!(kind, "EQUITY" | "ETF" | "INDEX" | "MUTUALFUND") {
            continue;
        }
        out.push(SearchHit {
            symbol: sym.to_string(),
            name: q["longname"]
                .as_str()
                .or_else(|| q["shortname"].as_str())
                .unwrap_or("")
                .to_string(),
            exchange: q["exchDisp"].as_str().unwrap_or("").to_string(),
            kind: kind.to_string(),
        });
    }
    Ok(out)
}

fn urlenc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}
