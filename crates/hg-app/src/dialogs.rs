//! Welcome / new table, graph gallery and Analyze dialogs.

use crate::app::{App, Selection};
use eframe::egui::{self, Color32, RichText};
use hg_core::analysis::{AnalysisKind, OneWayTest, Options, Tails, TwoGroupTest};
use hg_core::sample::sample_table;
use hg_core::{DataTable, GraphKind, Project, TableKind};
use hg_plot::Scene;
use hg_stats::nonlinear::Model;
use hg_stats::posthoc::Method;

/// A graph choice in the gallery: (kind, transposed).
pub type Choice = (GraphKind, bool);

/// Renders a small preview of every graph choice available for a table.
fn thumbnails(project: &Project, table_id: u64) -> Vec<(Choice, Option<Scene>)> {
    let Some(t) = project.table(table_id) else { return Vec::new() };
    let kind = t.kind;
    // Empty tables are previewed with sample data of the same kind.
    let mut p = project.clone();
    let tid = if t.nonempty_datasets().is_empty() { p.add_table(sample_table(kind)) } else { table_id };
    // Short labels keep the tiny previews readable.
    if let Some(tt) = p.table_mut(tid) {
        for (i, d) in tt.datasets.iter_mut().enumerate() {
            d.title = hg_core::table::dataset_letter(i);
        }
        for (i, r) in tt.row_titles.iter_mut().enumerate() {
            *r = (i + 1).to_string();
        }
        tt.x_title.clear();
    }
    GraphKind::gallery(kind)
        .into_iter()
        .map(|(k, tr)| {
            let gid = p.add_graph(tid, k);
            let mut g = p.graph(gid).unwrap().clone();
            g.transpose = tr;
            g.width = 300.0;
            g.height = 230.0;
            g.font_size = 16.0;
            g.marker_size = 6.0;
            g.line_width = 2.5;
            g.show_legend = false;
            g.y_label.clear();
            ((k, tr), hg_plot::render(&p, &g).ok())
        })
        .collect()
}

/// Gallery of graph thumbnails; returns the choice clicked.
fn gallery_ui(
    ui: &mut egui::Ui,
    kind: TableKind,
    thumbs: &[(Choice, Option<Scene>)],
    selected: Option<Choice>,
    recs: &[(Choice, String)],
) -> Option<Choice> {
    let mut picked = None;
    let p = crate::theme::pal();
    ui.horizontal_wrapped(|ui| {
        for (k, scene) in thumbs {
            let size = egui::vec2(132.0, 132.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let on = selected == Some(*k);
            let bg = if on {
                crate::theme::pal().selected
            } else if resp.hovered() {
                crate::theme::pal().hover
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, 6.0, bg);
            if on {
                ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(2.0_f32, crate::theme::accent()), egui::StrokeKind::Inside);
            }
            let img = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 6.0), egui::vec2(116.0, 90.0));
            if let Some(s) = scene {
                crate::paint::paint(ui.painter(), img, s);
            }
            let label =
                ui.painter().layout(k.0.gallery_label(k.1, kind), egui::FontId::proportional(11.5), crate::theme::pal().text, 124.0);
            ui.painter().galley(egui::pos2(rect.center().x - label.size().x / 2.0, img.bottom() + 4.0), label, crate::theme::pal().text);
            if resp.clicked() {
                picked = Some(*k);
            }
        }
    });
    // Suggestions for the data entered, below the gallery: click one to pick it.
    if !recs.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(crate::icons::image("wand-sparkles", 14.0, p.accent));
            ui.label(RichText::new("Suggested for these data").strong().color(p.accent));
        });
        for (k, why) in recs.iter().take(3) {
            let text = RichText::new(format!("{} — {why}", k.0.gallery_label(k.1, kind)));
            if ui.add(egui::SelectableLabel::new(selected == Some(*k), text)).clicked() {
                picked = Some(*k);
            }
        }
    }
    picked
}

fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).font(crate::theme::bold(14.0)).color(crate::theme::pal().accent_text))
            .fill(crate::theme::accent())
            .min_size(egui::vec2(104.0, 32.0)),
    )
    .clicked()
}

// ---------------------------------------------------------------- Welcome

pub struct WelcomeDialog {
    /// Shown at startup: offers to open an existing project too.
    startup: bool,
    kind: TableKind,
    name: String,
    sample: bool,
    /// Observations per group (subcolumns) for grouped and XY tables.
    replicates: usize,
    graph: Option<Choice>,
    thumbs: Option<(TableKind, Vec<(Choice, Option<Scene>)>)>,
}

impl WelcomeDialog {
    pub fn new(startup: bool) -> Self {
        WelcomeDialog {
            startup,
            kind: TableKind::Column,
            name: String::new(),
            sample: false,
            replicates: 1,
            graph: Some((GraphKind::ScatterDot, false)),
            thumbs: None,
        }
    }
}

