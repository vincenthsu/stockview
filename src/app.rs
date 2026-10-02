//! Application shell: toolbar, watchlist, stats table, mode switching, background fetches.

use crate::axis::*;
use crate::calc::{self, Resample, DAY};
use crate::candle::{self, Tool, ToolState};
use crate::compare::{self, Line};
use crate::data::{self, Bar, SearchHit, Series};
use crate::store::*;
use crate::theme::Palette;
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Rect, Response, Sense, Stroke, Ui};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

enum Msg {
    Series(String, Result<Series, String>),
    Hits(String, Result<Vec<SearchHit>, String>),
}

pub struct App {
    st: Persisted,
    pal: Palette,
    series: HashMap<String, Arc<Series>>,
    loading: HashSet<String>,
    errors: HashMap<String, String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cmp_view: (f64, f64),
    cdl_view: (f64, f64),
    fit_pending: bool,
    cdl_fit_pending: bool,
    tool: Tool,
    ts: ToolState,
    query: String,
    hits: Vec<SearchHit>,
    last_sent: String,
    last_edit: Instant,
    search_open: bool,
    pending_pick: bool,
    search_err: Option<String>,
    sort: (usize, bool),
    bars_cache: Option<((String, Interval, bool), Arc<Vec<Bar>>)>,
    from_txt: String,
    to_txt: String,
    range_err: Option<String>,
    drag_src: Option<String>,
    dirty: bool,
    shot_at: Option<u64>,
    last_save: Instant,
}

const MAX_COMPARE: usize = 8;

