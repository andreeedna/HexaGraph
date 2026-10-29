//! Application state, toolbar and the project navigator.

use crate::dialogs::{AnalyzeDialog, GraphGallery, WelcomeDialog};
use crate::{icons, theme};
use eframe::egui::{self, Color32, RichText};
use hg_core::{AnalysisOutput, Project};
use std::collections::HashMap;
use std::path::PathBuf;

/// Actions that discard the current project, so they ask to save it first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    New,
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Table(u64),
    Analysis(u64),
    Graph(u64),
}

pub struct App {
    pub project: Project,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub selection: Selection,
    /// Bumped on every change; caches compare against it.
    pub revision: u64,
    pub welcome: Option<WelcomeDialog>,
    pub analyze: Option<AnalyzeDialog>,
    pub gallery: Option<GraphGallery>,
    pub colors: Option<crate::colors::ColorWindow>,
    pub export: Option<crate::graph_view::ExportWindow>,
    /// Tool selected in the graph editor's tool strip.
    pub graph_tool: crate::graph_view::Tool,
    pub grid: crate::grid::GridState,
    pub status: String,
    results_cache: HashMap<u64, (u64, Result<AnalysisOutput, String>)>,
    pub scene_cache: HashMap<u64, (u64, Result<hg_plot::Scene, String>)>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    committed: Project,
    /// Action waiting for the "save changes?" answer.
    confirm: Option<Pending>,
    /// Action to run at the end of the frame.
    pending: Option<Pending>,
    allow_close: bool,
    show_about: bool,
    last_title: String,
    /// Dark interface theme.
    pub dark: bool,
    /// Color palettes saved by the user (kept across sessions).
    pub user_palettes: Vec<(String, Vec<[u8; 3]>)>,
    /// Notes about the last Prism import: (file, notes).
    import_notes: Option<(String, Vec<String>)>,
}

impl App {
    /// `view` is an optional "table:N", "results:N", "graph:N" or "welcome"
    /// to show at startup, used to produce documentation screenshots.
    pub fn new(ctx: &egui::Context, path: Option<PathBuf>, view: Option<String>, dark: bool) -> Self {
        egui_extras::install_image_loaders(ctx);
        theme::apply(ctx, dark, true);
        let mut app = App {
            dark,
            user_palettes: Vec::new(),
            import_notes: None,
            project: Project::default(),
            path: None,
            dirty: false,
            selection: Selection::None,
            revision: 0,
            welcome: Some(WelcomeDialog::new(true)),
            analyze: None,
            gallery: None,
            colors: None,
            export: None,
            graph_tool: Default::default(),
            grid: Default::default(),
            status: String::new(),
            results_cache: HashMap::new(),
            scene_cache: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            committed: Project::default(),
            confirm: None,
            pending: None,
            allow_close: false,
            show_about: false,
            last_title: String::new(),
        };
        if let Some(p) = path {
            app.open_path(p);
        }
        if let Some(v) = view.as_deref() {
            app.show_view(v);
            if let (Some(sel), Selection::Graph(g)) = (v.strip_suffix(":legend"), app.selection) {
                let _ = sel;
                let mut cv = crate::canvas::view(ctx, g);
                cv.selected = Some(hg_plot::Element::Legend);
                crate::canvas::set_view(ctx, g, cv);
            }
        }
        app
    }

    fn show_view(&mut self, v: &str) {
        if v == "welcome" {
            self.welcome = Some(WelcomeDialog::new(true));
            return;
        }
        let mut parts = v.split(':');
        let (Some(kind), Some(n)) = (parts.next(), parts.next()) else { return };
        let n: usize = n.parse().unwrap_or(0);
        if let (Some(extra), Some(g)) = (parts.next(), self.project.graphs.get(n).map(|g| g.id)) {
            use crate::graph_view::Tool;
            match extra {
                "stars" => self.graph_tool = Tool::Significance,
                "data" => self.graph_tool = Tool::Data,
                "style" => self.graph_tool = Tool::Style,
                "colors" => self.colors = Some(crate::colors::ColorWindow::new(g)),
                "export" => self.export = Some(crate::graph_view::ExportWindow::new(g)),
                _ => {}
            }
        }
        let p = &self.project;
        let sel = match kind {
            "table" => p.tables.get(n).map(|t| Selection::Table(t.id)),
            "results" => p.analyses.get(n).map(|a| Selection::Analysis(a.id)),
            "graph" => p.graphs.get(n).map(|g| Selection::Graph(g.id)),
            _ => None,
        };
        if let Some(s) = sel {
            self.selection = s;
        }
        match kind {
            "analyze" => {
                self.selection = p.tables.get(n).map_or(Selection::None, |t| Selection::Table(t.id));
                self.start_analysis();
            }
            "gallery" => {
                self.selection = p.tables.get(n).map_or(Selection::None, |t| Selection::Table(t.id));
                self.start_graph_gallery();
            }
            _ => {}
        }
    }

