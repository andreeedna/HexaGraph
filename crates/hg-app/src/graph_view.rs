//! Graph sheet: a strip of tools on the left (like a drawing program), the
//! options of the selected tool next to it, and the live graph on the right.

use crate::app::App;
use crate::export::Format;
use eframe::egui::{self, Color32, RichText, Vec2};
use hg_core::graph::{Bracket, ErrorBar, Fill, GraphKind, LabelStyle, MarkerShape, Scale, TextKind};
use hg_core::{AnalysisKind, TableKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Type,
    Data,
    Points,
    Bars,
    Titles,
    Axes,
    Significance,
    Colors,
    Style,
    Curves,
}

impl Tool {
    fn icon(self) -> &'static str {
        match self {
            Tool::Type => "chart-column",
            Tool::Data => "activity",
            Tool::Points => "circle-dot",
            Tool::Bars => "chart-column-big",
            Tool::Titles => "type",
            Tool::Axes => "ruler",
            Tool::Significance => "asterisk",
            Tool::Colors => "palette",
            Tool::Style => "pen-tool",
            Tool::Curves => "spline",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Tool::Type => "Type",
            Tool::Data => "Data",
            Tool::Points => "Points",
            Tool::Bars => "Bars",
            Tool::Titles => "Titles",
            Tool::Axes => "Axes",
            Tool::Significance => "Stars",
            Tool::Colors => "Colors",
            Tool::Style => "Style",
            Tool::Curves => "Curves",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Tool::Type => "Graph type",
            Tool::Data => "Data and error bars",
            Tool::Points => "Points",
            Tool::Bars => "Bars and boxes",
            Tool::Titles => "Titles",
            Tool::Axes => "Axes",
            Tool::Significance => "Significance",
            Tool::Colors => "Colors",
            Tool::Style => "Style",
            Tool::Curves => "Fitted curves",
        }
    }
}

// ---------------------------------------------------------------- export

pub struct ExportWindow {
    graph_id: u64,
    format: Format,
    dpi: f32,
}

impl ExportWindow {
    pub fn new(graph_id: u64) -> Self {
        ExportWindow { graph_id, format: Format::Png, dpi: 300.0 }
    }
}

pub fn export_window(app: &mut App, ctx: &egui::Context) {
    let Some(gid) = app.export.as_ref().map(|w| w.graph_id) else { return };
    let Some(name) = app.project.graph(gid).map(|g| g.name.clone()) else {
        app.export = None;
        return;
    };
    let scene = app.scene(gid);
    let Some(w) = app.export.as_mut() else { return };
    let mut open = true;
    let mut go = false;
    egui::Window::new("Export graph")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Format").strong());
            ui.horizontal(|ui| {
                for f in Format::ALL {
                    ui.selectable_value(&mut w.format, f, RichText::new(f.label()).size(14.0));
                }
            });
            ui.add_space(6.0);
            if w.format.is_raster() {
                ui.label(RichText::new("Resolution").strong());
                ui.horizontal(|ui| {
                    for dpi in [72.0, 150.0, 300.0, 600.0] {
                        ui.selectable_value(&mut w.dpi, dpi, format!("{dpi} dpi"));
                    }
                    ui.add(egui::DragValue::new(&mut w.dpi).range(36.0..=1200.0).suffix(" dpi"));
                });
                if let Ok(s) = &scene {
                    let (px, py) = crate::export::pixel_size(s, w.dpi);
                    ui.label(
                        RichText::new(format!("{px} × {py} pixels · {:.1} × {:.1} cm", s.width / 72.0 * 2.54, s.height / 72.0 * 2.54))
                            .weak(),
                    );
                }
                ui.label(RichText::new("300 dpi is the usual requirement of journals.").weak().small());
            } else {
                ui.label(RichText::new("Vector format: sharp at any size, editable in Illustrator or Inkscape.").weak());
            }
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        scene.is_ok(),
                        egui::Button::new(RichText::new("Export…").font(crate::theme::bold(14.0)).color(crate::theme::pal().accent_text))
                            .fill(crate::theme::accent())
                            .min_size(Vec2::new(96.0, 28.0)),
                    )
                    .clicked()
                {
                    go = true;
                }
            });
        });
    if go {
        let (format, dpi) = (w.format, w.dpi);
        let ext = format.extension();
        if let Some(path) = rfd::FileDialog::new().add_filter(format.label(), &[ext]).set_file_name(format!("{name}.{ext}")).save_file() {
            let path = if path.extension().is_none() { path.with_extension(ext) } else { path };
            app.status = match crate::export::export(scene.as_ref().unwrap(), format, dpi, &path) {
                Ok(()) => format!("Exported {}", path.display()),
                Err(e) => format!("Export failed: {e}"),
            };
            app.export = None;
        }
    } else if !open {
        app.export = None;
    }
}