fn default_name(sym: &str) -> Option<&'static str> {
    Some(match sym {
        "2330.TW" => "台積電",
        "2317.TW" => "鴻海",
        "2454.TW" => "聯發科",
        "2412.TW" => "中華電",
        "2308.TW" => "台達電",
        "2882.TW" => "國泰金",
        "2881.TW" => "富邦金",
        "0050.TW" => "元大台灣50",
        "0056.TW" => "元大高股息",
        "^TWII" => "台灣加權指數",
        "^GSPC" => "S&P 500",
        "^IXIC" => "NASDAQ 綜合",
        "^DJI" => "道瓊工業",
        "AAPL" => "Apple",
        "NVDA" => "NVIDIA",
        "MSFT" => "Microsoft",
        "TSLA" => "Tesla",
        "AMZN" => "Amazon",
        "GOOGL" => "Alphabet",
        "META" => "Meta",
        "SPY" => "SPDR S&P 500 ETF",
        "QQQ" => "Invesco QQQ",
        _ => return None,
    })
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        let st = Persisted::load();
        let pal = Palette::new(st.dark, st.red_up);
        pal.apply(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = Self {
            st,
            pal,
            series: HashMap::new(),
            loading: HashSet::new(),
            errors: HashMap::new(),
            tx,
            rx,
            cmp_view: (0.0, 1.0),
            cdl_view: (0.0, 1.0),
            fit_pending: true,
            cdl_fit_pending: true,
            tool: Tool::Cursor,
            ts: ToolState::default(),
            query: String::new(),
            hits: Vec::new(),
            last_sent: String::new(),
            last_edit: Instant::now(),
            search_open: false,
            pending_pick: false,
            search_err: None,
            sort: (3, true),
            bars_cache: None,
            from_txt: String::new(),
            to_txt: String::new(),
            range_err: None,
            drag_src: None,
            dirty: false,
            shot_at: None,
            last_save: Instant::now(),
        };
        // make sure no two compared series share an ink (older saved states could)
        let mut seen = HashSet::new();
        for sym in app.st.compare.clone() {
            if let Some(i) = app.st.colors.get(&sym) {
                if !seen.insert(*i) {
                    app.st.colors.remove(&sym);
                }
            }
        }
        for sym in app.st.compare.clone() {
            app.color_of(&sym);
        }
        let mut want: Vec<String> = app.st.compare.clone();
        want.push(app.st.selected.clone());
        want.extend(app.st.watchlist.clone());
        for s in want {
            app.request(&cc.egui_ctx, &s);
        }
        app
    }

    fn request(&mut self, ctx: &egui::Context, sym: &str) {
        if self.loading.contains(sym) {
            return;
        }
        let mut need_fetch = true;
        if !self.series.contains_key(sym) {
            if let Some(c) = data::load_cached(sym) {
                need_fetch = !data::is_fresh(&c);
                self.series.insert(sym.to_string(), Arc::new(c));
            }
        } else if let Some(s) = self.series.get(sym) {
            need_fetch = !data::is_fresh(s);
        }
        if !need_fetch {
            return;
        }
        self.errors.remove(sym);
        self.loading.insert(sym.to_string());
        let (tx, ctx, sym) = (self.tx.clone(), ctx.clone(), sym.to_string());
        std::thread::spawn(move || {
            let r = data::fetch_series(&sym);
            let _ = tx.send(Msg::Series(sym, r));
            ctx.request_repaint();
        });
    }

    fn send_search(&mut self, ctx: &egui::Context) {
        let q = self.query.trim().to_string();
        if q.is_empty() || q == self.last_sent {
            return;
        }
        self.last_sent = q.clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = data::search(&q);
            let _ = tx.send(Msg::Hits(q, r));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Series(sym, r) => {
                    self.loading.remove(&sym);
                    match r {
                        Ok(s) => {
                            self.series.insert(sym.clone(), Arc::new(s));
                            self.bars_cache = None;
                            if sym == self.st.selected {
                                self.cdl_fit_pending = self.cdl_fit_pending || false;
                            }
                        }
                        Err(e) => {
                            if sym.ends_with(".TW") && e.contains("找不到") {
                                let alt = sym.replace(".TW", ".TWO");
                                self.rename_symbol(&sym, &alt);
                                self.request(ctx, &alt);
                            } else if self.series.contains_key(&sym) {
                                // keep showing the cached copy; just note staleness
                                self.errors.insert(sym, format!("使用快取資料({e})"));
                            } else {
                                self.errors.insert(sym, e);
                            }
                        }
                    }
                }
                Msg::Hits(q, r) => {
                    if q != self.query.trim() {
                        continue;
                    }
                    match r {
                        Ok(h) => {
                            self.hits = self.local_hits(&q, h);
                            self.search_err = None;
                        }
                        Err(e) => {
                            self.hits = self.local_hits(&q, vec![]);
                            self.search_err = Some(e);
                        }
                    }
                    self.search_open = true;
                    if self.pending_pick {
                        self.pending_pick = false;
                        if let Some(h) = self.hits.first().cloned() {
                            self.pick(ctx, h);
                        }
                    }
                }
            }
        }
        if self.fit_pending {
            let ready = self
                .st
                .compare
                .iter()
                .all(|s| self.series.contains_key(s) || self.errors.contains_key(s))
                && self.st.compare.iter().any(|s| self.series.contains_key(s));
            if ready {
                self.fit_compare();
                self.fit_pending = false;
            }
        }
        if self.cdl_fit_pending && self.series.contains_key(&self.st.selected) {
            self.fit_candle();
            self.cdl_fit_pending = false;
        }
    }

    fn local_hits(&self, q: &str, mut hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let q = q.trim();
        let digits = q.len() >= 4 && q.len() <= 6 && q.chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
        if digits && q.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            let sym = format!("{q}.TW");
            if !hits.iter().any(|h| h.symbol == sym || h.symbol == format!("{q}.TWO")) {
                hits.insert(0, SearchHit { symbol: sym, name: String::new(), exchange: "台灣".into(), kind: "EQUITY".into() });
            }
        } else if q.len() <= 5 && q.chars().all(|c| c.is_ascii_alphabetic()) {
            let sym = q.to_uppercase();
            if !hits.iter().any(|h| h.symbol == sym) {
                hits.push(SearchHit { symbol: sym, name: String::new(), exchange: String::new(), kind: "EQUITY".into() });
            }
        }
        hits
    }

    fn rename_symbol(&mut self, from: &str, to: &str) {
        for v in [&mut self.st.watchlist, &mut self.st.compare] {
            for s in v.iter_mut() {
                if s == from {
                    *s = to.to_string();
                }
            }
        }
        if self.st.selected == from {
            self.st.selected = to.to_string();
        }
        if let Some(c) = self.st.colors.remove(from) {
            self.st.colors.insert(to.to_string(), c);
        }
        self.errors.remove(from);
        self.dirty = true;
    }

    fn pick(&mut self, ctx: &egui::Context, h: SearchHit) {
        let sym = h.symbol.clone();
        if !h.name.is_empty() {
            self.st.names.insert(sym.clone(), h.name.clone());
        }
        if !self.st.watchlist.contains(&sym) {
            self.st.watchlist.push(sym.clone());
        }
        match self.st.mode {
            Mode::Compare => self.add_compare(&sym),
            Mode::Chart => {
                self.st.selected = sym.clone();
                self.cdl_fit_pending = true;
            }
        }
        self.request(ctx, &sym);
        self.query.clear();
        self.hits.clear();
        self.last_sent.clear();
        self.search_open = false;
        self.dirty = true;
    }

    fn add_compare(&mut self, sym: &str) {
        if self.st.compare.iter().any(|s| s == sym) {
            return;
        }
        if self.st.compare.len() >= MAX_COMPARE {
            self.st.compare.remove(0);
        }
        self.st.compare.push(sym.to_string());
        // a remembered ink may now be taken by another series; pick a free one
        let taken = self.st.compare.iter().filter(|x| *x != sym).filter_map(|x| self.st.colors.get(x)).any(|c| Some(c) == self.st.colors.get(sym));
        if taken {
            self.st.colors.remove(sym);
        }
        self.color_of(sym);
        self.dirty = true;
    }

    fn color_idx_used(&self) -> HashSet<usize> {
        self.st.compare.iter().filter_map(|s| self.st.colors.get(s).copied()).collect()
    }

    fn color_of(&mut self, sym: &str) -> Color32 {
        if let Some(i) = self.st.colors.get(sym) {
            return self.pal.series[*i % 8];
        }
        let used = self.color_idx_used();
        let idx = (0..8).find(|i| !used.contains(i)).unwrap_or(0);
        self.st.colors.insert(sym.to_string(), idx);
        self.pal.series[idx]
    }

    fn color_peek(&self, sym: &str) -> Color32 {
        self.st.colors.get(sym).map(|i| self.pal.series[*i % 8]).unwrap_or(self.pal.dim)
    }

    fn display_name(&self, sym: &str) -> String {
        if let Some(n) = self.st.names.get(sym) {
            if !n.is_empty() {
                return n.clone();
            }
        }
        if let Some(n) = default_name(sym) {
            return n.to_string();
        }
        self.series.get(sym).map(|s| s.name.clone()).unwrap_or_default()
    }

    fn lines(&self) -> Vec<Line> {
        self.st
            .compare
            .iter()
            .filter_map(|s| {
                self.series.get(s).map(|ser| Line { symbol: s.clone(), color: self.color_peek(s), series: ser.clone() })
            })
            .collect()
    }

    /// Visible window (unix seconds) for the selected range given the data bounds.
    fn range_window(&self, lo: f64, hi: f64) -> (f64, f64) {
        match self.st.range {
            Range::Custom if self.st.custom_to > self.st.custom_from => {
                (self.st.custom_from as f64, self.st.custom_to as f64)
            }
            r => match r.days() {
                Some(d) => ((hi - d * DAY as f64).max(lo), hi),
                None => (lo, hi),
            },
        }
    }

    fn fit_compare(&mut self) {
        let lines = self.lines();
        if let Some((lo, hi)) = compare::bounds(&lines) {
            let (tl, tr) = self.range_window(lo as f64, hi as f64);
            let pad = (tr - tl) * 0.015;
            self.cmp_view = (tl - pad, tr + pad * 1.5);
        }
    }

    fn fit_candle(&mut self) {
        if let Some(bars) = self.chart_bars() {
            let n = bars.len() as f64;
            let (lo, hi) = (bars[0].t as f64, bars[bars.len() - 1].t as f64);
            let (tl, tr) = self.range_window(lo, hi);
            let il = candle::idx_of_t(&bars, tl as i64).max(0.0);
            let ir = candle::idx_of_t(&bars, tr as i64).min(n - 1.0);
            let pad = ((ir - il) * 0.04).max(1.5);
            self.cdl_view = (il - 0.5, ir + pad);
        }
    }

    fn chart_bars(&mut self) -> Option<Arc<Vec<Bar>>> {
        let key = (self.st.selected.clone(), self.st.interval, self.st.total_return);
        if let Some((k, b)) = &self.bars_cache {
            if *k == key {
                return Some(b.clone());
            }
        }
        let s = self.series.get(&self.st.selected)?.clone();
        let adj: Vec<Bar> = if self.st.total_return {
            s.bars
                .iter()
                .map(|b| {
                    let f = if b.c != 0.0 { b.adj / b.c } else { 1.0 };
                    Bar { t: b.t, o: b.o * f, h: b.h * f, l: b.l * f, c: b.adj, adj: b.adj, v: b.v }
                })
                .collect()
        } else {
            s.bars.clone()
        };
        let unit = match self.st.interval {
            Interval::D => Resample::Day,
            Interval::W => Resample::Week,
            Interval::M => Resample::Month,
        };
        let out = Arc::new(calc::resample(&adj, unit));
        self.bars_cache = Some((key, out.clone()));
        Some(out)
    }

    /// Dev aid: STOCKVIEW_SHOT=<png path> saves a screenshot once data has loaded, then exits.
    /// STOCKVIEW_MODE=chart|compare, STOCKVIEW_DARK=1 pick the view.
    fn debug_shot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("STOCKVIEW_SHOT") else { return };
        let frame = ctx.cumulative_pass_nr();
        if frame == 1 {
            match std::env::var("STOCKVIEW_MODE").as_deref() {
                Ok("chart") => {
                    self.st.mode = Mode::Chart;
                    self.st.indicators.bollinger = true;
                    self.st.indicators.rsi = true;
                    self.st.indicators.macd = true;
                    self.st.range = Range::Y3;
                    self.cdl_fit_pending = true;
                }
                _ => {}
            }
            if let Ok(r) = std::env::var("STOCKVIEW_RANGE") {
                if let Some(r) = Range::ALL.iter().copied().find(|x| x.label() == r) {
                    self.set_range(ctx, r);
                }
            }
            if std::env::var("STOCKVIEW_DARK").is_ok() {
                self.st.dark = true;
                self.apply_theme(ctx);
            }
        }
        let ready = self.loading.is_empty() && !self.fit_pending;
        ctx.request_repaint_after(Duration::from_millis(100));
        if ready && self.shot_at.is_none() {
            self.shot_at = Some(frame + 8);
        }
        if let Some(at) = self.shot_at {
            if frame == at {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        let img = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let [w, h] = img.size;
            let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = image::save_buffer(&path, &raw, w as u32, h as u32, image::ColorType::Rgba8);
            std::process::exit(0);
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        self.pal = Palette::new(self.st.dark, self.st.red_up);
        self.pal.apply(ctx);
        self.dirty = true;
    }

    // ---------------------------------------------------------------- UI pieces

    fn top_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            // wordmark with a small vermilion baseline
            let (r, _) = ui.allocate_exact_size(vec2(92.0, 28.0), Sense::hover());
            ui.painter().text(r.left_center() - vec2(0.0, 3.0), Align2::LEFT_CENTER, "StockView", FontId::proportional(16.5), self.pal.ink);
            ui.painter().line_segment(
                [pos2(r.left(), r.bottom() - 4.0), pos2(r.left() + 78.0, r.bottom() - 4.0)],
                Stroke::new(2.0, self.pal.baseline),
            );
            ui.add_space(6.0);
            self.search_box(ui);
            ui.add_space(8.0);
            let mode = self.st.mode;
            if chip_btn(ui, &self.pal, "比較", mode == Mode::Compare).clicked() {
                self.st.mode = Mode::Compare;
                self.dirty = true;
            }
            if chip_btn(ui, &self.pal, "K 線", mode == Mode::Chart).clicked() {
                self.st.mode = Mode::Chart;
                self.cdl_fit_pending = true;
                self.dirty = true;
            }
            sep(ui, &self.pal);
            for r in Range::ALL {
                if chip_btn(ui, &self.pal, r.label(), self.st.range == r).clicked() {
                    self.set_range(&ctx, r);
                }
            }
            self.custom_range_menu(ui);
            if self.st.mode == Mode::Chart {
                sep(ui, &self.pal);
                for (iv, l) in [(Interval::D, "日"), (Interval::W, "週"), (Interval::M, "月")] {
                    if chip_btn(ui, &self.pal, l, self.st.interval == iv).clicked() {
                        self.st.interval = iv;
                        self.cdl_fit_pending = true;
                        self.dirty = true;
                    }
                }
                self.indicator_menu(ui);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                let night = self.st.dark;
                if chip_btn(ui, &self.pal, if night { "夜間" } else { "日間" }, false)
                    .on_hover_text("切換日間 / 夜間")
                    .clicked()
                {
                    self.st.dark = !night;
                    self.apply_theme(&ctx);
                }
                let ru = self.st.red_up;
                if chip_btn(ui, &self.pal, if ru { "紅漲綠跌" } else { "綠漲紅跌" }, false)
                    .on_hover_text("切換漲跌顏色慣例(台股紅漲 / 美股綠漲)")
                    .clicked()
                {
                    self.st.red_up = !ru;
                    self.apply_theme(&ctx);
                }
                if chip_btn(ui, &self.pal, "對數", self.st.log_scale).on_hover_text("對數座標").clicked() {
                    self.st.log_scale = !self.st.log_scale;
                    self.dirty = true;
                }
                if chip_btn(ui, &self.pal, "含息", self.st.total_return)
                    .on_hover_text("含息報酬:使用還原股價(股息再投入)。關閉則為純價格報酬")
                    .clicked()
                {
                    self.st.total_return = !self.st.total_return;
                    self.dirty = true;
                }
            });
        });
        ui.add_space(6.0);
    }

    /// Apply a range and refresh both views immediately; stale data is re-fetched in the background.
    fn set_range(&mut self, ctx: &egui::Context, r: Range) {
        self.st.range = r;
        self.fit_compare();
        self.fit_candle();
        let mut syms = self.st.compare.clone();
        syms.push(self.st.selected.clone());
        for s in syms {
            let stale = self.series.get(&s).map_or(false, |x| data::now_secs() - x.fetched_at > 3600);
            if stale {
                self.request_force(ctx, &s);
            }
        }
        self.dirty = true;
        ctx.request_repaint();
    }

    fn request_force(&mut self, ctx: &egui::Context, sym: &str) {
        if self.loading.contains(sym) {
            return;
        }
        self.errors.remove(sym);
        self.loading.insert(sym.to_string());
        let (tx, ctx, sym) = (self.tx.clone(), ctx.clone(), sym.to_string());
        std::thread::spawn(move || {
            let r = data::fetch_series(&sym);
            let _ = tx.send(Msg::Series(sym, r));
            ctx.request_repaint();
        });
    }

    fn custom_range_menu(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let resp = chip_btn(ui, &self.pal, "自訂", self.st.range == Range::Custom);
        if resp.clicked() {
            // prefill with what is currently visible
            let (a, b) = if self.st.mode == Mode::Compare {
                self.cmp_view
            } else {
                self.chart_bars()
                    .map(|bars| {
                        (
                            candle::t_of_idx(&bars, self.cdl_view.0.max(0.0)) as f64,
                            candle::t_of_idx(&bars, self.cdl_view.1.min(bars.len() as f64 - 1.0)) as f64,
                        )
                    })
                    .unwrap_or(self.cmp_view)
            };
            self.from_txt = fmt_date(a as i64);
            self.to_txt = fmt_date(b as i64);
            self.range_err = None;
        }
        let mut apply = false;
        egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(230.0);
            ui.label(egui::RichText::new("自訂日期範圍").size(12.0).color(self.pal.dim));
            ui.add_space(4.0);
            egui::Grid::new("custom_range").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("起");
                ui.add(egui::TextEdit::singleline(&mut self.from_txt).hint_text("YYYY-MM-DD").desired_width(130.0));
                ui.end_row();
                ui.label("迄");
                let r = ui.add(egui::TextEdit::singleline(&mut self.to_txt).hint_text("YYYY-MM-DD").desired_width(130.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                ui.end_row();
            });
            if let Some(e) = &self.range_err {
                ui.colored_label(self.pal.baseline, e);
            }
            ui.add_space(4.0);
            if chip_btn(ui, &self.pal, "套用", true).clicked() {
                apply = true;
            }
        });
        if apply {
            let parse = |t: &str| chrono::NaiveDate::parse_from_str(t.trim(), "%Y-%m-%d").ok();
            match (parse(&self.from_txt), parse(&self.to_txt)) {
                (Some(a), Some(b)) if a < b => {
                    self.st.custom_from = ts_of(a);
                    self.st.custom_to = ts_of(b);
                    self.range_err = None;
                    self.set_range(&ctx, Range::Custom);
                    egui::Popup::close_all(&ctx);
                }
                (Some(_), Some(_)) => self.range_err = Some("起始日必須早於結束日".into()),
                _ => self.range_err = Some("日期格式應為 YYYY-MM-DD".into()),
            }
        }
    }

    fn indicator_menu(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let ind = &mut self.st.indicators;
        let active = ind.ma || ind.ema || ind.bollinger || ind.rsi || ind.macd;
        let resp = chip_btn(ui, &self.pal, "指標", active);
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(170.0);
            changed |= ui.checkbox(&mut ind.volume, "成交量").changed();
            changed |= ui.checkbox(&mut ind.ma, "移動平均 MA 5 / 20 / 60").changed();
            changed |= ui.checkbox(&mut ind.ema, "指數移動平均 EMA 50").changed();
            changed |= ui.checkbox(&mut ind.bollinger, "布林通道 (20, 2)").changed();
            changed |= ui.checkbox(&mut ind.rsi, "RSI (14)").changed();
            changed |= ui.checkbox(&mut ind.macd, "MACD (12, 26, 9)").changed();
        });
        if changed {
            self.dirty = true;
        }
    }

    fn search_box(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let te = egui::TextEdit::singleline(&mut self.query)
            .hint_text("搜尋代號或名稱:2330、AAPL、台積電")
            .desired_width(190.0)
            .margin(vec2(8.0, 5.0));
        let resp = ui.add(te);
        if resp.changed() {
            self.last_edit = Instant::now();
            self.search_open = true;
            self.pending_pick = false;
            let q = self.query.trim().to_string();
            self.hits = self.local_hits(&q, vec![]);
        }
        if !self.query.trim().is_empty()
            && self.query.trim() != self.last_sent
            && self.last_edit.elapsed() > Duration::from_millis(280)
        {
            self.send_search(&ctx);
        } else if !self.query.trim().is_empty() && self.query.trim() != self.last_sent {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.query.trim().is_empty() {
            if let Some(h) = self.hits.first().cloned() {
                self.pick(&ctx, h);
            } else {
                self.pending_pick = true;
                self.last_sent.clear();
                self.send_search(&ctx);
            }
        }
        if self.search_open && !self.hits.is_empty() && !self.query.trim().is_empty() {
            let mut chosen: Option<SearchHit> = None;
            let area = egui::Area::new(Id::new("search_pop"))
                .order(egui::Order::Foreground)
                .fixed_pos(resp.rect.left_bottom() + vec2(0.0, 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(380.0);
                        for h in self.hits.iter().take(10) {
                            let (r, rs) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                            if rs.hovered() {
                                ui.painter().rect_filled(r, 2.0, self.pal.grid_major);
                            }
                            ui.painter().text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, &h.symbol, FontId::monospace(12.5), self.pal.ink);
                            let mut name = if h.name.is_empty() { default_name(&h.symbol).unwrap_or("").to_string() } else { h.name.clone() };
                            if name.chars().count() > 26 {
                                name = name.chars().take(25).collect::<String>() + "…";
                            }
                            ui.painter().text(r.left_center() + vec2(86.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(12.5), self.pal.ink);
                            ui.painter().text(r.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, &h.exchange, FontId::proportional(11.0), self.pal.dim);
                            if rs.clicked() {
                                chosen = Some(h.clone());
                            }
                        }
                        if let Some(e) = &self.search_err {
                            ui.colored_label(self.pal.baseline, e);
                        }
                    });
                });
            if let Some(h) = chosen {
                self.pick(&ctx, h);
            } else if ctx.input(|i| i.pointer.any_pressed()) {
                let pos = ctx.input(|i| i.pointer.interact_pos());
                if let Some(p) = pos {
                    if !area.response.rect.contains(p) && !resp.rect.contains(p) {
                        self.search_open = false;
                    }
                }
            }
        }
    }

    fn legend_strip(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut remove: Option<String> = None;
        let mut retry: Option<String> = None;
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            for sym in self.st.compare.clone() {
                let col = self.color_peek(&sym);
                let name = self.display_name(&sym);
                let loading = self.loading.contains(&sym);
                let err = self.errors.get(&sym).cloned();
                let label = if name.is_empty() { sym.clone() } else { format!("{sym}  {name}") };
                let g = ui.painter().layout_no_wrap(label, FontId::proportional(12.5), self.pal.ink);
                let w = g.size().x + 54.0;
                let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(r, 2.0, self.pal.panel);
                p.rect_stroke(r, 2.0, Stroke::new(1.0, if resp.hovered() { self.pal.ink } else { self.pal.hair }), egui::StrokeKind::Inside);
                p.rect_filled(Rect::from_min_size(r.min + vec2(7.0, 8.0), vec2(8.0, 8.0)), 1.0, col);
                p.galley(pos2(r.left() + 22.0, r.center().y - g.size().y / 2.0), g, self.pal.ink);
                let xr = Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(14.0, 14.0));
                let hx = ui.rect_contains_pointer(xr);
                let xc = if hx { self.pal.baseline } else { self.pal.dim };
                p.line_segment([xr.center() - vec2(3.5, 3.5), xr.center() + vec2(3.5, 3.5)], Stroke::new(1.4, xc));
                p.line_segment([xr.center() - vec2(-3.5, 3.5), xr.center() + vec2(-3.5, 3.5)], Stroke::new(1.4, xc));
                if loading {
                    p.text(pos2(r.right() - 28.0, r.center().y), Align2::RIGHT_CENTER, "載入中…", FontId::proportional(10.5), self.pal.dim);
                }
                if resp.clicked() {
                    if hx {
                        remove = Some(sym.clone());
                    } else if err.is_some() {
                        retry = Some(sym.clone());
                    }
                }
                if let Some(e) = err {
                    resp.on_hover_text(e);
                    ui.painter().circle_filled(r.right_top() + vec2(-3.0, 3.0), 3.5, self.pal.baseline);
                }
            }
            if self.st.compare.is_empty() {
                ui.colored_label(self.pal.dim, "尚未加入任何股票 — 從上方搜尋,或點選右側自選清單");
            }
        });
        if let Some(s) = remove {
            self.st.compare.retain(|x| *x != s);
            self.dirty = true;
        }
        if let Some(s) = retry {
            self.errors.remove(&s);
            self.request(&ctx, &s);
        }
        ui.add_space(4.0);
    }

    fn watchlist(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(format!("自選清單  {}", self.st.watchlist.len())).size(12.5).color(self.pal.dim));
        });
        ui.add_space(4.0);
        let mut toggle: Option<String> = None;
        let mut open: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut rects: Vec<Rect> = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for sym in self.st.watchlist.clone() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click_and_drag());
                rects.push(r);
                if resp.drag_started() {
                    self.drag_src = Some(sym.clone());
                }
                if resp.hovered() || self.drag_src.as_deref() == Some(sym.as_str()) {
                    for dx in [3.0, 6.0] {
                        for dy in [-5.0, 0.0, 5.0] {
                            ui.painter().circle_filled(pos2(r.left() + dx, r.center().y + dy), 1.0, self.pal.dim);
                        }
                    }
                    ui.ctx().set_cursor_icon(if self.drag_src.is_some() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                }
                let selected = match self.st.mode {
                    Mode::Chart => self.st.selected == sym,
                    Mode::Compare => false,
                };
                let in_cmp = self.st.compare.contains(&sym);
                let p = ui.painter();
                if selected {
                    p.rect_filled(r, 0.0, self.pal.grid_major);
                } else if resp.hovered() {
                    p.rect_filled(r, 0.0, self.pal.grid);
                }
                p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, self.pal.grid));
                let sw = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 10.0), vec2(10.0, 10.0));
                if in_cmp {
                    p.rect_filled(sw, 1.0, self.color_peek(&sym));
                } else {
                    p.rect_stroke(sw, 1.0, Stroke::new(1.2, self.pal.hair), egui::StrokeKind::Inside);
                }
                p.text(pos2(r.left() + 28.0, r.top() + 15.0), Align2::LEFT_CENTER, &sym, FontId::proportional(13.5), self.pal.ink);
                let mut name = self.display_name(&sym);
                if name.chars().count() > 16 {
                    name = name.chars().take(15).collect::<String>() + "…";
                }
                p.text(pos2(r.left() + 28.0, r.top() + 33.0), Align2::LEFT_CENTER, name, FontId::proportional(11.5), self.pal.dim);
                if let Some(s) = self.series.get(&sym) {
                    let n = s.bars.len();
                    let last = &s.bars[n - 1];
                    let prev = &s.bars[n - 2];
                    let chg = last.c / prev.c - 1.0;
                    let col = if chg >= 0.0 { self.pal.up } else { self.pal.down };
                    p.text(pos2(r.right() - 12.0, r.top() + 15.0), Align2::RIGHT_CENTER, fmt_price(last.c), FontId::monospace(12.5), self.pal.ink);
                    p.text(pos2(r.right() - 12.0, r.top() + 33.0), Align2::RIGHT_CENTER, fmt_pct(chg), FontId::monospace(11.5), col);
                } else if self.loading.contains(&sym) {
                    p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, "…", FontId::proportional(13.0), self.pal.dim);
                } else if self.errors.contains_key(&sym) {
                    p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, "無資料", FontId::proportional(11.5), self.pal.baseline);
                }
                if resp.clicked() {
                    toggle = Some(sym.clone());
                }
                if resp.double_clicked() {
                    open = Some(sym.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button("以 K 線檢視").clicked() {
                        open = Some(sym.clone());
                        ui.close();
                    }
                    if ui.button(if in_cmp { "從比較移除" } else { "加入比較" }).clicked() {
                        toggle = Some(sym.clone());
                        ui.close();
                    }
                    if ui.button("從自選移除").clicked() {
                        remove = Some(sym.clone());
                        ui.close();
                    }
                });
            }
        });
        // drag-to-reorder: insertion point = first row whose centre is below the pointer
        if let Some(src) = self.drag_src.clone() {
            let ptr = ctx.input(|i| i.pointer.interact_pos());
            let target = ptr.map(|p| rects.iter().position(|r| r.center().y > p.y).unwrap_or(rects.len()));
            if let (Some(t), true) = (target, !rects.is_empty()) {
                let y = if t < rects.len() { rects[t].top() } else { rects[rects.len() - 1].bottom() };
                let x0 = rects[0].left();
                let x1 = rects[0].right();
                ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.5, self.pal.baseline));
            }
            if ctx.input(|i| i.pointer.any_released()) {
                if let (Some(t), Some(from)) = (target, self.st.watchlist.iter().position(|x| *x == src)) {
                    let item = self.st.watchlist.remove(from);
                    let t = if t > from { t - 1 } else { t };
                    self.st.watchlist.insert(t.min(self.st.watchlist.len()), item);
                    self.dirty = true;
                }
                self.drag_src = None;
                toggle = None;
            }
        }
        if let Some(s) = toggle {
            match self.st.mode {
                Mode::Compare => {
                    if self.st.compare.contains(&s) {
                        self.st.compare.retain(|x| *x != s);
                    } else {
                        self.add_compare(&s);
                    }
                }
                Mode::Chart => {
                    self.st.selected = s.clone();
                    self.cdl_fit_pending = true;
                    self.ts = ToolState::default();
                }
            }
            self.request(&ctx, &s);
            self.dirty = true;
        }
        if let Some(s) = open {
            self.st.selected = s;
            self.st.mode = Mode::Chart;
            self.cdl_fit_pending = true;
            self.ts = ToolState::default();
            self.dirty = true;
        }
        if let Some(s) = remove {
            self.st.watchlist.retain(|x| *x != s);
            self.st.compare.retain(|x| *x != s);
            self.dirty = true;
        }
    }

    fn stats_panel(&mut self, ui: &mut Ui) {
        let lines = self.lines();
        let (tl, tr) = self.cmp_view;
        let t0 = compare::base_time(&lines, tl as i64);
        let t1 = (tr as i64).min(lines.iter().filter_map(|l| l.series.bars.last().map(|b| b.t)).max().unwrap_or(0));
        struct Row {
            sym: String,
            name: String,
            color: Color32,
            st: Option<calc::Stats>,
            cur: String,
        }
        let mut rows: Vec<Row> = lines
            .iter()
            .map(|l| Row {
                sym: l.symbol.clone(),
                name: self.display_name(&l.symbol),
                color: l.color,
                st: calc::stats(&l.series.bars, t0, t1, self.st.total_return),
                cur: l.series.currency.clone(),
            })
            .collect();
        let key = |r: &Row| -> f64 {
            let s = r.st.clone().unwrap_or_default();
            match self.sort.0 {
                3 => s.total_return,
                4 => s.cagr,
                5 => s.max_drawdown,
                6 => s.volatility,
                7 => s.sharpe,
                _ => 0.0,
            }
        };
        rows.sort_by(|a, b| {
            if self.sort.0 == 0 {
                a.sym.cmp(&b.sym)
            } else {
                key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        if self.sort.1 && self.sort.0 != 0 {
            rows.reverse();
        }
        let maxabs = rows.iter().filter_map(|r| r.st.as_ref().map(|s| s.total_return.abs())).fold(0.0f64, f64::max).max(1e-9);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let basis = if self.st.total_return { "含息總報酬(還原股價)" } else { "價格報酬" };
            ui.label(
                egui::RichText::new(format!(
                    "比較統計  ·  {} → {}  ·  {}  ·  各檔以本幣計算,% 報酬不受匯率影響",
                    fmt_date(t0),
                    fmt_date(t1),
                    basis
                ))
                .size(12.0)
                .color(self.pal.dim),
            );
        });
        ui.add_space(4.0);
        let avail = ui.available_width();
        let cols: [(&str, f32); 8] = [
            ("代號", 112.0),
            ("名稱", (avail - 802.0).max(120.0)),
            ("年數", 80.0),
            ("總報酬", 230.0),
            ("年化 CAGR", 92.0),
            ("最大回撤", 92.0),
            ("年化波動", 92.0),
            ("夏普值", 80.0),
        ];
        let x0 = ui.cursor().left() + 12.0;
        // header
        let (hr, _) = ui.allocate_exact_size(vec2(avail, 24.0), Sense::hover());
        let mut x = x0;
        let mut new_sort: Option<usize> = None;
        for (i, (name, w)) in cols.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(x, hr.top()), vec2(*w, 24.0));
            let sortable = !matches!(i, 1 | 2);
            let resp = ui.interact(cell, Id::new(("sorthdr", i)), if sortable { Sense::click() } else { Sense::hover() });
            let right = i >= 2;
            let active = self.sort.0 == i;
            let arrow = if active { if self.sort.1 { " ↓" } else { " ↑" } } else { "" };
            let al = if right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER };
            let px = if right { cell.right() - 10.0 } else { cell.left() };
            ui.painter().text(
                pos2(px, cell.center().y),
                al,
                format!("{name}{arrow}"),
                FontId::proportional(11.5),
                if active || resp.hovered() { self.pal.ink } else { self.pal.dim },
            );
            if resp.clicked() && sortable {
                new_sort = Some(i);
            }
            x += *w;
        }
        if let Some(i) = new_sort {
            self.sort = if self.sort.0 == i { (i, !self.sort.1) } else { (i, true) };
        }
        ui.painter().line_segment([hr.left_bottom(), hr.right_bottom()], Stroke::new(1.0, self.pal.hair));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in &rows {
                let (r, resp) = ui.allocate_exact_size(vec2(avail, 28.0), Sense::hover());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, self.pal.grid);
                }
                ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, self.pal.grid));
                let p = ui.painter();
                let mut x = x0;
                let cy = r.center().y;
                p.rect_filled(Rect::from_center_size(pos2(x + 5.0, cy), vec2(10.0, 10.0)), 1.0, row.color);
                p.text(pos2(x + 18.0, cy), Align2::LEFT_CENTER, &row.sym, FontId::proportional(13.0), self.pal.ink);
                x += cols[0].1;
                let mut nm = row.name.clone();
                let maxc = ((cols[1].1 / 7.5) as usize).max(8);
                if nm.chars().count() > maxc {
                    nm = nm.chars().take(maxc - 1).collect::<String>() + "…";
                }
                p.text(pos2(x, cy), Align2::LEFT_CENTER, format!("{}  {}{}", nm, row.cur, match &row.st { Some(st) if st.start_t > t0 + 7 * DAY => format!("  · 自 {} 起", fmt_date(st.start_t)), _ => String::new() }), FontId::proportional(12.0), self.pal.dim);
                x += cols[1].1;
                let mono = FontId::monospace(12.5);
                match &row.st {
                    None => {
                        p.text(pos2(x + cols[2].1 - 10.0, cy), Align2::RIGHT_CENTER, "資料不足", FontId::proportional(11.5), self.pal.dim);
                    }
                    Some(s) => {
                        p.text(pos2(x + cols[2].1 - 10.0, cy), Align2::RIGHT_CENTER, format!("{:.1}", s.years), mono.clone(), self.pal.dim);
                        x += cols[2].1;
                        // total return with proportional bar
                        let cell = Rect::from_min_size(pos2(x + 10.0, r.top() + 4.0), vec2(cols[3].1 - 20.0, 20.0));
                        let frac = (s.total_return.abs() / maxabs) as f32;
                        let bw = (cell.width() * frac).max(2.0);
                        p.rect_filled(Rect::from_min_size(cell.min, vec2(bw, cell.height())), 1.0, row.color.gamma_multiply(0.22));
                        p.rect_filled(Rect::from_min_size(cell.min, vec2(3.0, cell.height())), 0.0, row.color);
                        let col = if s.total_return >= 0.0 { self.pal.up } else { self.pal.down };
                        p.text(cell.right_center() - vec2(2.0, 0.0), Align2::RIGHT_CENTER, fmt_pct(s.total_return), mono.clone(), col);
                        x += cols[3].1;
                        let c = if s.cagr >= 0.0 { self.pal.up } else { self.pal.down };
                        p.text(pos2(x + cols[4].1 - 10.0, cy), Align2::RIGHT_CENTER, fmt_pct(s.cagr), mono.clone(), c);
                        x += cols[4].1;
                        p.text(pos2(x + cols[5].1 - 10.0, cy), Align2::RIGHT_CENTER, fmt_pct_plain(s.max_drawdown), mono.clone(), self.pal.down);
                        x += cols[5].1;
                        p.text(pos2(x + cols[6].1 - 10.0, cy), Align2::RIGHT_CENTER, fmt_pct_plain(s.volatility), mono.clone(), self.pal.ink);
                        x += cols[6].1;
                        p.text(pos2(x + cols[7].1 - 10.0, cy), Align2::RIGHT_CENTER, format!("{:.2}", s.sharpe), mono.clone(), self.pal.ink);
                    }
                }
            }
        });
    }

    fn tools_panel(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        let tools = [
            (Tool::Cursor, 0, "游標 / 選取(Delete 刪除選取線)"),
            (Tool::Trend, 1, "趨勢線"),
            (Tool::HLine, 2, "水平線"),
            (Tool::Fib, 3, "斐波那契回檔"),
        ];
        ui.vertical_centered(|ui| {
            for (t, icon, tip) in tools {
                if tool_btn(ui, &self.pal, icon, self.tool == t, tip).clicked() {
                    self.tool = t;
                    self.ts.pending = None;
                }
            }
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(24.0, 1.0), Sense::hover());
            ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, self.pal.hair));
            ui.add_space(8.0);
            let sym = self.st.selected.clone();
            let n = self.st.drawings.get(&sym).map(|v| v.len()).unwrap_or(0);
            if tool_btn(ui, &self.pal, 4, false, "復原上一條線").clicked() && n > 0 {
                self.st.drawings.get_mut(&sym).map(|v| v.pop());
                self.ts.selected = None;
                self.dirty = true;
            }
            if tool_btn(ui, &self.pal, 5, false, "清除此檔所有繪圖").clicked() && n > 0 {
                self.st.drawings.remove(&sym);
                self.ts.selected = None;
                self.dirty = true;
            }
        });
    }

    fn central(&mut self, ui: &mut Ui) {
        match self.st.mode {
            Mode::Compare => {
                self.legend_strip(ui);
                let rect = ui.available_rect_before_wrap();
                let lines = self.lines();
                let prm = compare::Params { pal: &self.pal, lines: &lines, total_return: self.st.total_return, log: self.st.log_scale };
                let mut v = self.cmp_view;
                compare::show(ui, rect, &mut v, &prm);
                self.cmp_view = v;
            }
            Mode::Chart => {
                let sym = self.st.selected.clone();
                if let Some(bars) = self.chart_bars() {
                    let rect = ui.available_rect_before_wrap();
                    let label = {
                        let n = self.display_name(&sym);
                        if n.is_empty() { sym.clone() } else { format!("{sym}  {n}") }
                    };
                    let iv = match self.st.interval {
                        Interval::D => "日",
                        Interval::W => "週",
                        Interval::M => "月",
                    };
                    let prm = candle::Params {
                        pal: &self.pal,
                        bars: &bars,
                        label: &label,
                        interval: iv,
                        ind: &self.st.indicators,
                        log: self.st.log_scale,
                        tool: self.tool,
                    };
                    let mut v = self.cdl_view;
                    let mut dr = self.st.drawings.remove(&sym).unwrap_or_default();
                    let before = dr.clone();
                    let done = candle::show(ui, rect, &mut v, &prm, &mut dr, &mut self.ts);
                    if dr != before {
                        self.dirty = true;
                    }
                    if !dr.is_empty() {
                        self.st.drawings.insert(sym, dr);
                    }
                    self.cdl_view = v;
                    if done {
                        self.tool = Tool::Cursor;
                    }
                } else {
                    let rect = ui.available_rect_before_wrap();
                    let msg = if self.loading.contains(&sym) {
                        format!("載入 {sym} …")
                    } else if let Some(e) = self.errors.get(&sym) {
                        format!("{sym}:{e}")
                    } else {
                        "請從右側自選清單選擇一檔股票".to_string()
                    };
                    ui.painter().rect_filled(rect, 0.0, self.pal.ground);
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(14.0), self.pal.dim);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        self.poll(ctx);
        self.debug_shot(ctx);
        if !self.loading.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) && self.st.mode == Mode::Chart {
            let sym = self.st.selected.clone();
            if let Some(v) = self.st.drawings.get_mut(&sym) {
                v.pop();
                self.dirty = true;
            }
        }
        let pal = self.pal.clone();
        let bar = |fill: Color32| egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, pal.hair));
        egui::Panel::top("top").frame(bar(pal.panel)).show(root, |ui| self.top_bar(ui));
        egui::Panel::right("watch")
            .frame(bar(pal.panel))
            .resizable(false)
            .exact_size(264.0)
            .show(root, |ui| self.watchlist(ui));
        if self.st.mode == Mode::Compare {
            egui::Panel::bottom("stats")
                .frame(bar(pal.panel))
                .resizable(true)
                .default_size(210.0)
                .size_range(120.0..=420.0)
                .show(root, |ui| self.stats_panel(ui));
        } else {
            egui::Panel::left("tools")
                .frame(bar(pal.panel))
                .resizable(false)
                .exact_size(48.0)
                .show(root, |ui| self.tools_panel(ui));
        }
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(pal.ground))
            .show(root, |ui| self.central(ui));

        if self.dirty && self.last_save.elapsed() > Duration::from_millis(800) {
            self.st.save();
            self.dirty = false;
            self.last_save = Instant::now();
        } else if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(850));
        }
    }

    fn on_exit(&mut self) {
        self.st.save();
    }
}

