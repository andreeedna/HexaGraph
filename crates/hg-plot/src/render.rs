//! Lays out a graph into a `Scene`.

use crate::axis::{nice_range, tick_label, Axis};
use crate::palette_color;
use crate::scene::{text_width, Anchor, Color, Element, Prim, Scene};
use hg_core::graph::{auto_brackets, Bracket, Fill, MarkerShape, TextKind};
use hg_core::{DataTable, ErrorBar, Graph, GraphKind, Project, Scale};
use hg_stats::descriptive::describe;
use hg_stats::survival;

const AXIS: Color = Color::rgb(0x22, 0x22, 0x22);

/// Center and error bar extent for a set of replicates.
pub fn summarize(values: &[f64], eb: ErrorBar) -> Option<(f64, f64, f64)> {
    let d = describe(values).ok()?;
    let spread = |s: f64| if s.is_finite() { s } else { 0.0 };
    Some(match eb {
        ErrorBar::Sd => (d.mean, d.mean - spread(d.sd), d.mean + spread(d.sd)),
        ErrorBar::Sem => (d.mean, d.mean - spread(d.sem), d.mean + spread(d.sem)),
        ErrorBar::Ci95 => d.mean_ci95.map_or((d.mean, d.mean, d.mean), |c| (d.mean, c.low, c.high)),
        ErrorBar::Range => (d.median, d.min, d.max),
        ErrorBar::None => (d.mean, d.mean, d.mean),
    })
}

struct Frame {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

struct Ctx<'a> {
    g: &'a Graph,
    scene: Scene,
    font: f32,
    /// ANOVA effects of the linked analysis (factor, P value).
    effects: Vec<(String, f64)>,
}

impl Ctx<'_> {
    /// The tested ANOVA effects, one line each, in the top-left corner of the plot.
    fn effects_box(&mut self, fr: &Frame) {
        let size = self.ts(TextKind::Brackets).0 * 0.85;
        let effects = std::mem::take(&mut self.effects);
        for (k, (name, p)) in effects.iter().enumerate() {
            let value = if *p < 0.0001 { "< 0.0001".to_string() } else { format!("= {:.4}", p) };
            let text = format!("{name}: P {value} ({})", hg_stats::p_summary(*p));
            let pos = (fr.left + 10.0, fr.top + size * (1.0 + 1.35 * k as f32));
            self.scene.prims.push(Prim::Text { pos, text, size, color: AXIS, anchor: Anchor::Start, angle: 0.0, bold: false });
        }
    }

    fn color(&self, i: usize) -> Color {
        match self.g.colors.get(i) {
            Some(&[r, g, b]) => Color::rgb(r, g, b),
            None => palette_color(self.g.palette, i),
        }
    }

    /// Border color of a data set's bars and symbols (`default` unless chosen).
    fn border(&self, i: usize, default: Color) -> Color {
        match self.g.series_style(i).border {
            Some([r, g, b]) => Color::rgb(r, g, b),
            None => default,
        }
    }

    /// Size and weight of a kind of text.
    fn ts(&self, k: TextKind) -> (f32, bool) {
        self.g.texts.resolve(k, self.font)
    }

    fn text(&mut self, pos: (f32, f32), text: impl Into<String>, k: TextKind, anchor: Anchor, angle: f32) {
        let (size, bold) = self.ts(k);
        self.scene.prims.push(Prim::Text { pos, text: text.into(), size, color: AXIS, anchor, angle, bold });
    }

    /// A data symbol; `filled` is the default of the graph, the series style
    /// can override shape and fill.
    fn marker(&mut self, c: (f32, f32), series: usize, filled: bool) {
        let col = self.color(series);
        let style = self.g.series_style(series);
        let filled = match style.fill {
            Fill::Auto => filled,
            Fill::Filled => true,
            Fill::Hollow => false,
        };
        self.scene.prims.push(Prim::Marker {
            center: c,
            shape: style.shape.unwrap_or(MarkerShape::ALL[series % 4]),
            size: self.g.marker_size,
            fill: if filled { col } else { Color::WHITE },
            stroke: self.border(series, col),
            stroke_width: self.g.widths.symbol_border,
        });
    }

    fn error_bar(&mut self, x: f32, lo: f32, hi: f32, cap: f32, color: Color) {
        let w = self.g.widths.error_bars;
        if (hi - lo).abs() < 0.5 {
            return;
        }
        let cap = cap * self.g.cap_width;
        self.scene.line((x, lo), (x, hi), color, w);
        self.scene.line((x - cap, lo), (x + cap, lo), color, w);
        self.scene.line((x - cap, hi), (x + cap, hi), color, w);
    }

    /// Error bar for a bar graph: only the upper half when asked to.
    fn bar_error(&mut self, x: f32, center: f32, lo: f32, hi: f32, cap: f32) {
        if self.g.error_up_only {
            // In pixels "up" is the smaller y.
            self.error_bar(x, center, lo.min(hi), cap, AXIS);
        } else {
            self.error_bar(x, lo, hi, cap, AXIS);
        }
    }

    fn title(&mut self, frame: &Frame) {
        if !self.g.title.is_empty() {
            let t = self.g.title.clone();
            let o = self.g.offsets.title;
            let (size, _) = self.ts(TextKind::Title);
            let pos = ((frame.left + frame.right) / 2.0 + o.0, size * 1.3 + o.1);
            let w = text_width(&t, size) / 2.0;
            self.scene.hit(Element::Title, (pos.0 - w, pos.1 - size), (pos.0 + w, pos.1 + size * 0.3));
            self.text(pos, t, TextKind::Title, Anchor::Middle, 0.0);
        }
    }

    fn y_axis(&mut self, ax: &Axis, frame: &Frame, label: &str) {
        let w = self.g.widths.axis;
        let x = frame.left - 6.0;
        let (tick, _) = self.ts(TextKind::TickLabels);
        self.scene.line((x, frame.top), (x, frame.bottom), AXIS, w);
        let angle = self.g.y_tick_angle;
        let widest = ax.ticks().iter().map(|v| text_width(&tick_label(*v), tick)).fold(0.0, f32::max);
        // Horizontal extent of the numbers once rotated.
        let a = angle.to_radians();
        let tick_w = widest * a.cos().abs() + tick * a.sin().abs();
        self.scene.hit(Element::YAxis, (x - 10.0 - tick_w, frame.top - tick * 0.5), (x + 4.0, frame.bottom + tick * 0.5));
        for v in ax.ticks() {
            let y = ax.map(v);
            self.scene.line((x - 5.0, y), (x, y), AXIS, w);
            if angle >= 60.0 {
                self.text((x - 8.0 - tick * 0.2, y), tick_label(v), TextKind::TickLabels, Anchor::Middle, angle);
            } else {
                self.text((x - 8.0, y + tick * 0.35 * a.cos()), tick_label(v), TextKind::TickLabels, Anchor::End, angle);
            }
        }
        if ax.scale == Scale::Log10 {
            // Minor ticks for each decade.
            let ticks = ax.ticks();
            for d in ticks.windows(2) {
                for k in 2..10 {
                    let y = ax.map(d[0] * k as f64);
                    self.scene.line((x - 3.0, y), (x, y), AXIS, w * 0.7);
                }
            }
        }
        if !label.is_empty() {
            let o = self.g.offsets.y_title;
            let (size, _) = self.ts(TextKind::AxisTitles);
            let pos = (x - 18.0 - tick_w * 1.15 - size * 0.25 + o.0, (frame.top + frame.bottom) / 2.0 + o.1);
            let half = text_width(label, size) / 2.0;
            self.scene.hit(Element::YTitle, (pos.0 - size, pos.1 - half), (pos.0 + size * 0.3, pos.1 + half));
            self.text(pos, label, TextKind::AxisTitles, Anchor::Middle, 90.0);
        }
    }

    fn x_title(&mut self, frame: &Frame, y: f32, label: &str) {
        let o = self.g.offsets.x_title;
        let (size, _) = self.ts(TextKind::AxisTitles);
        let pos = ((frame.left + frame.right) / 2.0 + o.0, y + size + o.1);
        let half = text_width(label, size) / 2.0;
        self.scene.hit(Element::XTitle, (pos.0 - half, pos.1 - size), (pos.0 + half, pos.1 + size * 0.3));
        self.text(pos, label, TextKind::AxisTitles, Anchor::Middle, 0.0);
    }

    fn x_axis_numeric(&mut self, ax: &Axis, frame: &Frame, label: &str) {
        let w = self.g.widths.axis;
        let y = frame.bottom + 6.0;
        let (tick, _) = self.ts(TextKind::TickLabels);
        self.scene.line((frame.left, y), (frame.right, y), AXIS, w);
        self.scene.hit(Element::XAxis, (frame.left - 4.0, y - 4.0), (frame.right + 4.0, y + 9.0 + tick));
        let angle = self.g.x_tick_angle.unwrap_or(0.0);
        let widest = ax.ticks().iter().map(|v| text_width(&tick_label(*v), tick)).fold(0.0, f32::max);
        for v in ax.ticks() {
            let x = ax.map(v);
            self.scene.line((x, y), (x, y + 5.0), AXIS, w);
            if angle > 0.0 {
                let a = angle.to_radians();
                self.text(
                    (x + tick * 0.35 * a.sin(), y + 9.0 + tick * 0.4 * a.cos()),
                    tick_label(v),
                    TextKind::TickLabels,
                    Anchor::End,
                    angle,
                );
            } else {
                self.text((x, y + 7.0 + tick), tick_label(v), TextKind::TickLabels, Anchor::Middle, 0.0);
            }
        }
        if !label.is_empty() {
            let a = angle.to_radians();
            let below = if angle > 0.0 { widest * a.sin() + tick * a.cos() + 6.0 } else { 1.2 * tick };
            self.x_title(frame, y + 12.0 + below, label);
        }
    }

    /// Category axis; labels are centered on `centers`.
    fn x_axis_categories(&mut self, centers: &[f32], labels: &[String], frame: &Frame, angle: f32) {
        let w = self.g.widths.axis;
        let y = frame.bottom + 6.0;
        let (cat, _) = self.ts(TextKind::Categories);
        self.scene.line((frame.left, y), (frame.right, y), AXIS, w);
        self.scene.hit(Element::XAxis, (frame.left - 4.0, y - 4.0), (frame.right + 4.0, y + 9.0 + cat));
        let slot = if centers.len() > 1 { centers[1] - centers[0] } else { frame.right - frame.left };
        for (c, l) in centers.iter().zip(labels) {
            self.scene.line((*c, y), (*c, y + 5.0), AXIS, w);
            if angle > 0.0 {
                let a = angle.to_radians();
                self.text((*c + cat * 0.35 * a.sin(), y + 9.0 + cat * 0.4 * a.cos()), l.clone(), TextKind::Categories, Anchor::End, angle);
            } else {
                for (k, line) in wrap(l, slot * 0.95, cat).0.into_iter().enumerate() {
                    self.text((*c, y + 7.0 + cat * (1.0 + 1.2 * k as f32)), line, TextKind::Categories, Anchor::Middle, 0.0);
                }
            }
        }
        if !self.g.x_label.is_empty() {
            let t = self.g.x_label.clone();
            let max_w = labels.iter().map(|l| text_width(l, cat)).fold(0.0, f32::max);
            let lines = labels.iter().map(|l| wrap(l, slot * 0.95, cat).0.len()).max().unwrap_or(1) as f32;
            let a = angle.to_radians();
            let off = if angle > 0.0 { max_w * a.sin() + cat * a.cos() + 14.0 } else { (0.9 + 1.2 * lines) * cat + 8.0 };
            self.x_title(frame, y + off, &t);
        }
    }

    /// Legend texts: the user's own where typed, else the data set titles.
    fn legend_names(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| match self.g.legend_labels.get(i) {
                Some(l) if !l.trim().is_empty() => l.clone(),
                _ => n.clone(),
            })
            .collect()
    }

    fn legend(&mut self, names: &[String], x: f32, y: f32, swatch: bool) {
        let names = &self.legend_names(names);
        let (size, _) = self.ts(TextKind::Legend);
        let (x, y) = (x + self.g.offsets.legend.0, y + self.g.offsets.legend.1);
        let w = names.iter().map(|n| text_width(n, size)).fold(0.0, f32::max) + size * 1.8;
        self.scene.hit(Element::Legend, (x - 6.0, y - size * 1.2), (x + w + 4.0, y + (names.len() as f32 - 1.0) * size * 1.6 + size * 0.5));
        for (i, n) in names.iter().enumerate() {
            let yy = y + i as f32 * size * 1.6;
            let c = self.color(i);
            if swatch {
                // Bars: the swatch looks like the bars (opacity and border).
                let bars = matches!(
                    self.g.kind,
                    GraphKind::Bar | GraphKind::GroupedBar | GraphKind::StackedBar | GraphKind::Histogram
                );
                let fill = if bars { c.with_alpha((self.g.bar_opacity.clamp(0.0, 1.0) * 255.0) as u8) } else { c };
                let stroke = if bars { self.border(i, c) } else { AXIS };
                self.scene.prims.push(Prim::Rect {
                    min: (x, yy - size * 0.8),
                    max: (x + size, yy + size * 0.2),
                    fill,
                    stroke: Some((stroke, 0.8_f32.max(self.g.widths.bar_border.min(2.0)))),
                });
            } else {
                self.scene.line((x - 2.0, yy - size * 0.3), (x + size + 2.0, yy - size * 0.3), c, self.g.widths.lines);
                self.marker((x + size / 2.0, yy - size * 0.3), i, true);
            }
            self.text((x + size * 1.6, yy), n.clone(), TextKind::Legend, Anchor::Start, 0.0);
        }
    }

    fn legend_width(&self, names: &[String]) -> f32 {
        if !self.g.show_legend || names.len() < 2 {
            return 0.0;
        }
        let (size, _) = self.ts(TextKind::Legend);
        self.legend_names(names).iter().map(|n| text_width(n, size)).fold(0.0, f32::max) + size * 2.6 + 16.0
    }

    /// Vertical space taken by one level of significance brackets.
    fn bracket_step(&self) -> f32 {
        self.ts(TextKind::Brackets).0 * 2.1
    }
}

