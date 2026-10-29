//! Spreadsheet editor for data tables.
//!
//! The sheet is unbounded in both directions, like a spreadsheet: there is
//! always another row below and another data set to the right. Only the
//! visible cells are drawn, so it stays fast however far you scroll.
//!
//! Mouse: click selects, Shift+click or drag selects a block, double click
//! edits, right click opens the context menu. Keyboard: arrows, Tab, Enter
//! move; Shift+arrows extend the selection; typing replaces the cell; F2 edits
//! it; Delete clears; Ctrl+X/C/V cut, copy and paste; Ctrl+Shift+V pastes
//! transposed; Ctrl+A selects all the data.

use crate::app::App;
use crate::{icons, theme};
use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Sense, Vec2};
use hg_core::table::{dataset_letter, GridColumn};
use hg_core::{DataTable, TableKind};

const ROW_H: f32 = 24.0;
const HEAD_TITLE: f32 = 28.0;
const HEAD_SUB: f32 = 20.0;
const HEAD: f32 = HEAD_TITLE + HEAD_SUB;
const NUM_W: f32 = 52.0;
const COL_W: f32 = 100.0;
const TITLE_COL_W: f32 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleTarget {
    Dataset(usize),
    X,
}

#[derive(Default)]
pub struct GridState {
    /// Active cell: (row, grid column index).
    pub(crate) cursor: Option<(usize, usize)>,
    /// Other corner of the selected block (None = just the cursor).
    anchor: Option<(usize, usize)>,
    /// Text being typed into the active cell.
    pub(crate) editing: Option<String>,
    focus_editor: bool,
    scroll: bool,
    title_edit: Option<(TitleTarget, String)>,
    /// Rows and columns currently reachable by scrolling (grows on demand).
    extent: (usize, usize),
}

impl GridState {
    /// Selected block as ((first row, last row), (first col, last col)).
    fn block(&self) -> Option<((usize, usize), (usize, usize))> {
        let c = self.cursor?;
        let a = self.anchor.unwrap_or(c);
        Some(((a.0.min(c.0), a.0.max(c.0)), (a.1.min(c.1), a.1.max(c.1))))
    }

    fn in_block(&self, r: usize, c: usize) -> bool {
        self.block().is_some_and(|((r0, r1), (c0, c1))| r >= r0 && r <= r1 && c >= c0 && c <= c1)
    }

    fn select(&mut self, cell: (usize, usize), extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = self.cursor;
            }
        } else {
            self.anchor = None;
        }
        self.cursor = Some(cell);
        self.scroll = true;
    }
}

/// Left edge of a grid column, relative to the first data column.
fn col_x(t: &DataTable, i: usize) -> f32 {
    if t.show_row_titles && i > 0 {
        TITLE_COL_W + (i - 1) as f32 * COL_W
    } else {
        i as f32 * COL_W
    }
}

fn col_w(t: &DataTable, i: usize) -> f32 {
    if t.show_row_titles && i == 0 {
        TITLE_COL_W
    } else {
        COL_W
    }
}

fn col_at_x(t: &DataTable, x: f32) -> usize {
    let x = x.max(0.0);
    if t.show_row_titles {
        if x < TITLE_COL_W {
            0
        } else {
            1 + ((x - TITLE_COL_W) / COL_W) as usize
        }
    } else {
        (x / COL_W) as usize
    }
}

const EDITOR_ID: &str = "grid_cell_editor";
/// Invisible widget that holds the keyboard focus while a cell is selected,
/// so Tab and the arrows move between cells instead of between widgets.
const NAV_ID: &str = "grid_navigation";

fn keep_keys(ctx: &egui::Context, id: egui::Id) {
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
    });
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Clipboard and editing commands, shared by keyboard, menu and toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Cut,
    Copy,
    Paste,
    PasteTransposed,
    Clear,
    InsertRow,
    DeleteRows,
    SelectAll,
    /// Exclude the selected values from analyses and graphs (or include them again).
    Exclude,
}

fn run_command(app: &mut App, ctx: &egui::Context, id: u64, cmd: Command, clipboard: Option<String>) {
    let Some(t) = app.project.table_mut(id) else { return };
    if cmd == Command::SelectAll {
        let last_col = (t.grid_columns().len()).max(1) - 1;
        app.grid.anchor = Some((0, 0));
        app.grid.cursor = Some((t.used_rows().max(1) - 1, last_col));
        return;
    }
    let Some(((r0, r1), (c0, c1))) = app.grid.block() else {
        app.status = "Select a cell first".into();
        return;
    };
    match cmd {
        Command::Copy | Command::Cut => {
            ctx.copy_text(t.block_text((r0, r1), (c0, c1)));
            if cmd == Command::Cut {
                t.clear_block((r0, r1), (c0, c1));
            }
            let n = (r1 - r0 + 1) * (c1 - c0 + 1);
            app.status = format!("{} {n} cell{}", if cmd == Command::Cut { "Cut" } else { "Copied" }, if n == 1 { "" } else { "s" });
        }
        Command::Paste | Command::PasteTransposed => match clipboard.or_else(read_clipboard) {
            Some(text) => {
                let unreadable = if cmd == Command::Paste { t.paste(r0, c0, &text) } else { t.paste_transposed(r0, c0, &text) };
                app.status = if cmd == Command::Paste { "Pasted".into() } else { "Pasted transposed".into() };
                if unreadable > 0 {
                    app.status += &format!(": {unreadable} value(s) were not numbers and were left empty");
                }
            }
            None => app.status = "The clipboard is empty".into(),
        },
        Command::Clear => t.clear_block((r0, r1), (c0, c1)),
        Command::Exclude => {
            t.toggle_excluded((r0, r1), (c0, c1));
            app.status = format!("{} value(s) excluded in this table", t.excluded.len());
        }
        Command::InsertRow => app.project.insert_row(id, r0),
        Command::DeleteRows => app.project.delete_rows(id, r0, r1),
        Command::SelectAll => {}
    }
}

