//! Bundled Lucide icons (ISC license, see assets/icons/LICENSE-lucide.txt).
//! The SVGs are white so they can be tinted to any color.

use eframe::egui::{self, Color32, Vec2};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[$(($name, include_bytes!(concat!("../../../assets/icons/", $name, ".svg")))),*];
    };
}

icons!(
    "activity",
    "arrow-down-up",
    "asterisk",
    "ban",
    "bookmark-plus",
    "chart-column-big",
    "chart-column-stacked",
    "chart-column",
    "chart-line",
    "chart-pie",
    "chart-scatter",
    "check",
    "chevron-down",
    "chevron-right",
    "circle-check",
    "circle-dot",
    "circle",
    "clipboard-paste",
    "columns-3",
    "copy",
    "diamond",
    "download",
    "eraser",
    "eye-off",
    "eye",
    "file-input",
    "file-output",
    "file-plus-2",
    "file-text",
    "flask-conical",
    "folder-open",
    "folder",
    "grid-3x3",
    "hand",
    "hexagon",
    "image-down",
    "info",
    "list-checks",
    "list-plus",
    "list-x",
    "maximize",
    "minus",
    "moon",
    "mouse-pointer-2",
    "move",
    "palette",
    "pen-tool",
    "plus",
    "redo-2",
    "rows-3",
    "ruler",
    "save",
    "scaling",
    "scan",
    "scissors",
    "search",
    "settings",
    "shapes",
    "shield-check",
    "sigma",
    "sliders-horizontal",
    "spline",
    "square-function",
    "square",
    "sun",
    "table-2",
    "text-cursor-input",
    "trash-2",
    "trending-down",
    "triangle-alert",
    "triangle",
    "type",
    "undo-2",
    "wand-sparkles",
    "x",
    "zoom-in",
    "zoom-out",
);

fn bytes(name: &str) -> &'static [u8] {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b).unwrap_or_else(|| panic!("unknown icon {name}"))
}

/// An icon image of the given size, tinted with `color`.
pub fn image(name: &str, size: f32, color: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(format!("bytes://icon/{name}.svg"), bytes(name)).fit_to_exact_size(Vec2::splat(size)).tint(color)
}

/// Paints an icon centered in `rect`.
pub fn paint(ui: &egui::Ui, name: &str, rect: egui::Rect, color: Color32) {
    image(name, rect.width().min(rect.height()), color).paint_at(ui, rect);
}

/// A small square button with just an icon.
pub fn button(ui: &mut egui::Ui, name: &str, tooltip: &str, enabled: bool) -> bool {
    let p = crate::theme::pal();
    let size = Vec2::splat(28.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let color = if enabled { p.text } else { p.disabled };
    paint(ui, name, rect.shrink(6.0), color);
    enabled && resp.on_hover_text(tooltip).clicked()
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_icons_are_tintable_svgs() {
        for (name, b) in super::ICONS {
            let s = std::str::from_utf8(b).unwrap();
            assert!(s.contains("<svg") && !s.contains("currentColor"), "{name}");
        }
    }
}
