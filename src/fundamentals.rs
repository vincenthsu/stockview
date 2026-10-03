//! Yahoo company fundamentals; raw ratios, explicit missing values, daily disk cache.
use crate::data;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Pe, ForwardPe, Pb, Eps, Yield, Roe, RevenueGrowth, EarningsGrowth,
    Margin, DebtEquity, Revenue, Profit, Cash, Debt, FreeCashFlow, MarketCap,
}

impl Metric {
    pub const ALL: [Self; 16] = [Self::Pe, Self::ForwardPe, Self::Pb, Self::Eps, Self::Yield,
        Self::Roe, Self::RevenueGrowth, Self::EarningsGrowth, Self::Margin, Self::DebtEquity,
        Self::Revenue, Self::Profit, Self::Cash, Self::Debt, Self::FreeCashFlow, Self::MarketCap];
    pub const FILTERS: [Self; 9] = [Self::Pe, Self::ForwardPe, Self::Pb, Self::Yield,
        Self::Roe, Self::RevenueGrowth, Self::EarningsGrowth, Self::Margin, Self::DebtEquity];
    pub fn label(self) -> &'static str {
        match self {
            Self::Pe => "本益比 (TTM)", Self::ForwardPe => "預估本益比", Self::Pb => "股價淨值比",
            Self::Eps => "每股盈餘 (TTM)", Self::Yield => "預估年股息殖利率 %", Self::Roe => "ROE % (TTM)",
            Self::RevenueGrowth => "營收年增率 % (最近一季)", Self::EarningsGrowth => "盈餘年增率 % (最近一季)",
            Self::Margin => "淨利率 % (TTM)", Self::DebtEquity => "負債 / 權益 % (最近一季)",
            Self::Revenue => "營收 (TTM)", Self::Profit => "淨利 (TTM)", Self::Cash => "現金 (最近一季)",
            Self::Debt => "負債 (最近一季)", Self::FreeCashFlow => "自由現金流 (TTM)", Self::MarketCap => "市值",
        }
    }
    fn source(self) -> (&'static str, &'static str, f64) {
        match self {
            Self::Pe => ("summaryDetail", "trailingPE", 1.0),
            Self::ForwardPe => ("summaryDetail", "forwardPE", 1.0),
            Self::Pb => ("defaultKeyStatistics", "priceToBook", 1.0),
            Self::Eps => ("defaultKeyStatistics", "trailingEps", 1.0),
            Self::Yield => ("summaryDetail", "dividendYield", 100.0),
            Self::Roe => ("financialData", "returnOnEquity", 100.0),
            Self::RevenueGrowth => ("financialData", "revenueGrowth", 100.0),
            Self::EarningsGrowth => ("financialData", "earningsGrowth", 100.0),
            Self::Margin => ("financialData", "profitMargins", 100.0),
            // Yahoo already expresses debtToEquity as a percentage.
            Self::DebtEquity => ("financialData", "debtToEquity", 1.0),
            Self::Revenue => ("financialData", "totalRevenue", 1.0),
            Self::Profit => ("defaultKeyStatistics", "netIncomeToCommon", 1.0),
            Self::Cash => ("financialData", "totalCash", 1.0),
            Self::Debt => ("financialData", "totalDebt", 1.0),
            Self::FreeCashFlow => ("financialData", "freeCashflow", 1.0),
            Self::MarketCap => ("price", "marketCap", 1.0),
        }
    }
    pub fn money(self) -> bool {
        matches!(self, Self::Revenue | Self::Profit | Self::Cash | Self::Debt | Self::FreeCashFlow | Self::MarketCap)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fundamentals {
    pub symbol: String,
    pub name: String,
    pub kind: String,
    pub sector: String,
    pub industry: String,
    pub description: String,
    pub currency: String,
    pub financial_currency: String,
    pub values: [Option<f64>; 16],
    pub report_at: Option<i64>,
    pub fetched_at: i64,
}

impl Fundamentals {
    pub fn value(&self, m: Metric) -> Option<f64> { self.values[m as usize] }
    pub fn equity(&self) -> bool { self.kind == "EQUITY" }
    pub fn fresh(&self) -> bool { (0..86_400).contains(&(data::now_secs() - self.fetched_at)) }
    pub fn money_currency(&self, m: Metric) -> &str {
        if m == Metric::MarketCap { &self.currency } else { &self.financial_currency }
    }
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v["raw"].as_f64()).filter(|n| n.is_finite())
}

fn text(v: &Value) -> String { v.as_str().unwrap_or("").to_string() }

