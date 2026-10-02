//! Multi-symbol percent-return overlay on a shared 0% baseline.

use crate::axis::*;
use crate::calc::DAY;
use crate::data::{Bar, Series};
use crate::theme::Palette;
use egui::{epaint::Shape, pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, Ui};
use std::sync::Arc;

pub struct Line {
    pub symbol: String,
    pub color: Color32,
    pub series: Arc<Series>,
}

pub const GUTTER: f32 = 132.0;
const AXIS_H: f32 = 26.0;

fn px(b: &Bar, total: bool) -> f64 {
    if total { b.adj } else { b.c }
}

/// Baseline date: the view's left edge, but never earlier than the oldest series.
/// Series that listed later start at their own first bar (0% there) instead of truncating the rest.
pub fn base_time(lines: &[Line], tl: i64) -> i64 {
    let first = lines.iter().filter_map(|l| l.series.bars.first().map(|b| b.t)).min().unwrap_or(tl);
    tl.max(first)
}

/// Full-history time bounds across lines.
pub fn bounds(lines: &[Line]) -> Option<(i64, i64)> {
    let lo = lines.iter().filter_map(|l| l.series.bars.first().map(|b| b.t)).min()?;
    let hi = lines.iter().filter_map(|l| l.series.bars.last().map(|b| b.t)).max()?;
    Some((lo, hi))
}

pub fn chip(p: &egui::Painter, rect: Rect, fill: Color32, text: &str, color: Color32, font: FontId) {
    p.rect_filled(rect, 2.0, fill);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
}

fn decimate(pts: &[Pos2]) -> Vec<Pos2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len() / 2);
    let mut col = pts[0].x.floor();
    let (mut first, mut last) = (pts[0], pts[0]);
    let (mut lo, mut hi) = (pts[0], pts[0]);
    let mut lo_i = 0usize;
    let mut hi_i = 0usize;
    let mut cnt = 0usize;
    let flush = |out: &mut Vec<Pos2>, first: Pos2, last: Pos2, lo: Pos2, hi: Pos2, lo_i: usize, hi_i: usize| {
        out.push(first);
        let (a, b) = if lo_i <= hi_i { (lo, hi) } else { (hi, lo) };
        out.push(a);
        out.push(b);
        out.push(last);
    };
    for (i, p) in pts.iter().enumerate() {
        let c = p.x.floor();
        if c != col {
            flush(&mut out, first, last, lo, hi, lo_i, hi_i);
            col = c;
            first = *p;
            lo = *p;
            hi = *p;
            lo_i = i;
            hi_i = i;
            cnt = 0;
        }
        if p.y > lo.y {
            lo = *p;
            lo_i = i;
        }
        if p.y < hi.y {
            hi = *p;
            hi_i = i;
        }
        last = *p;
        cnt += 1;
    }
    let _ = cnt;
    flush(&mut out, first, last, lo, hi, lo_i, hi_i);
    out
}

pub struct Params<'a> {
    pub pal: &'a Palette,
    pub lines: &'a [Line],
    pub total_return: bool,
    pub log: bool,
}

