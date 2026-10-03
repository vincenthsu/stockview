//! Company details and a screener over an explicitly selected candidate universe.
use crate::fundamentals::{self, Client, Filter, Fundamentals, Metric};
use egui::{Context, Ui};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc::{self, Receiver, Sender}};

const TW: &str = "2330 2317 2454 2308 2412 2881 2882 2891 2886 2303 3711 2382 2357 3231 2603 2609 1301 1303 2002 1216 2207 2912 3008 3034 3045 4904 5871 5876 5880 6505 6488.TWO 3529.TWO 5274.TWO";
const US: &str = "AAPL MSFT NVDA AMZN GOOGL META TSLA BRK-B JPM V UNH XOM MA COST HD PG JNJ ABBV BAC NFLX KO PEP WMT AMD AVGO ORCL CRM ADBE QCOM INTC TSM";

pub enum Action { Chart(String), Add(String) }

pub struct State {
    pub detail_open: bool,
    pub screen_open: bool,
    selected: String,
    input: String,
    input_error: Option<String>,
    filters: Vec<Filter>,
    sector: String,
    sort: Metric,
    descending: bool,
    candidates: Vec<String>,
    queue: VecDeque<String>,
    active: Option<String>,
    values: HashMap<String, Fundamentals>,
    errors: HashMap<String, String>,
    tx: Sender<(String, Result<Fundamentals, String>)>,
    rx: Receiver<(String, Result<Fundamentals, String>)>,
    client: Arc<Mutex<Client>>,
    next_request: std::time::Instant,
}

impl Default for State {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let filters = Metric::FILTERS.iter().map(|&m| {
            let (min, max) = match m {
                Metric::Pe | Metric::ForwardPe => (0.01, 25.0),
                Metric::Pb => (0.01, 3.0), Metric::Yield => (3.0, 100.0),
                Metric::Roe => (10.0, 1000.0), Metric::RevenueGrowth | Metric::EarningsGrowth => (10.0, 10000.0),
                Metric::Margin => (10.0, 100.0), Metric::DebtEquity => (0.0, 100.0), _ => (0.0, 100.0),
            };
            Filter { metric: m, enabled: m == Metric::Pe, min, max }
        }).collect();
        Self { detail_open: false, screen_open: false, selected: String::new(), input: String::new(), input_error: None,
            filters, sector: String::new(), sort: Metric::Pe, descending: false, candidates: Vec::new(),
            queue: VecDeque::new(), active: None, values: HashMap::new(), errors: HashMap::new(),
            tx, rx, client: Arc::new(Mutex::new(Client::default())), next_request: std::time::Instant::now() }
    }
}