    /// Marks the project as modified.
    pub fn touch(&mut self) {
        self.revision += 1;
        self.dirty = true;
    }

    pub fn results(&mut self, id: u64) -> Result<AnalysisOutput, String> {
        if let Some((rev, r)) = self.results_cache.get(&id) {
            if *rev == self.revision {
                return r.clone();
            }
        }
        let r = self.project.run_analysis(id);
        self.results_cache.insert(id, (self.revision, r.clone()));
        r
    }

    pub fn scene(&mut self, id: u64) -> Result<hg_plot::Scene, String> {
        if let Some((rev, s)) = self.scene_cache.get(&id) {
            if *rev == self.revision {
                return s.clone();
            }
        }
        let s = self.project.graph(id).map_or_else(|| Err("the graph was deleted".into()), |g| hg_plot::render(&self.project, g));
        self.scene_cache.insert(id, (self.revision, s.clone()));
        s
    }

    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.committed = self.project.clone();
        self.revision += 1;
        self.grid = Default::default();
    }

    /// Records an undo step when the project changed during this frame.
    fn commit_history(&mut self) {
        if self.project != self.committed {
            let prev = std::mem::replace(&mut self.committed, self.project.clone());
            self.undo.push(prev);
            // Each step is a full copy: keep fewer steps for big projects (about 10M cells, ~160 MB, in all).
            let cells: usize = self.project.tables.iter().map(|t| t.rows * t.replicates * (t.datasets.len() + 1)).sum();
            let keep = (10_000_000 / cells.max(1)).clamp(1, 100);
            if self.undo.len() > keep {
                self.undo.drain(..self.undo.len() - keep);
            }
            self.redo.clear();
        }
    }

    fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
            self.after_history_jump();
        }
    }

    fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
            self.after_history_jump();
        }
    }

    fn after_history_jump(&mut self) {
        self.touch();
        self.grid = Default::default();
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
    }

    fn selection_exists(&self) -> bool {
        match self.selection {
            Selection::Table(id) => self.project.table(id).is_some(),
            Selection::Analysis(id) => self.project.analysis(id).is_some(),
            Selection::Graph(id) => self.project.graph(id).is_some(),
            Selection::None => true,
        }
    }

    pub fn new_project(&mut self) {
        self.project = Project::default();
        self.path = None;
        self.dirty = false;
        self.selection = Selection::None;
        self.reset_history();
        self.welcome = Some(WelcomeDialog::new(true));
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("All supported projects", &["hxn", "hxg", "prism", "pzfx"])
            .add_filter("HexaGraph project", &["hxn", "hxg"])
            .add_filter("Prism project", &["prism", "pzfx"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    pub fn open_path(&mut self, p: PathBuf) {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pzfx") || e.eq_ignore_ascii_case("prism")) {
            return self.import_prism(p);
        }
        match Project::load(&p) {
            Ok(proj) => {
                self.project = proj;
                self.selection = self.project.tables.first().map_or(Selection::None, |t| Selection::Table(t.id));
                self.status = format!("Opened {}", p.display());
                self.path = Some(p);
                self.dirty = false;
                self.welcome = None;
                self.reset_history();
            }
            Err(e) => self.status = format!("Could not open {}: {e}", p.display()),
        }
    }

    /// Imports the data tables of a Prism XML file into a new, unsaved project.
    fn import_prism(&mut self, p: PathBuf) {
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Could not read {}: {e}", p.display());
                return;
            }
        };
        let is_xml = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pzfx"));
        let result = if is_xml { hg_core::pzfx::import(&String::from_utf8_lossy(&bytes)) } else { hg_core::prism::import(&bytes) };
        match result {
            Ok(imported) => {
                self.project = imported.project;
                self.project.title = p.file_stem().map_or("Imported".into(), |s| s.to_string_lossy().into_owned());
                self.selection = self.project.tables.first().map_or(Selection::None, |t| Selection::Table(t.id));
                self.status = format!("Imported {} table(s) from {}", self.project.tables.len(), p.display());
                // Saving asks for a new .hxn file instead of overwriting the Prism file.
                self.path = None;
                self.welcome = None;
                self.reset_history();
                self.dirty = true;
                self.import_notes = Some((p.display().to_string(), imported.warnings));
            }
            Err(e) => self.status = format!("Could not import {}: {e}", p.display()),
        }
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some((file, notes)) = self.import_notes.clone() else { return };
        let mut open = true;
        egui::Window::new("Prism file imported").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(RichText::new(format!("The data tables of {file} were imported ({} tables).", self.project.tables.len())).size(14.0));
            let n_an = self.project.analyses.len();
            let msg = if n_an > 0 {
                format!("{n_an} analyses were imported and recalculated; each one shows the values Prism reported, to compare. Graphs were rebuilt.")
            } else {
                "Graphs were rebuilt from the data; run your analyses again from Analyze.".to_string()
            };
            ui.label(RichText::new(msg).weak());
            if !notes.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Notes").strong());
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for n in &notes {
                        ui.label(format!("• {n}"));
                    }
                });
            }
            ui.add_space(6.0);
            if theme::primary_button(ui, "OK", true) {
                self.import_notes = None;
            }
        });
        if !open {
            self.import_notes = None;
        }
    }

    pub fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("HexaGraph project", &["hxn"])
                .set_file_name(format!("{}.hxn", self.project.title))
                .save_file()
                .map(|p| if p.extension().is_none() { p.with_extension("hxn") } else { p }),
        };
        if let Some(p) = path {
            if let Some(stem) = p.file_stem() {
                self.project.title = stem.to_string_lossy().into_owned();
            }
            match self.project.save(&p) {
                Ok(()) => {
                    self.status = format!("Saved {}", p.display());
                    self.committed.title = self.project.title.clone();
                    self.path = Some(p);
                    self.dirty = false;
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    /// The table behind the current selection.
    pub fn current_table(&self) -> Option<u64> {
        match self.selection {
            Selection::Table(id) => Some(id),
            Selection::Analysis(id) => self.project.analysis(id).map(|a| a.table_id),
            Selection::Graph(id) => self.project.graph(id).map(|g| g.table_id),
            Selection::None => None,
        }
    }

    pub fn start_analysis(&mut self) {
        if let Some(t) = self.current_table().and_then(|id| self.project.table(id)) {
            self.analyze = Some(AnalyzeDialog::new(t, None));
        }
    }

    pub fn start_graph_gallery(&mut self) {
        if let Some(tid) = self.current_table() {
            self.gallery = Some(GraphGallery::new(tid));
        }
    }

    fn delete_selection(&mut self) {
        match self.selection {
            Selection::Table(id) => self.project.remove_table(id),
            Selection::Analysis(id) => self.project.remove_analysis(id),
            Selection::Graph(id) => self.project.remove_graph(id),
            Selection::None => return,
        }
        self.selection = Selection::None;
        self.touch();
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        let sc = |m, k| KeyboardShortcut::new(m, k);
        let editing_text = ctx.wants_keyboard_input();
        ctx.input_mut(|i| {
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)) {
                self.save(false);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
                self.save(true);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)) {
                self.request(Pending::Open);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::N)) {
                self.welcome = Some(WelcomeDialog::new(false));
            }
            if !editing_text {
                if i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
                    || i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Y))
                {
                    self.redo();
                } else if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Z)) {
                    self.undo();
                }
            }
        });
    }

    fn menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project…").clicked() {
                        self.request(Pending::New);
                        ui.close_menu();
                    }
                    if ui.button("Open…  (Ctrl+O)").clicked() {
                        ui.close_menu();
                        self.request(Pending::Open);
                    }
                    if ui.button("Save  (Ctrl+S)").clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button("Save as…  (Ctrl+Shift+S)").clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  (Ctrl+Z)")).clicked() {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  (Ctrl+Y)")).clicked() {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add_enabled(self.selection != Selection::None, egui::Button::new("Delete sheet")).clicked() {
                        self.delete_selection();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Insert", |ui| {
                    if ui.button("New data table…  (Ctrl+N)").clicked() {
                        self.welcome = Some(WelcomeDialog::new(false));
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.current_table().is_some(), egui::Button::new("New graph…")).clicked() {
                        self.start_graph_gallery();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Analysis", |ui| {
                    if ui.add_enabled(self.current_table().is_some(), egui::Button::new("Analyze…")).clicked() {
                        self.start_analysis();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About HexaGraph").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let has_table = self.current_table().is_some();
        let is_graph = matches!(self.selection, Selection::Graph(_));
        let is_result = matches!(self.selection, Selection::Analysis(_));
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(10, 6)).stroke(egui::Stroke::new(1.0_f32, p.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    logo(ui, 30.0);
                    ui.label(RichText::new("HexaGraph").font(theme::bold(16.0)).color(p.strong));
                    ui.add_space(10.0);
                    divider(ui);
                    if tool(ui, "file-plus-2", "New", true) {
                        self.request(Pending::New);
                    }
                    if tool(ui, "folder-open", "Open", true) {
                        self.request(Pending::Open);
                    }
                    if tool(ui, "save", "Save", true) {
                        self.save(false);
                    }
                    divider(ui);
                    if tool(ui, "table-2", "New table", true) {
                        self.welcome = Some(WelcomeDialog::new(false));
                    }
                    if tool(ui, "chart-column-big", "New graph", has_table) {
                        self.start_graph_gallery();
                    }
                    if tool(ui, "trash-2", "Delete", self.selection != Selection::None) {
                        self.delete_selection();
                    }
                    divider(ui);
                    if tool(ui, "undo-2", "Undo", !self.undo.is_empty()) {
                        self.undo();
                    }
                    if tool(ui, "redo-2", "Redo", !self.redo.is_empty()) {
                        self.redo();
                    }
                    divider(ui);
                    if tool(ui, "sliders-horizontal", "Parameters", is_result) {
                        if let Selection::Analysis(id) = self.selection {
                            if let Some(a) = self.project.analysis(id).cloned() {
                                if let Some(t) = self.project.table(a.table_id) {
                                    self.analyze = Some(AnalyzeDialog::new(t, Some(&a)));
                                }
                            }
                        }
                    }
                    if tool(ui, "image-down", "Export", is_graph) {
                        if let Selection::Graph(id) = self.selection {
                            self.export = Some(crate::graph_view::ExportWindow::new(id));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let icon = if self.dark { "sun" } else { "moon" };
                        if icons::button(ui, icon, if self.dark { "Light theme" } else { "Dark theme" }, true) {
                            self.dark = !self.dark;
                            theme::apply(ctx, self.dark, false);
                            self.scene_cache.clear();
                        }
                        ui.add_space(6.0);
                        let analyze = egui::Button::image_and_text(
                            icons::image("sigma", 16.0, p.accent_text),
                            RichText::new("Analyze").font(theme::bold(14.0)).color(p.accent_text),
                        )
                        .fill(p.accent)
                        .min_size(egui::vec2(110.0, 34.0));
                        if ui.add_enabled(has_table, analyze).on_hover_text("Run a statistical test on the current data table").clicked() {
                            self.start_analysis();
                        }
                    });
                });
            });
    }

    fn navigator(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        egui::SidePanel::left("navigator")
            .resizable(true)
            .default_width(240.0)
            .frame(
                egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(8, 8)).stroke(egui::Stroke::new(1.0_f32, p.border)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut select: Option<Selection> = None;
                    let mut delete: Option<Selection> = None;
                    let mut action: Option<&str> = None;
                    let tables: Vec<Item> = self
                        .project
                        .tables
                        .iter()
                        .map(|t| (Selection::Table(t.id), table_icon(t.kind), t.name.clone(), t.kind.label().to_string()))
                        .collect();
                    if section(ui, "table-2", "Data Tables", &tables, self.selection, &mut select, &mut delete, true) {
                        action = Some("table");
                    }
                    let analyses: Vec<Item> = self
                        .project
                        .analyses
                        .iter()
                        .map(|a| {
                            let table = self.project.table(a.table_id).map_or("?".to_string(), |t| t.name.clone());
                            (Selection::Analysis(a.id), "sigma", a.display_name(&table), format!("{} · data: {table}", a.kind.label()))
                        })
                        .collect();
                    let can = self.current_table().is_some();
                    if section(ui, "file-text", "Results", &analyses, self.selection, &mut select, &mut delete, can) {
                        action = Some("analysis");
                    }
                    let graphs: Vec<Item> = self
                        .project
                        .graphs
                        .iter()
                        .map(|g| (Selection::Graph(g.id), "chart-line", g.name.clone(), g.kind.label().to_string()))
                        .collect();
                    if section(ui, "chart-column", "Graphs", &graphs, self.selection, &mut select, &mut delete, can) {
                        action = Some("graph");
                    }
                    if let Some(tid) = self.current_table() {
                        ui.add_space(12.0);
                        egui::Frame::new()
                            .fill(p.panel)
                            .corner_radius(8.0)
                            .inner_margin(8.0)
                            .stroke(egui::Stroke::new(1.0_f32, p.border))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new("FAMILY").font(theme::bold(11.0)).color(p.weak));
                                let mut family: Vec<Item> = Vec::new();
                                if let Some(t) = self.project.table(tid) {
                                    family.push((Selection::Table(tid), table_icon(t.kind), t.name.clone(), String::new()));
                                }
                                family.extend(
                                    self.project
                                        .analyses
                                        .iter()
                                        .filter(|a| a.table_id == tid)
                                        .map(|a| (Selection::Analysis(a.id), "sigma", self.project.analysis_name(a.id), String::new())),
                                );
                                family.extend(
                                    self.project
                                        .graphs
                                        .iter()
                                        .filter(|g| g.table_id == tid)
                                        .map(|g| (Selection::Graph(g.id), "chart-line", g.name.clone(), String::new())),
                                );
                                for (sel, icon, label, _) in family {
                                    if nav_item(ui, icon, &label, self.selection == sel).clicked() {
                                        select = Some(sel);
                                    }
                                }
                            });
                    }
                    if let Some(s) = select {
                        self.selection = s;
                        self.grid = Default::default();
                    }
                    if let Some(d) = delete {
                        self.selection = d;
                        self.delete_selection();
                    }
                    match action {
                        Some("table") => self.welcome = Some(WelcomeDialog::new(false)),
                        Some("analysis") => self.start_analysis(),
                        Some("graph") => self.start_graph_gallery(),
                        _ => {}
                    }
                });
            });
    }

    fn central(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.app_bg).inner_margin(12.0)).show(ctx, |ui| match self.selection {
            Selection::Table(id) => crate::grid::table_view(self, ui, id),
            Selection::Analysis(id) => crate::results::results_view(self, ui, id),
            Selection::Graph(id) => crate::graph_view::graph_view(self, ui, id),
            Selection::None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 4.0);
                    logo(ui, 72.0);
                    ui.add_space(8.0);
                    ui.label(RichText::new("HexaGraph").font(theme::bold(34.0)).color(p.strong));
                    ui.label(RichText::new("Scientific graphing and statistics. Open source, written in Rust.").size(16.0).color(p.weak));
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        let w = 2.0 * 230.0 + 16.0;
                        ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
                        if start_card(ui, "table-2", "New data table", "Enter or paste data and pick a graph") {
                            self.welcome = Some(WelcomeDialog::new(false));
                        }
                        ui.add_space(16.0);
                        if start_card(ui, "folder-open", "Open a project", "Continue working on a .hxn file") {
                            self.request(Pending::Open);
                        }
                    });
                });
            }
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .stroke(egui::Stroke::new(1.0_f32, p.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let name = self.path.as_ref().map_or("Unsaved project".to_string(), |p| p.display().to_string());
                    ui.label(RichText::new(name).small().color(p.weak));
                    if self.dirty {
                        ui.label(RichText::new("● modified").small().color(p.accent));
                    }
                    ui.separator();
                    ui.label(RichText::new(&self.status).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = (self.project.tables.len(), self.project.analyses.len(), self.project.graphs.len());
                        ui.label(RichText::new(format!("{} tables · {} results · {} graphs", n.0, n.1, n.2)).small().color(p.weak));
                    });
                });
            });
    }

    /// Runs `p` now, or asks first whether to save the current project.
    pub fn request(&mut self, p: Pending) {
        if self.dirty {
            self.confirm = Some(p);
        } else {
            self.pending = Some(p);
        }
    }

    fn run_pending(&mut self, ctx: &egui::Context) {
        match self.pending.take() {
            Some(Pending::New) => self.new_project(),
            Some(Pending::Open) => self.open_dialog(),
            Some(Pending::Close) => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    fn close_guard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Close);
        }
        let Some(action) = self.confirm else { return };
        let verb = match action {
            Pending::New => "creating a new project",
            Pending::Open => "opening another project",
            Pending::Close => "closing",
        };
        egui::Window::new("Save changes?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.label(RichText::new(format!("Do you want to save “{}” before {verb}?", self.project.title)).size(15.0));
                ui.label(RichText::new("Your changes will be lost if you don't save them.").weak());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Save", true) {
                        self.save(false);
                        if !self.dirty {
                            self.pending = Some(action);
                        }
                        self.confirm = None;
                    }
                    if ui.button("Don't save").clicked() {
                        self.pending = Some(action);
                        self.confirm = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm = None;
                    }
                });
            },
        );
    }

    fn about(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("About HexaGraph").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading(RichText::new("HexaGraph").color(theme::accent()));
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("Open-source scientific graphing and statistics, written in Rust.");
            ui.label("Licensed under the GNU GPL v3 or later.");
        });
        self.show_about = open;
    }
}