// ---------------------------------------------------------------- tools

fn strip_button(ui: &mut egui::Ui, icon: &str, label: &str, on: bool) -> egui::Response {
    let p = crate::theme::pal();
    let size = Vec2::new(60.0, 52.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let bg = if on {
        p.selected
    } else if resp.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect.shrink(2.0), 8.0, bg);
    if on {
        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min + Vec2::new(0.0, 10.0), Vec2::new(3.0, size.y - 20.0)), 2.0, p.accent);
    }
    let fg = if on { p.accent } else { p.text };
    crate::icons::paint(ui, icon, egui::Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 17.0), Vec2::splat(20.0)), fg);
    ui.painter().text(rect.center_bottom() - Vec2::new(0.0, 6.0), egui::Align2::CENTER_BOTTOM, label, egui::FontId::proportional(11.0), fg);
    resp
}

fn tool_button(ui: &mut egui::Ui, tool: Tool, current: &mut Tool) {
    if strip_button(ui, tool.icon(), tool.label(), *current == tool).on_hover_text(tool.title()).clicked() {
        *current = tool;
    }
}

/// A row of large toggle buttons, one per option.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (v, label) in options {
            if ui.add(egui::SelectableLabel::new(*value == *v, RichText::new(*label).size(13.5))).clicked() && *value != *v {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title.to_uppercase()).font(crate::theme::bold(11.5)).color(crate::theme::pal().weak));
    ui.add_space(2.0);
}

pub fn graph_view(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let Some(g) = app.project.graph(id) else { return };
    let table_id = g.table_id;
    let Some(t) = app.project.table(table_id) else { return };
    let table_kind = t.kind;
    let kind = g.kind;
    let tools: Vec<Tool> = {
        let mut v = vec![Tool::Type, Tool::Data];
        if !matches!(kind, GraphKind::Pie | GraphKind::Donut | GraphKind::Heatmap) {
            v.push(Tool::Points);
        }
        if matches!(
            kind,
            GraphKind::Bar
                | GraphKind::GroupedBar
                | GraphKind::StackedBar
                | GraphKind::Histogram
                | GraphKind::BoxWhisker
                | GraphKind::GroupedBox
                | GraphKind::Violin
        ) {
            v.push(Tool::Bars);
        }
        v.extend([Tool::Titles, Tool::Axes]);
        if bracket_kind(kind) {
            v.push(Tool::Significance);
        }
        v.extend([Tool::Colors, Tool::Style]);
        if kind == GraphKind::Xy {
            v.push(Tool::Curves);
        }
        v
    };
    if !tools.contains(&app.graph_tool) {
        app.graph_tool = Tool::Type;
    }

    let p = crate::theme::pal();
    egui::SidePanel::left("graph_tools")
        .exact_width(68.0)
        .resizable(false)
        .frame(egui::Frame::new().fill(p.sidebar).inner_margin(4.0).corner_radius(10.0))
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            for tool in &tools {
                tool_button(ui, *tool, &mut app.graph_tool);
            }
            ui.add_space(6.0);
            ui.separator();
            if strip_button(ui, "image-down", "Export", false).on_hover_text("Export as PNG, JPEG, TIFF or SVG").clicked() {
                app.export = Some(ExportWindow::new(id));
            }
        });

    egui::SidePanel::left("graph_tool_panel")
        .default_width(310.0)
        .min_width(270.0)
        .resizable(true)
        .frame(
            egui::Frame::new()
                .fill(p.panel)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .corner_radius(10.0)
                .stroke(egui::Stroke::new(1.0_f32, p.border)),
        )
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                crate::icons::paint(
                    ui,
                    app.graph_tool.icon(),
                    egui::Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 2.0), Vec2::splat(20.0)),
                    p.accent,
                );
                ui.add_space(26.0);
                ui.heading(app.graph_tool.title());
            });
            ui.add_space(2.0);
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match app.graph_tool {
                Tool::Type => type_panel(app, ui, id),
                Tool::Data => data_panel(app, ui, id),
                Tool::Points => points_panel(app, ui, id),
                Tool::Bars => bars_panel(app, ui, id),
                Tool::Titles => titles_panel(app, ui, id),
                Tool::Axes => axes_panel(app, ui, id),
                Tool::Significance => significance_panel(app, ui, id),
                Tool::Colors => colors_panel(app, ui, id),
                Tool::Style => style_panel(app, ui, id),
                Tool::Curves => curves_panel(app, ui, id),
            });
        });

    let scene = app.scene(id);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 0, top: 0, bottom: 0 }))
        .show_inside(ui, |ui| match scene {
            Ok(scene) => crate::canvas::canvas(app, ui, id, &scene),
            Err(e) => {
                ui.centered_and_justified(|ui| {
                    let hint = if table_kind == TableKind::Survival {
                        e.to_string()
                    } else {
                        format!("{e} — open the data table in the navigator")
                    };
                    ui.label(RichText::new(hint).weak().size(16.0));
                });
            }
        });
}