// ------------------------------------------------------------------ widgets

fn sep(ui: &mut Ui, pal: &Palette) {
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, pal.hair));
    ui.add_space(4.0);
}

fn chip_btn(ui: &mut Ui, pal: &Palette, text: &str, active: bool) -> Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(13.0), pal.ink);
    let size = vec2((g.size().x + 14.0).max(30.0), 26.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let (fill, fg, stroke) = if active {
        (pal.ink, pal.on_ink, Stroke::NONE)
    } else if resp.hovered() {
        (pal.grid_major, pal.ink, Stroke::new(1.0, pal.ink))
    } else {
        (Color32::TRANSPARENT, pal.ink, Stroke::new(1.0, pal.hair))
    };
    ui.painter().rect_filled(r, 2.0, fill);
    if stroke != Stroke::NONE {
        ui.painter().rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
    }
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn tool_btn(ui: &mut Ui, pal: &Palette, icon: u8, active: bool, tip: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let (fill, fg) = if active {
        (pal.ink, pal.on_ink)
    } else if resp.hovered() {
        (pal.grid_major, pal.ink)
    } else {
        (Color32::TRANSPARENT, pal.ink)
    };
    let p = ui.painter();
    p.rect_filled(r, 2.0, fill);
    let c = r.center();
    let s = Stroke::new(1.6, fg);
    match icon {
        0 => {
            // arrow cursor
            let pts = vec![c + vec2(-5.0, -8.0), c + vec2(-5.0, 6.0), c + vec2(-1.5, 2.5), c + vec2(2.0, 9.0), c + vec2(4.5, 7.5), c + vec2(1.5, 1.5), c + vec2(6.5, 1.0)];
            p.add(egui::Shape::closed_line(pts, s));
        }
        1 => {
            p.line_segment([c + vec2(-8.0, 7.0), c + vec2(8.0, -7.0)], s);
            for q in [c + vec2(-8.0, 7.0), c + vec2(8.0, -7.0)] {
                p.circle_filled(q, 2.6, fg);
            }
        }
        2 => {
            p.line_segment([c + vec2(-10.0, 0.0), c + vec2(10.0, 0.0)], s);
            p.circle_filled(c, 2.6, fg);
        }
        3 => {
            for k in 0..4 {
                let y = -8.0 + k as f32 * 5.3;
                p.line_segment([c + vec2(-9.0, y), c + vec2(9.0, y)], Stroke::new(if k == 0 || k == 3 { 1.8 } else { 1.0 }, fg));
            }
        }
        4 => {
            // undo arrow
            p.line_segment([c + vec2(-7.0, -2.0), c + vec2(4.0, -2.0)], s);
            p.line_segment([c + vec2(4.0, -2.0), c + vec2(7.0, 1.0)], s);
            p.line_segment([c + vec2(7.0, 1.0), c + vec2(7.0, 5.0)], s);
            p.line_segment([c + vec2(-7.0, -2.0), c + vec2(-3.0, -6.0)], s);
            p.line_segment([c + vec2(-7.0, -2.0), c + vec2(-3.0, 2.0)], s);
        }
        _ => {
            // trash can
            p.line_segment([c + vec2(-7.0, -5.0), c + vec2(7.0, -5.0)], s);
            p.line_segment([c + vec2(-2.5, -5.0), c + vec2(-2.5, -8.0)], s);
            p.line_segment([c + vec2(2.5, -5.0), c + vec2(2.5, -8.0)], s);
            p.line_segment([c + vec2(-2.5, -8.0), c + vec2(2.5, -8.0)], s);
            p.add(egui::Shape::closed_line(vec![c + vec2(-5.0, -5.0), c + vec2(-4.0, 8.0), c + vec2(4.0, 8.0), c + vec2(5.0, -5.0)], s));
        }
    }
    resp.on_hover_text(tip)
}