fn y_axis_from(g: &Graph, lo: f64, hi: f64, include_zero: bool, frame: &Frame) -> Axis {
    let (mut a, mut b) = nice_range(lo, hi, g.y_scale, include_zero && g.y_scale == Scale::Linear);
    if let Some(v) = g.y_min {
        a = v;
    }
    if let Some(v) = g.y_max {
        b = v;
    }
    Axis { min: a, max: b, scale: g.y_scale, p0: frame.bottom, p1: frame.top }
}

fn bounds(values: impl IntoIterator<Item = f64>) -> Option<(f64, f64)> {
    let mut it = values.into_iter().filter(|v| v.is_finite());
    let first = it.next()?;
    Some(it.fold((first, first), |(a, b), v| (a.min(v), b.max(v))))
}

/// Greedy word wrap of category labels into lines no wider than `width`.
/// Returns the lines and whether some single word still does not fit.
fn wrap(label: &str, width: f32, font: f32) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    for word in label.split_whitespace() {
        match lines.last_mut() {
            Some(l) if text_width(&format!("{l} {word}"), font) <= width => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    let overflow = lines.iter().any(|l| text_width(l, font) > width);
    (lines, overflow)
}

/// Decides how category labels are drawn: wrapped lines, or rotated text
/// (the user's angle, or vertical when a word is too wide).
/// Returns (angle in degrees, extra bottom margin).
fn category_layout(labels: &[String], slot: f32, font: f32, forced: Option<f32>) -> (f32, f32) {
    let max = labels.iter().map(|l| text_width(l, font)).fold(0.0, f32::max);
    let angled = |deg: f32| {
        let a = deg.to_radians();
        (deg, (max * a.sin() + font * a.cos() - font).max(0.0))
    };
    if let Some(deg) = forced.filter(|d| *d > 0.0) {
        return angled(deg);
    }
    let wrapped: Vec<(Vec<String>, bool)> = labels.iter().map(|l| wrap(l, slot * 0.95, font)).collect();
    if forced.is_none() && wrapped.iter().any(|w| w.1) {
        angled(90.0)
    } else {
        let lines = wrapped.iter().map(|w| w.0.len()).max().unwrap_or(1);
        (0.0, (lines.saturating_sub(1)) as f32 * font * 1.2)
    }
}

/// Spreads points horizontally so markers do not overlap (simple beeswarm).
fn beeswarm(ys: &[f32], d: f32, max_off: f32) -> Vec<f32> {
    let mut order: Vec<usize> = (0..ys.len()).collect();
    order.sort_by(|&a, &b| ys[a].total_cmp(&ys[b]));
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut out = vec![0.0; ys.len()];
    for i in order {
        let y = ys[i];
        let mut k: usize = 0;
        let off = loop {
            let cand = if k == 0 {
                0.0
            } else if k % 2 == 1 {
                d * k.div_ceil(2) as f32
            } else {
                -d * (k / 2) as f32
            };
            let free = placed.iter().all(|&(px, py)| ((px - cand).powi(2) + (py - y).powi(2)).sqrt() >= d * 0.95);
            if free || cand.abs() > max_off {
                break cand.clamp(-max_off, max_off);
            }
            k += 1;
        };
        placed.push((off, y));
        out[i] = off;
    }
    out
}

/// Brackets typed by the user plus those that follow the linked analysis.
pub fn graph_brackets(project: &Project, g: &Graph) -> Vec<Bracket> {
    let mut v = g.brackets.clone();
    if let Some(out) = g
        .auto_significance
        .analysis
        .filter(|a| project.analysis(*a).is_some_and(|a| a.table_id == g.table_id))
        .and_then(|a| project.run_analysis(a).ok())
    {
        v.extend(auto_brackets(&out, &g.auto_significance, g.transpose));
    }
    v
}

pub fn render(project: &Project, g: &Graph) -> Result<Scene, String> {
    let table = project.table(g.table_id).ok_or("the data table for this graph was deleted")?;
    // Excluded values are not drawn.
    let view = table.analysis_view();
    let t: &DataTable = &view;
    let effects = g
        .auto_significance
        .analysis
        .filter(|a| project.analysis(*a).is_some_and(|a| a.table_id == g.table_id))
        .and_then(|a| project.run_analysis(a).ok())
        .map(|o| o.effects)
        .unwrap_or_default();
    let mut ctx = Ctx { g, scene: Scene::new(g.width, g.height), font: g.font_size, effects };
    match g.kind {
        GraphKind::Pie | GraphKind::Donut => pie(&mut ctx, t),
        GraphKind::Histogram => histogram(&mut ctx, t),
        GraphKind::QqPlot => qq_plot(&mut ctx, t),
        GraphKind::Heatmap => heatmap(&mut ctx, t),
        GraphKind::Survival => survival_curve(&mut ctx, t),
        GraphKind::Xy => xy(&mut ctx, project, t, &graph_brackets(project, g)),
        GraphKind::GroupedBar | GraphKind::StackedBar | GraphKind::GroupedScatter | GraphKind::GroupedBox | GraphKind::GroupedLine => {
            grouped(&mut ctx, t, &graph_brackets(project, g))
        }
        _ => columns(&mut ctx, t, &graph_brackets(project, g)),
    }?;
    Ok(ctx.scene)
}

fn frame(ctx: &Ctx, legend_w: f32, bottom_extra: f32) -> Frame {
    let f = ctx.font;
    let (title, _) = ctx.ts(TextKind::Title);
    let (axis_title, _) = ctx.ts(TextKind::AxisTitles);
    let (tick, _) = ctx.ts(TextKind::TickLabels);
    let top = if ctx.g.title.is_empty() { f * 1.2 } else { title * 2.6 };
    let y_numbers = if ctx.g.y_tick_angle >= 60.0 { tick * 1.6 } else { tick * 3.6 };
    let left = y_numbers + 18.0 + if ctx.g.y_label.is_empty() { 0.0 } else { axis_title * 1.7 };
    let x_label = if ctx.g.x_label.is_empty() { 0.0 } else { axis_title * 1.8 };
    Frame { left, right: ctx.g.width - 12.0 - legend_w, top, bottom: ctx.g.height - (tick * 2.6 + 10.0 + x_label + bottom_extra) }
}

/// Extra room below numeric X axes whose numbers are rotated.
fn numeric_x_extra(ctx: &Ctx) -> f32 {
    match ctx.g.x_tick_angle {
        Some(a) if a > 0.0 => ctx.ts(TextKind::TickLabels).0 * 3.2 * a.to_radians().sin(),
        _ => 0.0,
    }
}

fn columns(ctx: &mut Ctx, t: &DataTable, brackets: &[Bracket]) -> Result<(), String> {
    let g = ctx.g;
    let groups = t.nonempty_datasets();
    if groups.is_empty() {
        return Err("enter some data to see the graph".into());
    }
    let data: Vec<Vec<f64>> = groups.iter().map(|&d| t.column_values(d)).collect();
    let labels: Vec<String> = groups.iter().map(|&d| t.datasets[d].title.clone()).collect();
    let mut all: Vec<f64> = data.iter().flatten().copied().collect();
    for v in &data {
        if let Some((_, lo, hi)) = summarize(v, g.error_bar) {
            all.extend([lo, hi]);
        }
    }
    let (lo, hi) = bounds(all).unwrap();
    // Category label layout.
    let probe = frame(ctx, 0.0, 0.0);
    let slot = (probe.right - probe.left) / groups.len() as f32;
    let (angle, extra) = category_layout(&labels, slot, ctx.ts(TextKind::Categories).0, g.x_tick_angle);
    let mut fr = frame(ctx, 0.0, extra);
    let spans: Vec<(usize, usize, &str, usize)> = brackets
        .iter()
        .enumerate()
        .filter(|(_, b)| b.row.is_none())
        .filter_map(|(k, b)| Some((groups.iter().position(|&d| d == b.a)?, groups.iter().position(|&d| d == b.b)?, b.label.as_str(), k)))
        .collect();
    let levels = bracket_levels(&spans.iter().map(|s| (s.0.min(s.1) as f32, s.0.max(s.1) as f32)).collect::<Vec<_>>());
    fr.top += bracket_space(ctx, &levels);
    let include_zero = matches!(g.kind, GraphKind::Bar);
    let ax = y_axis_from(g, lo, hi, include_zero, &fr);
    let centers: Vec<f32> = (0..groups.len()).map(|i| fr.left + slot * (i as f32 + 0.5)).collect();
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, &g.y_label);
    ctx.x_axis_categories(&centers, &labels, &fr, angle);
    let (lw, bw_line, eb) = (g.widths.lines, g.widths.bar_border, g.widths.error_bars);
    let half = slot * g.bar_width.clamp(0.1, 1.0) * 0.43;
    if g.kind == GraphKind::LineMean {
        let pts: Vec<(f32, f32)> =
            data.iter().zip(&centers).filter_map(|(v, c)| summarize(v, g.error_bar).map(|s| (*c, ax.map(s.0)))).collect();
        let col = ctx.color(0);
        ctx.scene.prims.push(Prim::Polyline { points: pts, color: col, width: lw, dashed: false });
    }
    for (i, v) in data.iter().enumerate() {
        let (lo_v, hi_v) =
            bounds(v.iter().copied().chain(summarize(v, g.error_bar).map(|s| [s.1, s.2]).into_iter().flatten())).unwrap_or((0.0, 0.0));
        let base = if g.kind == GraphKind::Bar { ax.map(0f64.clamp(ax.min, ax.max)) } else { ax.map(lo_v) };
        ctx.scene.hit(Element::Series(i), (centers[i] - half, ax.map(hi_v) - 4.0), (centers[i] + half, base.max(ax.map(lo_v)) + 4.0));
    }
    if g.kind == GraphKind::BeforeAfter {
        let rows = t.complete_rows(&groups);
        for r in &rows {
            let pts: Vec<(f32, f32)> = r.iter().zip(&centers).map(|(v, c)| (*c, ax.map(*v))).collect();
            ctx.scene.prims.push(Prim::Polyline { points: pts.clone(), color: Color::rgb(0x88, 0x88, 0x88), width: lw, dashed: false });
            for (i, p) in pts.iter().enumerate() {
                ctx.marker(*p, i, true);
            }
        }
    }
    for (i, v) in data.iter().enumerate() {
        let c = centers[i];
        let col = ctx.color(i);
        let Some((center, elo, ehi)) = summarize(v, g.error_bar) else { continue };
        match g.kind {
            GraphKind::Bar => {
                let base = ax.map(if g.y_scale == Scale::Log10 { ax.min } else { 0f64.clamp(ax.min, ax.max) });
                ctx.scene.prims.push(Prim::Rect {
                    min: (c - half, ax.map(center).min(base)),
                    max: (c + half, ax.map(center).max(base)),
                    fill: col.with_alpha((g.bar_opacity.clamp(0.0, 1.0) * 255.0) as u8),
                    stroke: Some((ctx.border(i, col), bw_line)),
                });
                ctx.bar_error(c, ax.map(center), ax.map(elo), ax.map(ehi), half * 0.45);
            }
            GraphKind::BoxWhisker | GraphKind::Violin => {
                let d = describe(v).unwrap();
                if g.kind == GraphKind::Violin && v.len() >= 3 && d.sd > 0.0 {
                    let h = 1.06 * d.sd * (v.len() as f64).powf(-0.2);
                    let steps = 64;
                    let ys: Vec<f64> = (0..=steps).map(|k| d.min + (d.max - d.min) * k as f64 / steps as f64).collect();
                    let dens: Vec<f64> = ys.iter().map(|y| v.iter().map(|x| (-0.5 * ((y - x) / h).powi(2)).exp()).sum()).collect();
                    let maxd = dens.iter().cloned().fold(0.0, f64::max);
                    let mut pts: Vec<(f32, f32)> =
                        ys.iter().zip(&dens).map(|(y, de)| (c + half * (de / maxd) as f32, ax.map(*y))).collect();
                    pts.extend(ys.iter().zip(&dens).rev().map(|(y, de)| (c - half * (de / maxd) as f32, ax.map(*y))));
                    ctx.scene.prims.push(Prim::Polygon { points: pts, fill: col.with_alpha(90), stroke: Some((ctx.border(i, col), bw_line)) });
                    for (q, dash) in [(d.q1, true), (d.median, false), (d.q3, true)] {
                        ctx.scene.prims.push(Prim::Line {
                            from: (c - half * 0.5, ax.map(q)),
                            to: (c + half * 0.5, ax.map(q)),
                            color: AXIS,
                            width: bw_line,
                            dashed: dash,
                        });
                    }
                } else {
                    box_plot(ctx, c, half * 0.8, v, &ax, col, i);
                }
            }
            GraphKind::MeanError | GraphKind::LineMean => {
                ctx.error_bar(c, ax.map(elo), ax.map(ehi), half * 0.35, col);
                ctx.marker((c, ax.map(center)), i, true);
            }
            GraphKind::BeforeAfter => {}
            _ => {
                // Scatter dot plot: mean line plus error bars over the points.
                ctx.scene.line((c - half * 0.7, ax.map(center)), (c + half * 0.7, ax.map(center)), AXIS, eb * 1.6);
                ctx.error_bar(c, ax.map(elo), ax.map(ehi), half * 0.35, AXIS);
            }
        }
        // Before-after graphs always show their points (with the lines).
        let points_wanted = g.kind != GraphKind::BeforeAfter && g.show_points;
        if points_wanted {
            let ys: Vec<f32> = v.iter().map(|y| ax.map(*y)).collect();
            let offs = beeswarm(&ys, g.marker_size * 2.1, half * 0.95);
            for (y, o) in ys.iter().zip(offs) {
                ctx.marker((c + o, *y), i, g.kind == GraphKind::ScatterDot);
            }
        }
    }
    let placed: Vec<(f32, f32, &str, usize)> = spans.iter().map(|&(a, b, l, k)| (centers[a], centers[b], l, k)).collect();
    draw_brackets(ctx, brackets, &placed, &levels, fr.top);
    Ok(())
}

