//! Graph definitions (how a table is drawn). Rendering lives in `hg-plot`.

use crate::table::TableKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphKind {
    /// Every point, with mean (or median) and error bars.
    ScatterDot,
    Bar,
    BoxWhisker,
    Violin,
    MeanError,
    BeforeAfter,
    GroupedBar,
    StackedBar,
    GroupedScatter,
    Xy,
    Survival,
    Pie,
    Donut,
    /// Frequency distribution of each data set.
    Histogram,
    /// Normal quantile-quantile plot of each data set.
    QqPlot,
    /// Means (± error) of the groups joined by a line.
    LineMean,
    /// One line per data set across the row groups (interaction plot).
    GroupedLine,
    /// Box and whiskers for each cell of a grouped table.
    GroupedBox,
    /// Colored grid of the cell means (or counts).
    Heatmap,
}

impl GraphKind {
    pub fn label(self) -> &'static str {
        match self {
            GraphKind::ScatterDot => "Scatter dot plot",
            GraphKind::Bar => "Bar (mean ± error)",
            GraphKind::BoxWhisker => "Box and whiskers",
            GraphKind::Violin => "Violin plot",
            GraphKind::MeanError => "Mean with error bars",
            GraphKind::BeforeAfter => "Before-after (paired lines)",
            GraphKind::GroupedBar => "Grouped bars",
            GraphKind::StackedBar => "Stacked bars",
            GraphKind::GroupedScatter => "Grouped scatter",
            GraphKind::Xy => "XY points and lines",
            GraphKind::Survival => "Survival curve (Kaplan-Meier)",
            GraphKind::Pie => "Pie chart",
            GraphKind::Donut => "Donut chart",
            GraphKind::Histogram => "Histogram (frequency distribution)",
            GraphKind::QqPlot => "QQ plot (normality)",
            GraphKind::LineMean => "Line of the means",
            GraphKind::GroupedLine => "Lines (interaction plot)",
            GraphKind::GroupedBox => "Grouped box plots",
            GraphKind::Heatmap => "Heat map",
        }
    }

    /// Choices offered in the graph gallery: (kind, transposed). Grouped data
    /// can be plotted with rows or with data sets (columns) on the X axis.
    pub fn gallery(kind: TableKind) -> Vec<(GraphKind, bool)> {
        let base = Self::for_table(kind).iter().map(|&k| (k, false));
        if matches!(kind, TableKind::Grouped | TableKind::Contingency) {
            base.flat_map(|(k, _)| if k == GraphKind::Heatmap { vec![(k, false)] } else { vec![(k, false), (k, true)] }).collect()
        } else {
            base.collect()
        }
    }

    pub fn gallery_label(self, transposed: bool, kind: TableKind) -> String {
        if matches!(kind, TableKind::Grouped | TableKind::Contingency) {
            format!("{} — by {}", self.label(), if transposed { "column" } else { "row" })
        } else {
            self.label().to_string()
        }
    }

    pub fn for_table(kind: TableKind) -> &'static [GraphKind] {
        match kind {
            TableKind::Column => &[
                GraphKind::ScatterDot,
                GraphKind::Bar,
                GraphKind::BoxWhisker,
                GraphKind::Violin,
                GraphKind::MeanError,
                GraphKind::LineMean,
                GraphKind::BeforeAfter,
                GraphKind::Histogram,
                GraphKind::QqPlot,
            ],
            TableKind::Grouped => &[
                GraphKind::GroupedBar,
                GraphKind::GroupedScatter,
                GraphKind::GroupedBox,
                GraphKind::GroupedLine,
                GraphKind::StackedBar,
                GraphKind::Heatmap,
            ],
            TableKind::Contingency => &[GraphKind::GroupedBar, GraphKind::StackedBar, GraphKind::Heatmap],
            TableKind::XY => &[GraphKind::Xy],
            TableKind::Survival => &[GraphKind::Survival],
            TableKind::PartsOfWhole => &[GraphKind::Pie, GraphKind::Donut, GraphKind::StackedBar],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorBar {
    Sd,
    Sem,
    Ci95,
    Range,
    None,
}

impl ErrorBar {
    pub const ALL: [ErrorBar; 5] = [ErrorBar::Sd, ErrorBar::Sem, ErrorBar::Ci95, ErrorBar::Range, ErrorBar::None];
    pub fn label(self) -> &'static str {
        match self {
            ErrorBar::Sd => "Mean ± SD",
            ErrorBar::Sem => "Mean ± SEM",
            ErrorBar::Ci95 => "Mean with 95% CI",
            ErrorBar::Range => "Median with range",
            ErrorBar::None => "Mean only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    Linear,
    Log10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerShape {
    Circle,
    Square,
    Triangle,
    Diamond,
    TriangleDown,
    Cross,
    Plus,
}

impl MarkerShape {
    /// Shapes given to data sets in turn.
    pub const ALL: [MarkerShape; 4] = [MarkerShape::Circle, MarkerShape::Square, MarkerShape::Triangle, MarkerShape::Diamond];
    /// Every shape that can be chosen.
    pub const CHOICES: [MarkerShape; 7] = [
        MarkerShape::Circle,
        MarkerShape::Square,
        MarkerShape::Triangle,
        MarkerShape::TriangleDown,
        MarkerShape::Diamond,
        MarkerShape::Cross,
        MarkerShape::Plus,
    ];
}

/// Whether symbols are drawn filled with the series color or hollow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Fill {
    /// The default for the kind of graph.
    #[default]
    Auto,
    Filled,
    Hollow,
}

/// Symbol overrides for one data set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SeriesStyle {
    pub shape: Option<MarkerShape>,
    pub fill: Fill,
    /// Border of bars, boxes and symbols (None = the data set color).
    #[serde(default)]
    pub border: Option<[u8; 3]>,
}

/// Size and weight of one kind of text (None = derived from the base font size).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TextSpec {
    pub size: Option<f32>,
    pub bold: Option<bool>,
}