/// The HexaGraph logo (same design as assets/logo.svg): a faceted hexagon
/// with a rising chart line.
pub fn logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let o = rect.min;
    let p = |x: f32, y: f32| o + egui::vec2(x, y) * (size / 256.0);
    let verts = [p(128.0, 8.0), p(232.0, 68.0), p(232.0, 188.0), p(128.0, 248.0), p(24.0, 188.0), p(24.0, 68.0)];
    let center = p(128.0, 128.0);
    let top = Color32::from_rgb(0x5b, 0x7c, 0xfa);
    let bottom = Color32::from_rgb(0x7a, 0x3c, 0xf0);
    let mix = |t: f32| {
        Color32::from_rgb(
            (top.r() as f32 + (bottom.r() as f32 - top.r() as f32) * t) as u8,
            (top.g() as f32 + (bottom.g() as f32 - top.g() as f32) * t) as u8,
            (top.b() as f32 + (bottom.b() as f32 - top.b() as f32) * t) as u8,
        )
    };
    // Six facets with a gradient and light/shadow, like a cut gem.
    let shade = [1.16, 1.06, 1.0, 0.88, 0.96, 1.0];
    let mut mesh = egui::Mesh::default();
    for k in 0..6 {
        let (a, b) = (verts[k], verts[(k + 1) % 6]);
        let t = ((a.y + b.y) / 2.0 - rect.top()) / size;
        let base = mix(t.clamp(0.0, 1.0));
        let f = shade[k];
        let c = Color32::from_rgb(
            (base.r() as f32 * f).min(255.0) as u8,
            (base.g() as f32 * f).min(255.0) as u8,
            (base.b() as f32 * f).min(255.0) as u8,
        );
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(center, c);
        mesh.colored_vertex(a, c);
        mesh.colored_vertex(b, c);
        mesh.add_triangle(i, i + 1, i + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    let line = [p(62.0, 172.0), p(104.0, 132.0), p(142.0, 150.0), p(194.0, 88.0)];
    ui.painter().add(egui::Shape::line(line.to_vec(), egui::Stroke::new(14.0 * size / 256.0, Color32::WHITE)));
    for q in line {
        ui.painter().circle_filled(q, 13.0 * size / 256.0, Color32::WHITE);
    }
}

pub fn table_icon(kind: hg_core::TableKind) -> &'static str {
    use hg_core::TableKind::*;
    match kind {
        XY => "chart-scatter",
        Column => "chart-column",
        Grouped => "chart-column-stacked",
        Contingency => "grid-3x3",
        Survival => "trending-down",
        PartsOfWhole => "chart-pie",
    }
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 38.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::pal().border);
    ui.add_space(4.0);
}