/// Assigns stacking levels to bracket spans (x1 <= x2) so that brackets on
/// the same level never overlap; narrow brackets go lowest.
fn bracket_levels(spans: &[(f32, f32)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|&i, &j| (spans[i].1 - spans[i].0).total_cmp(&(spans[j].1 - spans[j].0)));
    let mut levels = vec![0; spans.len()];
    let mut taken: Vec<Vec<(f32, f32)>> = Vec::new();
    for i in order {
        let (a, b) = spans[i];
        let lvl =
            (0..).find(|&l| taken.get(l).is_none_or(|v: &Vec<(f32, f32)>| v.iter().all(|&(x, y)| b < x - 0.01 || a > y + 0.01))).unwrap();
        if taken.len() <= lvl {
            taken.resize(lvl + 1, Vec::new());
        }
        taken[lvl].push((a, b));
        levels[i] = lvl;
    }
    levels
}

/// Room needed above the plot for the stacked brackets.
fn bracket_space(ctx: &Ctx, levels: &[usize]) -> f32 {
    match levels.iter().max() {
        Some(&top) => (top + 1) as f32 * ctx.bracket_step() + ctx.ts(TextKind::Brackets).0 * 0.6 * ctx.g.bracket_gap,
        None => 0.0,
    }
}

/// Draws brackets (x1, x2, label, index into `brackets`) stacked upwards
/// from `base` (the top of the plot), with a gap so that neither the
/// bracket nor its label touch the data.
fn draw_brackets(ctx: &mut Ctx, brackets: &[Bracket], placed: &[(f32, f32, &str, usize)], levels: &[usize], base: f32) {
    let lw = ctx.g.widths.brackets;
    let (bs, _) = ctx.ts(TextKind::Brackets);
    let step = ctx.bracket_step();
    let tick = (bs * 0.45).max(4.0) * ctx.g.bracket_tick;
    for (&(x1, x2, label, k), &lvl) in placed.iter().zip(levels) {
        let dy = brackets.get(k).map_or(0.0, |b| ctx.g.bracket_offset((b.row, b.a, b.b)));
        let y = base - bs * 0.6 * ctx.g.bracket_gap - tick.max(2.0) - lvl as f32 * step + dy;
        let (x1, x2) = (x1.min(x2), x1.max(x2));
        ctx.scene.hit(Element::Bracket(k), (x1 - 3.0, y - bs * 1.4), (x2 + 3.0, y + tick + 2.0));
        ctx.scene.line((x1, y + tick), (x1, y), AXIS, lw);
        ctx.scene.line((x1, y), (x2, y), AXIS, lw);
        ctx.scene.line((x2, y), (x2, y + tick), AXIS, lw);
        ctx.text(((x1 + x2) / 2.0, y - bs * 0.4), label, TextKind::Brackets, Anchor::Middle, 0.0);
    }
}