fn bracket_kind(k: GraphKind) -> bool {
    matches!(
        k,
        GraphKind::ScatterDot
            | GraphKind::Bar
            | GraphKind::BoxWhisker
            | GraphKind::Violin
            | GraphKind::MeanError
            | GraphKind::BeforeAfter
            | GraphKind::GroupedBar
            | GraphKind::GroupedScatter
    )
}

fn grouped(k: GraphKind) -> bool {
    matches!(k, GraphKind::GroupedBar | GraphKind::GroupedScatter | GraphKind::StackedBar)
}

fn graph_mut(app: &mut App, id: u64) -> &mut hg_core::Graph {
    app.project.graphs.iter_mut().find(|g| g.id == id).unwrap()
}

fn type_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let table_kind = app.project.graph(id).and_then(|g| app.project.table(g.table_id)).map(|t| t.kind).unwrap();
    let table_id = app.project.graph(id).unwrap().table_id;
    let g = graph_mut(app, id);
    ui.label("Name of this graph");
    ui.text_edit_singleline(&mut g.name);
    section(ui, "Kind of graph");
    for &k in GraphKind::for_table(table_kind) {
        if ui.selectable_label(g.kind == k, RichText::new(k.label()).size(14.0)).clicked() {
            g.kind = k;
        }
    }
    if grouped(g.kind) {
        section(ui, "On the X axis");
        segmented(ui, &mut g.transpose, &[(false, "Rows"), (true, "Data sets (columns)")]);
    }
    ui.add_space(10.0);
    if ui.button("🖼 Browse the gallery…").clicked() {
        app.gallery = Some(crate::dialogs::GraphGallery::retype(table_id, id));
    }
}

/// Draws a symbol shape in a small button.
fn shape_button(ui: &mut egui::Ui, shape: MarkerShape, filled: bool, color: Color32, on: bool) -> bool {
    let p = crate::theme::pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
    if on {
        ui.painter().rect(rect, 5.0, p.selected, egui::Stroke::new(1.5_f32, p.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, p.hover);
    }
    let mut scene = hg_plot::Scene::new(24.0, 24.0);
    scene.background = hg_plot::Color::TRANSPARENT;
    let c = hg_plot::Color::rgb(color.r(), color.g(), color.b());
    scene.prims.push(hg_plot::Prim::Marker {
        center: (12.0, 12.0),
        shape,
        size: 6.0,
        fill: if filled { c } else { hg_plot::Color::WHITE },
        stroke: c,
        stroke_width: 1.2,
    });
    crate::paint::paint_at(ui.painter(), rect.min, 1.0, &scene);
    resp.clicked()
}

fn data_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let g = graph_mut(app, id);
    let has_error = !matches!(
        g.kind,
        GraphKind::Pie
            | GraphKind::Donut
            | GraphKind::Survival
            | GraphKind::BoxWhisker
            | GraphKind::Violin
            | GraphKind::BeforeAfter
            | GraphKind::GroupedBox
            | GraphKind::Histogram
            | GraphKind::QqPlot
            | GraphKind::Heatmap
    );
    if has_error {
        section(ui, "Center and error bars");
        for e in ErrorBar::ALL {
            ui.radio_value(&mut g.error_bar, e, e.label());
        }
        section(ui, "Error bar style");
        ui.add(egui::Slider::new(&mut g.widths.error_bars, 0.25..=5.0).text("thickness"));
        ui.add(egui::Slider::new(&mut g.cap_width, 0.0..=3.0).text("cap width"));
    } else {
        ui.label(RichText::new("This kind of graph has no error bars: it shows the data directly.").weak());
    }
    if matches!(
        g.kind,
        GraphKind::Xy | GraphKind::LineMean | GraphKind::GroupedLine | GraphKind::Survival | GraphKind::QqPlot | GraphKind::BeforeAfter
    ) {
        section(ui, "Lines");
        if g.kind == GraphKind::Xy {
            ui.checkbox(&mut g.connect_points, "Connect the points with lines");
        }
        ui.add(egui::Slider::new(&mut g.widths.lines, 0.25..=6.0).text("line thickness"));
    }
}

