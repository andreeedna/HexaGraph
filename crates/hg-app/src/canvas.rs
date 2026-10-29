//! Interactive graph canvas: zoom, pan, select and move graph elements, like
//! in a vector drawing program.

use crate::app::App;
use crate::graph_view::Tool;
use crate::{icons, theme};
use eframe::egui::{self, Color32, CursorIcon, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use hg_plot::{Element, Scene};

/// Per-graph view state, kept in egui memory.
#[derive(Clone, Default)]
pub struct View {
    /// Zoom relative to "fit to window" (1.0).
    pub zoom: f32,
    pub pan: Vec2,
    pub selected: Option<Element>,
    pub hand: bool,
    dragging: Option<Element>,
    resize: Option<Resize>,
}

/// A page resize in progress: the scale is frozen and the opposite side of the
/// page stays in place, so the edge follows the pointer.
#[derive(Clone, Copy)]
struct Resize {
    /// Which sides move: (left, right, top, bottom).
    sides: (bool, bool, bool, bool),
    scale: f32,
    /// The page corners on screen when the drag started.
    min: Pos2,
    max: Pos2,
}

impl View {
    fn zoom(&self) -> f32 {
        if self.zoom <= 0.0 {
            1.0
        } else {
            self.zoom
        }
    }
}

fn view_id(graph: u64) -> egui::Id {
    egui::Id::new(("graph_view", graph))
}

pub fn view(ctx: &egui::Context, graph: u64) -> View {
    ctx.data(|d| d.get_temp(view_id(graph))).unwrap_or_default()
}

pub fn set_view(ctx: &egui::Context, graph: u64, v: View) {
    ctx.data_mut(|d| d.insert_temp(view_id(graph), v));
}

/// The tool panel that edits an element.
fn tool_for(e: Element) -> Tool {
    match e {
        Element::Title | Element::XTitle | Element::YTitle | Element::Legend => Tool::Titles,
        Element::XAxis | Element::YAxis => Tool::Axes,
        Element::Series(_) => Tool::Colors,
        Element::Bracket(_) => Tool::Significance,
        Element::Plot => Tool::Data,
    }
}

/// Elements that can be dragged: movable labels and (vertically) brackets.
fn draggable(e: Element) -> bool {
    e.movable() || matches!(e, Element::Bracket(_))
}

/// Moves an element by (dx, dy) points: labels in both directions, brackets
/// vertically.
fn nudge(project: &mut hg_core::Project, id: u64, e: Element, dx: f32, dy: f32) {
    let key = match e {
        Element::Bracket(k) => {
            project.graph(id).and_then(|g| hg_plot::render::graph_brackets(project, g).get(k).map(|b| (b.row, b.a, b.b)))
        }
        _ => None,
    };
    if let Some(g) = project.graphs.iter_mut().find(|g| g.id == id) {
        if let Some(o) = offset_mut(g, e) {
            o.0 += dx;
            o.1 += dy;
        }
        if let Some(key) = key {
            *g.bracket_offset_mut(key) += dy;
        }
    }
}

fn offset_mut(g: &mut hg_core::Graph, e: Element) -> Option<&mut (f32, f32)> {
    match e {
        Element::Title => Some(&mut g.offsets.title),
        Element::XTitle => Some(&mut g.offsets.x_title),
        Element::YTitle => Some(&mut g.offsets.y_title),
        Element::Legend => Some(&mut g.offsets.legend),
        _ => None,
    }
}

fn zoom_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> bool {
    icons::button(ui, icon, tip, true)
}

/// The canvas toolbar and the drawing area for one graph.
pub fn canvas(app: &mut App, ui: &mut egui::Ui, id: u64, scene: &Scene) {
    let p = theme::pal();
    let ctx = ui.ctx().clone();
    let mut v = view(&ctx, id);
    let manual_brackets = app.project.graph(id).map_or(0, |g| g.brackets.len());

    // --- toolbar
    ui.horizontal(|ui| {
        let sel = |ui: &mut egui::Ui, icon: &str, tip: &str, on: bool| -> bool {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
            if on {
                ui.painter().rect_filled(rect, 6.0, p.selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 6.0, p.hover);
            }
            icons::paint(ui, icon, rect.shrink(7.0), if on { p.accent } else { p.text });
            resp.on_hover_text(tip).clicked()
        };
        if sel(ui, "mouse-pointer-2", "Select and move (V)", !v.hand) {
            v.hand = false;
        }
        if sel(ui, "hand", "Pan the view (H, or drag with the middle button)", v.hand) {
            v.hand = true;
        }
        ui.separator();
        if zoom_button(ui, "zoom-out", "Zoom out (Ctrl −)") {
            v.zoom = (v.zoom() / 1.25).max(0.2);
        }
        ui.label(RichText::new(format!("{:.0}%", v.zoom() * 100.0)).monospace());
        if zoom_button(ui, "zoom-in", "Zoom in (Ctrl +)") {
            v.zoom = (v.zoom() * 1.25).min(8.0);
        }
        if zoom_button(ui, "maximize", "Fit to window (Ctrl 0)") {
            v.zoom = 1.0;
            v.pan = Vec2::ZERO;
        }
        ui.separator();
        match v.selected {
            Some(e) => {
                ui.label(RichText::new(e.label()).font(theme::bold(13.0)).color(p.accent));
                if e.movable() {
                    let moved = app.project.graph(id).and_then(|g| {
                        let mut g = g.clone();
                        offset_mut(&mut g, e).map(|o| *o != (0.0, 0.0))
                    });
                    if moved == Some(true) && ui.small_button("Reset position").clicked() {
                        if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == id) {
                            if let Some(o) = offset_mut(g, e) {
                                *o = (0.0, 0.0);
                            }
                        }
                    }
                    ui.label(RichText::new("drag or use the arrows to move").small().color(p.weak));
                } else if matches!(e, Element::Bracket(k) if k < manual_brackets) {
                    ui.label(RichText::new("Delete removes it").small().color(p.weak));
                } else if matches!(e, Element::Series(_)) {
                    ui.label(RichText::new("double-click to change its color").small().color(p.weak));
                }
            }
            None => {
                ui.label(RichText::new("Click an element of the graph to edit it").small().color(p.weak));
            }
        }
    });

    // --- drawing area
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, p.canvas);
    let fit = crate::paint::fit_scale(rect.shrink(28.0), scene);
    let (s, size, origin) = match v.resize {
        Some(r) => {
            let size = Vec2::new(scene.width, scene.height) * r.scale;
            let x = if r.sides.0 { r.max.x - size.x } else { r.min.x };
            let y = if r.sides.2 { r.max.y - size.y } else { r.min.y };
            (r.scale, size, Pos2::new(x, y))
        }
        None => {
            let s = fit * v.zoom();
            let size = Vec2::new(scene.width, scene.height) * s;
            (s, size, rect.center() - size / 2.0 + v.pan)
        }
    };
    let to_scene = |q: Pos2| ((q.x - origin.x) / s, (q.y - origin.y) / s);
    let to_screen = |q: (f32, f32)| Pos2::new(origin.x + q.0 * s, origin.y + q.1 * s);

    // Zoom with Ctrl + wheel around the pointer, scroll to pan.
    if resp.hovered() {
        let (scroll, zoom_delta, pointer) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        if zoom_delta != 1.0 {
            if let Some(pos) = pointer {
                let new_zoom = (v.zoom() * zoom_delta).clamp(0.2, 8.0);
                let world = to_scene(pos);
                let s2 = s / v.zoom() * new_zoom;
                let origin2 = pos - Vec2::new(world.0, world.1) * s2;
                v.pan = origin2 - (rect.center() - Vec2::new(scene.width, scene.height) * s2 / 2.0);
                v.zoom = new_zoom;
            }
        } else if scroll != Vec2::ZERO {
            v.pan += scroll;
        }
    }

    crate::paint::paint_at(&painter, origin, s, scene);

    // Resize the page from its edges (one dimension) or corners (both).
    let page = Rect::from_min_size(origin, size);
    let (band, corner) = (6.0, 9.0);
    let zones: [((bool, bool, bool, bool), Rect, CursorIcon); 8] = [
        ((true, false, false, false), Rect::from_x_y_ranges(page.left() - band..=page.left() + band, page.y_range()), CursorIcon::ResizeHorizontal),
        ((false, true, false, false), Rect::from_x_y_ranges(page.right() - band..=page.right() + band, page.y_range()), CursorIcon::ResizeHorizontal),
        ((false, false, true, false), Rect::from_x_y_ranges(page.x_range(), page.top() - band..=page.top() + band), CursorIcon::ResizeVertical),
        ((false, false, false, true), Rect::from_x_y_ranges(page.x_range(), page.bottom() - band..=page.bottom() + band), CursorIcon::ResizeVertical),
        ((true, false, true, false), Rect::from_center_size(page.left_top(), Vec2::splat(corner * 2.0)), CursorIcon::ResizeNwSe),
        ((false, true, false, true), Rect::from_center_size(page.right_bottom(), Vec2::splat(corner * 2.0)), CursorIcon::ResizeNwSe),
        ((false, true, true, false), Rect::from_center_size(page.right_top(), Vec2::splat(corner * 2.0)), CursorIcon::ResizeNeSw),
        ((true, false, false, true), Rect::from_center_size(page.left_bottom(), Vec2::splat(corner * 2.0)), CursorIcon::ResizeNeSw),
    ];
    let mut resizing = false;
    let mut hot = false;
    let mut released = false;
    for (k, (sides, zone, cursor)) in zones.into_iter().enumerate() {
        let r = ui.interact(zone, ui.id().with(("page_resize", id, k)), Sense::drag());
        if r.drag_started() {
            v.resize = Some(Resize { sides, scale: s, min: page.min, max: page.max });
        }
        if r.hovered() || r.dragged() {
            ctx.set_cursor_icon(cursor);
            hot = true;
        }
        if r.dragged() {
            resizing = true;
            let d = r.drag_delta() / s;
            if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == id) {
                let dw = if sides.0 { -d.x } else if sides.1 { d.x } else { 0.0 };
                let dh = if sides.2 { -d.y } else if sides.3 { d.y } else { 0.0 };
                g.width = (g.width + dw).clamp(200.0, 2400.0);
                g.height = (g.height + dh).clamp(150.0, 2400.0);
            }
        }
        released |= r.drag_stopped();
    }
    if released {
        // Keep the page where it is: turn the frozen scale and position back
        // into zoom and pan.
        if let Some(r) = v.resize.take() {
            v.zoom = r.scale / fit.max(1e-6);
            v.pan = origin - (rect.center() - size / 2.0);
        }
    }
    let handle = Rect::from_center_size(page.right_bottom(), Vec2::splat(10.0));
    painter.rect_filled(handle, 2.0, if hot || resizing { p.accent } else { p.border });
    if hot || resizing {
        painter.rect_stroke(page, 0.0, Stroke::new(1.0_f32, p.accent), egui::StrokeKind::Outside);
    }
    if hot || resizing {
        painter.text(
            page.right_bottom() + Vec2::new(-4.0, 10.0),
            egui::Align2::RIGHT_TOP,
            format!(
                "{:.0} × {:.0} pt  ({:.1} × {:.1} cm)",
                scene.width,
                scene.height,
                scene.width / 72.0 * 2.54,
                scene.height / 72.0 * 2.54
            ),
            egui::FontId::proportional(12.0),
            p.text,
        );
    }

    let pointer = resp.hover_pos();
    let hovered = pointer.filter(|_| !v.hand).and_then(|q| scene.hit_test(to_scene(q), 4.0 / s)).map(|h| h.element);
    let panning = v.hand || ctx.input(|i| i.pointer.middle_down() || i.key_down(Key::Space));

    // Dragging: pan, or move a label.
    if resp.drag_started() && !panning {
        if let Some(e) = hovered.filter(|e| draggable(*e)) {
            v.dragging = Some(e);
            v.selected = Some(e);
            app.graph_tool = tool_for(e);
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        match v.dragging {
            Some(e) if !panning => nudge(&mut app.project, id, e, d.x / s, d.y / s),
            _ if !resizing => v.pan += d,
            _ => {}
        }
    }
    if resp.drag_stopped() {
        v.dragging = None;
    }
    if panning && resp.hovered() {
        ctx.set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
    } else if let Some(e) = v.dragging.or(hovered) {
        ctx.set_cursor_icon(match e {
            Element::Bracket(_) => CursorIcon::ResizeVertical,
            e if e.movable() => CursorIcon::Move,
            _ => CursorIcon::PointingHand,
        });
    }

    // Click selects, double click opens the most useful editor.
    if resp.clicked() && !panning {
        v.selected = hovered.filter(|e| *e != Element::Plot).or(hovered);
        if let Some(e) = v.selected {
            app.graph_tool = tool_for(e);
        }
    }
    if resp.double_clicked() {
        if let Some(Element::Series(i)) = hovered {
            let mut w = crate::colors::ColorWindow::new(id);
            w.select(i);
            app.colors = Some(w);
        }
    }

    // Keyboard: nudge, delete, deselect, zoom.
    if !ctx.wants_keyboard_input() && (resp.hovered() || v.selected.is_some()) {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::COMMAND, Key::Num0) {
                v.zoom = 1.0;
                v.pan = Vec2::ZERO;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::Equals) || i.consume_key(egui::Modifiers::COMMAND, Key::Plus) {
                v.zoom = (v.zoom() * 1.25).min(8.0);
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::Minus) {
                v.zoom = (v.zoom() / 1.25).max(0.2);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::H) {
                v.hand = true;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::V) {
                v.hand = false;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                v.selected = None;
            }
        });
        if let Some(e) = v.selected {
            let (mut dx, mut dy) = (0.0, 0.0);
            ctx.input_mut(|i| {
                for (m, step) in [(egui::Modifiers::NONE, 1.0), (egui::Modifiers::SHIFT, 10.0)] {
                    if i.consume_key(m, Key::ArrowLeft) {
                        dx -= step;
                    }
                    if i.consume_key(m, Key::ArrowRight) {
                        dx += step;
                    }
                    if i.consume_key(m, Key::ArrowUp) {
                        dy -= step;
                    }
                    if i.consume_key(m, Key::ArrowDown) {
                        dy += step;
                    }
                }
            });
            let delete = ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Delete) || i.consume_key(egui::Modifiers::NONE, Key::Backspace));
            if dx != 0.0 || dy != 0.0 {
                nudge(&mut app.project, id, e, dx, dy);
            }
            if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == id) {
                if let (true, Element::Bracket(k)) = (delete, e) {
                    if k < g.brackets.len() {
                        g.brackets.remove(k);
                        v.selected = None;
                    }
                }
            }
        }
    }

    // Hover and selection outlines.
    let outline = |e: Element, stroke: Stroke, handles: bool| {
        if let Some(h) = scene.bounds(e) {
            let r = Rect::from_min_max(to_screen(h.min), to_screen(h.max)).expand(3.0);
            painter.rect_stroke(r, 3.0, stroke, egui::StrokeKind::Outside);
            if handles {
                for c in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                    let hr = Rect::from_center_size(c, Vec2::splat(7.0));
                    painter.rect_filled(hr, 1.5, Color32::WHITE);
                    painter.rect_stroke(hr, 1.5, Stroke::new(1.5_f32, p.accent), egui::StrokeKind::Middle);
                }
            }
        }
    };
    if let Some(e) = hovered.filter(|e| Some(*e) != v.selected) {
        outline(e, Stroke::new(1.0_f32, p.accent.gamma_multiply(0.7)), false);
        if let Some(q) = pointer {
            painter.text(q + Vec2::new(14.0, 18.0), egui::Align2::LEFT_TOP, e.label(), egui::FontId::proportional(11.5), p.weak);
        }
    }
    if let Some(e) = v.selected {
        outline(e, Stroke::new(1.5_f32, p.accent), true);
    }
    set_view(&ctx, id, v);
}