fn grouped(ctx: &mut Ctx, t: &DataTable, brackets: &[Bracket]) -> Result<(), String> {
    let g = ctx.g;
    let rows: Vec<usize> = (0..t.used_rows()).filter(|&r| (0..t.datasets.len()).any(|d| !t.cell_values(d, r).is_empty())).collect();
    if rows.is_empty() {
        return Err("enter some data to see the graph".into());
    }
    let sets: Vec<usize> = (0..t.datasets.len()).filter(|&d| rows.iter().any(|&r| !t.cell_values(d, r).is_empty())).collect();
    // Categories go on the X axis, series are the bars inside each category.
    let (cats, series, cat_labels, names): (Vec<usize>, Vec<usize>, Vec<String>, Vec<String>) = if g.transpose {
        (
            sets.clone(),
            rows.clone(),
            sets.iter().map(|&d| t.datasets[d].title.clone()).collect(),
            rows.iter().map(|&r| t.row_label(r)).collect(),
        )
    } else {
        (
            rows.clone(),
            sets.clone(),
            rows.iter().map(|&r| t.row_label(r)).collect(),
            sets.iter().map(|&d| t.datasets[d].title.clone()).collect(),
        )
    };
    let values = |ci: usize, si: usize| if g.transpose { t.cell_values(cats[ci], series[si]) } else { t.cell_values(series[si], cats[ci]) };
    let ns = series.len();
    let cell = |ci: usize, si: usize| summarize(&values(ci, si), g.error_bar);
    let stacked = g.kind == GraphKind::StackedBar;
    // Line plots keep every series on the category itself (no side by side offset).
    let line = g.kind == GraphKind::GroupedLine;
    let mut vals = vec![0.0];
    for ci in 0..cats.len() {
        if stacked {
            vals.push((0..ns).filter_map(|si| cell(ci, si)).map(|c| c.0).sum());
        } else {
            for si in 0..ns {
                if let Some((c, lo, hi)) = cell(ci, si) {
                    vals.extend([c, lo, hi]);
                }
                vals.extend(values(ci, si));
            }
        }
    }
    let (lo, hi) = bounds(vals).unwrap();
    let vals_max = hi;
    let legend_w = ctx.legend_width(&names);
    let probe = frame(ctx, legend_w, 0.0);
    let slot = (probe.right - probe.left) / cats.len() as f32;
    let (angle, extra) = category_layout(&cat_labels, slot, ctx.ts(TextKind::Categories).0, g.x_tick_angle);
    let mut fr = frame(ctx, legend_w, extra);
    let group_w = slot * (g.bar_width * 1.15).clamp(0.2, 0.95);
    let bar_w = if stacked { group_w * 0.6 } else { group_w / ns as f32 };
    // Bracket x positions in slot units, so levels can be computed before layout.
    let unit = |ci: usize, si: usize| ci as f32 + 0.5 - 0.4 + (0.8 / ns as f32) * (si as f32 + 0.5);
    let spans: Vec<(f32, f32, &str, usize)> = if stacked || line {
        Vec::new()
    } else {
        brackets
            .iter()
            .enumerate()
            .filter_map(|(k, b)| {
                let ci = cats.iter().position(|&c| Some(c) == b.row)?;
                let (sa, sb) = (series.iter().position(|&s| s == b.a)?, series.iter().position(|&s| s == b.b)?);
                Some((unit(ci, sa.min(sb)), unit(ci, sa.max(sb)), b.label.as_str(), k))
            })
            .collect()
    };
    let levels = bracket_levels(&spans.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>());
    fr.top += bracket_space(ctx, &levels);
    let ax = y_axis_from(g, lo, hi, matches!(g.kind, GraphKind::GroupedBar | GraphKind::StackedBar), &fr);
    let centers: Vec<f32> = (0..cats.len()).map(|i| fr.left + slot * (i as f32 + 0.5)).collect();
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, &g.y_label);
    ctx.x_axis_categories(&centers, &cat_labels, &fr, angle);
    let (bw_line, eb) = (g.widths.bar_border, g.widths.error_bars);
    let base0 = ax.map(if g.y_scale == Scale::Log10 { ax.min } else { 0f64.clamp(ax.min, ax.max) });
    if g.kind == GraphKind::GroupedLine {
        // Lines first, so the symbols are drawn over them.
        for si in 0..ns {
            let pts: Vec<(f32, f32)> = (0..cats.len())
                .filter_map(|ci| cell(ci, si).map(|(c, _, _)| (centers[ci], ax.map(c))))
                .collect();
            let col = ctx.color(si);
            ctx.scene.prims.push(Prim::Polyline { points: pts, color: col, width: g.widths.lines, dashed: false });
        }
    }
    for ci in 0..cats.len() {
        let mut stack = 0.0;
        for si in 0..ns {
            let Some((c, elo, ehi)) = cell(ci, si) else { continue };
            let col = ctx.color(si);
            if stacked {
                let x = centers[ci];
                let (y0, y1) = (ax.map(stack), ax.map(stack + c));
                ctx.scene.hit(Element::Series(si), (x - bar_w / 2.0, y1.min(y0)), (x + bar_w / 2.0, y1.max(y0)));
                ctx.scene.prims.push(Prim::Rect {
                    min: (x - bar_w / 2.0, y1.min(y0)),
                    max: (x + bar_w / 2.0, y1.max(y0)),
                    fill: col.with_alpha((g.bar_opacity.clamp(0.0, 1.0) * 255.0) as u8),
                    stroke: Some((ctx.border(si, AXIS), bw_line * 0.6)),
                });
                stack += c;
                continue;
            }
            let x = if line { centers[ci] } else { centers[ci] - group_w / 2.0 + bar_w * (si as f32 + 0.5) };
            let vs = values(ci, si);
            let top = vs.iter().copied().chain([c, ehi]).fold(f64::NEG_INFINITY, f64::max);
            let bottom = vs.iter().copied().chain([c, elo]).fold(f64::INFINITY, f64::min);
            let bottom_px = if g.kind == GraphKind::GroupedBar { base0 } else { ax.map(bottom) };
            ctx.scene.hit(Element::Series(si), (x - bar_w * 0.45, ax.map(top) - 3.0), (x + bar_w * 0.45, bottom_px + 3.0));
            match g.kind {
                GraphKind::GroupedBar => {
                    ctx.scene.prims.push(Prim::Rect {
                        min: (x - bar_w * 0.45, ax.map(c).min(base0)),
                        max: (x + bar_w * 0.45, ax.map(c).max(base0)),
                        fill: col.with_alpha((g.bar_opacity.clamp(0.0, 1.0) * 255.0) as u8),
                        stroke: Some((ctx.border(si, col), bw_line)),
                    });
                    ctx.bar_error(x, ax.map(c), ax.map(elo), ax.map(ehi), bar_w * 0.2);
                }
                GraphKind::GroupedBox => box_plot(ctx, x, bar_w * 0.4, &vs, &ax, col, si),
                GraphKind::GroupedLine => {
                    ctx.error_bar(x, ax.map(elo), ax.map(ehi), bar_w * 0.15, col);
                    ctx.marker((x, ax.map(c)), si, true);
                }
                _ => {
                    ctx.scene.line((x - bar_w * 0.3, ax.map(c)), (x + bar_w * 0.3, ax.map(c)), AXIS, eb * 1.5);
                    ctx.error_bar(x, ax.map(elo), ax.map(ehi), bar_w * 0.15, AXIS);
                }
            }
            if g.show_points {
                let ys: Vec<f32> = values(ci, si).iter().map(|v| ax.map(*v)).collect();
                if line {
                    // Replicates stay on the X position, drawn as open symbols.
                    for y in &ys {
                        ctx.marker((x, *y), si, false);
                    }
                } else {
                    for (y, o) in ys.iter().zip(beeswarm(&ys, g.marker_size * 2.1, bar_w * 0.4)) {
                        ctx.marker((x + o, *y), si, true);
                    }
                }
            }
        }
    }
    let data_top = ax.map(vals_max).min(fr.top);
    let placed: Vec<(f32, f32, &str, usize)> = spans.iter().map(|&(a, b, l, k)| (fr.left + slot * a, fr.left + slot * b, l, k)).collect();
    let _ = data_top;
    draw_brackets(ctx, brackets, &placed, &levels, fr.top);
    if line {
        // Comparisons at one category: the strongest result above the points.
        for (ci, &c) in cats.iter().enumerate() {
            let Some(label) = brackets
                .iter()
                .filter(|b| b.row == Some(c))
                .map(|b| b.label.as_str())
                .max_by_key(|l| (l.chars().filter(|&ch| ch == '*').count(), l.len()))
            else {
                continue;
            };
            let top = (0..ns)
                .flat_map(|si| {
                    let mut v = cell(ci, si).map(|s| vec![s.0, s.2]).unwrap_or_default();
                    if g.show_points {
                        v.extend(values(ci, si));
                    }
                    v
                })
                .fold(f64::NEG_INFINITY, f64::max);
            if top.is_finite() {
                let y = ax.map(top.clamp(ax.min, ax.max)) - ctx.font * 0.6;
                ctx.text((centers[ci], y), label, TextKind::Brackets, Anchor::Middle, 0.0);
            }
        }
    }
    ctx.effects_box(&fr);
    if legend_w > 0.0 {
        ctx.legend(&names, fr.right + 16.0, fr.top + ctx.font, true);
    }
    Ok(())
}