fn points_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let names = crate::colors::series_names(&app.project, id);
    let colors: Vec<[u8; 3]> = (0..names.len()).map(|i| crate::colors::series_color(&app.project, id, i)).collect();
    let g = graph_mut(app, id);
    section(ui, "Visibility");
    ui.checkbox(&mut g.show_points, "Show each individual value");
    section(ui, "Size and border");
    ui.add(egui::Slider::new(&mut g.marker_size, 1.0..=14.0).text("size"));
    ui.add(egui::Slider::new(&mut g.widths.symbol_border, 0.0..=4.0).text("border thickness"));
    section(ui, "Shape and fill of each data set");
    for (i, name) in names.iter().enumerate() {
        let [r, gr, b] = colors[i];
        let color = Color32::from_rgb(r, gr, b);
        let style = g.series_style(i);
        let default_shape = MarkerShape::ALL[i % 4];
        let filled_now = style.fill != Fill::Hollow;
        ui.label(RichText::new(name).font(crate::theme::bold(13.0)));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for shape in MarkerShape::CHOICES {
                let on = style.shape.unwrap_or(default_shape) == shape;
                if shape_button(ui, shape, filled_now, color, on) {
                    g.series_style_mut(i).shape = Some(shape);
                }
            }
        });
        ui.horizontal(|ui| {
            let mut fill = style.fill;
            if segmented(ui, &mut fill, &[(Fill::Auto, "Auto"), (Fill::Filled, "Filled"), (Fill::Hollow, "Hollow")]) {
                g.series_style_mut(i).fill = fill;
            }
        });
        ui.add_space(4.0);
    }
}

fn bars_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let g = graph_mut(app, id);
    section(ui, "Fill");
    ui.add(egui::Slider::new(&mut g.bar_opacity, 0.0..=1.0).text("opacity"));
    section(ui, "Size");
    ui.add(egui::Slider::new(&mut g.bar_width, 0.2..=0.95).text("width"));
    ui.add(egui::Slider::new(&mut g.widths.bar_border, 0.0..=5.0).text("border thickness"));
    if matches!(g.kind, GraphKind::Bar | GraphKind::GroupedBar) {
        section(ui, "Error bars");
        ui.checkbox(&mut g.error_up_only, "Above the bar only");
    }
}

fn titles_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let names = crate::colors::series_names(&app.project, id);
    let base = app.project.graph(id).map_or(12.0, |g| g.font_size);
    let g = graph_mut(app, id);
    for (label, value, hint) in
        [("Graph title", &mut g.title, "none"), ("Y axis title", &mut g.y_label, "none"), ("X axis title", &mut g.x_label, "automatic")]
    {
        section(ui, label);
        ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(f32::INFINITY));
    }
    section(ui, "Legend");
    ui.checkbox(&mut g.show_legend, "Show the legend");
    let mut size = g.texts.resolve(TextKind::Legend, base).0;
    if ui.add(egui::Slider::new(&mut size, 6.0..=48.0).text("text size")).changed() {
        g.texts.get_mut(TextKind::Legend).size = Some(size);
    }
    ui.label(RichText::new("Text of each entry (empty = the data set title)").weak());
    if g.legend_labels.len() < names.len() {
        g.legend_labels.resize(names.len(), String::new());
    }
    for (i, n) in names.iter().enumerate() {
        ui.add(egui::TextEdit::singleline(&mut g.legend_labels[i]).hint_text(n.as_str()).desired_width(f32::INFINITY));
    }
    if g.offsets.legend != (0.0, 0.0) && ui.button("Put the legend back in place").clicked() {
        g.offsets.legend = (0.0, 0.0);
    }
    ui.label(RichText::new("Drag the legend on the graph to move it.").weak());
}