/// Keyboard handling. Runs before the sheet is drawn so that navigation keys
/// never reach the cell editor or other widgets.
fn keyboard(app: &mut App, ctx: &egui::Context, id: u64) {
    let Some((row, col)) = app.grid.cursor else { return };
    let editor = egui::Id::new(EDITOR_ID);
    let nav = egui::Id::new(NAV_ID);
    let focused = ctx.memory(|m| m.focused());
    // Another widget (a title field, a dialog…) has the keyboard.
    if focused.is_some() && focused != Some(editor) && focused != Some(nav) {
        if let Some(buf) = app.grid.editing.take() {
            if let Some(t) = app.project.table_mut(id) {
                let c = t.column_at(col);
                t.set_text(row, c, &buf);
            }
        }
        return;
    }
    let editing = app.grid.editing.is_some();
    let mut target: Option<((usize, usize), bool)> = None;
    let mut cmd: Option<Command> = None;
    ctx.input_mut(|i| {
        use egui::Modifiers as M;
        let right = (row, col + 1);
        let left = (row, col.saturating_sub(1));
        let down = (row + 1, col);
        let up = (row.saturating_sub(1), col);
        if i.consume_key(M::SHIFT, Key::Tab) {
            target = Some((left, false));
        } else if i.consume_key(M::NONE, Key::Tab) {
            target = Some((right, false));
        } else if i.consume_key(M::SHIFT, Key::Enter) {
            target = Some((up, false));
        } else if !editing && i.consume_key(M::SHIFT, Key::ArrowDown) {
            // Shifted arrows first: egui matches unshifted shortcuts loosely.
            target = Some((down, true));
        } else if !editing && i.consume_key(M::SHIFT, Key::ArrowUp) {
            target = Some((up, true));
        } else if !editing && i.consume_key(M::SHIFT, Key::ArrowRight) {
            target = Some((right, true));
        } else if !editing && i.consume_key(M::SHIFT, Key::ArrowLeft) {
            target = Some((left, true));
        } else if i.consume_key(M::NONE, Key::Enter) || i.consume_key(M::NONE, Key::ArrowDown) {
            target = Some((down, false));
        } else if i.consume_key(M::NONE, Key::ArrowUp) {
            target = Some((up, false));
        } else if !editing && i.consume_key(M::NONE, Key::ArrowRight) {
            target = Some((right, false));
        } else if !editing && i.consume_key(M::NONE, Key::ArrowLeft) {
            target = Some((left, false));
        } else if i.consume_key(M::NONE, Key::Escape) {
            if editing {
                app.grid.editing = None;
            } else {
                app.grid.anchor = None;
            }
        } else if !editing && i.consume_key(M::COMMAND | M::SHIFT, Key::V) {
            cmd = Some(Command::PasteTransposed);
        } else if !editing && i.consume_key(M::COMMAND, Key::A) {
            cmd = Some(Command::SelectAll);
        } else if !editing && i.consume_key(M::COMMAND, Key::E) {
            cmd = Some(Command::Exclude);
        }
    });
    if let Some(c) = cmd {
        // Drop the plain paste event that may accompany Ctrl+Shift+V.
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
        run_command(app, ctx, id, c, None);
        return;
    }
    if let Some((next, extend)) = target {
        if let Some(buf) = app.grid.editing.take() {
            if let Some(t) = app.project.table_mut(id) {
                let c = t.column_at(col);
                t.set_text(row, c, &buf);
            }
        }
        app.grid.select(next, extend);
        ctx.memory_mut(|m| m.surrender_focus(editor));
        return;
    }
    if editing {
        // A multi-cell paste while editing goes into the table.
        let pasted = ctx.input_mut(|i| {
            let pos = i.events.iter().position(|e| matches!(e, egui::Event::Paste(s) if s.contains('\t') || s.contains('\n')))?;
            match i.events.remove(pos) {
                egui::Event::Paste(s) => Some(s),
                _ => None,
            }
        });
        if let Some(text) = pasted {
            app.grid.editing = None;
            run_command(app, ctx, id, Command::Paste, Some(text));
        }
        return;
    }
    // Not editing: typing starts an edit, Delete clears, F2 edits in place.
    let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
    let mut start: Option<String> = None;
    for e in events {
        match e {
            egui::Event::Text(s) if !s.is_empty() => start = Some(start.unwrap_or_default() + &s),
            egui::Event::Paste(s) => run_command(app, ctx, id, Command::Paste, Some(s)),
            egui::Event::Copy => run_command(app, ctx, id, Command::Copy, None),
            egui::Event::Cut => run_command(app, ctx, id, Command::Cut, None),
            egui::Event::Key { key: Key::Delete | Key::Backspace, pressed: true, .. } => run_command(app, ctx, id, Command::Clear, None),
            egui::Event::Key { key: Key::F2, pressed: true, .. } => {
                if let Some(t) = app.project.table(id) {
                    start = Some(t.cell_text(row, t.column_at(col)));
                }
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        app.grid.anchor = None;
        app.grid.editing = Some(s);
        app.grid.focus_editor = true;
    }
}

/// A toolbar button with an icon and a label.
fn tool(ui: &mut egui::Ui, icon: &str, label: &str, tip: &str) -> bool {
    let p = theme::pal();
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), p.text);
    let size = Vec2::new(galley.size().x + 38.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    icons::paint(ui, icon, Rect::from_min_size(rect.min + Vec2::new(9.0, 7.0), Vec2::splat(16.0)), p.text);
    ui.painter().galley(rect.min + Vec2::new(31.0, (size.y - galley.size().y) / 2.0), galley, p.text);
    resp.on_hover_text(tip).clicked()
}

fn group_sep(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 24.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0_f32, theme::pal().border));
}