fn table_icon(kind: TableKind) -> &'static str {
    match kind {
        TableKind::XY => "📈",
        TableKind::Column => "📊",
        TableKind::Grouped => "⊞",
        TableKind::Contingency => "#",
        TableKind::Survival => "⏳",
        TableKind::PartsOfWhole => "◔",
    }
}

pub fn welcome_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(d) = app.welcome.as_mut() else { return };
    if d.thumbs.as_ref().is_none_or(|t| t.0 != d.kind) {
        let mut p = Project::default();
        let id = p.add_table(sample_table(d.kind));
        d.thumbs = Some((d.kind, thumbnails(&p, id)));
    }
    let mut open = true;
    let mut create = false;
    let mut open_file = false;
    let title = if d.startup { "Welcome to HexaGraph" } else { "New data table and graph" };
    let default_name = format!("Data {}", app.project.tables.len() + 1);
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
        ctx,
        |ui| {
            ui.set_max_height(600.0);
            ui.horizontal_top(|ui| {
                // Left: what to create.
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    ui.label(RichText::new("CREATE").strong().color(crate::theme::accent()));
                    for k in TableKind::ALL {
                        let text = RichText::new(format!("{}  {}", table_icon(k), k.label())).size(15.0);
                        if ui.selectable_label(d.kind == k, text).clicked() && d.kind != k {
                            d.kind = k;
                            d.graph = Some((GraphKind::for_table(k)[0], false));
                            d.replicates = if k == TableKind::Grouped { 3 } else { 1 };
                        }
                    }
                    if d.startup {
                        ui.add_space(14.0);
                        ui.label(RichText::new("OPEN").strong().color(crate::theme::accent()));
                        if ui
                            .selectable_label(false, RichText::new("📂  Open a project (.hxn, Prism .prism or .pzfx)…").size(15.0))
                            .clicked()
                        {
                            open_file = true;
                        }
                    }
                });
                ui.separator();
                // Right: options for the selected kind.
                ui.vertical(|ui| {
                    ui.set_width(600.0);
                    ui.label(RichText::new(format!("{} table", d.kind.label())).strong().size(18.0));
                    ui.label(d.kind.description());
                    ui.add_space(8.0);
                    ui.label(RichText::new("Data table").strong());
                    ui.radio_value(&mut d.sample, false, "Enter or import data into a new table");
                    ui.radio_value(&mut d.sample, true, "Start with sample data to follow an example");
                    if !d.sample {
                        ui.horizontal(|ui| {
                            ui.label("Table name");
                            ui.add(egui::TextEdit::singleline(&mut d.name).hint_text(&default_name));
                        });
                        if d.kind.allows_replicates() {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let what = if d.kind == TableKind::Grouped { "Observations per group" } else { "Replicate Y values per X" };
                                ui.label(what);
                                ui.add(egui::DragValue::new(&mut d.replicates).range(1..=100));
                                let help = if d.kind == TableKind::Grouped {
                                    "side-by-side subcolumns for the values of each condition (you can change it later)"
                                } else {
                                    "side-by-side subcolumns (you can change it later)"
                                };
                                ui.label(RichText::new(help).weak().small());
                            });
                        }
                        ui.label(
                            RichText::new("The sheet grows as you type: add as many groups, columns and rows as you need.").weak().small(),
                        );
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Graph").strong());
                        ui.label(RichText::new("— choose how the data will be plotted (you can change it later)").weak());
                    });
                    if let Some((_, thumbs)) = &d.thumbs {
                        // No data yet: suggestions come later, when a graph is made from the table.
                        if let Some(k) = gallery_ui(ui, d.kind, thumbs, d.graph, &[]) {
                            d.graph = Some(k);
                        }
                    }
                    let mut none = d.graph.is_none();
                    if ui.checkbox(&mut none, "Don't create a graph now").changed() {
                        d.graph = if none { None } else { Some((GraphKind::for_table(d.kind)[0], false)) };
                    }
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, "Create", true) {
                    create = true;
                }
            });
        },
    );
    if open_file {
        app.welcome = None;
        app.request(crate::app::Pending::Open);
        return;
    }
    if create {
        let d = app.welcome.take().unwrap();
        let table = if d.sample {
            sample_table(d.kind)
        } else {
            let name = if d.name.trim().is_empty() { default_name } else { d.name.clone() };
            DataTable::new(0, name, d.kind, 1, 0, d.replicates)
        };
        let id = app.project.add_table(table);
        if let Some((k, tr)) = d.graph {
            let gid = app.project.add_graph(id, k);
            if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == gid) {
                g.transpose = tr;
            }
        }
        app.selection = Selection::Table(id);
        app.grid = Default::default();
        app.touch();
    } else if !open {
        app.welcome = None;
    }
}

// ---------------------------------------------------------------- Graph gallery

pub struct GraphGallery {
    table_id: u64,
    selected: Option<Choice>,
    thumbs: Option<Vec<(Choice, Option<Scene>)>>,
    /// Change the type of this graph instead of creating a new one.
    retype: Option<u64>,
}

impl GraphGallery {
    pub fn new(table_id: u64) -> Self {
        GraphGallery { table_id, selected: None, thumbs: None, retype: None }
    }