fn xy(ctx: &mut Ctx, project: &Project, t: &DataTable, brackets: &[Bracket]) -> Result<(), String> {
    let g = ctx.g;
    let sets = t.nonempty_datasets();
    if sets.is_empty() {
        return Err("enter some data to see the graph".into());
    }
    let names: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    // Per data set: (x, center, lo, hi, replicates)
    let series: Vec<Vec<(f64, f64, f64, f64, Vec<f64>)>> = sets
        .iter()
        .map(|&d| {
            let mut v: Vec<_> = (0..t.rows)
                .filter_map(|r| {
                    let x = t.x[r]?;
                    let reps = t.cell_values(d, r);
                    let (c, lo, hi) = summarize(&reps, g.error_bar)?;
                    Some((x, c, lo, hi, reps))
                })
                .collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v
        })
        .collect();
    let curves: Vec<hg_core::analysis::Curve> =
        g.overlay_analyses.iter().filter_map(|&id| project.run_analysis(id).ok()).flat_map(|o| o.curves).collect();
    let xs = series.iter().flatten().map(|p| p.0);
    let ys = series.iter().flatten().flat_map(|p| [p.2, p.3].into_iter().chain(p.4.iter().copied().filter(|_| g.show_points)));
    let (xlo, xhi) = bounds(xs.clone()).ok_or("enter X values to see the graph")?;
    let (ylo, yhi) = bounds(ys.chain(curves.iter().flat_map(|c| c.points.iter().map(|p| p.1)).filter(|v| v.is_finite()))).unwrap();
    let legend_w = ctx.legend_width(&names);
    // The X title defaults to the table's X column title: leave room for it.
    let auto_x = if g.x_label.is_empty() && !t.x_title.is_empty() { ctx.font * 1.8 } else { 0.0 };
    let fr = frame(ctx, legend_w, auto_x + numeric_x_extra(ctx));
    let ax = y_axis_from(g, ylo, yhi, false, &fr);
    let (xa, xb) = nice_range(xlo, xhi, g.x_scale, false);
    let axx = Axis { min: xa, max: xb, scale: g.x_scale, p0: fr.left, p1: fr.right };
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, &g.y_label);
    let xl = if g.x_label.is_empty() { t.x_title.clone() } else { g.x_label.clone() };
    ctx.x_axis_numeric(&axx, &fr, &xl);
    let lw = g.widths.lines;
    for c in &curves {
        if let Some(i) = sets.iter().position(|&d| d == c.dataset) {
            let pts: Vec<(f32, f32)> =
                c.points.iter().filter(|p| p.1 >= ax.min && p.1 <= ax.max).map(|p| (axx.map(p.0), ax.map(p.1))).collect();
            let col = ctx.color(i);
            for w in pts.windows(2).step_by(4) {
                ctx.scene.hit(Element::Series(i), (w[0].0 - 3.0, w[0].1 - 3.0), (w[1].0 + 3.0, w[1].1 + 3.0));
            }
            ctx.scene.prims.push(Prim::Polyline { points: pts, color: col, width: lw, dashed: c.dashed });
        }
    }
    for (i, s) in series.iter().enumerate() {
        let col = ctx.color(i);
        if g.connect_points {
            let pts: Vec<(f32, f32)> = s.iter().map(|p| (axx.map(p.0), ax.map(p.1))).collect();
            ctx.scene.prims.push(Prim::Polyline { points: pts, color: col, width: lw, dashed: false });
        }
        for p in s {
            let x = axx.map(p.0);
            let hi_v = p.4.iter().copied().chain([p.3]).fold(f64::NEG_INFINITY, f64::max);
            let lo_v = p.4.iter().copied().chain([p.2]).fold(f64::INFINITY, f64::min);
            ctx.scene.hit(Element::Series(i), (x - g.marker_size - 2.0, ax.map(hi_v) - 3.0), (x + g.marker_size + 2.0, ax.map(lo_v) + 3.0));
            // Mean (or the single value) with its error bar; individual
            // replicates on request.
            if g.error_bar != ErrorBar::None {
                ctx.error_bar(x, ax.map(p.2), ax.map(p.3), g.marker_size * 1.2, col);
            }
            if g.show_points && p.4.len() > 1 {
                for y in &p.4 {
                    ctx.marker((x, ax.map(*y)), i, false);
                }
            }
            ctx.marker((x, ax.map(p.1)), i, true);
        }
    }
    // Comparisons between data sets at one X: the strongest result above the points.
    let mut rows: Vec<usize> = brackets.iter().filter_map(|b| b.row).collect();
    rows.sort_unstable();
    rows.dedup();
    for r in rows {
        let Some(xv) = t.x.get(r).copied().flatten() else { continue };
        let label = brackets
            .iter()
            .filter(|b| b.row == Some(r))
            .map(|b| b.label.as_str())
            .max_by_key(|l| (l.chars().filter(|&c| c == '*').count(), l.len()))
            .unwrap_or_default();
        let top = sets
            .iter()
            .flat_map(|&d| {
                let reps = t.cell_values(d, r);
                let mut v = summarize(&reps, g.error_bar).map(|s| vec![s.1, s.2]).unwrap_or_default();
                if g.show_points {
                    v.extend(reps);
                }
                v
            })
            .filter(|v| v.is_finite())
            .fold(f64::NEG_INFINITY, f64::max);
        if top.is_finite() {
            let y = ax.map(top.clamp(ax.min, ax.max)) - ctx.font * 0.6;
            ctx.text((axx.map(xv), y), label, TextKind::Brackets, Anchor::Middle, 0.0);
        }
    }
    ctx.effects_box(&fr);
    if legend_w > 0.0 {
        ctx.legend(&names, fr.right + 16.0, fr.top + ctx.font, false);
    }
    Ok(())
}

