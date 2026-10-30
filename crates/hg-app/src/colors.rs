//! Color picker for graph series: disc, classic, harmony, values and palettes.

use crate::app::App;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use hg_core::{GraphKind, Project};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Disc,
    Classic,
    Harmony,
    Values,
    Palettes,
}

impl Mode {
    const ALL: [Mode; 5] = [Mode::Disc, Mode::Classic, Mode::Harmony, Mode::Values, Mode::Palettes];
    fn label(self) -> &'static str {
        match self {
            Mode::Disc => "Disc",
            Mode::Classic => "Classic",
            Mode::Harmony => "Harmony",
            Mode::Values => "Values",
            Mode::Palettes => "Palettes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Harmony {
    #[default]
    Complementary,
    Analogous,
    Triadic,
    SplitComplementary,
    Tetradic,
}

impl Harmony {
    const ALL: [Harmony; 5] =
        [Harmony::Complementary, Harmony::Analogous, Harmony::Triadic, Harmony::SplitComplementary, Harmony::Tetradic];
    fn label(self) -> &'static str {
        match self {
            Harmony::Complementary => "Complementary",
            Harmony::Analogous => "Analogous",
            Harmony::Triadic => "Triadic",
            Harmony::SplitComplementary => "Split complementary",
            Harmony::Tetradic => "Tetradic",
        }
    }
    /// Hue offsets (in turns) of the scheme, starting with the base color.
    fn offsets(self) -> &'static [f32] {
        match self {
            Harmony::Complementary => &[0.0, 0.5],
            Harmony::Analogous => &[0.0, -1.0 / 12.0, 1.0 / 12.0, -2.0 / 12.0, 2.0 / 12.0],
            Harmony::Triadic => &[0.0, 1.0 / 3.0, 2.0 / 3.0],
            Harmony::SplitComplementary => &[0.0, 5.0 / 12.0, 7.0 / 12.0],
            Harmony::Tetradic => &[0.0, 0.25, 0.5, 0.75],
        }
    }
}

pub struct ColorWindow {
    graph_id: u64,
    series: usize,
    mode: Mode,
    harmony: Harmony,
    /// Hue, saturation, value in 0..1.
    hsv: [f32; 3],
    hex: String,
    loaded: Option<usize>,
    palette_name: String,
}

impl ColorWindow {
    pub fn new(graph_id: u64) -> Self {
        ColorWindow {
            graph_id,
            series: 0,
            mode: Mode::Disc,
            harmony: Harmony::Complementary,
            hsv: [0.0, 0.0, 0.0],
            hex: String::new(),
            loaded: None,
            palette_name: String::new(),
        }
    }

    /// Starts on the given series.
    pub fn select(&mut self, series: usize) {
        self.series = series;
        self.loaded = None;
    }
}

pub fn rgb_to_hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [((r + m) * 255.0).round() as u8, ((g + m) * 255.0).round() as u8, ((b + m) * 255.0).round() as u8]
}