    pub fn retype(table_id: u64, graph: u64) -> Self {
        GraphGallery { table_id, selected: None, thumbs: None, retype: Some(graph) }
    }
}

pub fn graph_gallery(app: &mut App, ctx: &egui::Context) {
    let Some(d) = app.gallery.as_mut() else { return };
    let Some(t) = app.project.table(d.table_id) else {
        app.gallery = None;
        return;
    };
    let tname = t.name.clone();
    let tkind = t.kind;
    let recs = if t.nonempty_datasets().is_empty() { Vec::new() } else { hg_core::graph::recommend(t) };
    if d.thumbs.is_none() {
        d.thumbs = Some(thumbnails(&app.project, d.table_id));
        let current = d.retype.and_then(|g| app.project.graph(g)).map(|g| (g.kind, g.transpose));
        d.selected = current.or_else(|| recs.first().map(|r| r.0)).or_else(|| d.thumbs.as_ref().and_then(|v| v.first()).map(|v| v.0));
    }
    let mut open = true;
    let mut ok = false;
    let title = if d.retype.is_some() { "Change graph type" } else { "New graph" };
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
        ctx,
        |ui| {
            ui.set_width(600.0);
            ui.label(format!("Graph the data in “{tname}” as:"));
            ui.add_space(6.0);
            if let Some(k) = gallery_ui(ui, tkind, d.thumbs.as_ref().unwrap(), d.selected, &recs) {
                d.selected = Some(k);
            }
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, if d.retype.is_some() { "Apply" } else { "Create graph" }, d.selected.is_some()) {
                    ok = true;
                }
            });
        },
    );
    if ok {
        let d = app.gallery.take().unwrap();
        let (k, tr) = d.selected.unwrap();
        let id = d.retype.unwrap_or_else(|| app.project.add_graph(d.table_id, k));
        if let Some(g) = app.project.graphs.iter_mut().find(|g| g.id == id) {
            g.kind = k;
            g.transpose = tr;
        }
        app.selection = Selection::Graph(id);
        app.touch();
    } else if !open {
        app.gallery = None;
    }
}

// ---------------------------------------------------------------- Analyze

pub struct AnalyzeDialog {
    table_id: u64,
    titles: Vec<String>,
    row_titles: Vec<String>,
    catalog: Vec<(String, Vec<AnalysisKind>)>,
    /// (category, index) of the chosen analysis.
    selected: (usize, usize),
    kind: AnalysisKind,
    datasets: Vec<bool>,
    options: Options,
    editing: Option<u64>,
    table_kind: TableKind,
    /// Values left out of the analysis: (data set, row, replicate).
    excluded: Vec<(usize, usize, usize)>,
}

impl AnalyzeDialog {
    pub fn new(t: &DataTable, existing: Option<&hg_core::Analysis>) -> Self {
        let catalog = AnalysisKind::catalog_all(t.kind);
        let find = |pred: &dyn Fn(&AnalysisKind) -> bool| {
            catalog.iter().enumerate().find_map(|(c, (_, v))| v.iter().position(pred).map(|i| (c, i)))
        };
        let (selected, kind) = match existing {
            Some(a) => {
                let label = a.kind.label();
                let pos = find(&|k| k.label() == label)
                    .or_else(|| find(&|k| std::mem::discriminant(k) == std::mem::discriminant(&a.kind)))
                    .unwrap_or((0, 0));
                (pos, a.kind.clone())
            }
            None => ((0, 0), catalog[0].1[0].clone()),
        };
        let n = t.datasets.len();
        let datasets = match existing.and_then(|a| a.datasets.clone()) {
            Some(sel) => (0..n).map(|i| sel.contains(&i)).collect(),
            None => (0..n).map(|i| !t.column_values(i).is_empty()).collect(),
        };
        let mut d = AnalyzeDialog {
            table_id: t.id,
            titles: t.datasets.iter().map(|d| d.title.clone()).collect(),
            row_titles: (0..t.used_rows()).map(|r| t.row_label(r)).collect(),
            catalog,
            selected,
            kind,
            datasets,
            options: existing.map(|a| a.options.clone()).unwrap_or_default(),
            editing: existing.map(|a| a.id),
            table_kind: t.kind,
            excluded: existing.map(|a| a.excluded.clone()).unwrap_or_default(),
        };
        if existing.is_none() {
            d.default_groups();
        }
        d
    }

    /// Two-group analyses default to the first two ticked data sets.
    fn default_groups(&mut self) {
        let ticked: Vec<usize> = (0..self.datasets.len()).filter(|&i| self.datasets[i]).collect();
        let (x, y) = (ticked.first().copied().unwrap_or(0), ticked.get(1).copied().unwrap_or(1));
        match &mut self.kind {
            AnalysisKind::TwoGroups { a, b, .. } | AnalysisKind::BlandAltman { a, b } => {
                *a = x;
                *b = y;
            }
            AnalysisKind::Roc { controls, patients } => {
                *controls = x;
                *patients = y;
            }
            _ => {}
        }
    }
}