fn survival_curve(ctx: &mut Ctx, t: &DataTable) -> Result<(), String> {
    let g = ctx.g;
    let sets = t.nonempty_datasets();
    let kms: Vec<survival::KaplanMeier> = sets.iter().filter_map(|&d| survival::kaplan_meier(&t.survival_subjects(d)).ok()).collect();
    if kms.is_empty() {
        return Err("enter times and outcomes (1 = event, 0 = censored)".into());
    }
    let names: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    let tmax = kms.iter().flat_map(|k| k.steps.iter().map(|s| s.time)).fold(0.0, f64::max);
    let legend_w = ctx.legend_width(&names);
    let auto_x = if g.x_label.is_empty() && !t.x_title.is_empty() { ctx.font * 1.8 } else { 0.0 };
    let fr = frame(ctx, legend_w, auto_x + numeric_x_extra(ctx));
    let ax = Axis { min: g.y_min.unwrap_or(0.0), max: g.y_max.unwrap_or(100.0), scale: Scale::Linear, p0: fr.bottom, p1: fr.top };
    let (_, xb) = nice_range(0.0, tmax, Scale::Linear, true);
    let axx = Axis { min: 0.0, max: xb, scale: Scale::Linear, p0: fr.left, p1: fr.right };
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, if g.y_label.is_empty() { "Percent survival" } else { &g.y_label });
    let xl = if g.x_label.is_empty() { t.x_title.clone() } else { g.x_label.clone() };
    ctx.x_axis_numeric(&axx, &fr, &xl);
    for (i, km) in kms.iter().enumerate() {
        let col = ctx.color(i);
        let mut pts = vec![(axx.map(0.0), ax.map(100.0))];
        let mut s = 100.0;
        for st in &km.steps {
            let x = axx.map(st.time);
            pts.push((x, ax.map(s)));
            s = 100.0 * st.survival;
            pts.push((x, ax.map(s)));
        }
        for w in pts.windows(2) {
            ctx.scene.hit(Element::Series(i), (w[0].0 - 3.0, w[0].1 - 3.0), (w[1].0 + 3.0, w[1].1 + 3.0));
        }
        ctx.scene.prims.push(Prim::Polyline { points: pts, color: col, width: g.widths.lines, dashed: false });
        for st in km.steps.iter().filter(|st| st.censored > 0) {
            let (x, y) = (axx.map(st.time), ax.map(100.0 * st.survival));
            ctx.scene.line((x, y - 5.0), (x, y + 5.0), col, g.widths.lines);
        }
    }
    if legend_w > 0.0 {
        ctx.legend(&names, fr.right + 16.0, fr.top + ctx.font, false);
    }
    Ok(())
}