/// Toolbar button: icon above a short label.
fn tool(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> bool {
    let p = theme::pal();
    let size = egui::vec2(64.0, 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let color = if enabled { p.text } else { p.disabled };
    icons::paint(ui, icon, egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 13.0), egui::Vec2::splat(20.0)), color);
    ui.painter().text(
        rect.center_bottom() - egui::vec2(0.0, 3.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(11.5),
        color,
    );
    enabled && resp.clicked()
}

/// A big clickable card on the start page.
fn start_card(ui: &mut egui::Ui, icon: &str, title: &str, text: &str) -> bool {
    let p = theme::pal();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(230.0, 120.0), egui::Sense::click());
    let stroke = if resp.hovered() { egui::Stroke::new(2.0_f32, p.accent) } else { egui::Stroke::new(1.0_f32, p.border) };
    ui.painter().rect(rect, 12.0, p.panel, stroke, egui::StrokeKind::Inside);
    icons::paint(ui, icon, egui::Rect::from_min_size(rect.min + egui::vec2(18.0, 18.0), egui::Vec2::splat(28.0)), p.accent);
    ui.painter().text(rect.min + egui::vec2(18.0, 62.0), egui::Align2::LEFT_TOP, title, theme::bold(16.0), p.strong);
    ui.painter().text(rect.min + egui::vec2(18.0, 86.0), egui::Align2::LEFT_TOP, text, egui::FontId::proportional(12.5), p.weak);
    resp.clicked()
}