fn toolbar(app: &mut App, ui: &mut egui::Ui, id: u64) -> Option<Command> {
    let p = theme::pal();
    let mut cmd = None;
    let t = app.project.table_mut(id)?;
    ui.horizontal(|ui| {
        icons::paint(
            ui,
            crate::app::table_icon(t.kind),
            Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 5.0), Vec2::splat(22.0)),
            p.accent,
        );
        ui.add_space(28.0);
        ui.add(egui::TextEdit::singleline(&mut t.name).font(theme::bold(20.0)).frame(false).desired_width(320.0));
        ui.label(RichText::new(format!("{} table", t.kind.label())).color(p.weak));
    });
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(p.panel)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .stroke(egui::Stroke::new(1.0_f32, p.border))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (icon, label, tip, c) in [
                    ("scissors", "Cut", "Cut (Ctrl+X)", Command::Cut),
                    ("copy", "Copy", "Copy (Ctrl+C)", Command::Copy),
                    ("clipboard-paste", "Paste", "Paste (Ctrl+V)", Command::Paste),
                    ("arrow-down-up", "Paste transposed", "Paste with rows and columns swapped (Ctrl+Shift+V)", Command::PasteTransposed),
                ] {
                    if tool(ui, icon, label, tip) {
                        cmd = Some(c);
                    }
                }
                group_sep(ui);
                for (icon, label, tip, c) in [
                    ("list-plus", "Insert row", "Insert an empty row above the selection", Command::InsertRow),
                    ("list-x", "Delete rows", "Delete the selected rows", Command::DeleteRows),
                    ("eraser", "Clear", "Empty the selected cells (Delete)", Command::Clear),
                    (
                        "ban",
                        "Exclude",
                        "Exclude the selected values from analyses and graphs, or include them again (Ctrl+E)",
                        Command::Exclude,
                    ),
                ] {
                    if tool(ui, icon, label, tip) {
                        cmd = Some(c);
                    }
                }
                group_sep(ui);
                if tool(ui, "file-input", "Import", "Import a CSV or text file") {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Text data", &["csv", "tsv", "txt"]).pick_file() {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => {
                                let unreadable = import_text(t, &text);
                                app.status = format!("Imported {}", path.display());
                                if unreadable > 0 {
                                    app.status += &format!(": {unreadable} value(s) were not numbers and were left empty");
                                }
                            }
                            Err(e) => app.status = format!("Could not read {}: {e}", path.display()),
                        }
                    }
                }
                if tool(ui, "file-output", "Export", "Save the table as a CSV file") {
                    if let Some(path) =
                        rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", t.name)).save_file()
                    {
                        let text = export_csv(t);
                        app.status = match std::fs::write(&path, text) {
                            Ok(()) => format!("Saved {}", path.display()),
                            Err(e) => format!("Could not save: {e}"),
                        };
                    }
                }
                if tool(ui, "table-2", "Copy all", "Copy the whole table, with titles") {
                    ui.ctx().copy_text(export_csv(t).replace(',', "\t"));
                    app.status = "Table copied — paste it into Excel or another sheet".into();
                }
                group_sep(ui);
                if t.kind.allows_replicates() {
                    ui.label(RichText::new(if t.kind == TableKind::XY { "Replicates" } else { "Observations per group" }).color(p.weak));
                    let mut reps = t.replicates;
                    if ui.add(egui::DragValue::new(&mut reps).range(1..=100)).changed() {
                        t.set_replicates(reps);
                    }
                    ui.add_space(6.0);
                }
                ui.checkbox(&mut t.show_row_titles, "Row titles");
            });
        });
    ui.label(RichText::new(hint(t.kind)).small().color(p.weak));
    ui.add_space(4.0);
    cmd
}

