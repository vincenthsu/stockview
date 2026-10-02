//! Single-symbol candlestick workspace: volume, overlays, RSI/MACD panes, drawing tools.

use crate::axis::*;
use crate::calc;
use crate::compare::chip;
use crate::data::Bar;
use crate::store::{Drawing, Indicators};
use crate::theme::Palette;
use egui::{epaint::Shape, pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, Ui};

pub const GUTTER: f32 = 84.0;
const AXIS_H: f32 = 26.0;
const SUB_H: f32 = 112.0;
pub const FIB: [f64; 7] = [0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Cursor,
    Trend,
    HLine,
    Fib,
}

#[derive(Default)]
pub struct ToolState {
    pub pending: Option<(i64, f64)>,
    pub selected: Option<usize>,
}

pub struct Params<'a> {
    pub pal: &'a Palette,
    pub bars: &'a [Bar],
    pub label: &'a str,
    pub interval: &'a str,
    pub ind: &'a Indicators,
    pub log: bool,
    pub tool: Tool,
}

pub fn idx_of_t(bars: &[Bar], t: i64) -> f64 {
    let n = bars.len();
    if n < 2 {
        return 0.0;
    }
    let k = bars.partition_point(|b| b.t < t);
    if k == 0 {
        let d = (bars[1].t - bars[0].t).max(1) as f64;
        return (t - bars[0].t) as f64 / d;
    }
    if k >= n {
        let d = (bars[n - 1].t - bars[n - 2].t).max(1) as f64;
        return (n - 1) as f64 + (t - bars[n - 1].t) as f64 / d;
    }
    let (a, b) = (&bars[k - 1], &bars[k]);
    (k - 1) as f64 + (t - a.t) as f64 / (b.t - a.t).max(1) as f64
}