/// Every kind of text in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Texts {
    pub title: TextSpec,
    pub axis_titles: TextSpec,
    pub tick_labels: TextSpec,
    pub categories: TextSpec,
    pub legend: TextSpec,
    pub brackets: TextSpec,
    pub data_labels: TextSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Title,
    AxisTitles,
    TickLabels,
    Categories,
    Legend,
    Brackets,
    DataLabels,
}

impl TextKind {
    pub const ALL: [TextKind; 7] = [
        TextKind::Title,
        TextKind::AxisTitles,
        TextKind::TickLabels,
        TextKind::Categories,
        TextKind::Legend,
        TextKind::Brackets,
        TextKind::DataLabels,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextKind::Title => "Graph title",
            TextKind::AxisTitles => "Axis titles",
            TextKind::TickLabels => "Axis numbers",
            TextKind::Categories => "Group names (X axis)",
            TextKind::Legend => "Legend",
            TextKind::Brackets => "Significance labels",
            TextKind::DataLabels => "Data labels (pie %)",
        }
    }

    /// Default size relative to the base font size, and default weight.
    pub fn defaults(self) -> (f32, bool) {
        match self {
            TextKind::Title => (1.25, true),
            TextKind::AxisTitles => (1.05, true),
            TextKind::Brackets => (1.0, true),
            _ => (1.0, false),
        }
    }
}

impl Texts {
    pub fn get(&self, k: TextKind) -> &TextSpec {
        match k {
            TextKind::Title => &self.title,
            TextKind::AxisTitles => &self.axis_titles,
            TextKind::TickLabels => &self.tick_labels,
            TextKind::Categories => &self.categories,
            TextKind::Legend => &self.legend,
            TextKind::Brackets => &self.brackets,
            TextKind::DataLabels => &self.data_labels,
        }
    }

    pub fn get_mut(&mut self, k: TextKind) -> &mut TextSpec {
        match k {
            TextKind::Title => &mut self.title,
            TextKind::AxisTitles => &mut self.axis_titles,
            TextKind::TickLabels => &mut self.tick_labels,
            TextKind::Categories => &mut self.categories,
            TextKind::Legend => &mut self.legend,
            TextKind::Brackets => &mut self.brackets,
            TextKind::DataLabels => &mut self.data_labels,
        }
    }

    /// Effective (size, bold) for a base font size.
    pub fn resolve(&self, k: TextKind, base: f32) -> (f32, bool) {
        let (rel, bold) = k.defaults();
        let s = self.get(k);
        (s.size.unwrap_or(base * rel), s.bold.unwrap_or(bold))
    }
}

/// A key identifying a bracket whatever its source: (group, a, b).
pub type BracketKey = (Option<usize>, usize, usize);

fn default_bar_opacity() -> f32 {
    0.45
}

