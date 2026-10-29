//! Visual identity: fonts, colors and spacing, in a light and a dark variant.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static DARK: AtomicBool = AtomicBool::new(false);

/// Colors of the interface (graphs themselves always use paper colors).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub app_bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub sidebar: Color32,
    pub border: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub weak: Color32,
    pub disabled: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub canvas: Color32,
    pub stripe: Color32,
    pub header: Color32,
    pub cell_cursor: Color32,
    pub good: Color32,
    pub warn_bg: Color32,
    pub warn_text: Color32,
    pub error_bg: Color32,
    pub error_text: Color32,
    pub star: Color32,
}

const LIGHT: Palette = Palette {
    app_bg: Color32::from_rgb(0xf4, 0xf5, 0xf8),
    panel: Color32::from_rgb(0xff, 0xff, 0xff),
    panel_alt: Color32::from_rgb(0xf7, 0xf8, 0xfb),
    sidebar: Color32::from_rgb(0xee, 0xf0, 0xf5),
    border: Color32::from_rgb(0xdc, 0xe0, 0xe8),
    text: Color32::from_rgb(0x2a, 0x30, 0x3c),
    strong: Color32::from_rgb(0x10, 0x14, 0x1c),
    weak: Color32::from_rgb(0x6e, 0x77, 0x86),
    disabled: Color32::from_rgb(0xb4, 0xba, 0xc4),
    hover: Color32::from_rgb(0xe4, 0xe8, 0xf1),
    selected: Color32::from_rgb(0xdd, 0xe5, 0xff),
    accent: Color32::from_rgb(0x41, 0x5f, 0xe0),
    accent_text: Color32::WHITE,
    canvas: Color32::from_rgb(0xe3, 0xe6, 0xec),
    stripe: Color32::from_rgb(0xe9, 0xee, 0xf7),
    header: Color32::from_rgb(0xdf, 0xe5, 0xf0),
    cell_cursor: Color32::from_rgb(0xff, 0xf6, 0xd6),
    good: Color32::from_rgb(0x16, 0x83, 0x4a),
    warn_bg: Color32::from_rgb(0xff, 0xf4, 0xd6),
    warn_text: Color32::from_rgb(0x7a, 0x4b, 0x00),
    error_bg: Color32::from_rgb(0xfd, 0xe8, 0xe8),
    error_text: Color32::from_rgb(0xa0, 0x20, 0x20),
    star: Color32::from_rgb(0xc8, 0x2c, 0x5a),
};

const DARK_PAL: Palette = Palette {
    app_bg: Color32::from_rgb(0x14, 0x17, 0x1d),
    panel: Color32::from_rgb(0x1c, 0x20, 0x27),
    panel_alt: Color32::from_rgb(0x21, 0x26, 0x2e),
    sidebar: Color32::from_rgb(0x18, 0x1b, 0x21),
    border: Color32::from_rgb(0x30, 0x36, 0x41),
    text: Color32::from_rgb(0xd9, 0xde, 0xe7),
    strong: Color32::from_rgb(0xf4, 0xf6, 0xfa),
    weak: Color32::from_rgb(0x8e, 0x98, 0xa8),
    disabled: Color32::from_rgb(0x55, 0x5d, 0x6a),
    hover: Color32::from_rgb(0x2a, 0x30, 0x3a),
    selected: Color32::from_rgb(0x2a, 0x36, 0x63),
    accent: Color32::from_rgb(0x72, 0x8c, 0xff),
    accent_text: Color32::from_rgb(0x0e, 0x12, 0x20),
    canvas: Color32::from_rgb(0x0f, 0x11, 0x15),
    stripe: Color32::from_rgb(0x23, 0x29, 0x33),
    header: Color32::from_rgb(0x28, 0x2f, 0x3b),
    cell_cursor: Color32::from_rgb(0x3a, 0x34, 0x1c),
    good: Color32::from_rgb(0x4a, 0xd0, 0x82),
    warn_bg: Color32::from_rgb(0x3a, 0x2f, 0x12),
    warn_text: Color32::from_rgb(0xf2, 0xc6, 0x6d),
    error_bg: Color32::from_rgb(0x3d, 0x1c, 0x1f),
    error_text: Color32::from_rgb(0xff, 0x9a, 0x9a),
    star: Color32::from_rgb(0xff, 0x6f, 0x9a),
};

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

pub fn pal() -> Palette {
    if is_dark() {
        DARK_PAL
    } else {
        LIGHT
    }
}

pub fn accent() -> Color32 {
    pal().accent
}

/// Heavier weight of the interface font, for titles and labels.
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, bold_family())
}

fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut f, "Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add(&mut f, "Inter-Medium", include_bytes!("../../../assets/fonts/Inter-Medium.ttf"));
    add(&mut f, "Inter-SemiBold", include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));
    let fallbacks: Vec<String> = f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["Inter".to_owned()];
    prop.extend(fallbacks.iter().cloned());
    f.families.insert(FontFamily::Proportional, prop);
    let mut b = vec!["Inter-SemiBold".to_owned()];
    b.extend(fallbacks);
    f.families.insert(bold_family(), b);
    f
}

/// Installs fonts (once) and the colors and spacing of the current theme.
pub fn apply(ctx: &egui::Context, dark: bool, install_fonts: bool) {
    DARK.store(dark, Ordering::Relaxed);
    if install_fonts {
        ctx.set_fonts(fonts());
    }
    let p = pal();
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, bold(19.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.window_margin = egui::Margin::same(16);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = if dark { Color32::from_rgb(0x12, 0x15, 0x1a) } else { Color32::WHITE };
    v.faint_bg_color = p.stripe;
    v.window_stroke = egui::Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.menu_corner_radius = egui::CornerRadius::same(8);
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 40 }) };
    v.selection.bg_fill = p.selected;
    v.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
    v.hyperlink_color = p.accent;
    v.override_text_color = Some(p.text);
    let r = egui::CornerRadius::same(6);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.border);
    v.widgets.inactive.weak_bg_fill = p.panel_alt;
    v.widgets.inactive.bg_fill = p.panel_alt;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, p.border);
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, p.accent);
    v.widgets.active.weak_bg_fill = p.selected;
    v.widgets.active.bg_fill = p.selected;
    style.visuals = v;
    ctx.set_style(style);
}

/// A filled call-to-action button in the accent color.
pub fn primary_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    let p = pal();
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).font(bold(14.0)).color(p.accent_text)).fill(p.accent).min_size(egui::vec2(104.0, 32.0)),
    )
    .clicked()
}