fn c32([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn hex_of([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Names of the colored series of a graph, in drawing order.
pub fn series_names(project: &Project, graph_id: u64) -> Vec<String> {
    let Some(g) = project.graph(graph_id) else { return Vec::new() };
    let Some(t) = project.table(g.table_id) else { return Vec::new() };
    let rows: Vec<usize> = (0..t.used_rows()).filter(|&r| (0..t.datasets.len()).any(|d| !t.cell_values(d, r).is_empty())).collect();
    match g.kind {
        GraphKind::Pie | GraphKind::Donut => rows.iter().map(|&r| t.row_label(r)).collect(),
        GraphKind::GroupedBar | GraphKind::StackedBar | GraphKind::GroupedScatter if g.transpose => {
            rows.iter().map(|&r| t.row_label(r)).collect()
        }
        GraphKind::GroupedBar | GraphKind::StackedBar | GraphKind::GroupedScatter => (0..t.datasets.len())
            .filter(|&d| rows.iter().any(|&r| !t.cell_values(d, r).is_empty()))
            .map(|d| t.datasets[d].title.clone())
            .collect(),
        _ => t.nonempty_datasets().into_iter().map(|d| t.datasets[d].title.clone()).collect(),
    }
}

/// The color currently used for a series (custom or from the palette).
pub fn series_color(project: &Project, graph_id: u64, i: usize) -> [u8; 3] {
    let g = project.graph(graph_id).unwrap();
    g.colors.get(i).copied().unwrap_or_else(|| {
        let c = hg_plot::palette_color(g.palette, i);
        [c.r, c.g, c.b]
    })
}

fn set_series_color(project: &mut Project, graph_id: u64, i: usize, rgb: [u8; 3]) {
    let current: Vec<[u8; 3]> = (0..=i).map(|k| series_color(project, graph_id, k)).collect();
    if let Some(g) = project.graphs.iter_mut().find(|g| g.id == graph_id) {
        if g.colors.len() <= i {
            g.colors = current;
        }
        g.colors[i] = rgb;
    }
}

/// Hue/saturation wheel; dragging changes h and s.
fn disc(ui: &mut egui::Ui, hsv: &mut [f32; 3], markers: &[f32]) -> bool {
    let size = 220.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let c = rect.center();
    let r = size / 2.0 - 4.0;
    let mut mesh = egui::Mesh::default();
    let (rings, segs) = (12, 72);
    for ri in 0..=rings {
        for si in 0..=segs {
            let s = ri as f32 / rings as f32;
            let h = si as f32 / segs as f32;
            let a = h * std::f32::consts::TAU;
            let p = c + Vec2::new(a.cos(), -a.sin()) * r * s;
            mesh.colored_vertex(p, c32(hsv_to_rgb([h, s, hsv[2]])));
        }
    }
    let w = segs + 1;
    for ri in 0..rings {
        for si in 0..segs {
            let i = (ri * w + si) as u32;
            mesh.add_triangle(i, i + 1, i + w as u32);
            mesh.add_triangle(i + 1, i + w as u32 + 1, i + w as u32);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_gray(160)));
    for (k, off) in markers.iter().enumerate() {
        let a = (hsv[0] + off) * std::f32::consts::TAU;
        let p = c + Vec2::new(a.cos(), -a.sin()) * r * hsv[1];
        let rad = if k == 0 { 8.0 } else { 5.0 };
        ui.painter().circle(p, rad, c32(hsv_to_rgb([hsv[0] + off, hsv[1], hsv[2]])), Stroke::new(2.0_f32, Color32::WHITE));
        ui.painter().circle_stroke(p, rad + 1.0, Stroke::new(1.0_f32, Color32::BLACK));
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        let d = pos - c;
        hsv[0] = (-d.y).atan2(d.x).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        hsv[1] = (d.length() / r).clamp(0.0, 1.0);
        return true;
    }
    false
}

/// Horizontal gradient bar for one HSV channel.
fn channel_bar(ui: &mut egui::Ui, hsv: &mut [f32; 3], channel: usize, width: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
    let steps = 48;
    let mut mesh = egui::Mesh::default();
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let mut col = *hsv;
        col[channel] = t;
        let x = rect.left() + rect.width() * t;
        let rgb = c32(hsv_to_rgb(col));
        mesh.colored_vertex(Pos2::new(x, rect.top()), rgb);
        mesh.colored_vertex(Pos2::new(x, rect.bottom()), rgb);
        if k > 0 {
            let i = (2 * k) as u32;
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i + 1, i);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    let x = rect.left() + rect.width() * hsv[channel];
    ui.painter().rect_stroke(
        Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::new(6.0, 22.0)),
        2.0,
        Stroke::new(2.0_f32, Color32::WHITE),
        egui::StrokeKind::Middle,
    );
    if let Some(pos) = resp.interact_pointer_pos() {
        hsv[channel] = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        return true;
    }
    false
}

/// Saturation/value square for the current hue.
fn sv_square(ui: &mut egui::Ui, hsv: &mut [f32; 3]) -> bool {
    let size = 220.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let n = 16;
    let mut mesh = egui::Mesh::default();
    for yi in 0..=n {
        for xi in 0..=n {
            let (s, v) = (xi as f32 / n as f32, 1.0 - yi as f32 / n as f32);
            let p = rect.min + Vec2::new(s, 1.0 - v) * size;
            mesh.colored_vertex(p, c32(hsv_to_rgb([hsv[0], s, v])));
        }
    }
    for yi in 0..n {
        for xi in 0..n {
            let i = (yi * (n + 1) + xi) as u32;
            let w = (n + 1) as u32;
            mesh.add_triangle(i, i + 1, i + w);
            mesh.add_triangle(i + 1, i + w + 1, i + w);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    let p = rect.min + Vec2::new(hsv[1], 1.0 - hsv[2]) * size;
    ui.painter().circle_stroke(p, 7.0, Stroke::new(2.0_f32, Color32::WHITE));
    ui.painter().circle_stroke(p, 8.0, Stroke::new(1.0_f32, Color32::BLACK));
    if let Some(pos) = resp.interact_pointer_pos() {
        let d = (pos - rect.min) / size;
        hsv[1] = d.x.clamp(0.0, 1.0);
        hsv[2] = (1.0 - d.y).clamp(0.0, 1.0);
        return true;
    }
    false
}

fn swatch(ui: &mut egui::Ui, rgb: [u8; 3], size: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    ui.painter().rect_filled(rect, 4.0, c32(rgb));
    let stroke = if selected { Stroke::new(3.0_f32, crate::theme::accent()) } else { Stroke::new(1.0_f32, Color32::from_gray(150)) };
    ui.painter().rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Outside);
    resp
}

pub fn color_window(app: &mut App, ctx: &egui::Context) {
    let Some(w) = app.colors.as_mut() else { return };
    if app.project.graph(w.graph_id).is_none() {
        app.colors = None;
        return;
    }
    let names = series_names(&app.project, w.graph_id);
    if names.is_empty() {
        app.colors = None;
        app.status = "Enter some data to choose its colors".into();
        return;
    }
    w.series = w.series.min(names.len() - 1);
    if w.loaded != Some(w.series) {
        let rgb = series_color(&app.project, w.graph_id, w.series);
        w.hsv = rgb_to_hsv(rgb);
        w.hex = hex_of(rgb);
        w.loaded = Some(w.series);
    }
    let mut open = true;
    let mut changed = false;
    let mut apply_many: Option<Vec<[u8; 3]>> = None;
    let mut use_palette: Option<usize> = None;
    let mut pick: Option<[u8; 3]> = None;
    let mut reset = false;
    egui::Window::new("Colors")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.screen_rect().center() - Vec2::new(200.0, 250.0))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Series").strong());
            ui.horizontal_wrapped(|ui| {
                for (i, n) in names.iter().enumerate() {
                    let rgb = series_color(&app.project, w.graph_id, i);
                    ui.vertical(|ui| {
                        if swatch(ui, rgb, 30.0, i == w.series).on_hover_text(n).clicked() {
                            w.series = i;
                        }
                    });
                }
            });
            ui.label(RichText::new(format!("Editing: {}", names[w.series])).weak());
            ui.separator();
            ui.horizontal(|ui| {
                for m in Mode::ALL {
                    ui.selectable_value(&mut w.mode, m, RichText::new(m.label()).size(14.0));
                }
            });
            ui.add_space(6.0);
            match w.mode {
                Mode::Disc => {
                    ui.vertical_centered(|ui| {
                        changed |= disc(ui, &mut w.hsv, &[0.0]);
                        ui.label("Brightness");
                        changed |= channel_bar(ui, &mut w.hsv, 2, 220.0);
                    });
                }
                Mode::Classic => {
                    ui.vertical_centered(|ui| {
                        changed |= sv_square(ui, &mut w.hsv);
                        ui.label("Hue");
                        changed |= channel_bar(ui, &mut w.hsv, 0, 220.0);
                    });
                }
                Mode::Harmony => {
                    egui::ComboBox::from_id_salt("harmony").selected_text(w.harmony.label()).show_ui(ui, |ui| {
                        for h in Harmony::ALL {
                            ui.selectable_value(&mut w.harmony, h, h.label());
                        }
                    });
                    ui.vertical_centered(|ui| {
                        changed |= disc(ui, &mut w.hsv, w.harmony.offsets());
                        changed |= channel_bar(ui, &mut w.hsv, 2, 220.0);
                    });
                    let scheme: Vec<[u8; 3]> = w.harmony.offsets().iter().map(|o| hsv_to_rgb([w.hsv[0] + o, w.hsv[1], w.hsv[2]])).collect();
                    ui.horizontal(|ui| {
                        for c in &scheme {
                            swatch(ui, *c, 28.0, false);
                        }
                    });
                    if ui.button("Use this scheme for all series").clicked() {
                        apply_many = Some((0..names.len()).map(|i| scheme[i % scheme.len()]).collect());
                    }
                }
                Mode::Values => {
                    let mut rgb = hsv_to_rgb(w.hsv);
                    let before = rgb;
                    egui::Grid::new("rgb").num_columns(2).show(ui, |ui| {
                        for (k, name) in ["R", "G", "B"].iter().enumerate() {
                            ui.label(*name);
                            ui.add(egui::Slider::new(&mut rgb[k], 0..=255));
                            ui.end_row();
                        }
                    });
                    if rgb != before {
                        w.hsv = rgb_to_hsv(rgb);
                        w.hex = hex_of(rgb);
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label("HEX");
                        if ui.add(egui::TextEdit::singleline(&mut w.hex).desired_width(90.0).font(egui::TextStyle::Monospace)).changed() {
                            if let Some(v) = parse_hex(&w.hex) {
                                w.hsv = rgb_to_hsv(v);
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    let [r, g, b] = hsv_to_rgb(w.hsv).map(|v| v as f32 / 255.0);
                    let k = 1.0 - r.max(g).max(b);
                    let mut cmyk = if k >= 1.0 {
                        [0.0, 0.0, 0.0, 100.0]
                    } else {
                        [(1.0 - r - k) / (1.0 - k) * 100.0, (1.0 - g - k) / (1.0 - k) * 100.0, (1.0 - b - k) / (1.0 - k) * 100.0, k * 100.0]
                    };
                    let before = cmyk;
                    egui::Grid::new("cmyk").num_columns(2).show(ui, |ui| {
                        for (i, name) in ["C", "M", "Y", "K"].iter().enumerate() {
                            ui.label(*name);
                            ui.add(egui::Slider::new(&mut cmyk[i], 0.0..=100.0).suffix("%").fixed_decimals(0));
                            ui.end_row();
                        }
                    });
                    if cmyk != before {
                        let [c, m, y, k] = cmyk.map(|v| v / 100.0);
                        let rgb = [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)].map(|v| (v * 255.0).round() as u8);
                        w.hsv = rgb_to_hsv(rgb);
                        w.hex = hex_of(rgb);
                        changed = true;
                    }
                }
                Mode::Palettes => {
                    let current: Vec<[u8; 3]> = (0..names.len()).map(|i| series_color(&app.project, w.graph_id, i)).collect();
                    ui.label(RichText::new("MY PALETTES").font(crate::theme::bold(11.5)).color(crate::theme::pal().weak));
                    let mut remove = None;
                    for (k, (name, cols)) in app.user_palettes.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let resp = ui.add(egui::Button::new(name).min_size(Vec2::new(130.0, 24.0)));
                            for c in cols.iter().take(8) {
                                swatch(ui, *c, 20.0, false);
                            }
                            if crate::icons::button(ui, "trash-2", "Delete this palette", true) {
                                remove = Some(k);
                            }
                            if resp.clicked() && !cols.is_empty() {
                                apply_many = Some((0..names.len()).map(|i| cols[i % cols.len()]).collect());
                            }
                        });
                    }
                    if let Some(k) = remove {
                        app.user_palettes.remove(k);
                    }
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut w.palette_name).hint_text("Name").desired_width(130.0));
                        let ok = !w.palette_name.trim().is_empty();
                        let save = egui::Button::image_and_text(
                            crate::icons::image("bookmark-plus", 14.0, crate::theme::pal().text),
                            "Save current colors",
                        );
                        if ui.add_enabled(ok, save).clicked() {
                            app.user_palettes.push((w.palette_name.trim().to_string(), current.clone()));
                            w.palette_name.clear();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("BUILT-IN").font(crate::theme::bold(11.5)).color(crate::theme::pal().weak));
                    ui.label(
                        RichText::new(format!("Click a name to use the whole palette, or click colors one by one: each goes to “{}”, then to the next data set.", names[w.series]))
                            .small()
                            .weak(),
                    );
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (k, (name, cols)) in hg_plot::PALETTES.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(*name).min_size(Vec2::new(130.0, 24.0))).clicked() {
                                    use_palette = Some(k);
                                }
                                for c in cols.iter() {
                                    let rgb = [c.r, c.g, c.b];
                                    if swatch(ui, rgb, 20.0, false).on_hover_text(hex_of(rgb)).clicked() {
                                        pick = Some(rgb);
                                    }
                                }
                            });
                        }
                    });
                    if ui.button("Reset to the graph palette").clicked() {
                        reset = true;
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let rgb = hsv_to_rgb(w.hsv);
                swatch(ui, rgb, 34.0, false);
                ui.label(RichText::new(hex_of(rgb)).monospace().size(15.0));
            });
        });
    if changed {
        let rgb = hsv_to_rgb(w.hsv);
        if w.mode != Mode::Values {
            w.hex = hex_of(rgb);
        }
        set_series_color(&mut app.project, w.graph_id, w.series, rgb);
    }
    if let Some(cols) = apply_many {
        if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == w.graph_id) {
            g.colors = cols;
        }
        w.loaded = None;
    }
    if let Some(k) = use_palette {
        if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == w.graph_id) {
            g.palette = k;
            g.colors.clear();
        }
        w.loaded = None;
    }
    if let Some(rgb) = pick {
        set_series_color(&mut app.project, w.graph_id, w.series, rgb);
        w.series = (w.series + 1) % names.len();
        w.loaded = None;
    }
    if reset {
        if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == w.graph_id) {
            g.colors.clear();
        }
        w.loaded = None;
    }
    if !open {
        app.colors = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trip() {
        for rgb in [[0, 0, 0], [255, 255, 255], [31, 95, 166], [224, 108, 31], [10, 200, 90]] {
            assert_eq!(hsv_to_rgb(rgb_to_hsv(rgb)), rgb);
        }
        assert_eq!(parse_hex("#1F5FA6"), Some([31, 95, 166]));
    }
}