fn default_bar_width() -> f32 {
    0.7
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub id: u64,
    pub name: String,
    pub table_id: u64,
    pub kind: GraphKind,
    pub error_bar: ErrorBar,
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub x_scale: Scale,
    pub y_scale: Scale,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub show_points: bool,
    pub connect_points: bool,
    pub show_legend: bool,
    pub palette: usize,
    pub width: f32,
    pub height: f32,
    pub marker_size: f32,
    pub line_width: f32,
    pub font_size: f32,
    /// Overlay curves from these analyses (fits, regression lines).
    #[serde(default)]
    pub overlay_analyses: Vec<u64>,
    /// Significance brackets typed by the user.
    #[serde(default)]
    pub brackets: Vec<Bracket>,
    /// Significance brackets that follow an analysis.
    #[serde(default)]
    pub auto_significance: AutoSignificance,
    /// Grouped graphs: put data sets (columns) on the X axis instead of rows.
    #[serde(default)]
    pub transpose: bool,
    /// Custom colors per data set (RGB); missing entries use the palette.
    #[serde(default)]
    pub colors: Vec<[u8; 3]>,
    /// Positions of movable labels relative to their automatic place.
    #[serde(default)]
    pub offsets: Offsets,
    /// Symbol shape and fill per data set (by drawing order).
    #[serde(default)]
    pub series_styles: Vec<SeriesStyle>,
    /// Size and weight of each kind of text.
    #[serde(default)]
    pub texts: Texts,
    /// Opacity of bar fills (0 = empty, 1 = solid).
    #[serde(default = "default_bar_opacity")]
    pub bar_opacity: f32,
    /// Width of bars as a fraction of the space of a group.
    #[serde(default = "default_bar_width")]
    pub bar_width: f32,
    /// Width of error bar caps (multiplier).
    #[serde(default = "one")]
    pub cap_width: f32,
    /// Bars: draw error bars above the bar only.
    #[serde(default)]
    pub error_up_only: bool,
    /// Vertical displacement of brackets the user dragged.
    #[serde(default)]
    pub bracket_offsets: Vec<(BracketKey, f32)>,
    /// Thickness of each kind of line.
    #[serde(default)]
    pub widths: Widths,
    /// Angle (degrees, counter-clockwise) of the X axis labels; None = automatic.
    #[serde(default)]
    pub x_tick_angle: Option<f32>,
    /// Angle (degrees) of the Y axis numbers.
    #[serde(default)]
    pub y_tick_angle: f32,
    /// Length of the bracket ends and gap above the data (multipliers).
    #[serde(default = "one")]
    pub bracket_tick: f32,
    #[serde(default = "one")]
    pub bracket_gap: f32,
    /// Legend text of each data set (empty = the data set title).
    #[serde(default)]
    pub legend_labels: Vec<String>,
}

/// Line thickness (points) of each part of a graph, set independently.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Widths {
    pub axis: f32,
    pub error_bars: f32,
    pub bar_border: f32,
    pub symbol_border: f32,
    pub lines: f32,
    pub brackets: f32,
}

impl Default for Widths {
    fn default() -> Self {
        Widths { axis: 1.5, error_bars: 1.5, bar_border: 1.5, symbol_border: 1.0, lines: 1.8, brackets: 1.3 }
    }
}

impl Graph {
    /// Copies the look of `src`: palette and colors, symbols, fonts, line
    /// widths, bars, sizes and legend. The data, titles, axes, error bars and
    /// significance brackets of this graph stay as they are.
    pub fn apply_style(&mut self, src: &Graph) {
        self.palette = src.palette;
        self.colors = src.colors.clone();
        self.series_styles = src.series_styles.clone();
        self.texts = src.texts;
        self.font_size = src.font_size;
        self.marker_size = src.marker_size;
        self.line_width = src.line_width;
        self.widths = src.widths;
        self.bar_opacity = src.bar_opacity;
        self.bar_width = src.bar_width;
        self.cap_width = src.cap_width;
        self.error_up_only = src.error_up_only;
        self.bracket_tick = src.bracket_tick;
        self.bracket_gap = src.bracket_gap;
        self.x_tick_angle = src.x_tick_angle;
        self.y_tick_angle = src.y_tick_angle;
        self.width = src.width;
        self.height = src.height;
        self.show_legend = src.show_legend;
        self.show_points = src.show_points;
        self.connect_points = src.connect_points;
    }

    pub fn series_style(&self, i: usize) -> SeriesStyle {
        self.series_styles.get(i).copied().unwrap_or_default()
    }

    pub fn series_style_mut(&mut self, i: usize) -> &mut SeriesStyle {
        if self.series_styles.len() <= i {
            self.series_styles.resize(i + 1, SeriesStyle::default());
        }
        &mut self.series_styles[i]
    }