pub fn parse(symbol: &str, body: &Value) -> Result<Fundamentals, String> {
    let summary = &body["quoteSummary"];
    if !summary["error"].is_null() {
        return Err(format!("{symbol}:{}", summary["error"]["description"].as_str().unwrap_or("基本面資料不可用")));
    }
    let r = summary["result"].get(0).filter(|r| r.is_object()).ok_or_else(|| format!("{symbol} 沒有基本面資料"))?;
    let p = &r["price"];
    let kind = text(&p["quoteType"]);
    if kind != "EQUITY" { return Err(format!("{symbol} 非個股，沒有公司基本面 ({kind})")); }
    let profile = &r["summaryProfile"];
    let values = Metric::ALL.map(|m| {
        let (module, field, scale) = m.source();
        number(&r[module][field]).map(|n| n * scale).filter(|n| n.is_finite())
    });
    if values.iter().all(Option::is_none) { return Err(format!("{symbol} 沒有基本面指標")); }
    Ok(Fundamentals {
        symbol: symbol.into(),
        name: p["longName"].as_str().or_else(|| p["shortName"].as_str()).unwrap_or(symbol).into(),
        kind, sector: text(&profile["sector"]), industry: text(&profile["industry"]),
        description: text(&profile["longBusinessSummary"]), currency: text(&p["currency"]),
        financial_currency: text(&r["financialData"]["financialCurrency"]), values,
        report_at: number(&r["defaultKeyStatistics"]["mostRecentQuarter"]).map(|n| n as i64),
        fetched_at: data::now_secs(),
    })
}

fn cache_path(symbol: &str) -> std::path::PathBuf {
    let safe: String = symbol.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') { c } else { '_' }).collect();
    data::cache_dir().join(format!("fundamentals-{safe}.json"))
}

pub fn cached(symbol: &str) -> Option<Fundamentals> {
    let f: Fundamentals = serde_json::from_str(&std::fs::read_to_string(cache_path(symbol)).ok()?).ok()?;
    (f.symbol == symbol && f.equity() && f.values.iter().flatten().all(|n| n.is_finite())).then_some(f)
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
        (b as char).to_string()
    } else { format!("%{b:02X}") }).collect()
}

/// One session is reused across a scan. Cookies and crumbs stay in memory.
pub struct Client { agent: ureq::Agent, cookie: String, crumb: String }
impl Default for Client {
    fn default() -> Self {
        Self { agent: ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(25)))
            .user_agent("Mozilla/5.0 (compatible; StockView/0.1)").http_status_as_error(false).build().into(),
            cookie: String::new(), crumb: String::new() }
    }
}

fn status_error(status: u16) -> String {
    match status {
        429 => "Yahoo 請求過於頻繁 (429)，稍後重新整理".into(),
        401 | 403 => "Yahoo 拒絕基本面存取，稍後重新整理".into(),
        404 => "找不到個股基本面 (404)".into(),
        _ => format!("基本面連線失敗 (HTTP {status})"),
    }
}

impl Client {
    fn authenticate(&mut self) -> Result<(), String> {
        let r = self.agent.get("https://fc.yahoo.com").call().map_err(|e| format!("Yahoo 工作階段失敗:{e}"))?;
        // fc.yahoo.com normally returns 404 while setting the session cookie.
        if !r.status().is_success() && r.status().as_u16() != 404 { return Err(status_error(r.status().as_u16())); }
        self.cookie = r.headers().get_all("set-cookie").iter().filter_map(|h| h.to_str().ok())
            .filter_map(|s| s.split(';').next()).collect::<Vec<_>>().join("; ");
        if self.cookie.is_empty() { return Err("Yahoo 未提供工作階段 cookie".into()); }
        let mut r = self.agent.get("https://query1.finance.yahoo.com/v1/test/getcrumb").header("Cookie", &self.cookie)
            .call().map_err(|e| format!("Yahoo 驗證失敗:{e}"))?;
        if !r.status().is_success() { return Err(status_error(r.status().as_u16())); }
        let crumb = r.body_mut().read_to_string().map_err(|e| format!("Yahoo 驗證解析失敗:{e}"))?;
        if crumb.is_empty() || crumb.len() > 128 || crumb.chars().any(char::is_whitespace) || crumb.contains('<') {
            return Err("Yahoo 驗證資料無效".into());
        }
        self.crumb = crumb;
        Ok(())
    }