fn axes_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let g = graph_mut(app, id);
    section(ui, "Y axis scale");
    segmented(ui, &mut g.y_scale, &[(Scale::Linear, "Linear"), (Scale::Log10, "Logarithmic")]);
    section(ui, "Y axis range");
    ui.horizontal(|ui| {
        ui.label("From");
        opt_value(ui, &mut g.y_min, "ymin");
        ui.label("to");
        opt_value(ui, &mut g.y_max, "ymax");
    });
    ui.label(RichText::new("Leave empty for an automatic range.").weak().small());
    if g.kind == GraphKind::Xy {
        section(ui, "X axis scale");
        segmented(ui, &mut g.x_scale, &[(Scale::Linear, "Linear"), (Scale::Log10, "Logarithmic")]);
    }
    section(ui, "X axis labels angle");
    let mut mode = g.x_tick_angle.unwrap_or(-1.0);
    ui.horizontal_wrapped(|ui| {
        for (v, label) in [(-1.0, "Auto"), (0.0, "0°"), (30.0, "30°"), (45.0, "45°"), (60.0, "60°"), (90.0, "90°")] {
            if ui.selectable_label(mode == v, label).clicked() {
                mode = v;
            }
        }
    });
    if mode >= 0.0 {
        ui.add(egui::Slider::new(&mut mode, 0.0..=90.0).suffix("°").text("custom"));
    }
    g.x_tick_angle = (mode >= 0.0).then_some(mode);
    section(ui, "Y axis numbers angle");
    ui.horizontal_wrapped(|ui| {
        for (v, label) in [(0.0, "0°"), (45.0, "45°"), (90.0, "90°")] {
            if ui.selectable_label(g.y_tick_angle == v, label).clicked() {
                g.y_tick_angle = v;
            }
        }
    });
    ui.add(egui::Slider::new(&mut g.y_tick_angle, 0.0..=90.0).suffix("°").text("custom"));
    section(ui, "Axis lines");
    ui.add(egui::Slider::new(&mut g.widths.axis, 0.25..=5.0).text("thickness"));
}

fn style_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let g = graph_mut(app, id);
    section(ui, "Size of the graph");
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut g.width).range(200.0..=2400.0).prefix("width ").suffix(" pt"));
        ui.add(egui::DragValue::new(&mut g.height).range(150.0..=2400.0).prefix("height ").suffix(" pt"));
    });
    ui.label(RichText::new("Tip: drag the handle at the bottom-right corner of the graph.").small().weak());
    section(ui, "Text");
    ui.add(egui::Slider::new(&mut g.font_size, 8.0..=24.0).text("base size"));
    ui.label(RichText::new("Each kind of text can have its own size; empty follows the base size.").small().weak());
    egui::Grid::new("texts").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
        for k in TextKind::ALL {
            let (size, bold) = g.texts.resolve(k, g.font_size);
            ui.label(k.label());
            let spec = g.texts.get_mut(k);
            let mut v = size;
            let r = ui.add(egui::DragValue::new(&mut v).range(6.0..=48.0).speed(0.2).suffix(" pt").fixed_decimals(1));
            if r.changed() {
                spec.size = Some(v);
            }
            ui.horizontal(|ui| {
                let mut b = bold;
                if ui.toggle_value(&mut b, RichText::new("B").strong()).on_hover_text("Bold").changed() {
                    spec.bold = Some(b);
                }
                if (spec.size.is_some() || spec.bold.is_some()) && ui.small_button("↺").on_hover_text("Back to default").clicked() {
                    *spec = Default::default();
                }
            });
            ui.end_row();
        }
    });
    section(ui, "Thickness of each part");
    egui::Grid::new("widths").num_columns(2).show(ui, |ui| {
        for (label, v) in [
            ("Axes", &mut g.widths.axis),
            ("Error bars", &mut g.widths.error_bars),
            ("Bar and box borders", &mut g.widths.bar_border),
            ("Symbol borders", &mut g.widths.symbol_border),
            ("Lines and curves", &mut g.widths.lines),
            ("Significance brackets", &mut g.widths.brackets),
        ] {
            ui.label(label);
            ui.add(egui::DragValue::new(v).range(0.0..=6.0).speed(0.05).suffix(" pt").fixed_decimals(2));
            ui.end_row();
        }
    });
}