impl State {
    pub fn busy(&self) -> bool { self.active.is_some() || !self.queue.is_empty() }
    fn enqueue(&mut self, symbol: &str, force: bool) {
        if self.active.as_deref() == Some(symbol) || self.queue.iter().any(|s| s == symbol) { return; }
        if !self.values.contains_key(symbol) {
            if let Some(f) = fundamentals::cached(symbol) { self.values.insert(symbol.into(), f); }
        }
        if !force && (self.values.get(symbol).is_some_and(Fundamentals::fresh) || self.errors.contains_key(symbol)) { return; }
        self.errors.remove(symbol);
        self.queue.push_back(symbol.into());
    }
    pub fn poll(&mut self, ctx: &Context) {
        while let Ok((symbol, result)) = self.rx.try_recv() {
            self.active = None;
            self.next_request = std::time::Instant::now() + std::time::Duration::from_millis(500);
            match result {
                Ok(f) => { self.errors.remove(&symbol); self.values.insert(symbol, f); }
                Err(e) => {
                    self.errors.insert(symbol, e.clone());
                    // Avoid repeating a failing session request for the entire universe.
                    if e.contains("429") || e.contains("Yahoo") {
                        for symbol in self.queue.drain(..) { self.errors.insert(symbol, e.clone()); }
                    }
                }
            }
        }
        if self.active.is_none() && std::time::Instant::now() >= self.next_request {
            if let Some(symbol) = self.queue.pop_front() {
                self.active = Some(symbol.clone());
                let (tx, ctx, client) = (self.tx.clone(), ctx.clone(), self.client.clone());
                std::thread::spawn(move || {
                    let result = client.lock().map_err(|_| "基本面資料工作階段失敗".to_string()).and_then(|mut c| c.fetch(&symbol));
                    let _ = tx.send((symbol, result));
                    ctx.request_repaint();
                });
            }
        }
        if self.busy() { ctx.request_repaint_after(std::time::Duration::from_millis(200)); }
    }
    pub fn open_detail(&mut self, symbol: &str) {
        self.detail_open = true;
        self.selected = symbol.into();
        self.enqueue(symbol, false);
    }
    pub fn open_screen(&mut self, watch: &[String]) {
        self.screen_open = true;
        if self.input.is_empty() { self.input = watch.join(" "); }
    }
    pub fn open_screen_with(&mut self, input: &str) {
        self.screen_open = true;
        self.input = input.into();
        self.start(false);
    }
    fn start(&mut self, force: bool) {
        match fundamentals::symbols(&self.input) {
            Ok(symbols) => {
                self.input_error = None;
                self.queue.clear();
                self.candidates = symbols.clone();
                for symbol in symbols { self.enqueue(&symbol, force); }
            }
            Err(e) => self.input_error = Some(e),
        }
    }
    pub fn windows(&mut self, ctx: &Context, watch: &[String]) -> Option<Action> {
        let mut action = None;
        if self.detail_open {
            let mut open = true;
            egui::Window::new("個股基本面").id(egui::Id::new("fundamentals"))
                .open(&mut open).default_width(620.0).show(ctx, |ui| self.detail(ui, watch, &mut action));
            self.detail_open = open;
        }
        if self.screen_open {
            let mut open = true;
            egui::Window::new("基本面選股").id(egui::Id::new("fundamental-screener"))
                .open(&mut open).default_width(960.0).vscroll(true).show(ctx, |ui| self.screener(ui, watch, &mut action));
            self.screen_open = open;
        }
        action
    }
    fn detail(&mut self, ui: &mut Ui, watch: &[String], action: &mut Option<Action>) {
        let before = self.selected.clone();
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("fundamental-symbol").selected_text(&self.selected).show_ui(ui, |ui| {
                let mut symbols = watch.to_vec();
                symbols.extend(self.candidates.iter().cloned());
                symbols.push(self.selected.clone());
                symbols.sort(); symbols.dedup();
                for s in symbols { ui.selectable_value(&mut self.selected, s.clone(), s); }
            });
            if ui.button("重新整理").clicked() { self.enqueue(&self.selected.clone(), true); }
            if ui.button("K 線").clicked() { *action = Some(Action::Chart(self.selected.clone())); }
            if ui.button("加入自選").clicked() { *action = Some(Action::Add(self.selected.clone())); }
        });
        if before != self.selected { self.enqueue(&self.selected.clone(), false); }
        if self.active.as_ref() == Some(&self.selected) || self.queue.contains(&self.selected) {
            ui.horizontal(|ui| { ui.spinner(); ui.label("載入基本面…"); });
        }
        if let Some(e) = self.errors.get(&self.selected) { ui.colored_label(ui.visuals().error_fg_color, e); }
        let Some(f) = self.values.get(&self.selected) else { return };
        ui.heading(format!("{} · {}", f.symbol, f.name));
        ui.label(format!("{} / {}", f.sector, f.industry));
        metadata(ui, f);
        egui::ScrollArea::vertical().id_salt("fundamental-details").max_height(550.0).show(ui, |ui| {
            egui::Grid::new("fundamental-metrics").num_columns(2).striped(true).show(ui, |ui| {
                for m in Metric::ALL { ui.label(m.label()); ui.monospace(display(f, m)); ui.end_row(); }
            });
            ui.separator();
            ui.label("TTM = 近十二個月；預估指標採資料商預測。— 表示未提供資料。");
            if !f.description.is_empty() { ui.collapsing("公司簡介", |ui| { ui.label(&f.description); }); }
            ui.hyperlink_to("Yahoo Finance 原始資料", format!("https://finance.yahoo.com/quote/{}/key-statistics/", f.symbol));
        });
    }
    fn screener(&mut self, ui: &mut Ui, watch: &[String], action: &mut Option<Action>) {
        ui.label("候選池：自選／預設清單／自訂代號（非全市場）。數字代號預設 .TW；上櫃請填 .TWO。");
        ui.horizontal_wrapped(|ui| {
            if ui.button("全部自選股").clicked() { self.input = watch.join(" "); }
            if ui.button("台股清單").clicked() { self.input = TW.into(); }
            if ui.button("美股清單").clicked() { self.input = US.into(); }
            if ui.button("開始選股").clicked() { self.start(false); }
            if ui.button("更新資料並選股").clicked() { self.start(true); }
            if self.busy() && ui.button("停止載入").clicked() { self.queue.clear(); }
        });
        ui.add(egui::TextEdit::multiline(&mut self.input).desired_rows(2).desired_width(f32::INFINITY)
            .hint_text("2330 6488.TWO AAPL MSFT（空白或逗號分隔）"));
        if let Some(e) = &self.input_error { ui.colored_label(ui.visuals().error_fg_color, e); }
        egui::CollapsingHeader::new("篩選條件（全部條件須同時符合）").default_open(true).show(ui, |ui| {
            egui::Grid::new("fundamental-filters").num_columns(4).show(ui, |ui| {
                for f in &mut self.filters {
                    ui.checkbox(&mut f.enabled, f.metric.label());
                    ui.add_enabled(f.enabled, egui::DragValue::new(&mut f.min).speed(0.1).prefix("≥ "));
                    ui.add_enabled(f.enabled, egui::DragValue::new(&mut f.max).speed(0.1).prefix("≤ "));
                    if f.enabled && (f.min > f.max || !f.min.is_finite() || !f.max.is_finite()) {
                        ui.colored_label(ui.visuals().error_fg_color, "範圍無效");
                    } else { ui.label(""); }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                ui.label("產業關鍵字");
                ui.text_edit_singleline(&mut self.sector);
                if ui.button("清除條件").clicked() { for f in &mut self.filters { f.enabled = false; } self.sector.clear(); }
            });
            ui.label("啟用指標缺值者排除；本益比下限 > 0 可排除虧損公司。條件變更即時重算。");
        });
        let mut rows = self.matching_rows();
        ui.horizontal_wrapped(|ui| {
            let complete = self.candidates.iter().filter(|s| self.active.as_ref() != Some(s) && !self.queue.contains(s)
                && (self.values.contains_key(*s) || self.errors.contains_key(*s))).count();
            let errors = self.candidates.iter().filter(|s| self.errors.contains_key(*s)).count();
            ui.label(format!("已處理 {complete}/{} · 符合 {} 檔 · 錯誤 {errors} 檔", self.candidates.len(), rows.len()));
            if self.busy() { ui.spinner(); }
            egui::ComboBox::from_id_salt("fundamental-sort").selected_text(self.sort.label()).show_ui(ui, |ui| {
                for m in Metric::FILTERS { ui.selectable_value(&mut self.sort, m, m.label()); }
            });
            ui.checkbox(&mut self.descending, "由高到低");
        });
        rows.sort_by(|a, b| match (a.value(self.sort), b.value(self.sort)) {
            (Some(a), Some(b)) => if self.descending { b.total_cmp(&a) } else { a.total_cmp(&b) },
            (Some(_), None) => std::cmp::Ordering::Less, (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.symbol.cmp(&b.symbol),
        });
        ui.label("資料：Yahoo Finance · 24 小時快取；更新失敗或過期快取不參與選股。");
        egui::ScrollArea::both().id_salt("fundamental-results").max_height(380.0).show(ui, |ui| {
            egui::Grid::new("fundamental-result-table").striped(true).show(ui, |ui| {
                for h in ["代號／名稱", "本益比", "淨值比", "殖利率 %", "ROE %", "營收年增 %", "淨利率 %", "負債/權益 %", "資料取得時間", "操作"] { ui.strong(h); }
                ui.end_row();
                for f in &rows {
                    if ui.selectable_label(false, format!("{} {}", f.symbol, f.name)).clicked() { self.open_detail(&f.symbol); }
                    for m in [Metric::Pe, Metric::Pb, Metric::Yield, Metric::Roe, Metric::RevenueGrowth, Metric::Margin, Metric::DebtEquity] {
                        ui.monospace(display(f, m));
                    }
                    ui.label(date_time(f.fetched_at));
                    ui.horizontal(|ui| {
                        if ui.small_button("K 線").clicked() { *action = Some(Action::Chart(f.symbol.clone())); }
                        if ui.small_button("＋自選").clicked() { *action = Some(Action::Add(f.symbol.clone())); }
                    });
                    ui.end_row();
                }
            });
        });
        let errors: Vec<_> = self.candidates.iter().filter_map(|s| self.errors.get(s).map(|e| format!("{s}: {e}"))).collect();
        if !errors.is_empty() {
            ui.collapsing("未取得資料的股票", |ui| {
                egui::ScrollArea::vertical().id_salt("fundamental-errors").max_height(130.0).show(ui, |ui| {
                    for e in errors { ui.label(e); }
                });
            });
        }
    }
    fn matching_rows(&self) -> Vec<Fundamentals> {
        let keyword = self.sector.trim().to_lowercase();
        self.candidates.iter().filter_map(|s| self.values.get(s))
            .filter(|f| f.fresh() && !self.errors.contains_key(&f.symbol)
                && self.active.as_ref() != Some(&f.symbol) && !self.queue.contains(&f.symbol)
                && f.equity() && self.filters.iter().all(|filter| filter.matches(f))
                && (keyword.is_empty() || format!("{} {}", f.sector, f.industry).to_lowercase().contains(&keyword)))
            .cloned().collect()
    }
}

fn date_time(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "—".into())
}
fn metadata(ui: &mut Ui, f: &Fundamentals) {
    ui.label(format!("Yahoo Finance · 取得時間 {}{}", date_time(f.fetched_at), if f.fresh() { "" } else { "（快取已過期）" }));
    if let Some(t) = f.report_at { ui.label(format!("最近財報季末 {}", date_time(t).split(' ').next().unwrap_or("—"))); }
    ui.label(format!("股價幣別 {} · 財報幣別 {}", if f.currency.is_empty() { "未提供" } else { &f.currency },
        if f.financial_currency.is_empty() { "未提供" } else { &f.financial_currency }));
}
fn display(f: &Fundamentals, m: Metric) -> String {
    let Some(v) = f.value(m) else { return "—".into() };
    if m.money() {
        let (n, unit) = if v.abs() >= 1e12 { (v / 1e12, "兆") } else if v.abs() >= 1e8 { (v / 1e8, "億") }
            else if v.abs() >= 1e4 { (v / 1e4, "萬") } else { (v, "") };
        format!("{n:.2}{unit} {}", f.money_currency(m))
    } else { format!("{v:.2}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn company(symbol: &str, pe: Option<f64>, roe: Option<f64>) -> Fundamentals {
        let mut values = [None; 16];
        values[Metric::Pe as usize] = pe;
        values[Metric::Roe as usize] = roe;
        Fundamentals { symbol: symbol.into(), name: symbol.into(), kind: "EQUITY".into(),
            sector: "Technology".into(), industry: "Semiconductors".into(), description: String::new(),
            currency: "USD".into(), financial_currency: "TWD".into(), values, report_at: None,
            fetched_at: crate::data::now_secs() }
    }
    #[test]
    fn screen_requires_all_conditions_and_current_successful_data() {
        let mut s = State::default();
        for (sym, pe, roe) in [("PASS", Some(20.0), Some(15.0)), ("EXPENSIVE", Some(30.0), Some(15.0)),
            ("MISSING", Some(20.0), None), ("LOSS", Some(-5.0), Some(15.0)),
            ("STALE", Some(20.0), Some(15.0)), ("ERROR", Some(20.0), Some(15.0)),
            ("PENDING", Some(20.0), Some(15.0))] {
            s.candidates.push(sym.into()); s.values.insert(sym.into(), company(sym, pe, roe));
        }
        s.filters.iter_mut().find(|f| f.metric == Metric::Roe).unwrap().enabled = true;
        s.values.get_mut("STALE").unwrap().fetched_at -= 86401;
        s.errors.insert("ERROR".into(), "network".into());
        s.queue.push_back("PENDING".into());
        assert_eq!(s.matching_rows().iter().map(|f| f.symbol.as_str()).collect::<Vec<_>>(), ["PASS"]);
        s.sector = " semiconductor ".into();
        assert_eq!(s.matching_rows().len(), 1);
        s.sector = "bank".into();
        assert!(s.matching_rows().is_empty());
    }
    #[test]
    fn cached_details_keep_financial_and_quote_currency_separate() {
        let mut f = company("TEST", None, None);
        f.values[Metric::MarketCap as usize] = Some(1e9);
        f.values[Metric::Revenue as usize] = Some(2e9);
        assert_eq!(display(&f, Metric::MarketCap), "10.00億 USD");
        assert_eq!(display(&f, Metric::Revenue), "20.00億 TWD");
        assert_eq!(display(&f, Metric::Pe), "—");
    }
    #[test]
    fn queue_deduplicates_and_session_failure_stops_scan() {
        let mut s = State::default();
        s.values.insert("PASS".into(), company("PASS", Some(20.0), None));
        s.enqueue("PASS", false);
        assert!(s.queue.is_empty());
        s.enqueue("PASS", true); s.enqueue("PASS", true);
        assert_eq!(s.queue.len(), 1);
        s.active = Some("FAIL".into());
        s.tx.send(("FAIL".into(), Err("Yahoo 請求過於頻繁 (429)".into()))).unwrap();
        s.poll(&Context::default());
        assert!(!s.busy());
        assert!(s.errors.contains_key("PASS"));
    }
    #[test]
    fn windows_render_details_and_screen_without_network() {
        let mut s = State::default();
        s.selected = "TEST".into();
        s.candidates.push("TEST".into());
        s.values.insert("TEST".into(), company("TEST", Some(20.0), Some(15.0)));
        s.detail_open = true; s.screen_open = true;
        let ctx = Context::default();
        for _ in 0..3 {
            ctx.begin_pass(egui::RawInput::default());
            assert!(s.windows(&ctx, &["TEST".into()]).is_none());
            let mut output = ctx.end_pass();
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
        }
        assert_eq!(s.matching_rows().len(), 1);
        assert!(!s.busy());
    }
}