    pub fn fetch(&mut self, symbol: &str) -> Result<Fundamentals, String> {
        for attempt in 0..2 {
            if self.crumb.is_empty() { self.authenticate()?; }
            let url = format!("https://query2.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=price%2CsummaryProfile%2CsummaryDetail%2CdefaultKeyStatistics%2CfinancialData&crumb={}", enc(symbol), enc(&self.crumb));
            let mut r = self.agent.get(&url).header("Cookie", &self.cookie).call().map_err(|_| "基本面連線失敗，請重新整理".to_string())?;
            let status = r.status().as_u16();
            if matches!(status, 401 | 403) {
                self.crumb.clear();
                if attempt == 0 { continue; }
            }
            if !r.status().is_success() { return Err(status_error(status)); }
            let body = r.body_mut().read_json().map_err(|e| format!("基本面解析失敗:{e}"))?;
            let f = parse(symbol, &body)?;
            if let Ok(s) = serde_json::to_string(&f) { let _ = std::fs::write(cache_path(symbol), s); }
            return Ok(f);
        }
        Err("Yahoo 驗證失敗".into())
    }
}

#[derive(Clone, Debug)]
pub struct Filter { pub metric: Metric, pub enabled: bool, pub min: f64, pub max: f64 }
impl Filter {
    pub fn matches(&self, f: &Fundamentals) -> bool {
        !self.enabled || (self.min.is_finite() && self.max.is_finite() && self.min <= self.max
            && f.value(self.metric).is_some_and(|v| v.is_finite() && v >= self.min && v <= self.max))
    }
}

pub fn symbols(input: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for s in input.split(|c: char| c.is_whitespace() || matches!(c, ',' | '，' | ';' | '；')) {
        if s.is_empty() { continue; }
        let mut s = s.to_ascii_uppercase();
        if s.len() > 32 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'^' | b'=')) {
            return Err(format!("無效代號:{s}"));
        }
        if s.bytes().all(|b| b.is_ascii_digit()) { s.push_str(".TW"); }
        if !out.contains(&s) { out.push(s); }
    }
    if out.is_empty() { return Err("請輸入至少一個代號".into()); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        json!({"quoteSummary":{"result":[{
            "price":{"quoteType":"EQUITY","longName":"Test","currency":"USD","marketCap":{"raw":1000000}},
            "summaryDetail":{"trailingPE":{"raw":20},"dividendYield":{"raw":0.025}},
            "financialData":{"financialCurrency":"USD","returnOnEquity":{"raw":0.15},"debtToEquity":{"raw":42},"revenueGrowth":-0.1},
            "defaultKeyStatistics":{"trailingEps":{"raw":-2},"mostRecentQuarter":{"raw":1751241600}}
        }],"error":null}})
    }
    #[test]
    fn raw_metrics_percent_units_and_missing() {
        let f = parse("TEST", &fixture()).unwrap();
        assert_eq!(f.value(Metric::Yield), Some(2.5));
        assert_eq!(f.value(Metric::Roe), Some(15.0));
        assert_eq!(f.value(Metric::DebtEquity), Some(42.0));
        assert_eq!(f.value(Metric::RevenueGrowth), Some(-10.0));
        assert_eq!(f.value(Metric::Eps), Some(-2.0));
        assert_eq!(f.value(Metric::Pb), None);
        assert_eq!(f.report_at, Some(1751241600));
    }
    #[test]
    fn filters_are_inclusive_and_exclude_missing_or_invalid() {
        let f = parse("TEST", &fixture()).unwrap();
        let mut filter = Filter { metric: Metric::Pe, enabled: true, min: 20.0, max: 20.0 };
        assert!(filter.matches(&f));
        filter.min = 21.0;
        assert!(!filter.matches(&f));
        filter.min = f64::NAN;
        assert!(!filter.matches(&f));
        filter.metric = Metric::Pb;
        filter.min = 0.0;
        assert!(!filter.matches(&f));
        filter.enabled = false;
        assert!(filter.matches(&f));
    }
    #[test]
    fn rejects_non_equities_empty_and_error_responses() {
        let mut v = fixture();
        v["quoteSummary"]["result"][0]["price"]["quoteType"] = json!("ETF");
        assert!(parse("ETF", &v).is_err());
        assert!(parse("TEST", &json!({"quoteSummary":{"result":[]}})).is_err());
        assert!(parse("TEST", &json!({"quoteSummary":{"error":{"description":"Not Found"}}})).unwrap_err().contains("Not Found"));
    }
    #[test]
    fn symbols_normalize_and_validate() {
        assert_eq!(symbols("2330, aapl AAPL；6488.TWO").unwrap(), ["2330.TW", "AAPL", "6488.TWO"]);
        assert!(symbols(" ").is_err());
        assert!(symbols("AAPL/../../").is_err());
    }
    #[test]
    #[ignore = "live Yahoo network smoke test"]
    fn live_fundamentals() {
        let mut client = Client::default();
        for symbol in ["AAPL", "2330.TW", "6488.TWO"] {
            let f = client.fetch(symbol).unwrap();
            assert_eq!(f.symbol, symbol);
            assert!(f.value(Metric::MarketCap).is_some_and(|v| v > 0.0));
            assert!(f.value(Metric::Pe).is_some());
        }
    }
}