    pub fn bracket_offset(&self, key: BracketKey) -> f32 {
        self.bracket_offsets.iter().find(|(k, _)| *k == key).map_or(0.0, |(_, d)| *d)
    }

    pub fn bracket_offset_mut(&mut self, key: BracketKey) -> &mut f32 {
        if let Some(i) = self.bracket_offsets.iter().position(|(k, _)| *k == key) {
            &mut self.bracket_offsets[i].1
        } else {
            self.bracket_offsets.push((key, 0.0));
            &mut self.bracket_offsets.last_mut().unwrap().1
        }
    }
}

/// Displacements (in points) of labels the user dragged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Offsets {
    pub title: (f32, f32),
    pub x_title: (f32, f32),
    pub y_title: (f32, f32),
    pub legend: (f32, f32),
}

/// A significance bracket between two data sets, typed by the user or
/// taken from an analysis. On grouped graphs `row` is the X axis group
/// (a row, or a data set when transposed) and `a`, `b` are bars within it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    /// Row group for grouped graphs (None on column graphs).
    pub row: Option<usize>,
    pub a: usize,
    pub b: usize,
    /// Text over the bracket, e.g. "**", "ns" or "P = 0.012".
    pub label: String,
}

impl Graph {
    pub fn new(id: u64, table_id: u64, kind: GraphKind, name: impl Into<String>) -> Self {
        Graph {
            id,
            name: name.into(),
            table_id,
            kind,
            error_bar: if kind == GraphKind::ScatterDot { ErrorBar::Sd } else { ErrorBar::Sem },
            title: String::new(),
            x_label: String::new(),
            y_label: String::new(),
            x_scale: Scale::Linear,
            y_scale: Scale::Linear,
            y_min: None,
            y_max: None,
            show_points: matches!(
                kind,
                GraphKind::ScatterDot | GraphKind::Xy | GraphKind::Bar | GraphKind::GroupedScatter | GraphKind::BoxWhisker
            ),
            connect_points: false,
            show_legend: true,
            palette: 0,
            width: 520.0,
            height: 400.0,
            marker_size: 4.0,
            line_width: 1.5,
            font_size: 13.0,
            overlay_analyses: Vec::new(),
            brackets: Vec::new(),
            auto_significance: AutoSignificance::default(),
            transpose: false,
            colors: Vec::new(),
            offsets: Offsets::default(),
            series_styles: Vec::new(),
            texts: Texts::default(),
            bar_opacity: default_bar_opacity(),
            bar_width: default_bar_width(),
            cap_width: 1.0,
            error_up_only: false,
            bracket_offsets: Vec::new(),
            widths: Widths::default(),
            x_tick_angle: None,
            y_tick_angle: 0.0,
            bracket_tick: 1.0,
            bracket_gap: 1.0,
            legend_labels: Vec::new(),
        }
    }
}

/// How bracket labels are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LabelStyle {
    /// ns, *, **, ***, ****
    #[default]
    Stars,
    /// P = 0.0123
    PValue,
    /// ** (P = 0.0034)
    Both,
}

impl LabelStyle {
    pub const ALL: [LabelStyle; 3] = [LabelStyle::Stars, LabelStyle::PValue, LabelStyle::Both];
    pub fn label(self) -> &'static str {
        match self {
            LabelStyle::Stars => "Asterisks",
            LabelStyle::PValue => "P values",
            LabelStyle::Both => "Asterisks and P",
        }
    }
    pub fn format(self, p: f64) -> String {
        let stars = hg_stats::p_summary(p);
        let pv = crate::format::p(p);
        let pv = if pv.starts_with('<') || pv.starts_with('>') { format!("P {pv}") } else { format!("P = {pv}") };
        match self {
            LabelStyle::Stars => stars.to_string(),
            LabelStyle::PValue => pv,
            LabelStyle::Both => format!("{stars} ({pv})"),
        }
    }
}

/// Significance brackets taken live from an analysis: they follow the data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AutoSignificance {
    pub analysis: Option<u64>,
    pub show_ns: bool,
    pub style: LabelStyle,
    /// Comparisons the user switched off: (group, a, b).
    pub hidden: Vec<(Option<usize>, usize, usize)>,
}