fn dataset_combo(ui: &mut egui::Ui, salt: &str, titles: &[String], v: &mut usize) {
    egui::ComboBox::from_id_salt(salt).selected_text(titles.get(*v).cloned().unwrap_or_default()).show_ui(ui, |ui| {
        for (i, t) in titles.iter().enumerate() {
            ui.selectable_value(v, i, t);
        }
    });
}

/// Multiple comparison methods: Tukey for all pairs, Dunnett against a control.
fn comparisons_combo(ui: &mut egui::Ui, v: &mut Option<Method>, all_pairs: bool) {
    let first = if all_pairs { Method::Tukey } else { Method::Dunnett };
    egui::ComboBox::from_id_salt("cmp").selected_text(v.map_or("None", |m| m.label())).show_ui(ui, |ui| {
        ui.selectable_value(v, None, "None");
        for m in [first, Method::Bonferroni, Method::Sidak, Method::HolmSidak, Method::FisherLsd] {
            ui.selectable_value(v, Some(m), m.label());
        }
    });
}

/// Options specific to one analysis.
fn parameters(ui: &mut egui::Ui, kind: &mut AnalysisKind, titles: &[String], row_titles: &[String]) {
    egui::Grid::new("params").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| match kind {
        AnalysisKind::OneSample { hypothetical, .. } => {
            ui.label("Hypothetical value");
            ui.add(egui::DragValue::new(hypothetical).speed(0.1));
            ui.end_row();
        }
        AnalysisKind::Outliers { alpha } => {
            ui.label("Grubbs' alpha");
            ui.add(egui::DragValue::new(alpha).range(0.0001..=0.2).speed(0.001));
            ui.end_row();
        }
        AnalysisKind::TwoGroups { a, b, test } => {
            ui.label("Experimental design");
            ui.vertical(|ui| {
                let mut paired = test.paired();
                ui.horizontal(|ui| {
                    if ui.radio_value(&mut paired, false, "Unpaired").changed()
                        || ui.radio_value(&mut paired, true, "Paired (matched rows)").changed()
                    {
                        *test = match (paired, test.parametric()) {
                            (true, true) => TwoGroupTest::Paired,
                            (true, false) => TwoGroupTest::Wilcoxon,
                            (false, true) => TwoGroupTest::Unpaired,
                            (false, false) => TwoGroupTest::MannWhitney,
                        };
                    }
                });
                let mut parametric = test.parametric();
                ui.horizontal(|ui| {
                    if ui.radio_value(&mut parametric, true, "Parametric (t test)").changed()
                        || ui.radio_value(&mut parametric, false, "Nonparametric").changed()
                    {
                        *test = match (test.paired(), parametric) {
                            (true, true) => TwoGroupTest::Paired,
                            (true, false) => TwoGroupTest::Wilcoxon,
                            (false, true) => TwoGroupTest::Unpaired,
                            (false, false) => TwoGroupTest::MannWhitney,
                        };
                    }
                });
            });
            ui.end_row();
            ui.label("Test");
            egui::ComboBox::from_id_salt("tt").width(280.0).selected_text(test.label()).show_ui(ui, |ui| {
                for t in TwoGroupTest::ALL {
                    ui.selectable_value(test, t, t.label());
                }
            });
            ui.end_row();
            ui.label("Group A");
            dataset_combo(ui, "ga", titles, a);
            ui.end_row();
            ui.label("Group B");
            dataset_combo(ui, "gb", titles, b);
            ui.end_row();
        }
        AnalysisKind::OneWay { test, comparisons, control } => {
            ui.label("Test");
            egui::ComboBox::from_id_salt("ow").width(280.0).selected_text(test.label()).show_ui(ui, |ui| {
                for t in OneWayTest::ALL {
                    ui.selectable_value(test, t, t.label());
                }
            });
            ui.end_row();
            ui.label("Compare");
            ui.vertical(|ui| {
                let mut vs = control.is_some();
                ui.radio_value(&mut vs, false, "Every group with every other group");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut vs, true, "Every group with a control:");
                    if vs {
                        let mut c = control.unwrap_or(0);
                        dataset_combo(ui, "ctrl", titles, &mut c);
                        *control = Some(c);
                    }
                });
                if !vs {
                    *control = None;
                }
            });
            ui.end_row();
            ui.label("Multiple comparisons");
            match test {
                OneWayTest::KruskalWallis => {
                    let mut on = comparisons.is_some();
                    ui.checkbox(&mut on, "Dunn's test");
                    *comparisons = on.then_some(Method::Bonferroni);
                }
                OneWayTest::Friedman | OneWayTest::Welch => {
                    ui.label(RichText::new("not available yet").weak());
                }
                _ => {
                    if control.is_some() && *comparisons == Some(Method::Tukey) {
                        *comparisons = Some(Method::Dunnett);
                    }
                    if control.is_none() && *comparisons == Some(Method::Dunnett) {
                        *comparisons = Some(Method::Tukey);
                    }
                    comparisons_combo(ui, comparisons, control.is_none());
                }
            }
            ui.end_row();
        }
        AnalysisKind::MultipleT { a, b, welch, fdr } => {
            ui.label("Data set A");
            dataset_combo(ui, "ma", titles, a);
            ui.end_row();
            ui.label("Data set B");
            dataset_combo(ui, "mb", titles, b);
            ui.end_row();
            ui.label("Each test");
            ui.vertical(|ui| {
                ui.radio_value(welch, false, "Unpaired t test (equal SDs)");
                ui.radio_value(welch, true, "Welch's t test (unequal SDs)");
            });
            ui.end_row();
            ui.label("Correction");
            ui.vertical(|ui| {
                ui.radio_value(fdr, Some(true), "False discovery rate (Benjamini-Hochberg)");
                ui.radio_value(fdr, Some(false), "Holm-Šídák (family-wise)");
                ui.radio_value(fdr, None, "None");
            });
            ui.end_row();
        }
        AnalysisKind::TwoWay { comparisons, within_columns, repeated } => {
            ui.label("Design");
            ui.vertical(|ui| {
                ui.radio_value(repeated, false, "Ordinary (independent replicates)");
                ui.radio_value(repeated, true, "Repeated measures: each replicate column is one subject across rows");
            });
            ui.end_row();
            ui.label("Multiple comparisons");
            comparisons_combo(ui, comparisons, true);
            ui.end_row();
            ui.label("Compare");
            ui.vertical(|ui| {
                ui.radio_value(within_columns, false, "Data sets (columns) within each row");
                ui.radio_value(within_columns, true, "Rows within each data set (column)");
            });
            ui.end_row();
            let _ = row_titles;
        }
        AnalysisKind::Correlation { spearman } => {
            ui.label("Method");
            ui.horizontal(|ui| {
                ui.radio_value(spearman, false, "Pearson");
                ui.radio_value(spearman, true, "Spearman (nonparametric)");
            });
            ui.end_row();
        }
        AnalysisKind::NonlinearFit { model, fixed } => {
            ui.label("Equation");
            egui::ComboBox::from_id_salt("model").width(330.0).selected_text(model.name()).show_ui(ui, |ui| {
                for m in Model::ALL {
                    if ui.selectable_value(model, m, m.name()).changed() {
                        fixed.clear();
                    }
                }
            });
            ui.end_row();
            ui.label("");
            ui.label(RichText::new(model.equation()).monospace());
            ui.end_row();
            ui.label("Constraints");
            ui.vertical(|ui| {
                let names = model.parameters();
                fixed.resize(names.len(), None);
                for (i, name) in names.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut on = fixed[i].is_some();
                        if ui.checkbox(&mut on, format!("{name} must equal")).changed() {
                            fixed[i] = on.then_some(match (*model, *name) {
                                (_, "Top") => 100.0,
                                _ => 0.0,
                            });
                        }
                        if let Some(v) = fixed[i].as_mut() {
                            ui.add(egui::DragValue::new(v).speed(0.1));
                        }
                    });
                }
            });
            ui.end_row();
        }
        AnalysisKind::AreaUnderCurve { baseline } => {
            ui.label("Baseline Y");
            ui.add(egui::DragValue::new(baseline).speed(0.1));
            ui.end_row();
        }
        AnalysisKind::Survival { gehan } => {
            ui.label("Compare curves with");
            ui.vertical(|ui| {
                ui.radio_value(gehan, false, "Log-rank (Mantel-Cox)");
                ui.radio_value(gehan, true, "Gehan-Breslow-Wilcoxon");
            });
            ui.end_row();
        }
        AnalysisKind::Contingency { yates } => {
            ui.label("2×2 tables");
            ui.checkbox(yates, "Yates' continuity correction");
            ui.end_row();
        }
        AnalysisKind::BlandAltman { a, b } => {
            ui.label("Method A");
            dataset_combo(ui, "ba", titles, a);
            ui.end_row();
            ui.label("Method B");
            dataset_combo(ui, "bb", titles, b);
            ui.end_row();
        }
        AnalysisKind::Roc { controls, patients } => {
            ui.label("Controls");
            dataset_combo(ui, "rc", titles, controls);
            ui.end_row();
            ui.label("Patients");
            dataset_combo(ui, "rp", titles, patients);
            ui.end_row();
        }
        AnalysisKind::ColumnStatistics | AnalysisKind::Normality | AnalysisKind::LinearRegression | AnalysisKind::FractionOfTotal => {
            ui.label(RichText::new("No specific options.").weak());
            ui.end_row();
        }
    });
}