fn pie(ctx: &mut Ctx, t: &DataTable) -> Result<(), String> {
    let g = ctx.g;
    let vals: Vec<(String, f64)> =
        (0..t.used_rows()).filter_map(|r| t.datasets[0].values[r][0].filter(|v| *v > 0.0).map(|v| (t.row_label(r), v))).collect();
    if vals.is_empty() {
        return Err("enter positive values to see the chart".into());
    }
    let names: Vec<String> = vals.iter().map(|v| v.0.clone()).collect();
    let total: f64 = vals.iter().map(|v| v.1).sum();
    let legend_w =
        if g.show_legend { names.iter().map(|n| text_width(n, ctx.font)).fold(0.0, f32::max) + ctx.font * 2.6 + 16.0 } else { 0.0 };
    let top = if g.title.is_empty() { 10.0 } else { ctx.font * 3.0 };
    let w = g.width - legend_w;
    let r = ((w.min(g.height - top) / 2.0) - ctx.font * 3.0).max(20.0);
    let c = (w / 2.0, top + (g.height - top) / 2.0);
    ctx.title(&Frame { left: 0.0, right: w, top, bottom: g.height });
    let mut angle = -std::f64::consts::FRAC_PI_2;
    let inner = if g.kind == GraphKind::Donut { r * 0.55 } else { 0.0 };
    for (i, (_, v)) in vals.iter().enumerate() {
        let sweep = v / total * std::f64::consts::TAU;
        let steps = ((sweep / 0.05).ceil() as usize).max(2);
        let arc = |rad: f32, k: usize| {
            let a = angle + sweep * k as f64 / steps as f64;
            (c.0 + rad * a.cos() as f32, c.1 + rad * a.sin() as f32)
        };
        let mut pts: Vec<(f32, f32)> = (0..=steps).map(|k| arc(r, k)).collect();
        if inner > 0.0 {
            pts.extend((0..=steps).rev().map(|k| arc(inner, k)));
        } else {
            pts.push(c);
        }
        let col = ctx.color(i);
        let mid_a = angle + sweep / 2.0;
        let cr = (r + inner) / 2.0;
        let cp = (c.0 + cr * mid_a.cos() as f32, c.1 + cr * mid_a.sin() as f32);
        let half = (r - inner).min(r * sweep as f32) * 0.35;
        ctx.scene.hit(Element::Series(i), (cp.0 - half, cp.1 - half), (cp.0 + half, cp.1 + half));
        ctx.scene.prims.push(Prim::Polygon { points: pts, fill: col, stroke: Some((Color::WHITE, 1.5)) });
        let mid = angle + sweep / 2.0;
        let lr = r + ctx.font * 1.4;
        let lp = (c.0 + lr * mid.cos() as f32, c.1 + lr * mid.sin() as f32 + ctx.font * 0.35);
        let anchor = if mid.cos() > 0.3 {
            Anchor::Start
        } else if mid.cos() < -0.3 {
            Anchor::End
        } else {
            Anchor::Middle
        };
        ctx.text(lp, format!("{}%", hg_core::format::num(100.0 * v / total)), TextKind::DataLabels, anchor, 0.0);
        angle += sweep;
    }
    if legend_w > 0.0 {
        ctx.legend(&names, w + 8.0, top + ctx.font, true);
    }
    Ok(())
}

/// Box (quartiles), median line and whiskers (min to max) at `x`.
fn box_plot(ctx: &mut Ctx, x: f32, half: f32, v: &[f64], ax: &Axis, col: Color, series: usize) {
    let Ok(d) = describe(v) else { return };
    let bw = ctx.g.widths.bar_border;
    ctx.scene.prims.push(Prim::Rect {
        min: (x - half, ax.map(d.q3)),
        max: (x + half, ax.map(d.q1)),
        fill: col.with_alpha(70),
        stroke: Some((ctx.border(series, col), bw)),
    });
    ctx.scene.line((x - half, ax.map(d.median)), (x + half, ax.map(d.median)), col, bw * 1.8);
    ctx.error_bar(x, ax.map(d.min), ax.map(d.q1), half * 0.5, col);
    ctx.error_bar(x, ax.map(d.q3), ax.map(d.max), half * 0.5, col);
}

fn histogram(ctx: &mut Ctx, t: &DataTable) -> Result<(), String> {
    let g = ctx.g;
    let sets = t.nonempty_datasets();
    if sets.is_empty() {
        return Err("enter some data to see the graph".into());
    }
    let data: Vec<Vec<f64>> = sets.iter().map(|&d| t.column_values(d)).collect();
    let names: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    let (lo, hi) = bounds(data.iter().flatten().copied()).unwrap();
    let n_max = data.iter().map(Vec::len).max().unwrap_or(1);
    // Sturges' rule, rounded to a nice bin width.
    let k = ((n_max as f64).log2().ceil() as usize + 1).clamp(4, 30);
    let step = crate::axis::nice_step((hi - lo).max(1e-9), k);
    let start = (lo / step).floor() * step;
    let nb = (((hi - start) / step).floor() as usize + 1).max(1);
    let counts: Vec<Vec<usize>> = data
        .iter()
        .map(|v| {
            let mut c = vec![0; nb];
            for x in v {
                c[(((x - start) / step).floor() as usize).min(nb - 1)] += 1;
            }
            c
        })
        .collect();
    let ymax = counts.iter().flatten().copied().max().unwrap_or(1) as f64;
    let legend_w = ctx.legend_width(&names);
    let fr = frame(ctx, legend_w, numeric_x_extra(ctx));
    let ax = y_axis_from(g, 0.0, ymax, true, &fr);
    let axx = Axis { min: start, max: start + nb as f64 * step, scale: Scale::Linear, p0: fr.left, p1: fr.right };
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, if g.y_label.is_empty() { "Number of values" } else { &g.y_label });
    ctx.x_axis_numeric(&axx, &fr, &g.x_label);
    let alpha = (g.bar_opacity.clamp(0.0, 1.0) * 255.0) as u8;
    for (i, c) in counts.iter().enumerate() {
        let col = ctx.color(i);
        for (b, &n) in c.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let x0 = axx.map(start + b as f64 * step) + 0.5;
            let x1 = axx.map(start + (b + 1) as f64 * step) - 0.5;
            let (y0, y1) = (ax.map(0.0), ax.map(n as f64));
            ctx.scene.hit(Element::Series(i), (x0, y1), (x1, y0));
            ctx.scene.prims.push(Prim::Rect {
                min: (x0, y1),
                max: (x1, y0),
                fill: col.with_alpha(alpha),
                stroke: Some((ctx.border(i, col), g.widths.bar_border)),
            });
        }
    }
    if legend_w > 0.0 {
        ctx.legend(&names, fr.right + 16.0, fr.top + ctx.font, true);
    }
    Ok(())
}

fn qq_plot(ctx: &mut Ctx, t: &DataTable) -> Result<(), String> {
    let g = ctx.g;
    let sets: Vec<usize> = t.nonempty_datasets().into_iter().filter(|&d| t.column_values(d).len() >= 3).collect();
    if sets.is_empty() {
        return Err("a QQ plot needs at least 3 values in a data set".into());
    }
    let names: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    // (theoretical z, observed value) with Blom's plotting positions.
    let pts: Vec<Vec<(f64, f64)>> = sets
        .iter()
        .map(|&d| {
            let v = hg_stats::descriptive::sorted(&t.column_values(d));
            let n = v.len() as f64;
            v.iter().enumerate().map(|(i, y)| (hg_stats::dist::norm_inv((i as f64 + 1.0 - 0.375) / (n + 0.25)), *y)).collect()
        })
        .collect();
    let (xlo, xhi) = bounds(pts.iter().flatten().map(|p| p.0)).unwrap();
    let (ylo, yhi) = bounds(pts.iter().flatten().map(|p| p.1)).unwrap();
    let legend_w = ctx.legend_width(&names);
    let fr = frame(ctx, legend_w, numeric_x_extra(ctx) + ctx.ts(TextKind::AxisTitles).0 * 1.8);
    let ax = y_axis_from(g, ylo, yhi, false, &fr);
    let (xa, xb) = nice_range(xlo, xhi, Scale::Linear, false);
    let axx = Axis { min: xa, max: xb, scale: Scale::Linear, p0: fr.left, p1: fr.right };
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    ctx.y_axis(&ax, &fr, if g.y_label.is_empty() { "Observed value" } else { &g.y_label });
    ctx.x_axis_numeric(&axx, &fr, if g.x_label.is_empty() { "Normal quantile (z)" } else { &g.x_label });
    for (i, (p, &d)) in pts.iter().zip(&sets).enumerate() {
        let col = ctx.color(i);
        // Reference line: the Gaussian with the sample mean and SD.
        let v = t.column_values(d);
        let (m, sd) = (hg_stats::descriptive::mean(&v), hg_stats::descriptive::sd(&v));
        let line = vec![(axx.map(xa), ax.map(m + sd * xa)), (axx.map(xb), ax.map(m + sd * xb))];
        ctx.scene.prims.push(Prim::Polyline { points: line, color: col, width: g.widths.lines, dashed: true });
        for &(z, y) in p {
            let q = (axx.map(z), ax.map(y));
            ctx.scene.hit(Element::Series(i), (q.0 - 4.0, q.1 - 4.0), (q.0 + 4.0, q.1 + 4.0));
            ctx.marker(q, i, true);
        }
    }
    if legend_w > 0.0 {
        ctx.legend(&names, fr.right + 16.0, fr.top + ctx.font, false);
    }
    Ok(())
}