pub fn t_of_idx(bars: &[Bar], i: f64) -> i64 {
    let n = bars.len();
    if n < 2 {
        return bars.first().map(|b| b.t).unwrap_or(0);
    }
    if i <= 0.0 {
        let d = (bars[1].t - bars[0].t) as f64;
        return bars[0].t + (i * d) as i64;
    }
    if i >= (n - 1) as f64 {
        let d = (bars[n - 1].t - bars[n - 2].t) as f64;
        return bars[n - 1].t + ((i - (n - 1) as f64) * d) as i64;
    }
    let k = i.floor() as usize;
    let f = i - k as f64;
    bars[k].t + ((bars[k + 1].t - bars[k].t) as f64 * f) as i64
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return (p - a).length();
    }
    let t = (((p - a).x * ab.x + (p - a).y * ab.y) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn line_pts(vals: &[f64], i0: usize, i1: usize, x_of: &impl Fn(f64) -> f32, y_of: &impl Fn(f64) -> f32) -> Vec<Pos2> {
    let mut v = Vec::new();
    for i in i0..i1.min(vals.len()) {
        if vals[i].is_finite() {
            v.push(pos2(x_of(i as f64), y_of(vals[i])));
        }
    }
    v
}

pub fn show(
    ui: &mut Ui,
    rect: Rect,
    view: &mut (f64, f64),
    prm: &Params,
    drawings: &mut Vec<Drawing>,
    ts: &mut ToolState,
) -> bool {
    let pal = prm.pal;
    let bars = prm.bars;
    let n = bars.len();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.ground);
    if n < 2 {
        painter.text(rect.center(), Align2::CENTER_CENTER, "沒有可顯示的資料", FontId::proportional(14.0), pal.dim);
        return false;
    }
    let nsub = prm.ind.rsi as usize + prm.ind.macd as usize;
    let plot_l = rect.left() + 4.0;
    let plot_r = rect.right() - GUTTER;
    let bottom = rect.bottom() - AXIS_H;
    let main = Rect::from_min_max(pos2(plot_l, rect.top() + 10.0), pos2(plot_r, bottom - nsub as f32 * SUB_H));
    if main.height() < 80.0 || main.width() < 80.0 {
        return false;
    }
    let mut subs: Vec<(&str, Rect)> = Vec::new();
    let mut y = main.bottom();
    if prm.ind.rsi {
        subs.push(("rsi", Rect::from_min_max(pos2(plot_l, y + 6.0), pos2(plot_r, y + SUB_H))));
        y += SUB_H;
    }
    if prm.ind.macd {
        subs.push(("macd", Rect::from_min_max(pos2(plot_l, y + 6.0), pos2(plot_r, y + SUB_H))));
    }
    let all = Rect::from_min_max(main.min, pos2(plot_r, bottom));

    let resp = ui.interact(all, Id::new("candle_plot"), Sense::click_and_drag());

    // --- pan / zoom (index space) ---
    let (mut il, mut ir) = *view;
    let mut span = (ir - il).max(2.0);
    if resp.dragged() {
        let d = -resp.drag_delta().x as f64 / main.width() as f64 * span;
        il += d;
        ir += d;
    }
    if resp.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        let f = (zoom as f64 * (1.0 + scroll.y as f64 * 0.0025)).clamp(0.2, 5.0);
        if (f - 1.0).abs() > 1e-6 {
            let px = resp.hover_pos().map(|p| p.x).unwrap_or(main.center().x);
            let frac = ((px - main.left()) / main.width()) as f64;
            let pivot = il + frac * span;
            let ns = (span / f).clamp(4.0, n as f64 * 1.3);
            il = pivot - frac * ns;
            ir = il + ns;
        }
        if scroll.x.abs() > 0.0 {
            let d = -scroll.x as f64 / main.width() as f64 * span;
            il += d;
            ir += d;
        }
    }
    span = ir - il;
    let max_r = n as f64 + span * 0.2;
    if ir > max_r {
        ir = max_r;
        il = ir - span;
    }
    let min_l = -span * 0.1;
    if il < min_l {
        il = min_l;
        ir = il + span;
    }
    *view = (il, ir);
    if resp.double_clicked() && prm.tool == Tool::Cursor {
        *view = ((n as f64 - 140.0).max(0.0), n as f64 + 10.0);
    }
    let (il, ir) = *view;
    let span = ir - il;
    let ppb = main.width() as f64 / span;
    let x_of = |i: f64| main.left() + ((i + 0.5 - il) * ppb) as f32;
    let i0 = (il.floor().max(0.0)) as usize;
    let i1 = ((ir.ceil() + 1.0).max(0.0) as usize).min(n);
    if i0 >= i1 {
        return false;
    }

    // --- indicators ---
    let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
    let ma = if prm.ind.ma {
        vec![(5usize, calc::sma(&closes, 5)), (20, calc::sma(&closes, 20)), (60, calc::sma(&closes, 60))]
    } else {
        vec![]
    };
    let ema50 = if prm.ind.ema { Some(calc::ema(&closes, 50)) } else { None };
    let bb = if prm.ind.bollinger { Some(calc::bollinger(&closes, 20, 2.0)) } else { None };

    // --- main y range ---
    let tf = |p: f64| if prm.log { p.max(1e-9).ln() } else { p };
    let inv = |f: f64| if prm.log { f.exp() } else { f };
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    let mut vmax = 0.0f64;
    for b in &bars[i0..i1] {
        lo = lo.min(b.l);
        hi = hi.max(b.h);
        vmax = vmax.max(b.v);
    }
    if let Some((_, u, l)) = &bb {
        for i in i0..i1 {
            if u[i].is_finite() {
                hi = hi.max(u[i]);
                lo = lo.min(l[i]);
            }
        }
    }
    let (mut flo, mut fhi) = (tf(lo), tf(hi));
    if fhi - flo < 1e-9 {
        fhi = flo + 1.0;
    }
    let rng = fhi - flo;
    flo -= rng * (0.06 + if prm.ind.volume { 0.16 } else { 0.0 });
    fhi += rng * 0.14;
    let y_of = |price: f64| main.bottom() - ((tf(price) - flo) / (fhi - flo)) as f32 * main.height();
    let price_at = |y: f32| inv(flo + ((main.bottom() - y) / main.height()) as f64 * (fhi - flo));

    let p = painter.with_clip_rect(rect);
    let pm = painter.with_clip_rect(main);
    let fp = |s: f32| FontId::proportional(s);
    let mono = FontId::monospace(11.5);

    // --- grid ---
    let t_l = t_of_idx(bars, il);
    let t_r = t_of_idx(bars, ir);
    for tk in time_ticks(t_l, t_r, main.width()) {
        let i = idx_of_t(bars, tk.t);
        let x = x_of(i).round() + 0.5;
        if x < main.left() || x > main.right() {
            continue;
        }
        p.line_segment([pos2(x, main.top()), pos2(x, bottom)], Stroke::new(1.0, if tk.major { pal.grid_major } else { pal.grid }));
        p.text(pos2(x, bottom + 6.0), Align2::CENTER_TOP, &tk.label, fp(11.5), if tk.major { pal.ink } else { pal.dim });
    }
    let yt: Vec<f64> = if prm.log {
        log_ticks(inv(flo).max(1e-6), inv(fhi), 8)
    } else {
        nice_ticks(flo, fhi, 8)
    };
    let mut price_labels: Vec<(f32, String)> = Vec::new();
    for v in yt {
        let y = y_of(v).round() + 0.5;
        if y < main.top() || y > main.bottom() {
            continue;
        }
        p.line_segment([pos2(main.left(), y), pos2(main.right(), y)], Stroke::new(1.0, pal.grid));
        price_labels.push((y, fmt_price(v)));
    }
    for (_, r) in std::iter::once(("", main)).chain(subs.iter().map(|(k, r)| (*k, *r))) {
        p.rect_stroke(r, 0.0, Stroke::new(1.0, pal.hair), egui::StrokeKind::Inside);
    }
    for c in [main.left_top(), main.right_top()] {
        let s = Stroke::new(1.0, pal.dim);
        p.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], s);
        p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], s);
    }

    // --- volume ---
    if prm.ind.volume && vmax > 0.0 {
        let vh = main.height() * 0.15;
        for col in columns(bars, i0, i1, ppb, &x_of) {
            let h = (col.v / vmax) as f32 * vh;
            let c = if col.c >= col.o { pal.up } else { pal.down }.gamma_multiply(0.38);
            let w = col.w.max(1.0);
            pm.rect_filled(Rect::from_min_max(pos2(col.x - w / 2.0, main.bottom() - h), pos2(col.x + w / 2.0, main.bottom())), 0.0, c);
        }
    }

    // --- Bollinger fill lines ---
    if let Some((mid, up, lw)) = &bb {
        let c = pal.series[5];
        pm.add(Shape::line(line_pts(up, i0, i1, &x_of, &y_of), Stroke::new(1.0, c)));
        pm.add(Shape::line(line_pts(lw, i0, i1, &x_of, &y_of), Stroke::new(1.0, c)));
        pm.add(Shape::line(line_pts(mid, i0, i1, &x_of, &y_of), Stroke::new(1.0, c.gamma_multiply(0.6))));
    }

    // --- candles ---
    for col in columns(bars, i0, i1, ppb, &x_of) {
        let c = if col.c >= col.o { pal.up } else { pal.down };
        pm.line_segment([pos2(col.x.round() + 0.5, y_of(col.h)), pos2(col.x.round() + 0.5, y_of(col.l))], Stroke::new(1.0, c));
        let (yo, yc) = (y_of(col.o), y_of(col.c));
        let (top, bot) = (yo.min(yc), yo.max(yc));
        let w = col.w.max(1.0);
        let body = Rect::from_min_max(pos2(col.x - w / 2.0, top), pos2(col.x + w / 2.0, bot.max(top + 1.0)));
        pm.rect_filled(body, 0.0, c);
    }

    // --- overlays ---
    let ma_cols = [pal.series[1], pal.series[0], pal.series[2]];
    for (k, (_, v)) in ma.iter().enumerate() {
        pm.add(Shape::line(line_pts(v, i0, i1, &x_of, &y_of), Stroke::new(1.3, ma_cols[k])));
    }
    if let Some(e) = &ema50 {
        pm.add(Shape::line(line_pts(e, i0, i1, &x_of, &y_of), Stroke::new(1.3, pal.series[3])));
    }

    // --- last price line ---
    let last = &bars[n - 1];
    let prev_c = bars[n - 2].c;
    let last_col = if last.c >= prev_c { pal.up } else { pal.down };
    let ly = y_of(last.c);
    if ly > main.top() && ly < main.bottom() {
        p.extend(Shape::dashed_line(&[pos2(main.left(), ly), pos2(main.right(), ly)], Stroke::new(1.0, last_col), 4.0, 3.0));
    }

    // --- sub panes ---
    let mut sub_info: Vec<(&str, Rect, Box<dyn Fn(f32) -> f64>)> = Vec::new();
    for (kind, r) in &subs {
        match *kind {
            "rsi" => {
                let v = calc::rsi(&closes, 14);
                let yr = |val: f64| r.bottom() - ((val / 100.0) as f32) * r.height();
                for lvl in [30.0, 50.0, 70.0] {
                    let yy = yr(lvl);
                    p.extend(Shape::dashed_line(&[pos2(r.left(), yy), pos2(r.right(), yy)], Stroke::new(1.0, pal.grid_major), 3.0, 3.0));
                    p.text(pos2(r.right() + 8.0, yy), Align2::LEFT_CENTER, format!("{lvl:.0}"), mono.clone(), pal.dim);
                }
                p.with_clip_rect(*r).add(Shape::line(line_pts(&v, i0, i1, &x_of, &yr), Stroke::new(1.4, pal.series[5])));
                let cur = v.get(n - 1).copied().unwrap_or(f64::NAN);
                p.text(r.min + vec2(8.0, 6.0), Align2::LEFT_TOP, format!("RSI 14   {cur:.1}"), fp(11.5), pal.series[5]);
                let rr = *r;
                sub_info.push(("rsi", rr, Box::new(move |y| (rr.bottom() - y) as f64 / rr.height() as f64 * 100.0)));
            }
            _ => {
                let (m, s, h) = calc::macd(&closes, 12, 26, 9);
                let mut mx = 1e-9f64;
                for i in i0..i1 {
                    mx = mx.max(m[i].abs()).max(s[i].abs()).max(h[i].abs());
                }
                let yr = |val: f64| r.center().y - (val / mx) as f32 * (r.height() * 0.44);
                p.line_segment([pos2(r.left(), yr(0.0)), pos2(r.right(), yr(0.0))], Stroke::new(1.0, pal.grid_major));
                let pr = p.with_clip_rect(*r);
                let bw = (ppb * 0.6).max(1.0) as f32;
                for i in i0..i1 {
                    let x = x_of(i as f64);
                    let c = if h[i] >= 0.0 { pal.up } else { pal.down }.gamma_multiply(0.6);
                    pr.rect_filled(Rect::from_x_y_ranges(x - bw / 2.0..=x + bw / 2.0, yr(h[i]).min(yr(0.0))..=yr(h[i]).max(yr(0.0))), 0.0, c);
                }
                pr.add(Shape::line(line_pts(&m, i0, i1, &x_of, &yr), Stroke::new(1.3, pal.series[0])));
                pr.add(Shape::line(line_pts(&s, i0, i1, &x_of, &yr), Stroke::new(1.3, pal.series[1])));
                p.text(
                    r.min + vec2(8.0, 6.0),
                    Align2::LEFT_TOP,
                    format!("MACD 12,26,9   {:.2}  {:.2}  {:.2}", m[n - 1], s[n - 1], h[n - 1]),
                    fp(11.5),
                    pal.series[0],
                );
                let rr = *r;
                sub_info.push(("macd", rr, Box::new(move |y| (rr.center().y - y) as f64 / (rr.height() * 0.44) as f64 * mx)));
            }
        }
    }

    // --- drawings ---
    let to_screen = |t: i64, pr: f64| pos2(x_of(idx_of_t(bars, t)), y_of(pr));
    let mut hit_cache: Vec<(usize, Vec<(Pos2, Pos2)>)> = Vec::new();
    for (di, d) in drawings.iter().enumerate() {
        let sel = ts.selected == Some(di);
        let w = if sel { 2.4 } else { 1.5 };
        let mut segs: Vec<(Pos2, Pos2)> = Vec::new();
        match *d {
            Drawing::Trend { t1, p1, t2, p2 } => {
                let (a, b) = (to_screen(t1, p1), to_screen(t2, p2));
                pm.line_segment([a, b], Stroke::new(w, pal.series[0]));
                if sel {
                    for q in [a, b] {
                        pm.rect_filled(Rect::from_center_size(q, vec2(8.0, 8.0)), 1.0, pal.series[0]);
                    }
                }
                segs.push((a, b));
            }
            Drawing::HLine { p: pv } => {
                let yy = y_of(pv);
                pm.line_segment([pos2(main.left(), yy), pos2(main.right(), yy)], Stroke::new(w, pal.series[1]));
                let r = Rect::from_center_size(pos2(main.right() + GUTTER / 2.0 - 4.0, yy), vec2(GUTTER - 10.0, 18.0));
                chip(&p, r, pal.series[1], &fmt_price(pv), pal.on_series(pal.series[1]), mono.clone());
                segs.push((pos2(main.left(), yy), pos2(main.right(), yy)));
            }
            Drawing::Fib { t1, p1, t2, p2 } => {
                let xa = x_of(idx_of_t(bars, t1));
                let xb = x_of(idx_of_t(bars, t2));
                let xl = xa.min(xb);
                for (k, lvl) in FIB.iter().enumerate() {
                    let price = p2 - (p2 - p1) * lvl;
                    let yy = y_of(price);
                    let strong = k == 0 || k == FIB.len() - 1;
                    let c = pal.series[5].gamma_multiply(if strong { 1.0 } else { 0.8 });
                    pm.line_segment([pos2(xl, yy), pos2(main.right(), yy)], Stroke::new(if sel { w } else { 1.1 }, c));
                    pm.text(pos2(xl + 4.0, yy - 2.0), Align2::LEFT_BOTTOM, format!("{lvl:.3}  {}", fmt_price(price)), fp(10.5), pal.series[5]);
                    segs.push((pos2(xl, yy), pos2(main.right(), yy)));
                }
                let (a, b) = (to_screen(t1, p1), to_screen(t2, p2));
                pm.extend(Shape::dashed_line(&[a, b], Stroke::new(1.0, pal.series[5]), 3.0, 3.0));
                if sel {
                    for q in [a, b] {
                        pm.rect_filled(Rect::from_center_size(q, vec2(8.0, 8.0)), 1.0, pal.series[5]);
                    }
                }
            }
        }
        hit_cache.push((di, segs));
    }

    // --- price axis labels ---
    let mut occupied: Vec<Rect> = Vec::new();
    let lr = Rect::from_center_size(pos2(main.right() + GUTTER / 2.0 - 4.0, ly), vec2(GUTTER - 10.0, 18.0));
    if ly > main.top() && ly < main.bottom() {
        chip(&p, lr, last_col, &fmt_price(last.c), pal.on_series(last_col), mono.clone());
        occupied.push(lr);
    }
    for (yy, s) in &price_labels {
        let r = Rect::from_center_size(pos2(main.right() + 40.0, *yy), vec2(70.0, 14.0));
        if occupied.iter().any(|o| o.intersects(r)) {
            continue;
        }
        p.text(pos2(main.right() + 8.0, *yy), Align2::LEFT_CENTER, s, mono.clone(), pal.dim);
    }

    // --- hover / tools ---
    let hover = resp.hover_pos().filter(|h| all.contains(*h));
    let hover_idx = hover.map(|h| (((h.x - main.left()) as f64 / ppb + il - 0.5).round()).clamp(0.0, (n - 1) as f64) as usize);
    if let Some(h) = hover {
        let dash = Stroke::new(1.0, pal.dim);
        let x = hover_idx.map(|i| x_of(i as f64)).unwrap_or(h.x);
        p.extend(Shape::dashed_line(&[pos2(x, main.top()), pos2(x, bottom)], dash, 3.0, 3.0));
        p.extend(Shape::dashed_line(&[pos2(main.left(), h.y), pos2(main.right(), h.y)], dash, 3.0, 3.0));
        let yr = Rect::from_center_size(pos2(main.right() + GUTTER / 2.0 - 4.0, h.y), vec2(GUTTER - 10.0, 18.0));
        if main.contains(h) {
            chip(&p, yr, pal.ink, &fmt_price(price_at(h.y)), pal.on_ink, mono.clone());
        } else if let Some((_, _, f)) = sub_info.iter().find(|(_, r, _)| r.contains(h)) {
            chip(&p, yr, pal.ink, &format!("{:.2}", f(h.y)), pal.on_ink, mono.clone());
        }
        if let Some(i) = hover_idx {
            let xr = Rect::from_center_size(pos2(x, bottom + 14.0), vec2(82.0, 18.0));
            chip(&p, xr, pal.ink, &fmt_date(bars[i].t), pal.on_ink, mono.clone());
        }
    }

    // tool interaction
    let mouse_data = hover.filter(|h| main.contains(*h)).map(|h| {
        let i = ((h.x - main.left()) as f64 / ppb + il - 0.5).max(-1000.0);
        let mut price = price_at(h.y);
        // magnet to the nearest OHLC when close
        if let Some(bi) = hover_idx {
            let b = &bars[bi];
            if let Some(best) = [b.o, b.h, b.l, b.c].into_iter().min_by(|a, c| (y_of(*a) - h.y).abs().total_cmp(&(y_of(*c) - h.y).abs())) {
                if (y_of(best) - h.y).abs() < 9.0 {
                    price = best;
                }
            }
        }
        (t_of_idx(bars, i), price, h)
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ts.pending = None;
        ts.selected = None;
    }
    if (ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))) && !ui.ctx().egui_wants_keyboard_input() {
        if let Some(s) = ts.selected.take() {
            if s < drawings.len() {
                drawings.remove(s);
            }
        }
    }
    // live preview of the second point
    if let (Some((t1, p1)), Some((t2, p2, _))) = (ts.pending, mouse_data) {
        let (a, b) = (to_screen(t1, p1), to_screen(t2, p2));
        match prm.tool {
            Tool::Trend => {
                pm.line_segment([a, b], Stroke::new(1.5, pal.series[0]));
            }
            Tool::Fib => {
                for lvl in FIB {
                    let yy = y_of(p2 - (p2 - p1) * lvl);
                    pm.line_segment([pos2(a.x.min(b.x), yy), pos2(main.right(), yy)], Stroke::new(1.0, pal.series[5].gamma_multiply(0.6)));
                }
            }
            _ => {}
        }
    }
    let mut new_tool_done = false;
    if resp.clicked() {
        if let Some((t, pr, h)) = mouse_data {
            match prm.tool {
                Tool::Cursor => {
                    ts.selected = hit_cache
                        .iter()
                        .filter_map(|(di, segs)| {
                            let d = segs.iter().map(|(a, b)| seg_dist(h, *a, *b)).fold(f32::MAX, f32::min);
                            (d < 7.0).then_some((*di, d))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|x| x.0);
                }
                Tool::HLine => {
                    drawings.push(Drawing::HLine { p: pr });
                    ts.selected = Some(drawings.len() - 1);
                    new_tool_done = true;
                }
                Tool::Trend | Tool::Fib => match ts.pending.take() {
                    None => ts.pending = Some((t, pr)),
                    Some((t1, p1)) => {
                        drawings.push(if prm.tool == Tool::Trend {
                            Drawing::Trend { t1, p1, t2: t, p2: pr }
                        } else {
                            Drawing::Fib { t1, p1, t2: t, p2: pr }
                        });
                        ts.selected = Some(drawings.len() - 1);
                        new_tool_done = true;
                    }
                },
            }
        }
    }

    // --- legend ---
    let hb = &bars[hover_idx.unwrap_or(n - 1)];
    let prev = if let Some(i) = hover_idx { if i > 0 { bars[i - 1].c } else { hb.o } } else { prev_c };
    let chg = hb.c / prev - 1.0;
    let col = if hb.c >= prev { pal.up } else { pal.down };
    let head = format!("{}  ·  {}", prm.label, prm.interval);
    let g = p.layout_no_wrap(head, FontId::proportional(13.0), pal.ink);
    let gw = g.size().x;
    p.galley(main.min + vec2(10.0, 8.0), g, pal.ink);
    let ohlc = format!(
        "開 {}  高 {}  低 {}  收 {}  {}",
        fmt_price(hb.o), fmt_price(hb.h), fmt_price(hb.l), fmt_price(hb.c), fmt_pct(chg)
    );
    p.text(main.min + vec2(10.0 + gw + 14.0, 8.0), Align2::LEFT_TOP, ohlc, mono.clone(), col);
    let mut lx = main.left() + 10.0;
    let ly2 = main.top() + 28.0;
    let mut put = |txt: String, c: Color32| {
        let g = p.layout_no_wrap(txt, FontId::proportional(11.5), c);
        let w = g.size().x;
        p.galley(pos2(lx, ly2), g, c);
        lx += w + 14.0;
    };
    let at = hover_idx.unwrap_or(n - 1);
    for (k, (per, v)) in ma.iter().enumerate() {
        if v[at].is_finite() {
            put(format!("MA{per} {}", fmt_price(v[at])), ma_cols[k]);
        }
    }
    if let Some(e) = &ema50 {
        if e[at].is_finite() {
            put(format!("EMA50 {}", fmt_price(e[at])), pal.series[3]);
        }
    }
    if let Some((_, u, l)) = &bb {
        if u[at].is_finite() {
            put(format!("BB20 {} / {}", fmt_price(u[at]), fmt_price(l[at])), pal.series[5]);
        }
    }
    if prm.ind.volume {
        put(format!("量 {}", fmt_vol(bars[at].v)), pal.dim);
    }
    if let Some(pd) = ts.pending {
        let _ = pd;
        p.text(
            pos2(main.center().x, main.bottom() - 10.0),
            Align2::CENTER_BOTTOM,
            "點擊第二個位置完成 · Esc 取消",
            fp(12.0),
            pal.dim,
        );
    }
    new_tool_done
}