/// One row of the navigator: icon, label, selection pill.
fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let p = theme::pal();
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 6.0, p.selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let color = if selected { p.accent } else { p.weak };
    icons::paint(ui, icon, egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 5.0), egui::Vec2::splat(16.0)), color);
    let text_color = if selected { p.strong } else { p.text };
    let galley = ui.painter().layout(label.to_string(), egui::FontId::proportional(13.5), text_color, w - 40.0);
    let galley = if galley.rows.len() > 1 {
        ui.painter().layout_no_wrap(truncate(label, ((w - 40.0) / 7.0) as usize), egui::FontId::proportional(13.5), text_color)
    } else {
        galley
    };
    ui.painter().galley(rect.min + egui::vec2(32.0, (26.0 - galley.size().y) / 2.0), galley, text_color);
    resp
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

type Item = (Selection, &'static str, String, String);

/// A navigator section. Returns true when its "+" button was clicked.
fn section(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    items: &[Item],
    current: Selection,
    select: &mut Option<Selection>,
    delete: &mut Option<Selection>,
    can_add: bool,
) -> bool {
    let p = theme::pal();
    let mut clicked_new = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        icons::paint(ui, icon, egui::Rect::from_min_size(ui.cursor().min + egui::vec2(2.0, 3.0), egui::Vec2::splat(14.0)), p.weak);
        ui.add_space(20.0);
        ui.label(RichText::new(title.to_uppercase()).font(theme::bold(11.5)).color(p.weak));
        ui.label(RichText::new(items.len().to_string()).small().color(p.disabled));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, "plus", &format!("New {}", title.to_lowercase().trim_end_matches('s')), can_add) {
                clicked_new = true;
            }
        });
    });
    if items.is_empty() {
        ui.label(RichText::new("   nothing yet").small().italics().color(p.disabled));
    }
    for (sel, icon, label, hint) in items {
        let r = nav_item(ui, icon, label, current == *sel).on_hover_text(hint);
        if r.clicked() {
            *select = Some(*sel);
        }
        r.context_menu(|ui| {
            if ui.button("Delete").clicked() {
                *delete = Some(*sel);
                ui.close_menu();
            }
        });
    }
    clicked_new
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "dark", &self.dark);
        eframe::set_value(storage, "palettes", &self.user_palettes);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.close_guard(ctx);
        self.shortcuts(ctx);
        self.menu(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.navigator(ctx);
        self.central(ctx);
        crate::dialogs::welcome_dialog(self, ctx);
        crate::dialogs::analyze_dialog(self, ctx);
        crate::dialogs::graph_gallery(self, ctx);
        crate::colors::color_window(self, ctx);
        crate::graph_view::export_window(self, ctx);
        self.about(ctx);
        self.import_window(ctx);
        self.run_pending(ctx);
        if self.project != self.committed {
            self.touch();
            self.commit_history();
        }
        let title = format!("{}{} — HexaGraph", self.project.title, if self.dirty { " *" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}