fn colors_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let names = crate::colors::series_names(&app.project, id);
    section(ui, "Colors of the data sets");
    if names.is_empty() {
        ui.label(RichText::new("Enter some data first.").weak());
    }
    for (i, n) in names.iter().enumerate() {
        let [r, g, b] = crate::colors::series_color(&app.project, id, i);
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 22.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(r, g, b));
            ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0_f32, crate::theme::pal().border), egui::StrokeKind::Outside);
            let open = resp.clicked() | ui.link(n).clicked();
            if open {
                let mut w = crate::colors::ColorWindow::new(id);
                w.select(i);
                app.colors = Some(w);
            }
        });
    }
    ui.add_space(8.0);
    if ui.add(egui::Button::new(RichText::new("🎨 Open the color picker…").size(14.0)).min_size(Vec2::new(220.0, 30.0))).clicked() {
        app.colors = Some(crate::colors::ColorWindow::new(id));
    }
    section(ui, "Borders of bars and symbols");
    let fills: Vec<[u8; 3]> = (0..names.len()).map(|i| crate::colors::series_color(&app.project, id, i)).collect();
    let g = graph_mut(app, id);
    ui.horizontal(|ui| {
        ui.label("All:");
        if ui.button("Same as fill").clicked() {
            for i in 0..names.len() {
                g.series_style_mut(i).border = None;
            }
        }
        if ui.button("Black").clicked() {
            for i in 0..names.len() {
                g.series_style_mut(i).border = Some([0, 0, 0]);
            }
        }
        if ui.button("White").clicked() {
            for i in 0..names.len() {
                g.series_style_mut(i).border = Some([255, 255, 255]);
            }
        }
    });
    for (i, n) in names.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut c = g.series_style(i).border.unwrap_or(fills[i]);
            if egui::color_picker::color_edit_button_srgb(ui, &mut c).changed() {
                g.series_style_mut(i).border = Some(c);
            }
            ui.label(n);
            if g.series_style(i).border.is_some() && ui.small_button("↺").on_hover_text("Same as the fill color").clicked() {
                g.series_style_mut(i).border = None;
            }
        });
    }
    section(ui, "Quick palettes");
    let user = app.user_palettes.clone();
    let n_series = names.len();
    let g = graph_mut(app, id);
    for (name, cols) in &user {
        ui.horizontal(|ui| {
            if ui.selectable_label(false, name).clicked() && !cols.is_empty() {
                g.colors = (0..n_series.max(1)).map(|i| cols[i % cols.len()]).collect();
            }
            for c in cols.iter().take(6) {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
            }
        });
    }
    for (i, (name, cols)) in hg_plot::PALETTES.iter().enumerate() {
        ui.horizontal(|ui| {
            let on = g.palette == i && g.colors.is_empty();
            if ui.selectable_label(on, *name).clicked() {
                g.palette = i;
                g.colors.clear();
            }
            for c in cols.iter().take(6) {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(c.r, c.g, c.b));
            }
        });
    }
}

fn curves_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let table_id = app.project.graph(id).unwrap().table_id;
    let curves: Vec<(u64, String)> = app
        .project
        .analyses
        .iter()
        .filter(|a| a.table_id == table_id && matches!(a.kind, AnalysisKind::NonlinearFit { .. } | AnalysisKind::LinearRegression))
        .map(|a| (a.id, app.project.analysis_name(a.id)))
        .collect();
    if curves.is_empty() {
        ui.label(RichText::new("No curve fits on this table yet.").weak());
    }
    let g = graph_mut(app, id);
    for (aid, label) in &curves {
        let mut on = g.overlay_analyses.contains(aid);
        if ui.checkbox(&mut on, label).changed() {
            if on {
                g.overlay_analyses.push(*aid);
            } else {
                g.overlay_analyses.retain(|a| a != aid);
            }
        }
    }
    ui.add_space(8.0);
    if ui.button("∑ Fit a curve…").clicked() {
        app.start_analysis();
    }
}

const LABELS: [&str; 5] = ["ns", "*", "**", "***", "****"];