/// Brackets for the pairwise comparisons of an analysis, mapped to the graph
/// layout (`transposed` grouped graphs put data sets on the X axis).
pub fn auto_brackets(out: &crate::AnalysisOutput, auto: &AutoSignificance, transposed: bool) -> Vec<Bracket> {
    out.significance
        .iter()
        .filter(|s| auto.show_ns || s.p < 0.05)
        .filter(|s| s.group.is_none() || s.group_is_dataset == transposed)
        .filter(|s| !auto.hidden.contains(&(s.group, s.a, s.b)))
        .map(|s| Bracket { row: s.group, a: s.a, b: s.b, label: auto.style.format(s.p) })
        .collect()
}

/// Suggests the graphs that suit the data best, best first, each with a
/// short reason. Based on the kind of table, the number of values per group
/// and the shape of the design.
pub fn recommend(t: &crate::DataTable) -> Vec<((GraphKind, bool), String)> {
    let sets = t.nonempty_datasets();
    let ns: Vec<usize> = sets.iter().map(|&d| t.column_values(d).len()).collect();
    let max_n = ns.iter().copied().max().unwrap_or(0);
    let rows = t.used_rows();
    let mut out: Vec<((GraphKind, bool), String)> = Vec::new();
    let mut add = |k: GraphKind, tr: bool, why: &str| out.push(((k, tr), why.to_string()));
    match t.kind {
        TableKind::Column => {
            if max_n == 0 {
                add(GraphKind::ScatterDot, false, "Shows every value with the mean and SD: the most honest graph for small samples.");
            } else if max_n <= 12 {
                add(GraphKind::ScatterDot, false, "Few values per group: show every point, not just a bar.");
                add(GraphKind::BoxWhisker, false, "Median and quartiles, robust to outliers.");
                // Before-after is not suggested: equal group sizes do not mean the rows are the same subjects.
            } else if max_n <= 40 {
                add(GraphKind::BoxWhisker, false, "Medium samples: box plots summarize the spread; turn on the points to see them all.");
                add(GraphKind::ScatterDot, false, "Every value with mean and SD.");
                add(GraphKind::Violin, false, "Shows the shape of each distribution.");
            } else {
                add(GraphKind::Violin, false, "Many values per group: a violin shows the shape of the distribution.");
                add(GraphKind::BoxWhisker, false, "Median and quartiles.");
                add(GraphKind::Histogram, false, "Frequency distribution of each group.");
            }
            add(GraphKind::QqPlot, false, "Check whether each group follows a Gaussian distribution.");
        }
        TableKind::Grouped => {
            let numeric_rows = rows > 2
                && (0..rows).all(|r| {
                    let title = t.row_label(r);
                    title.chars().any(|c| c.is_ascii_digit())
                });
            if rows >= 6 && sets.len() >= 4 {
                add(GraphKind::Heatmap, false, "Many rows and columns: a heat map shows the whole table at a glance.");
            }
            if numeric_rows {
                add(GraphKind::GroupedLine, false, "The rows look like times or doses: lines show how each group changes.");
            }
            if t.replicates > 1 && t.replicates <= 12 {
                add(GraphKind::GroupedScatter, false, "Few replicates per cell: show every value.");
                add(GraphKind::GroupedBar, false, "Bars with error bars, a classic for grouped data.");
            } else if t.replicates > 12 {
                add(GraphKind::GroupedBox, false, "Many replicates per cell: box plots summarize each cell.");
            } else {
                add(GraphKind::GroupedBar, false, "One value per cell: bars compare them directly.");
            }
            add(GraphKind::GroupedBar, true, "The same graph grouped by column instead of by row.");
        }
        TableKind::Contingency => {
            add(GraphKind::StackedBar, false, "Stacked bars show how the outcomes split within each group.");
            add(GraphKind::GroupedBar, false, "Side-by-side counts.");
            if rows * sets.len() > 12 {
                add(GraphKind::Heatmap, false, "A large table of counts reads best as a heat map.");
            }
        }
        TableKind::XY => add(GraphKind::Xy, false, "Points with error bars and fitted curves."),
        TableKind::Survival => add(GraphKind::Survival, false, "Kaplan-Meier curves with censored subjects marked."),
        TableKind::PartsOfWhole => {
            if rows <= 6 {
                add(GraphKind::Pie, false, "Few categories: a pie or donut is easy to read.");
            } else {
                add(GraphKind::StackedBar, false, "Many categories: a bar is easier to compare than a pie.");
            }
            add(GraphKind::Donut, false, "A donut leaves room for a label in the middle.");
        }
    }
    let mut seen = Vec::new();
    out.retain(|(c, _)| {
        let new = !seen.contains(c);
        seen.push(*c);
        new
    });
    out
}