/// Options common to all analyses: confidence level, alpha and tails.
fn general_options(ui: &mut egui::Ui, kind: &AnalysisKind, o: &mut Options, titles: &[String]) {
    egui::Grid::new("general").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label("Confidence intervals");
        ui.horizontal(|ui| {
            for level in [0.90, 0.95, 0.99] {
                ui.selectable_value(&mut o.ci_level, level, format!("{}%", (level * 100.0).round()));
            }
        });
        ui.end_row();
        ui.label("Significance level (α)");
        ui.horizontal(|ui| {
            for a in [0.01, 0.05, 0.10] {
                ui.selectable_value(&mut o.alpha, a, format!("{a}"));
            }
            ui.add(egui::DragValue::new(&mut o.alpha).range(0.0001..=0.5).speed(0.001));
        });
        ui.end_row();
        if kind.supports_tails() {
            let (a, b) = match kind {
                AnalysisKind::TwoGroups { a, b, .. } => {
                    (titles.get(*a).cloned().unwrap_or_default(), titles.get(*b).cloned().unwrap_or_default())
                }
                _ => ("hypothetical".to_string(), "mean".to_string()),
            };
            ui.label("P value");
            ui.vertical(|ui| {
                ui.radio_value(&mut o.tails, Tails::Two, "Two-tailed (recommended)");
                ui.radio_value(&mut o.tails, Tails::Greater, format!("One-tailed: {b} > {a}"));
                ui.radio_value(&mut o.tails, Tails::Less, format!("One-tailed: {b} < {a}"));
            });
            ui.end_row();
        }
    });
}