/// Sequential color scale from a light tint to the first series color.
fn heat_color(base: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0) as f32;
    let light = (0xf4 as f32, 0xf6 as f32, 0xfb as f32);
    let dark = (base.r as f32 * 0.55, base.g as f32 * 0.55, base.b as f32 * 0.55);
    let (a, b, u) = if t < 0.6 {
        (light, (base.r as f32, base.g as f32, base.b as f32), t / 0.6)
    } else {
        ((base.r as f32, base.g as f32, base.b as f32), dark, (t - 0.6) / 0.4)
    };
    Color::rgb((a.0 + (b.0 - a.0) * u) as u8, (a.1 + (b.1 - a.1) * u) as u8, (a.2 + (b.2 - a.2) * u) as u8)
}

fn heatmap(ctx: &mut Ctx, t: &DataTable) -> Result<(), String> {
    let g = ctx.g;
    let rows: Vec<usize> = (0..t.used_rows()).filter(|&r| (0..t.datasets.len()).any(|d| !t.cell_values(d, r).is_empty())).collect();
    let sets: Vec<usize> = (0..t.datasets.len()).filter(|&d| rows.iter().any(|&r| !t.cell_values(d, r).is_empty())).collect();
    if rows.is_empty() || sets.is_empty() {
        return Err("enter some data to see the graph".into());
    }
    let value = |r: usize, d: usize| {
        let v = t.cell_values(d, r);
        (!v.is_empty()).then(|| hg_stats::descriptive::mean(&v))
    };
    let (lo, hi) = bounds(rows.iter().flat_map(|&r| sets.iter().filter_map(move |&d| value(r, d)))).unwrap();
    let (cat, _) = ctx.ts(TextKind::Categories);
    let row_labels: Vec<String> = rows.iter().map(|&r| t.row_label(r)).collect();
    let col_labels: Vec<String> = sets.iter().map(|&d| t.datasets[d].title.clone()).collect();
    let label_w = row_labels.iter().map(|l| text_width(l, cat)).fold(0.0, f32::max) + 14.0;
    let top = if g.title.is_empty() { ctx.font } else { ctx.ts(TextKind::Title).0 * 2.6 };
    let scale_w = 70.0;
    let probe_slot = (g.width - label_w - scale_w - 20.0) / sets.len() as f32;
    let (angle, extra) = category_layout(&col_labels, probe_slot, cat, g.x_tick_angle);
    let fr = Frame { left: label_w + 6.0, right: g.width - scale_w, top, bottom: g.height - (cat * 2.6 + 10.0 + extra) };
    let cw = (fr.right - fr.left) / sets.len() as f32;
    let ch = (fr.bottom - fr.top) / rows.len() as f32;
    ctx.scene.hit(Element::Plot, (fr.left, fr.top), (fr.right, fr.bottom));
    ctx.title(&fr);
    let base = ctx.color(0);
    let (tick, _) = ctx.ts(TextKind::DataLabels);
    for (ri, &r) in rows.iter().enumerate() {
        let y0 = fr.top + ri as f32 * ch;
        ctx.text((fr.left - 8.0, y0 + ch / 2.0 + cat * 0.35), row_labels[ri].clone(), TextKind::Categories, Anchor::End, 0.0);
        for (ci, &d) in sets.iter().enumerate() {
            let x0 = fr.left + ci as f32 * cw;
            let Some(v) = value(r, d) else { continue };
            let u = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
            let c = heat_color(base, u);
            ctx.scene.hit(Element::Series(ci), (x0, y0), (x0 + cw, y0 + ch));
            ctx.scene.prims.push(Prim::Rect { min: (x0, y0), max: (x0 + cw, y0 + ch), fill: c, stroke: Some((Color::WHITE, 1.5)) });
            if ch > tick * 1.3 && cw > text_width("0000", tick) {
                let dark = (c.r as f32 * 0.299 + c.g as f32 * 0.587 + c.b as f32 * 0.114) < 140.0;
                let size = tick;
                ctx.scene.prims.push(Prim::Text {
                    pos: (x0 + cw / 2.0, y0 + ch / 2.0 + size * 0.35),
                    text: hg_core::format::num(v),
                    size,
                    color: if dark { Color::WHITE } else { AXIS },
                    anchor: Anchor::Middle,
                    angle: 0.0,
                    bold: false,
                });
            }
        }
    }
    let centers: Vec<f32> = (0..sets.len()).map(|i| fr.left + cw * (i as f32 + 0.5)).collect();
    // Column labels under the grid (no axis line).
    let y = fr.bottom + 6.0;
    for (c, l) in centers.iter().zip(&col_labels) {
        if angle > 0.0 {
            let a = angle.to_radians();
            ctx.text((*c + cat * 0.35 * a.sin(), y + 9.0 + cat * 0.4 * a.cos()), l.clone(), TextKind::Categories, Anchor::End, angle);
        } else {
            ctx.text((*c, y + 7.0 + cat), l.clone(), TextKind::Categories, Anchor::Middle, 0.0);
        }
    }
    // Color scale.
    let (sx, sy0, sy1) = (fr.right + 18.0, fr.top, fr.bottom.min(fr.top + 180.0));
    let steps = 24;
    for k in 0..steps {
        let u0 = k as f32 / steps as f32;
        let ya = sy1 - (sy1 - sy0) * u0;
        let yb = sy1 - (sy1 - sy0) * (u0 + 1.0 / steps as f32);
        ctx.scene.prims.push(Prim::Rect {
            min: (sx, yb),
            max: (sx + 14.0, ya),
            fill: heat_color(base, u0 as f64 + 0.5 / steps as f64),
            stroke: None,
        });
    }
    ctx.scene.prims.push(Prim::Rect { min: (sx, sy0), max: (sx + 14.0, sy1), fill: Color::TRANSPARENT, stroke: Some((AXIS, 0.8)) });
    ctx.scene.hit(Element::Legend, (sx - 2.0, sy0 - 2.0), (sx + 50.0, sy1 + 2.0));
    ctx.text((sx + 18.0, sy0 + tick * 0.35), hg_core::format::num(hi), TextKind::Legend, Anchor::Start, 0.0);
    ctx.text((sx + 18.0, sy1 + tick * 0.35), hg_core::format::num(lo), TextKind::Legend, Anchor::Start, 0.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hg_core::sample::sample_table;
    use hg_core::{AnalysisKind, TableKind};

    #[test]
    fn every_graph_kind_renders_sample_data() {
        let mut p = Project::default();
        for kind in TableKind::ALL {
            let id = p.add_table(sample_table(kind));
            if kind == TableKind::XY {
                p.add_analysis(
                    id,
                    AnalysisKind::NonlinearFit { model: hg_stats::nonlinear::Model::DoseResponseVariableSlope, fixed: Vec::new() },
                );
            }
            for (gk, transposed) in GraphKind::gallery(kind) {
                let gid = p.add_graph(id, gk);
                let mut g = p.graph(gid).unwrap().clone();
                g.transpose = transposed;
                g.colors = vec![[200, 30, 30]];
                if matches!(kind, TableKind::Column) {
                    g.brackets = vec![
                        hg_core::graph::Bracket { row: None, a: 0, b: 1, label: "**".into() },
                        hg_core::graph::Bracket { row: None, a: 0, b: 2, label: "ns".into() },
                    ];
                } else if kind == TableKind::Grouped {
                    g.brackets =
                        vec![hg_core::graph::Bracket { row: Some(if transposed { 1 } else { 2 }), a: 0, b: 1, label: "****".into() }];
                }
                g.title = format!("{gk:?}");
                let scene = render(&p, &g).unwrap_or_else(|e| panic!("{gk:?}: {e}"));
                assert!(scene.hits.iter().any(|h| matches!(h.element, Element::Series(_))), "{gk:?} has no selectable series");
                if !matches!(gk, GraphKind::Pie | GraphKind::Donut | GraphKind::Heatmap) {
                    assert!(scene.bounds(Element::YAxis).is_some(), "{gk:?} has no Y axis region");
                }
                assert!(scene.prims.len() > 5, "{gk:?} drew almost nothing");
                let svg = crate::to_svg(&scene);
                assert!(svg.starts_with("<svg"));
            }
        }
    }

    #[test]
    fn summaries() {
        let (c, lo, hi) = summarize(&[1.0, 2.0, 3.0], ErrorBar::Range).unwrap();
        assert_eq!((c, lo, hi), (2.0, 1.0, 3.0));
    }
}