pub fn table_view(app: &mut App, ui: &mut egui::Ui, id: u64) {
    keyboard(app, ui.ctx(), id);
    if let Some(cmd) = toolbar(app, ui, id) {
        run_command(app, ui.ctx(), id, cmd, None);
    }
    sheet(app, ui, id);
}

fn sheet(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let p = theme::pal();
    let ctx = ui.ctx().clone();
    let Some(t) = app.project.table(id) else { return };
    let grid = &mut app.grid;
    // The scrollable extent grows as data, the cursor or the view move on.
    let used_cols = t.grid_columns().len();
    let cur = grid.cursor.unwrap_or((0, 0));
    grid.extent.0 = grid.extent.0.max(t.used_rows() + 200).max(cur.0 + 100).max(300);
    grid.extent.1 = grid.extent.1.max(used_cols + 12).max(cur.1 + 8).max(16);
    let content = Vec2::new(NUM_W + col_x(t, grid.extent.1), HEAD + grid.extent.0 as f32 * ROW_H);
    let mut clicked: Option<((usize, usize), bool, bool)> = None; // cell, double, shift
    let mut drag_to: Option<(usize, usize)> = None;
    let mut header_click: Option<(usize, bool)> = None; // column, double
    let mut row_click: Option<usize> = None;
    let mut commit: Option<((usize, usize), String)> = None;
    let mut title_commit: Option<(TitleTarget, String)> = None;
    let mut menu_cmd: Option<Command> = None;
    let mut remove_dataset: Option<usize> = None;
    let mut grow = (false, false);

    egui::Frame::new().fill(p.panel).corner_radius(10.0).stroke(egui::Stroke::new(1.0_f32, p.border)).inner_margin(1.0).show(ui, |ui| {
        egui::ScrollArea::both().id_salt(("sheet", id)).auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            ui.set_min_size(content);
            let base = ui.max_rect().min;
            let screen = |x: f32, y: f32| Pos2::new(base.x + x, base.y + y);
            let vis = Rect::from_min_max(screen(viewport.min.x, viewport.min.y), screen(viewport.max.x, viewport.max.y));
            let painter = ui.painter_at(vis);
            // Visible range.
            let r0 = (viewport.min.y / ROW_H) as usize;
            let r1 = ((viewport.max.y - HEAD) / ROW_H).ceil().max(0.0) as usize + 1;
            let c0 = col_at_x(t, viewport.min.x);
            let c1 = col_at_x(t, viewport.max.x - NUM_W) + 1;
            grow = (r1 + 50 > grid.extent.0, c1 + 4 > grid.extent.1);
            let cell_rect = |r: usize, c: usize| {
                Rect::from_min_size(screen(NUM_W + col_x(t, c), HEAD + r as f32 * ROW_H), Vec2::new(col_w(t, c), ROW_H))
            };

            // Body.
            for r in r0..=r1 {
                let y = HEAD + r as f32 * ROW_H;
                let row_rect = Rect::from_min_max(screen(viewport.min.x, y), screen(viewport.max.x, y + ROW_H));
                if r % 2 == 1 {
                    painter.rect_filled(row_rect, 0.0, p.stripe);
                }
                for c in c0..=c1 {
                    let rect = cell_rect(r, c);
                    if grid.in_block(r, c) && grid.anchor.is_some() {
                        painter.rect_filled(rect, 0.0, p.selected);
                    }
                    let col = t.column_at(c);
                    if grid.cursor == Some((r, c)) && grid.editing.is_some() {
                        continue;
                    }
                    let text = t.cell_text(r, col);
                    if text.is_empty() {
                        continue;
                    }
                    let bad_code = t.kind == TableKind::Survival
                        && matches!(col, GridColumn::Value { .. })
                        && t.get(r, col).is_some_and(|v| v != 0.0 && v != 1.0);
                    let excluded = t.is_excluded(r, col);
                    let color = if bad_code {
                        Color32::RED
                    } else if excluded {
                        p.disabled
                    } else {
                        p.strong
                    };
                    if col == GridColumn::RowTitle {
                        painter.text(rect.left_center() + Vec2::new(8.0, 0.0), egui::Align2::LEFT_CENTER, text, theme::bold(13.0), color);
                    } else {
                        let r = painter.text(
                            rect.right_center() - Vec2::new(8.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            text,
                            egui::FontId::proportional(13.5),
                            color,
                        );
                        if excluded {
                            // Struck through, like an excluded value in Prism.
                            painter.hline(r.x_range(), r.center().y, egui::Stroke::new(1.2_f32, p.weak));
                        }
                    }
                }
            }
            // Vertical lines between columns (stronger between data sets).
            for c in c0..=c1 + 1 {
                let x = base.x + NUM_W + col_x(t, c);
                let first_of_set = matches!(t.column_at(c), GridColumn::Value { replicate: 0, .. }) || c <= t.leading_columns();
                let stroke = egui::Stroke::new(1.0_f32, if first_of_set { p.border } else { p.border.gamma_multiply(0.45) });
                painter.vline(x, vis.y_range(), stroke);
            }
            // Active cell and block outline.
            if let Some(((br0, br1), (bc0, bc1))) = grid.block() {
                let r = cell_rect(br0, bc0).union(cell_rect(br1, bc1));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, p.accent), egui::StrokeKind::Inside);
            }
            if let Some((r, c)) = grid.cursor {
                let rect = cell_rect(r, c);
                if grid.editing.is_none() {
                    painter.rect_filled(rect.shrink(1.0), 0.0, p.cell_cursor.gamma_multiply(0.55));
                    let text = t.cell_text(r, t.column_at(c));
                    if !text.is_empty() {
                        let align =
                            if t.column_at(c) == GridColumn::RowTitle { egui::Align2::LEFT_CENTER } else { egui::Align2::RIGHT_CENTER };
                        let pos = if align == egui::Align2::LEFT_CENTER {
                            rect.left_center() + Vec2::new(8.0, 0.0)
                        } else {
                            rect.right_center() - Vec2::new(8.0, 0.0)
                        };
                        painter.text(pos, align, text, egui::FontId::proportional(13.5), p.strong);
                    }
                }
                painter.rect_stroke(rect, 2.0, egui::Stroke::new(2.0_f32, p.accent), egui::StrokeKind::Inside);
                if grid.scroll {
                    ui.scroll_to_rect(Rect::from_min_max(rect.min - Vec2::new(NUM_W, HEAD), rect.max + Vec2::new(8.0, 8.0)), None);
                }
            }

            // Sticky row numbers.
            let num_rect = Rect::from_min_max(screen(viewport.min.x, viewport.min.y), screen(viewport.min.x + NUM_W, viewport.max.y));
            painter.rect_filled(num_rect, 0.0, p.header);
            for r in r0..=r1 {
                let y = HEAD + r as f32 * ROW_H;
                let rr = Rect::from_min_size(screen(viewport.min.x, y), Vec2::new(NUM_W, ROW_H));
                let selected_row = grid.block().is_some_and(|((a, b), _)| r >= a && r <= b);
                if selected_row {
                    painter.rect_filled(rr, 0.0, p.selected);
                }
                painter.text(
                    rr.center(),
                    egui::Align2::CENTER_CENTER,
                    (r + 1).to_string(),
                    egui::FontId::proportional(11.5),
                    if selected_row { p.accent } else { p.weak },
                );
            }
            painter.vline(num_rect.right(), num_rect.y_range(), egui::Stroke::new(1.0_f32, p.border));

            // Sticky header: data set titles and subcolumn letters.
            let head_rect = Rect::from_min_max(screen(viewport.min.x, viewport.min.y), screen(viewport.max.x, viewport.min.y + HEAD));
            painter.rect_filled(head_rect, 0.0, p.header);
            let hy = viewport.min.y;
            for c in c0..=c1 {
                let x = NUM_W + col_x(t, c);
                let w = col_w(t, c);
                let col = t.column_at(c);
                let sub = Rect::from_min_size(screen(x, hy + HEAD_TITLE), Vec2::new(w, HEAD_SUB));
                let (title, sub_label, exists) = match col {
                    GridColumn::RowTitle => ("Row title".to_string(), String::new(), true),
                    GridColumn::X => (if t.x_title.is_empty() { "X".to_string() } else { t.x_title.clone() }, "X".to_string(), true),
                    GridColumn::Value { dataset, replicate } => {
                        let exists = dataset < t.datasets.len();
                        let title = if exists { t.datasets[dataset].title.clone() } else { format!("Group {}", dataset_letter(dataset)) };
                        let sub = if t.replicates > 1 {
                            format!("{}:Y{}", dataset_letter(dataset), replicate + 1)
                        } else {
                            dataset_letter(dataset)
                        };
                        (if replicate == 0 { title } else { String::new() }, sub, exists)
                    }
                };
                let span = match col {
                    GridColumn::Value { replicate: 0, .. } => t.replicates as f32 * COL_W,
                    GridColumn::Value { .. } => 0.0,
                    _ => w,
                };
                if span > 0.0 {
                    let tr = Rect::from_min_size(screen(x, hy), Vec2::new(span, HEAD_TITLE));
                    let editing_this = match (grid.title_edit.as_ref(), col) {
                        (Some((TitleTarget::Dataset(d), _)), GridColumn::Value { dataset, .. }) => *d == dataset,
                        (Some((TitleTarget::X, _)), GridColumn::X) => true,
                        _ => false,
                    };
                    if !editing_this {
                        let color = if exists { p.strong } else { p.disabled };
                        let font = if exists { theme::bold(13.0) } else { egui::FontId::proportional(13.0) };
                        let galley = painter.layout(title, font, color, span - 12.0);
                        painter.galley(Pos2::new(tr.left() + 8.0, tr.center().y - galley.size().y / 2.0), galley, color);
                    }
                    painter.vline(tr.left(), tr.y_range(), egui::Stroke::new(1.0_f32, p.border));
                }
                let selected_col = grid.block().is_some_and(|(_, (a, b))| c >= a && c <= b);
                if selected_col {
                    painter.rect_filled(sub, 0.0, p.selected);
                }
                painter.text(
                    sub.center(),
                    egui::Align2::CENTER_CENTER,
                    sub_label,
                    egui::FontId::proportional(11.0),
                    if selected_col { p.accent } else { p.weak },
                );
            }
            painter.hline(head_rect.x_range(), head_rect.top() + HEAD_TITLE, egui::Stroke::new(1.0_f32, p.border.gamma_multiply(0.6)));
            painter.hline(head_rect.x_range(), head_rect.bottom(), egui::Stroke::new(1.0_f32, p.border));
            let corner = Rect::from_min_size(screen(viewport.min.x, viewport.min.y), Vec2::new(NUM_W, HEAD));
            painter.rect_filled(corner, 0.0, p.header);
            icons::paint(ui, crate::app::table_icon(t.kind), Rect::from_center_size(corner.center(), Vec2::splat(16.0)), p.weak);

            // Cell editor.
            if let (Some((r, c)), Some(buf)) = (grid.cursor, grid.editing.as_mut()) {
                let rect = cell_rect(r, c).shrink2(Vec2::new(4.0, 2.0));
                let resp = ui.put(
                    rect,
                    egui::TextEdit::singleline(buf)
                        .id(egui::Id::new(EDITOR_ID))
                        .lock_focus(true)
                        .frame(false)
                        .horizontal_align(egui::Align::RIGHT),
                );
                if grid.focus_editor {
                    resp.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
                        let end = egui::text::CCursor::new(buf.chars().count());
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(ui.ctx(), resp.id);
                    }
                    grid.focus_editor = false;
                } else if resp.lost_focus() {
                    commit = Some(((r, c), buf.clone()));
                }
            }
            // Title editor.
            if let Some((target, buf)) = grid.title_edit.as_mut() {
                let c = match *target {
                    TitleTarget::X => (0..t.leading_columns()).find(|&i| t.column_at(i) == GridColumn::X).unwrap_or(0),
                    TitleTarget::Dataset(d) => t.leading_columns() + d * t.replicates,
                };
                let span = if matches!(target, TitleTarget::Dataset(_)) { t.replicates as f32 * COL_W } else { COL_W };
                let rect = Rect::from_min_size(screen(NUM_W + col_x(t, c), hy), Vec2::new(span, HEAD_TITLE)).shrink(3.0);
                let resp = ui.put(rect, egui::TextEdit::singleline(buf).font(theme::bold(13.0)));
                if !resp.has_focus() && !resp.lost_focus() {
                    resp.request_focus();
                }
                if resp.lost_focus() {
                    title_commit = Some((*target, buf.clone()));
                }
            }

            // Mouse.
            let resp = ui.interact(vis, ui.id().with("sheet_area"), Sense::click_and_drag());
            let hit = |pos: Pos2| -> (Option<(usize, usize)>, Option<usize>, Option<usize>) {
                let q = pos - base;
                let in_head = q.y - viewport.min.y < HEAD;
                let in_nums = q.x - viewport.min.x < NUM_W;
                let r = ((q.y - HEAD) / ROW_H).max(0.0) as usize;
                let c = col_at_x(t, q.x - NUM_W);
                match (in_head, in_nums) {
                    (true, true) => (None, None, None),
                    (true, false) => (None, Some(c), None),
                    (false, true) => (None, None, Some(r)),
                    (false, false) => (Some((r, c)), None, None),
                }
            };
            if let Some(pos) = resp.interact_pointer_pos() {
                let (cell, head, num) = hit(pos);
                let shift = ui.input(|i| i.modifiers.shift);
                if resp.double_clicked() {
                    if let Some(c) = head {
                        header_click = Some((c, true));
                    } else if let Some(cell) = cell {
                        clicked = Some((cell, true, false));
                    }
                } else if resp.clicked() || resp.drag_started() {
                    if let Some(c) = head {
                        header_click = Some((c, false));
                    } else if let Some(r) = num {
                        row_click = Some(r);
                    } else if let Some(cell) = cell {
                        clicked = Some((cell, false, shift));
                    }
                } else if resp.dragged() {
                    drag_to = cell;
                }
            }
            if resp.secondary_clicked() {
                if let Some(cell) = resp.interact_pointer_pos().and_then(|p| hit(p).0) {
                    if !grid.in_block(cell.0, cell.1) {
                        clicked = Some((cell, false, false));
                    }
                }
            }
            resp.context_menu(|ui| {
                ui.set_min_width(220.0);
                for (icon, label, c) in [
                    ("scissors", "Cut          Ctrl+X", Command::Cut),
                    ("copy", "Copy         Ctrl+C", Command::Copy),
                    ("clipboard-paste", "Paste        Ctrl+V", Command::Paste),
                    ("arrow-down-up", "Paste transposed  Ctrl+Shift+V", Command::PasteTransposed),
                    ("eraser", "Clear        Delete", Command::Clear),
                    ("ban", "Exclude / include   Ctrl+E", Command::Exclude),
                    ("list-plus", "Insert row above", Command::InsertRow),
                    ("list-x", "Delete rows", Command::DeleteRows),
                ] {
                    if ui.add(egui::Button::image_and_text(icons::image(icon, 14.0, theme::pal().text), label)).clicked() {
                        menu_cmd = Some(c);
                        ui.close_menu();
                    }
                }
                if let Some((_, (bc0, _))) = grid.block() {
                    if let GridColumn::Value { dataset, .. } = t.column_at(bc0) {
                        if dataset < t.datasets.len() && t.datasets.len() > 1 {
                            ui.separator();
                            if ui
                                .add(egui::Button::image_and_text(
                                    icons::image("trash-2", 14.0, theme::pal().text),
                                    format!("Delete data set “{}”", t.datasets[dataset].title),
                                ))
                                .clicked()
                            {
                                remove_dataset = Some(dataset);
                                ui.close_menu();
                            }
                        }
                    }
                }
            });
        });
    });
    if grow.0 {
        grid.extent.0 += 300;
    }
    if grow.1 {
        grid.extent.1 += 12;
    }
    grid.scroll = false;

    // Keep the keyboard on the sheet while a cell is selected.
    let nav = egui::Id::new(NAV_ID);
    if grid.cursor.is_some() && grid.editing.is_none() && grid.title_edit.is_none() {
        let resp = ui.interact(Rect::from_min_size(ui.min_rect().min, Vec2::splat(1.0)), nav, Sense::focusable_noninteractive());
        let focused = ctx.memory(|m| m.focused());
        if !resp.has_focus() && (focused.is_none() || focused == Some(egui::Id::new(EDITOR_ID))) {
            resp.request_focus();
        }
        keep_keys(&ctx, nav);
    }
    if grid.editing.is_some() {
        keep_keys(&ctx, egui::Id::new(EDITOR_ID));
    }

    let Some(t) = app.project.table_mut(id) else { return };
    let grid = &mut app.grid;
    if let Some(((r, c), buf)) = commit {
        let col = t.column_at(c);
        t.set_text(r, col, &buf);
        grid.editing = None;
    }
    if let Some((target, text)) = title_commit {
        match target {
            TitleTarget::X => t.x_title = text,
            TitleTarget::Dataset(d) => {
                while t.datasets.len() <= d {
                    let n = t.datasets.len();
                    t.add_dataset(format!("Group {}", dataset_letter(n)));
                }
                t.datasets[d].title = text;
            }
        }
        grid.title_edit = None;
    }
    if let Some((cell, double, shift)) = clicked {
        if let (Some(buf), Some((er, ec))) = (grid.editing.take(), grid.cursor) {
            let col = t.column_at(ec);
            t.set_text(er, col, &buf);
        }
        grid.select(cell, shift);
        grid.scroll = false;
        ctx.memory_mut(|m| m.request_focus(nav));
        if double {
            grid.editing = Some(t.cell_text(cell.0, t.column_at(cell.1)));
            grid.focus_editor = true;
        }
    }
    if let Some(cell) = drag_to {
        if grid.cursor != Some(cell) {
            if grid.anchor.is_none() {
                grid.anchor = grid.cursor;
            }
            grid.cursor = Some(cell);
        }
    }
    if let Some((c, double)) = header_click {
        let col = t.column_at(c);
        if double {
            match col {
                GridColumn::Value { dataset, .. } => {
                    let title = t.datasets.get(dataset).map_or_else(|| format!("Group {}", dataset_letter(dataset)), |d| d.title.clone());
                    grid.title_edit = Some((TitleTarget::Dataset(dataset), title));
                }
                GridColumn::X => grid.title_edit = Some((TitleTarget::X, t.x_title.clone())),
                GridColumn::RowTitle => {}
            }
        } else {
            // Select the whole column (all the subcolumns of a data set).
            let (first, last) = match col {
                GridColumn::Value { dataset, .. } => {
                    let f = t.leading_columns() + dataset * t.replicates;
                    (f, f + t.replicates - 1)
                }
                _ => (c, c),
            };
            grid.anchor = Some((0, first));
            grid.cursor = Some((t.used_rows().max(1) - 1, last));
            ctx.memory_mut(|m| m.request_focus(nav));
        }
    }
    if let Some(r) = row_click {
        grid.anchor = Some((r, 0));
        grid.cursor = Some((r, t.grid_columns().len().max(1) - 1));
        ctx.memory_mut(|m| m.request_focus(nav));
    }
    if let Some(d) = remove_dataset {
        app.project.remove_dataset(id, d);
        grid.cursor = None;
        grid.anchor = None;
    }
    if let Some(cmd) = menu_cmd {
        run_command(app, &ctx, id, cmd, None);
    }
}