pub fn analyze_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(d) = app.analyze.as_mut() else { return };
    let tname = app.project.table(d.table_id).map_or(String::new(), |t| t.name.clone());
    let mut open = true;
    let mut ok = false;
    let title = if d.editing.is_some() { "Parameters" } else { "Analyze data" };
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
        ctx,
        |ui| {
            ui.set_max_height(620.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    ui.label(RichText::new("Which analysis?").strong().color(crate::theme::accent()));
                    egui::ScrollArea::vertical().id_salt("which").max_height(540.0).show(ui, |ui| {
                        let mut pick: Option<(usize, usize)> = None;
                        for (c, (cat, items)) in d.catalog.iter().enumerate() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(cat.as_str()).strong());
                            for (i, k) in items.iter().enumerate() {
                                let label = match k {
                                    AnalysisKind::NonlinearFit { .. } => "Nonlinear regression (curve fit)".to_string(),
                                    _ => k.label(),
                                };
                                if ui.selectable_label(d.selected == (c, i), format!("   {label}")).clicked() {
                                    pick = Some((c, i));
                                }
                            }
                        }
                        if let Some(p) = pick.filter(|p| *p != d.selected) {
                            d.selected = p;
                            d.kind = d.catalog[p.0].1[p.1].clone();
                            d.default_groups();
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("opts").max_height(560.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(440.0);
                        ui.label(RichText::new(d.kind.label()).strong().size(17.0));
                        ui.label(RichText::new(d.kind.help()).weak());
                        match d.kind.suitability(d.table_kind) {
                            Err(e) => callout(ui, "ban", &format!("Not possible on this table: {e}."), true),
                            Ok(Some(w)) => callout(ui, "triangle-alert", &w, false),
                            Ok(None) => {}
                        }
                        ui.add_space(8.0);
                        if d.kind.uses_dataset_selection() {
                            ui.label(RichText::new(format!("Data sets of “{tname}” to analyze")).strong().color(crate::theme::accent()));
                            ui.horizontal_wrapped(|ui| {
                                for (i, t) in d.titles.iter().enumerate() {
                                    ui.checkbox(&mut d.datasets[i], t);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.small_button("Select all").clicked() {
                                    d.datasets.iter_mut().for_each(|v| *v = true);
                                }
                                if ui.small_button("Select none").clicked() {
                                    d.datasets.iter_mut().for_each(|v| *v = false);
                                }
                            });
                            ui.add_space(8.0);
                        }
                        ui.label(RichText::new("Options").strong().color(crate::theme::accent()));
                        parameters(ui, &mut d.kind, &d.titles, &d.row_titles);
                        ui.add_space(8.0);
                        ui.label(RichText::new("Statistics").strong().color(crate::theme::accent()));
                        general_options(ui, &d.kind, &mut d.options, &d.titles);
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("check").max_height(560.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(330.0);
                        if let Some(t) = app.project.table(d.table_id) {
                            let view = t.analysis_view();
                            if let Some(suggestion) = data_check(ui, &view, d) {
                                let label = suggestion.label();
                                if let Some(pos) = d
                                    .catalog
                                    .iter()
                                    .enumerate()
                                    .find_map(|(c, (_, v))| v.iter().position(|k| k.label() == label).map(|i| (c, i)))
                                {
                                    d.selected = pos;
                                }
                                d.kind = suggestion;
                            }
                        }
                    });
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let any = (!d.kind.uses_dataset_selection() || d.datasets.iter().any(|v| *v)) && d.kind.suitability(d.table_kind).is_ok();
                if primary(ui, "OK", any) {
                    ok = true;
                }
            });
        },
    );
    if ok {
        let d = app.analyze.take().unwrap();
        let selection = (d.kind.uses_dataset_selection() && d.datasets.iter().any(|v| !v))
            .then(|| (0..d.datasets.len()).filter(|&i| d.datasets[i]).collect::<Vec<_>>());
        let id = match d.editing {
            Some(id) => id,
            None => app.project.add_analysis_of(d.table_id, d.kind.clone(), selection.clone()),
        };
        if let Some(a) = app.project.analyses.iter_mut().find(|a| a.id == id) {
            a.kind = d.kind;
            a.datasets = selection;
            a.options = d.options;
            a.excluded = d.excluded;
        }
        app.selection = Selection::Analysis(id);
        app.touch();
    } else if !open {
        app.analyze = None;
    }
}

