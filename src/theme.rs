//! "Chart paper" palette: mint graph-paper ground, ink, vermilion baseline.
//! Light = daylight office paper, Dark = night plate. Taiwan convention red-up by default.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};

#[derive(Clone)]
pub struct Palette {
    pub dark: bool,
    pub ground: Color32,
    pub panel: Color32,
    pub grid: Color32,
    pub grid_major: Color32,
    pub hair: Color32,
    pub ink: Color32,
    pub dim: Color32,
    pub baseline: Color32,
    pub up: Color32,
    pub down: Color32,
    pub series: [Color32; 8],
    pub on_ink: Color32,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Palette {
    pub fn new(dark: bool, red_up: bool) -> Self {
        let (mut up, mut down) = if dark {
            (c(0xFF, 0x5A, 0x47), c(0x3D, 0xD5, 0x98))
        } else {
            (c(0xC8, 0x32, 0x1F), c(0x0F, 0x7A, 0x55))
        };
        if !red_up {
            std::mem::swap(&mut up, &mut down);
        }
        if dark {
            Self {
                dark,
                ground: c(0x10, 0x17, 0x13),
                panel: c(0x15, 0x1D, 0x18),
                grid: c(0x1C, 0x27, 0x20),
                grid_major: c(0x2A, 0x38, 0x2F),
                hair: c(0x2F, 0x3E, 0x35),
                ink: c(0xE4, 0xED, 0xE2),
                dim: c(0x8F, 0xA0, 0x96),
                baseline: c(0xFF, 0x6B, 0x4F),
                up,
                down,
                series: [
                    c(0x6C, 0x9B, 0xFF),
                    c(0xFF, 0xA0, 0x33),
                    c(0xF0, 0x74, 0xC9),
                    c(0x35, 0xD0, 0xC0),
                    c(0xE0, 0xC0, 0x40),
                    c(0xA7, 0x8B, 0xFA),
                    c(0xFF, 0x7A, 0x90),
                    c(0xA5, 0xB4, 0xBB),
                ],
                on_ink: c(0x10, 0x17, 0x13),
            }
        } else {
            Self {
                dark,
                ground: c(0xEE, 0xF3, 0xEA),
                panel: c(0xE5, 0xEC, 0xE0),
                grid: c(0xDA, 0xE4, 0xD5),
                grid_major: c(0xC6, 0xD3, 0xC1),
                hair: c(0xB4, 0xC2, 0xAF),
                ink: c(0x1B, 0x24, 0x20),
                dim: c(0x56, 0x64, 0x5A),
                baseline: c(0xD6, 0x40, 0x2B),
                up,
                down,
                series: [
                    c(0x24, 0x59, 0xC4),
                    c(0xD9, 0x6F, 0x00),
                    c(0xB5, 0x33, 0x8A),
                    c(0x0B, 0x84, 0x79),
                    c(0x8A, 0x6F, 0x00),
                    c(0x6C, 0x4B, 0xC2),
                    c(0xD0, 0x2F, 0x4E),
                    c(0x4A, 0x5B, 0x63),
                ],
                on_ink: c(0xF7, 0xFA, 0xF4),
            }
        }
    }

    /// Text colour that stays legible on top of a series ink chip.
    pub fn on_series(&self, bg: Color32) -> Color32 {
        let l = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
        if l > 150.0 { c(0x12, 0x18, 0x14) } else { c(0xFA, 0xFC, 0xF8) }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.ground;
        v.window_fill = self.panel;
        v.extreme_bg_color = self.ground;
        v.faint_bg_color = self.panel;
        v.override_text_color = Some(self.ink);
        v.window_stroke = Stroke::new(1.0, self.hair);
        v.selection.bg_fill = self.ink;
        v.selection.stroke = Stroke::new(1.0, self.on_ink);
        v.hyperlink_color = self.series[0];
        let r = CornerRadius::same(2);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = r;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.hair);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.ink);
        v.widgets.inactive.bg_fill = self.panel;
        v.widgets.inactive.weak_bg_fill = self.panel;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, self.hair);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, self.ink);
        v.widgets.hovered.bg_fill = self.grid_major;
        v.widgets.hovered.weak_bg_fill = self.grid_major;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.ink);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, self.ink);
        v.widgets.active.bg_fill = self.ink;
        v.widgets.active.weak_bg_fill = self.ink;
        v.widgets.active.fg_stroke = Stroke::new(1.0, self.on_ink);
        v.widgets.active.bg_stroke = Stroke::new(1.0, self.ink);
        v.popup_shadow = egui::Shadow {
            offset: [0, 4],
            blur: 14,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 120 } else { 40 }),
        };
        v.window_shadow = v.popup_shadow;
        ctx.set_visuals(v);

        ctx.global_style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(6.0, 4.0);
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            s.spacing.interact_size.y = 24.0;
            s.spacing.scroll.bar_width = 6.0;
            s.visuals.text_cursor.stroke = Stroke::new(2.0, self.baseline);
            s.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::new(13.0, FontFamily::Proportional),
            );
            s.text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(13.0, FontFamily::Proportional),
            );
            s.text_styles.insert(
                egui::TextStyle::Small,
                egui::FontId::new(11.0, FontFamily::Proportional),
            );
            s.text_styles.insert(
                egui::TextStyle::Monospace,
                egui::FontId::new(12.5, FontFamily::Monospace),
            );
        });
    }
}

/// Add a system CJK font as fallback so Traditional Chinese names and labels render.
pub fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "C:/Windows/Fonts/msjh.ttc",
        "C:/Windows/Fonts/msjh.ttf",
        "C:/Windows/Fonts/msyh.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}