fn significance_panel(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let g = app.project.graph(id).unwrap().clone();
    let t = app.project.table(g.table_id).unwrap().clone();
    let is_grouped = grouped(g.kind);
    // Names of the bars (members) and, for grouped graphs, of the X axis groups.
    let rows: Vec<(usize, String)> = (0..t.used_rows())
        .filter(|&r| (0..t.datasets.len()).any(|d| !t.cell_values(d, r).is_empty()))
        .map(|r| (r, t.row_label(r)))
        .collect();
    let sets: Vec<(usize, String)> = t.nonempty_datasets().into_iter().map(|d| (d, t.datasets[d].title.clone())).collect();
    let (groups, members): (Vec<(usize, String)>, Vec<(usize, String)>) = match (is_grouped, g.transpose) {
        (false, _) => (Vec::new(), sets.clone()),
        (true, false) => (rows.clone(), sets.clone()),
        (true, true) => (sets.clone(), rows.clone()),
    };
    let name_of = |list: &[(usize, String)], i: usize| list.iter().find(|x| x.0 == i).map_or(format!("#{}", i + 1), |x| x.1.clone());
    let analyses: Vec<(u64, String)> = app
        .project
        .analyses
        .iter()
        .filter(|a| {
            a.table_id == g.table_id
                && matches!(
                    a.kind,
                    AnalysisKind::TwoGroups { .. }
                        | AnalysisKind::OneWay { comparisons: Some(_), .. }
                        | AnalysisKind::OneWay { test: hg_core::analysis::OneWayTest::KruskalWallis, .. }
                        | AnalysisKind::TwoWay { comparisons: Some(_), .. }
                )
        })
        .map(|a| (a.id, app.project.analysis_name(a.id)))
        .collect();

    // --- automatic
    egui::Frame::group(ui.style()).fill(crate::theme::pal().panel_alt).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("⚡ From a statistical test").strong().size(14.0));
        ui.label(RichText::new("The brackets follow the results: they update when the data change.").weak().small());
        ui.add_space(4.0);
        let mut auto = g.auto_significance.clone();
        let current = auto.analysis.and_then(|a| analyses.iter().find(|x| x.0 == a)).map_or("None".to_string(), |x| x.1.clone());
        egui::ComboBox::from_id_salt("auto_an").width(240.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut auto.analysis, None, "None");
            for (aid, label) in &analyses {
                ui.selectable_value(&mut auto.analysis, Some(*aid), label);
            }
        });
        if analyses.is_empty() {
            ui.label(RichText::new("No test with pairwise comparisons on this table yet.").weak());
            if ui.button("∑ Run a test…").clicked() {
                app.start_analysis();
            }
        }
        if auto.analysis.is_some() {
            ui.add_space(4.0);
            ui.label("Label");
            segmented(ui, &mut auto.style, &LabelStyle::ALL.map(|s| (s, s.label())));
            ui.checkbox(&mut auto.show_ns, "Also show non-significant comparisons (ns)");
            if let Some(out) = auto.analysis.and_then(|a| app.results(a).ok()) {
                let all: Vec<_> = out
                    .significance
                    .iter()
                    .filter(|s| s.group.is_none() || s.group_is_dataset == g.transpose)
                    .filter(|s| auto.show_ns || s.p < 0.05)
                    .copied()
                    .collect();
                let shown = all.iter().filter(|s| !auto.hidden.contains(&(s.group, s.a, s.b))).count();
                ui.add_space(4.0);
                let open_id = egui::Id::new(("comparisons_window", id));
                let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::image_and_text(
                            crate::icons::image("list-checks", 16.0, crate::theme::pal().text),
                            "Choose comparisons…",
                        ))
                        .clicked()
                    {
                        open = true;
                    }
                    ui.label(RichText::new(format!("{shown} of {} shown", all.len())).small().weak());
                });
                let filter_id = egui::Id::new(("comparisons_filter", id));
                let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
                egui::Window::new("Comparisons to show")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .default_size([420.0, 460.0])
                    .show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            ui.add(crate::icons::image("search", 16.0, crate::theme::pal().weak));
                            ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Filter by group name").desired_width(f32::INFINITY));
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("Show all").clicked() {
                                auto.hidden.clear();
                            }
                            if ui.small_button("Hide all").clicked() {
                                auto.hidden = all.iter().map(|s| (s.group, s.a, s.b)).collect();
                            }
                            if ui.small_button("Only P < 0.05").clicked() {
                                auto.hidden = all.iter().filter(|s| s.p >= 0.05).map(|s| (s.group, s.a, s.b)).collect();
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            let needle = filter.to_lowercase();
                            for s in &all {
                                let group = s.group.map_or(String::new(), |gr| format!("{}: ", name_of(&groups, gr)));
                                let text = format!("{group}{} vs. {}", name_of(&members, s.a), name_of(&members, s.b));
                                if !needle.is_empty() && !text.to_lowercase().contains(&needle) {
                                    continue;
                                }
                                let key = (s.group, s.a, s.b);
                                let mut on = !auto.hidden.contains(&key);
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut on, text).changed() {
                                        if on {
                                            auto.hidden.retain(|k| *k != key);
                                        } else {
                                            auto.hidden.push(key);
                                        }
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let color = if s.p < 0.05 { crate::theme::pal().star } else { crate::theme::pal().weak };
                                        ui.label(RichText::new(LabelStyle::Both.format(s.p)).color(color));
                                    });
                                });
                            }
                        });
                    });
                ui.data_mut(|d| {
                    d.insert_temp(open_id, open);
                    d.insert_temp(filter_id, filter);
                });
            }
        }
        if auto != g.auto_significance {
            graph_mut(app, id).auto_significance = auto;
        }
    });

    ui.add_space(8.0);
    // --- bracket style
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Bracket style").strong().size(14.0));
        let g = graph_mut(app, id);
        ui.add(egui::Slider::new(&mut g.widths.brackets, 0.25..=5.0).text("thickness"));
        ui.add(egui::Slider::new(&mut g.bracket_tick, 0.0..=3.0).text("end length"));
        ui.add(egui::Slider::new(&mut g.bracket_gap, 0.0..=4.0).text("gap above data"));
        ui.horizontal(|ui| {
            ui.label("Label size");
            let (size, _) = g.texts.resolve(TextKind::Brackets, g.font_size);
            let mut v = size;
            if ui.add(egui::DragValue::new(&mut v).range(6.0..=40.0).speed(0.2).suffix(" pt")).changed() {
                g.texts.brackets.size = Some(v);
            }
        });
        if !g.bracket_offsets.is_empty() && ui.small_button("Reset dragged positions").clicked() {
            g.bracket_offsets.clear();
        }
    });
    ui.add_space(8.0);
    // --- manual
    let mut brackets = g.brackets.clone();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("✋ Your own brackets").strong().size(14.0));
        ui.label(RichText::new("Pick two bars and the text to write above them.").weak().small());
        let mut remove = None;
        for (i, b) in brackets.iter_mut().enumerate() {
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(crate::theme::pal().panel)
                .corner_radius(6.0)
                .inner_margin(6.0)
                .stroke(egui::Stroke::new(1.0_f32, crate::theme::pal().border))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&b.label).strong().size(16.0).color(crate::theme::accent()));
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 34.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.small_button("🗑").on_hover_text("Remove this bracket").clicked() {
                                    remove = Some(i);
                                }
                            },
                        );
                    });
                    if is_grouped {
                        ui.horizontal(|ui| {
                            ui.label("In");
                            pick(ui, ("bg", i), &groups, b.row.get_or_insert(groups.first().map_or(0, |x| x.0)));
                        });
                    }
                    ui.horizontal(|ui| {
                        pick(ui, ("ba", i), &members, &mut b.a);
                        ui.label("vs.");
                        pick(ui, ("bb", i), &members, &mut b.b);
                    });
                    ui.horizontal(|ui| {
                        for l in LABELS {
                            if ui.selectable_label(b.label == l, RichText::new(l).size(14.0)).clicked() {
                                b.label = l.to_string();
                            }
                        }
                        ui.add(egui::TextEdit::singleline(&mut b.label).desired_width(70.0).hint_text("text"));
                    });
                });
        }
        if let Some(i) = remove {
            brackets.remove(i);
        }
        ui.add_space(6.0);
        if ui.add(egui::Button::new(RichText::new("➕ Add a bracket").size(14.0)).min_size(Vec2::new(160.0, 28.0))).clicked() {
            let a = members.first().map_or(0, |m| m.0);
            let b = members.get(1).map_or(a, |m| m.0);
            let row = is_grouped.then(|| groups.first().map_or(0, |x| x.0));
            brackets.push(Bracket { row, a, b, label: "*".into() });
        }
    });
    if brackets != g.brackets {
        graph_mut(app, id).brackets = brackets;
    }
}

fn pick(ui: &mut egui::Ui, salt: impl std::hash::Hash, list: &[(usize, String)], v: &mut usize) {
    let current = list.iter().find(|x| x.0 == *v).map_or("—".to_string(), |x| x.1.clone());
    egui::ComboBox::from_id_salt(salt).width(100.0).selected_text(current).show_ui(ui, |ui| {
        for (i, name) in list {
            ui.selectable_value(v, *i, name);
        }
    });
}

/// Optional number field ("auto" when empty). The text is buffered while
/// focused so partial input such as "-" or "1." can be typed.
fn opt_value(ui: &mut egui::Ui, v: &mut Option<f64>, salt: &str) {
    let id = ui.make_persistent_id(salt);
    let mut s = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| v.map(|x| format!("{x}")).unwrap_or_default());
    let r = ui.add(egui::TextEdit::singleline(&mut s).hint_text("auto").desired_width(64.0));
    if r.changed() {
        *v = hg_core::table::parse_number(&s);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, s));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}