fn callout(ui: &mut egui::Ui, icon: &str, text: &str, error: bool) {
    let p = crate::theme::pal();
    let (bg, fg) = if error { (p.error_bg, p.error_text) } else { (p.warn_bg, p.warn_text) };
    ui.add_space(6.0);
    egui::Frame::new().fill(bg).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add(crate::icons::image(icon, 16.0, fg));
            ui.add(egui::Label::new(RichText::new(text).color(fg)).wrap());
        });
    });
}

/// Values of a data set with their positions, without the excluded ones.
fn located_values(t: &DataTable, d: usize, excluded: &[(usize, usize, usize)]) -> Vec<(f64, usize, usize)> {
    let Some(ds) = t.datasets.get(d) else { return Vec::new() };
    ds.values
        .iter()
        .enumerate()
        .flat_map(|(r, row)| row.iter().enumerate().filter_map(move |(k, v)| v.map(|v| (v, r, k))))
        .filter(|(_, r, k)| !excluded.contains(&(d, *r, *k)))
        .collect()
}

fn badge(ui: &mut egui::Ui, ok: Option<bool>, yes: &str, no: &str) {
    let p = crate::theme::pal();
    let (text, color, icon) = match ok {
        Some(true) => (yes, p.good, "circle-check"),
        Some(false) => (no, p.warn_text, "triangle-alert"),
        None => ("too few values", p.weak, "info"),
    };
    ui.add(crate::icons::image(icon, 14.0, color));
    ui.label(RichText::new(text).small().color(color));
}