struct Col {
    x: f32,
    w: f32,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
}

/// One entry per bar when zoomed in, otherwise one aggregated entry per pixel column.
fn columns(bars: &[Bar], i0: usize, i1: usize, ppb: f64, x_of: &impl Fn(f64) -> f32) -> Vec<Col> {
    let mut out = Vec::new();
    if ppb >= 3.0 {
        let w = (ppb * 0.72).floor().max(1.0) as f32;
        for i in i0..i1 {
            let b = &bars[i];
            out.push(Col { x: x_of(i as f64), w, o: b.o, h: b.h, l: b.l, c: b.c, v: b.v });
        }
        return out;
    }
    let mut cur: Option<(f32, Col)> = None;
    for i in i0..i1 {
        let b = &bars[i];
        let x = x_of(i as f64);
        let cx = x.floor();
        match &mut cur {
            Some((k, c)) if *k == cx => {
                c.h = c.h.max(b.h);
                c.l = c.l.min(b.l);
                c.c = b.c;
                c.v += b.v;
            }
            _ => {
                if let Some((_, c)) = cur.take() {
                    out.push(c);
                }
                cur = Some((cx, Col { x: cx + 0.5, w: 1.0, o: b.o, h: b.h, l: b.l, c: b.c, v: b.v }));
            }
        }
    }
    if let Some((_, c)) = cur {
        out.push(c);
    }
    out
}