fn hint(kind: TableKind) -> &'static str {
    match kind {
        TableKind::Survival => "X = time. In each group enter 1 for an event (death) or 0 for a censored subject.",
        TableKind::Contingency => "Enter counts (number of subjects), not percentages.",
        TableKind::Grouped => {
            "Rows are one factor, data sets the other. Put the observations of a condition side by side in its subcolumns."
        }
        _ => "Double-click a column title to rename it. Right-click for cut, copy, paste and more.",
    }
}

/// The table as CSV, with a header line of column titles.
fn export_csv(t: &DataTable) -> String {
    let cols = t.grid_columns();
    let quote = |s: String| if s.contains(',') || s.contains('"') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s };
    let header: Vec<String> = cols
        .iter()
        .map(|c| match *c {
            GridColumn::RowTitle => "Row".to_string(),
            GridColumn::X => {
                if t.x_title.is_empty() {
                    "X".into()
                } else {
                    t.x_title.clone()
                }
            }
            GridColumn::Value { dataset, replicate } => {
                let title = t.datasets[dataset].title.clone();
                if t.replicates > 1 {
                    format!("{title} {}", replicate + 1)
                } else {
                    title
                }
            }
        })
        .map(quote)
        .collect();
    let mut out = header.join(",");
    for r in 0..t.used_rows() {
        out.push('\n');
        out.push_str(&cols.iter().map(|&c| quote(t.cell_text(r, c))).collect::<Vec<_>>().join(","));
    }
    out.push('\n');
    out
}