/// Draws the chart in `rect`; mutates `view` = (t_left, t_right) in unix seconds on pan/zoom.
pub fn show(ui: &mut Ui, rect: Rect, view: &mut (f64, f64), prm: &Params) {
    let pal = prm.pal;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.ground);
    let pr = Rect::from_min_max(
        pos2(rect.left() + 4.0, rect.top() + 10.0),
        pos2(rect.right() - GUTTER, rect.bottom() - AXIS_H),
    );
    if pr.width() < 80.0 || pr.height() < 60.0 {
        return;
    }
    let resp = ui.interact(pr, Id::new("compare_plot"), Sense::click_and_drag());
    let Some((lo, hi)) = bounds(prm.lines) else {
        painter.text(
            pr.center(),
            Align2::CENTER_CENTER,
            "加入股票開始比較(上方搜尋列輸入代號,例如 2330、AAPL)",
            FontId::proportional(14.0),
            pal.dim,
        );
        return;
    };

    // --- interaction ---
    let span_lo = lo as f64 - 30.0 * DAY as f64;
    let span_hi = hi as f64 + 30.0 * DAY as f64;
    let (mut tl, mut tr) = *view;
    let mut span = (tr - tl).max(1.0);
    if resp.dragged() {
        let dt = -resp.drag_delta().x as f64 / pr.width() as f64 * span;
        tl += dt;
        tr += dt;
    }
    if resp.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        let mut f = zoom as f64 * (1.0 + (scroll.y as f64) * 0.0025);
        if !(0.2..5.0).contains(&f) {
            f = f.clamp(0.2, 5.0);
        }
        if (f - 1.0).abs() > 1e-6 {
            let pivot_x = resp.hover_pos().map(|p| p.x).unwrap_or(pr.center().x);
            let frac = ((pivot_x - pr.left()) / pr.width()) as f64;
            let pivot = tl + frac * span;
            let ns = (span / f).clamp(3.0 * DAY as f64, (span_hi - span_lo) * 1.05);
            tl = pivot - frac * ns;
            tr = tl + ns;
        }
        if scroll.x.abs() > 0.0 {
            let dt = -scroll.x as f64 / pr.width() as f64 * span;
            tl += dt;
            tr += dt;
        }
    }
    span = tr - tl;
    if tl < span_lo {
        tl = span_lo;
        tr = tl + span;
    }
    if tr > span_hi {
        tr = span_hi;
        tl = tr - span;
    }
    *view = (tl, tr);
    if resp.double_clicked() {
        *view = (lo as f64, hi as f64);
    }
    let (tl, tr) = *view;
    let span = tr - tl;
    let x_of = |t: f64| pr.left() + ((t - tl) / span) as f32 * pr.width();

    // --- data in view ---
    let t0 = base_time(prm.lines, tl as i64);
    struct Pre {
        idx: usize,
        base: f64,
        i0: usize,
        i1: usize,
    }
    let mut pre: Vec<Option<Pre>> = Vec::new();
    let tf = |r: f64| if prm.log { (1.0 + r).max(1e-6).ln() } else { r };
    let mut fmin = 0.0f64;
    let mut fmax = 0.0f64;
    for (idx, l) in prm.lines.iter().enumerate() {
        let bars = &l.series.bars;
        let s = bars.partition_point(|b| b.t < t0);
        let a = s.max(bars.partition_point(|b| (b.t as f64) < tl));
        let b = bars.partition_point(|b| (b.t as f64) <= tr);
        if s >= bars.len() || a >= b {
            pre.push(None);
            continue;
        }
        let base = px(&bars[s], prm.total_return);
        for bar in &bars[a..b] {
            let f = tf(px(bar, prm.total_return) / base - 1.0);
            fmin = fmin.min(f);
            fmax = fmax.max(f);
        }
        pre.push(Some(Pre { idx, base, i0: a, i1: b }));
    }
    if fmax - fmin < 1e-4 {
        fmax = fmin + 0.1;
    }
    let pad = (fmax - fmin) * 0.08;
    let (fmin, fmax) = (fmin - pad, fmax + pad);
    let y_of = |f: f64| pr.bottom() - ((f - fmin) / (fmax - fmin)) as f32 * pr.height();
    let inv = |f: f64| if prm.log { f.exp() - 1.0 } else { f };

    // --- grid & axes ---
    let fp = |s: f32| FontId::proportional(s);
    let mono = FontId::monospace(11.5);
    let p = painter.with_clip_rect(rect);
    for tk in time_ticks(tl as i64, tr as i64, pr.width()) {
        let x = x_of(tk.t as f64).round() + 0.5;
        p.line_segment(
            [pos2(x, pr.top()), pos2(x, pr.bottom())],
            Stroke::new(1.0, if tk.major { pal.grid_major } else { pal.grid }),
        );
        p.text(
            pos2(x, pr.bottom() + 6.0),
            Align2::CENTER_TOP,
            &tk.label,
            fp(11.5),
            if tk.major { pal.ink } else { pal.dim },
        );
    }
    let ticks: Vec<f64> = if prm.log {
        log_ticks((inv(fmin) + 1.0).max(1e-3), inv(fmax) + 1.0, 8).into_iter().map(|v| v - 1.0).collect()
    } else {
        nice_ticks(fmin, fmax, 8)
    };
    let mut tick_labels: Vec<(f32, String)> = Vec::new();
    for r in ticks {
        let y = y_of(tf(r)).round() + 0.5;
        if y < pr.top() || y > pr.bottom() {
            continue;
        }
        p.line_segment([pos2(pr.left(), y), pos2(pr.right(), y)], Stroke::new(1.0, pal.grid));
        tick_labels.push((y, if r.abs() < 1e-9 { "0%".to_string() } else { fmt_pct(r) }));
    }

    // plate frame + registration crosses
    p.rect_stroke(pr, 0.0, Stroke::new(1.0, pal.hair), egui::StrokeKind::Inside);
    for c in [pr.left_top(), pr.right_top(), pr.left_bottom(), pr.right_bottom()] {
        let s = Stroke::new(1.0, pal.dim);
        p.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], s);
        p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], s);
    }

    // baseline: the vermilion 0% rule
    let y0 = y_of(0.0);
    if y0 >= pr.top() && y0 <= pr.bottom() {
        p.line_segment([pos2(pr.left(), y0), pos2(pr.right(), y0)], Stroke::new(2.0, pal.baseline));
        let xb = x_of(t0 as f64);
        if xb >= pr.left() && xb <= pr.right() {
            p.line_segment(
                [pos2(xb, y0 - 7.0), pos2(xb, y0 + 7.0)],
                Stroke::new(2.0, pal.baseline),
            );
        }
        let txt = format!("基準日 {}", fmt_date(t0));
        let tx = (xb + 6.0).clamp(pr.left() + 4.0, pr.right() - 110.0);
        p.text(pos2(tx, y0 - 6.0), Align2::LEFT_BOTTOM, txt, fp(11.0), pal.baseline);
    }

    // --- hover ---
    let hover = resp.hover_pos().filter(|h| pr.contains(*h));
    let t_h = hover.map(|h| tl + ((h.x - pr.left()) / pr.width()) as f64 * span);
    let mut hot: Option<usize> = None;
    if let (Some(h), Some(th)) = (hover, t_h) {
        let mut best = 26.0f32;
        for pe in pre.iter().flatten() {
            let bars = &prm.lines[pe.idx].series.bars;
            let k = bars.partition_point(|b| (b.t as f64) <= th);
            if k == 0 || bars[k - 1].t < t0 {
                continue;
            }
            let y = y_of(tf(px(&bars[k - 1], prm.total_return) / pe.base - 1.0));
            if (y - h.y).abs() < best {
                best = (y - h.y).abs();
                hot = Some(pe.idx);
            }
        }
    }

    // --- series ---
    let mut ends: Vec<(usize, f32, f64)> = Vec::new(); // (line idx, y, r)
    for pe in pre.iter().flatten() {
        let l = &prm.lines[pe.idx];
        let bars = &l.series.bars;
        let pts: Vec<Pos2> = bars[pe.i0..pe.i1]
            .iter()
            .map(|b| pos2(x_of(b.t as f64), y_of(tf(px(b, prm.total_return) / pe.base - 1.0))))
            .collect();
        let pts = if pts.len() as f32 > pr.width() * 1.5 { decimate(&pts) } else { pts };
        let w = if hot == Some(pe.idx) { 2.8 } else if hot.is_some() { 1.3 } else { 1.8 };
        let col = if hot.is_some() && hot != Some(pe.idx) { l.color.gamma_multiply(0.55) } else { l.color };
        let pp = painter.with_clip_rect(pr);
        if bars[0].t > t0 + 7 * DAY && pe.i0 == 0 {
            let c = pos2(x_of(bars[pe.i0].t as f64), y0);
            pp.circle_stroke(c, 4.5, Stroke::new(1.6, l.color));
            pp.circle_filled(c, 2.0, pal.ground);
        }
        pp.add(Shape::line(pts, Stroke::new(w, col)));
        let last = &bars[pe.i1 - 1];
        let r = px(last, prm.total_return) / pe.base - 1.0;
        ends.push((pe.idx, y_of(tf(r)), r));
    }

    // --- finish board: ranked end labels in the gutter ---
    let chip_h = 19.0;
    let ys: Vec<f32> = ends.iter().map(|e| e.1).collect();
    let spread_y = spread(&ys, chip_h + 2.0, pr.top() + chip_h / 2.0, pr.bottom() - chip_h / 2.0);
    let mut chip_rects: Vec<Rect> = Vec::new();
    for (k, (idx, y, r)) in ends.iter().enumerate() {
        let l = &prm.lines[*idx];
        let cy = spread_y[k];
        let cr = Rect::from_min_size(pos2(pr.right() + 8.0, cy - chip_h / 2.0), vec2(GUTTER - 12.0, chip_h));
        chip_rects.push(cr);
        let lx = bars_last_x(&l.series, tr, &x_of);
        p.line_segment(
            [pos2(lx.min(pr.right()), *y), pos2(cr.left(), cy)],
            Stroke::new(1.0, l.color.gamma_multiply(0.6)),
        );
        p.circle_filled(pos2(lx.min(pr.right()), *y), 2.6, l.color);
        let fg = pal.on_series(l.color);
        p.rect_filled(cr, 2.0, l.color);
        let sym = l.symbol.trim_end_matches(".TW").trim_end_matches(".TWO");
        p.text(cr.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, sym, fp(11.5), fg);
        p.text(cr.right_center() - vec2(5.0, 0.0), Align2::RIGHT_CENTER, fmt_pct(*r), mono.clone(), fg);
    }
    for (y, s) in &tick_labels {
        let r = Rect::from_center_size(pos2(pr.right() + 40.0, *y), vec2(70.0, 14.0));
        if chip_rects.iter().any(|c| c.expand2(vec2(0.0, 2.0)).intersects(r)) {
            continue;
        }
        let is0 = s == "0%";
        p.text(
            pos2(pr.right() + 8.0, *y),
            Align2::LEFT_CENTER,
            s,
            mono.clone(),
            if is0 { pal.baseline } else { pal.dim },
        );
    }

    // --- crosshair + tooltip ---
    if let (Some(h), Some(th)) = (hover, t_h) {
        let dash = Stroke::new(1.0, pal.dim);
        p.extend(Shape::dashed_line(&[pos2(h.x, pr.top()), pos2(h.x, pr.bottom())], dash, 3.0, 3.0));
        p.extend(Shape::dashed_line(&[pos2(pr.left(), h.y), pos2(pr.right(), h.y)], dash, 3.0, 3.0));
        // y chip
        let fy = fmin + ((pr.bottom() - h.y) / pr.height()) as f64 * (fmax - fmin);
        let yr = Rect::from_center_size(pos2(pr.right() + 40.0, h.y), vec2(70.0, 18.0));
        chip(&p, yr, pal.ink, &fmt_pct(inv(fy)), pal.on_ink, mono.clone());
        // x chip
        let xr = Rect::from_center_size(pos2(h.x, pr.bottom() + 14.0), vec2(82.0, 18.0));
        chip(&p, xr, pal.ink, &fmt_date(th as i64), pal.on_ink, mono.clone());

        let mut rows: Vec<(Color32, String, f64)> = Vec::new();
        for pe in pre.iter().flatten() {
            let l = &prm.lines[pe.idx];
            let bars = &l.series.bars;
            let k = bars.partition_point(|b| (b.t as f64) <= th);
            if k == 0 || bars[k - 1].t < t0 {
                continue;
            }
            rows.push((l.color, l.symbol.clone(), px(&bars[k - 1], prm.total_return) / pe.base - 1.0));
        }
        rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        if !rows.is_empty() {
            let row_h = 18.0;
            let w = 170.0;
            let hgt = 24.0 + rows.len() as f32 * row_h + 6.0;
            let mut o = pos2(h.x + 16.0, h.y + 16.0);
            if o.x + w > pr.right() {
                o.x = h.x - 16.0 - w;
            }
            if o.y + hgt > pr.bottom() {
                o.y = (pr.bottom() - hgt).max(pr.top());
            }
            let tb = Rect::from_min_size(o, vec2(w, hgt));
            p.rect_filled(tb.translate(vec2(0.0, 3.0)), 3.0, Color32::from_black_alpha(if pal.dark { 110 } else { 30 }));
            p.rect_filled(tb, 3.0, pal.panel);
            p.rect_stroke(tb, 3.0, Stroke::new(1.0, pal.hair), egui::StrokeKind::Inside);
            p.text(tb.min + vec2(10.0, 6.0), Align2::LEFT_TOP, fmt_date(th as i64), fp(11.5), pal.dim);
            for (i, (c, s, r)) in rows.iter().enumerate() {
                let y = tb.top() + 24.0 + i as f32 * row_h + row_h / 2.0;
                p.rect_filled(Rect::from_center_size(pos2(tb.left() + 14.0, y), vec2(8.0, 8.0)), 1.0, *c);
                p.text(pos2(tb.left() + 24.0, y), Align2::LEFT_CENTER, s, fp(12.0), pal.ink);
                p.text(pos2(tb.right() - 10.0, y), Align2::RIGHT_CENTER, fmt_pct(*r), mono.clone(), pal.ink);
            }
        }
    }
}

fn bars_last_x(s: &Series, tr: f64, x_of: &impl Fn(f64) -> f32) -> f32 {
    let bars = &s.bars;
    let k = bars.partition_point(|b| (b.t as f64) <= tr);
    x_of(bars[k.saturating_sub(1)].t as f64)
}