/// Checks the data before the test: outliers (Grubbs and Tukey fences),
/// normality (Shapiro-Wilk) and equal spread (Brown-Forsythe), then suggests
/// a test. Returns the suggested analysis when the user accepts it.
fn data_check(ui: &mut egui::Ui, t: &DataTable, d: &mut AnalyzeDialog) -> Option<AnalysisKind> {
    use hg_stats::{descriptive, normality, outliers};
    let p = crate::theme::pal();
    ui.horizontal(|ui| {
        ui.add(crate::icons::image("shield-check", 18.0, p.accent));
        ui.label(RichText::new("Data check").font(crate::theme::bold(16.0)));
    });
    let groups: Vec<usize> = match &d.kind {
        AnalysisKind::TwoGroups { a, b, .. } => vec![*a, *b],
        k if k.uses_dataset_selection() => (0..d.datasets.len()).filter(|&i| d.datasets[i] && !t.column_values(i).is_empty()).collect(),
        _ => Vec::new(),
    };
    if groups.is_empty() {
        ui.label(RichText::new("Nothing to check for this analysis.").weak());
        return None;
    }
    let mut samples: Vec<Vec<f64>> = Vec::new();
    let mut toggle: Option<(usize, usize, usize)> = None;
    for &g in &groups {
        let vals = located_values(t, g, &d.excluded);
        let x: Vec<f64> = vals.iter().map(|v| v.0).collect();
        samples.push(x.clone());
        egui::Frame::new().fill(p.panel_alt).corner_radius(8.0).inner_margin(8.0).stroke(egui::Stroke::new(1.0_f32, p.border)).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                let title = t.datasets.get(g).map_or("?".to_string(), |s| s.title.clone());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).font(crate::theme::bold(13.5)));
                    if let Ok(s) = descriptive::describe(&x) {
                        ui.label(
                            RichText::new(format!(
                                "n = {} · mean {} · SD {} · median {}",
                                s.n,
                                hg_core::format::num(s.mean),
                                hg_core::format::num(s.sd),
                                hg_core::format::num(s.median)
                            ))
                            .small()
                            .color(p.weak),
                        );
                    }
                });
                let normal = normality::shapiro_wilk(&x).ok().map(|r| r.p >= 0.05);
                ui.horizontal(|ui| badge(ui, normal, "looks Gaussian (Shapiro-Wilk)", "not Gaussian (Shapiro-Wilk P < 0.05)"));
                // Outliers: Grubbs (significant) and Tukey fences (suspect).
                let grubbs: Vec<usize> = outliers::grubbs(&x, 0.05).map(|g| g.outliers).unwrap_or_default();
                let sorted = descriptive::sorted(&x);
                let (q1, q3) = (descriptive::percentile_sorted(&sorted, 0.25), descriptive::percentile_sorted(&sorted, 0.75));
                let iqr = q3 - q1;
                let flagged: Vec<(usize, &str)> = (0..vals.len())
                    .filter_map(|i| {
                        if grubbs.contains(&i) {
                            Some((i, "outlier (Grubbs)"))
                        } else if x.len() >= 4 && (x[i] < q1 - 1.5 * iqr || x[i] > q3 + 1.5 * iqr) {
                            Some((i, "suspect (beyond 1.5×IQR)"))
                        } else {
                            None
                        }
                    })
                    .collect();
                if flagged.is_empty() {
                    ui.horizontal(|ui| badge(ui, Some(true), "no outliers", ""));
                }
                for (i, why) in flagged {
                    let (v, r, k) = vals[i];
                    ui.horizontal(|ui| {
                        ui.add(crate::icons::image("triangle-alert", 14.0, p.warn_text));
                        ui.label(RichText::new(format!("{} (row {}) {why}", hg_core::format::num(v), r + 1)).small().color(p.warn_text));
                        if ui.small_button("Exclude").on_hover_text("Leave this value out of this analysis only").clicked() {
                            toggle = Some((g, r, k));
                        }
                    });
                }
            },
        );
        ui.add_space(4.0);
    }
    // Values already excluded.
    let excluded_here: Vec<(usize, usize, usize)> = d.excluded.iter().copied().filter(|(g, _, _)| groups.contains(g)).collect();
    if !excluded_here.is_empty() {
        ui.label(RichText::new("Excluded from this analysis").small().color(p.weak));
        for (g, r, k) in excluded_here {
            let v = t.datasets.get(g).and_then(|s| s.values.get(r)).and_then(|row| row.get(k)).copied().flatten();
            ui.horizontal(|ui| {
                ui.add(crate::icons::image("ban", 14.0, p.weak));
                ui.label(
                    RichText::new(format!("{} — {} row {}", v.map_or("—".into(), hg_core::format::num), t.datasets[g].title, r + 1))
                        .small(),
                );
                if ui.small_button("Restore").clicked() {
                    toggle = Some((g, r, k));
                }
            });
        }
    }
    if let Some(key) = toggle {
        if let Some(pos) = d.excluded.iter().position(|e| *e == key) {
            d.excluded.remove(pos);
        } else {
            d.excluded.push(key);
        }
    }
    // Recommended test: design, distribution, spread, then the test and why.
    if !matches!(&d.kind, AnalysisKind::TwoGroups { .. } | AnalysisKind::OneWay { .. }) || groups.len() < 2 {
        return None;
    }
    let complete: Vec<usize> = (0..t.rows)
        .filter(|&r| {
            groups.iter().all(|&g| t.datasets[g].values[r].first().copied().flatten().is_some() && !d.excluded.contains(&(g, r, 0)))
        })
        .collect();
    let can_match = complete.len() >= 2;
    let chosen_paired = matches!(&d.kind, AnalysisKind::TwoGroups { test, .. } if test.paired())
        || matches!(&d.kind, AnalysisKind::OneWay { test: OneWayTest::RepeatedMeasures | OneWayTest::Friedman, .. });
    // Whether rows are the same subjects is a design choice, not something the data can show.
    let matched = can_match && chosen_paired;
    let mut accepted = None;
    ui.add_space(6.0);
    egui::Frame::new().fill(p.selected).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(crate::icons::image("wand-sparkles", 16.0, p.accent));
            ui.label(RichText::new("Recommended test").font(crate::theme::bold(13.5)).color(p.accent));
        });
        if chosen_paired && !can_match {
            ui.label(RichText::new("Rows are incomplete, so the groups are treated as independent.").small().color(p.weak));
        }
        let samples: Vec<Vec<f64>> = if matched {
            groups.iter().map(|&g| complete.iter().map(|&r| t.datasets[g].values[r][0].unwrap()).collect()).collect()
        } else {
            samples
        };
        let Some(advice) = hg_core::advice::suggest(&samples, &groups, matched, &d.kind) else { return };
        for c in &advice.checks {
            ui.horizontal_top(|ui| {
                let (icon, color) = match c.ok {
                    Some(true) => ("circle-check", p.good),
                    Some(false) => ("triangle-alert", p.warn_text),
                    None => ("info", p.weak),
                };
                ui.add(crate::icons::image(icon, 14.0, color));
                ui.add(egui::Label::new(RichText::new(&c.text).small().color(color)).wrap());
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new(advice.kind.label()).font(crate::theme::bold(15.0)));
        for r in &advice.reasons {
            ui.add(egui::Label::new(RichText::new(format!("• {r}")).small()).wrap());
        }
        if let Some(alt) = &advice.alternative {
            ui.add(egui::Label::new(RichText::new(format!("Alternative: {alt}")).small().color(p.weak)).wrap());
        }
        if advice.kind.label() == d.kind.label() {
            ui.horizontal(|ui| badge(ui, Some(true), "this is the test you chose", ""));
        } else if ui.button("Use this test").clicked() {
            accepted = Some(advice.kind);
        }
    });
    accepted
}