/// Imports delimited text: a non-numeric first line becomes the data set titles.
/// Imports a text file; returns the number of values that were not numbers.
fn import_text(t: &mut DataTable, text: &str) -> usize {
    let mut lines = text.lines();
    let first_value = t.leading_columns() - usize::from(t.kind.has_x());
    let mut body = text.to_string();
    // Chosen on the whole file, header included, so that "1,5" data under a
    // comma separated header are still split.
    let sep = hg_core::table::separator(text);
    if let Some(first) = lines.next() {
        let fields = hg_core::table::fields(first, sep);
        if fields.iter().any(|f| !f.trim().is_empty() && hg_core::table::parse_number(f).is_none()) {
            let value_fields = if t.kind.has_x() { &fields[1.min(fields.len())..] } else { &fields[..] };
            let per = t.replicates.max(1);
            for (k, chunk) in value_fields.chunks(per).enumerate() {
                if k >= t.datasets.len() {
                    t.add_dataset(String::new());
                }
                t.datasets[k].title = chunk[0].trim().trim_matches('"').to_string();
            }
            body = lines.collect::<Vec<_>>().join("\n");
        }
    }
    t.paste_with(0, first_value, &body, sep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Selection};

    /// Runs one frame of the table view with the given input events, then an
    /// idle frame (as the real app repaints after every input).
    fn frame(ctx: &egui::Context, app: &mut App, id: u64, events: Vec<egui::Event>) {
        run(ctx, app, id, events);
        run(ctx, app, id, vec![]);
    }

    fn run(ctx: &egui::Context, app: &mut App, id: u64, events: Vec<egui::Event>) {
        let input =
            egui::RawInput { events, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| table_view(app, ui, id));
        });
    }

    fn key(k: Key, shift: bool) -> egui::Event {
        let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn keyboard_navigation_typing_and_blocks() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, None, None, false);
        app.welcome = None;
        let id = app.project.add_table(DataTable::new(0, "t", TableKind::Column, 1, 0, 1));
        app.selection = Selection::Table(id);
        app.grid.cursor = Some((0, 0));
        frame(&ctx, &mut app, id, vec![]);
        // Typing starts editing; Enter stores the value and moves down.
        frame(&ctx, &mut app, id, vec![egui::Event::Text("4".into())]);
        assert_eq!(app.grid.editing.as_deref(), Some("4"));
        frame(&ctx, &mut app, id, vec![egui::Event::Text("2".into())]);
        frame(&ctx, &mut app, id, vec![key(Key::Enter, false)]);
        assert_eq!(app.project.table(id).unwrap().column_values(0), vec![42.0]);
        assert_eq!(app.grid.cursor, Some((1, 0)));
        // Arrows and Tab move; typing in an empty column far right adds data sets.
        frame(&ctx, &mut app, id, vec![key(Key::ArrowRight, false)]);
        assert_eq!(app.grid.cursor, Some((1, 1)));
        frame(&ctx, &mut app, id, vec![egui::Event::Text("7".into())]);
        frame(&ctx, &mut app, id, vec![key(Key::Tab, false)]);
        assert_eq!(app.grid.cursor, Some((1, 2)));
        assert_eq!(app.project.table(id).unwrap().column_values(1), vec![7.0]);
        frame(&ctx, &mut app, id, vec![key(Key::Tab, true)]);
        frame(&ctx, &mut app, id, vec![key(Key::ArrowUp, false)]);
        assert_eq!(app.grid.cursor, Some((0, 1)));
        // Paste a block, then select it with Shift+arrows and cut it.
        frame(&ctx, &mut app, id, vec![egui::Event::Paste("1\t2\n3\t4".into())]);
        assert_eq!(app.project.table(id).unwrap().column_values(2), vec![2.0, 4.0]);
        frame(&ctx, &mut app, id, vec![key(Key::ArrowDown, true)]);
        frame(&ctx, &mut app, id, vec![key(Key::ArrowRight, true)]);
        assert_eq!(app.grid.block(), Some(((0, 1), (1, 2))));
        frame(&ctx, &mut app, id, vec![egui::Event::Cut]);
        let t = app.project.table(id).unwrap();
        assert!(t.column_values(1).is_empty() && t.column_values(2).is_empty());
        // Far away cells work too: the sheet has no edge.
        app.grid.select((500, 30), false);
        frame(&ctx, &mut app, id, vec![egui::Event::Text("9".into())]);
        frame(&ctx, &mut app, id, vec![key(Key::Enter, false)]);
        assert_eq!(app.project.table(id).unwrap().column_values(30), vec![9.0]);
    }
}
