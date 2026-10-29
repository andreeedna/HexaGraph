//! Results sheets: large, readable tables with the key numbers highlighted.

use crate::app::App;
use crate::dialogs::AnalyzeDialog;
use eframe::egui::{self, Color32, RichText, Vec2};
use hg_core::analysis::pct;

/// Styles one result cell: P values, asterisks and yes/no answers stand out.
fn cell(text: &str, column: &str, row_label: &str, size: f32, first: bool) -> RichText {
    let t = RichText::new(text).size(size);
    if first {
        return t.color(crate::theme::pal().strong);
    }
    if text.starts_with('*') {
        return t.color(crate::theme::pal().star).strong();
    }
    if text == "ns" {
        return t.color(crate::theme::pal().weak);
    }
    if text == "Yes" || text == "Significant" {
        return t.color(crate::theme::pal().good).strong();
    }
    let p_like =
        column.contains("P value") || column.contains("Adjusted P") || row_label.starts_with("P value") || row_label.ends_with("P value");
    if p_like {
        return t.strong().color(crate::theme::pal().strong);
    }
    t.color(crate::theme::pal().text)
}

pub fn results_view(app: &mut App, ui: &mut egui::Ui, id: u64) {
    let Some(a) = app.project.analysis(id).cloned() else { return };
    let table = app.project.table(a.table_id).cloned();
    let table_name = table.as_ref().map_or("?".to_string(), |t| t.name.clone());
    let zoom_id = egui::Id::new("results_zoom");
    let mut zoom: f32 = ui.data(|d| d.get_temp(zoom_id)).unwrap_or(1.0);
    let out = app.results(id);

    // Header
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let mut name = app.project.analysis_name(id);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut name)
                .font(crate::theme::bold(24.0))
                .text_color(crate::theme::accent())
                .frame(false)
                .desired_width(560.0),
        );
        if resp.changed() {
            if let Some(a) = app.project.analyses.iter_mut().find(|a| a.id == id) {
                a.name = Some(name);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(RichText::new("A+").size(15.0))).on_hover_text("Larger text").clicked() {
                zoom = (zoom + 0.1).min(1.8);
            }
            if ui.add(egui::Button::new(RichText::new("A−").size(15.0))).on_hover_text("Smaller text").clicked() {
                zoom = (zoom - 0.1).max(0.7);
            }
        });
    });
    let sets = match (&a.datasets, &table) {
        (Some(sel), Some(t)) => sel.iter().filter_map(|&d| t.datasets.get(d).map(|x| x.title.clone())).collect::<Vec<_>>().join(", "),
        _ => "all data sets".to_string(),
    };
    ui.label(
        RichText::new(format!(
            "Data: {table_name}  ·  {sets}  ·  α = {}  ·  {} confidence intervals",
            a.options.alpha,
            pct(a.options.ci_level)
        ))
        .size(14.0)
        .weak(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new(RichText::new("⚙ Change parameters…").size(14.0)).min_size(Vec2::new(0.0, 28.0))).clicked() {
            if let Some(t) = app.project.table(a.table_id) {
                app.analyze = Some(AnalyzeDialog::new(t, Some(&a)));
            }
        }
        if let Ok(o) = &out {
            if ui.add(egui::Button::new(RichText::new("📋 Copy all results").size(14.0)).min_size(Vec2::new(0.0, 28.0))).clicked() {
                let text: Vec<String> = o.tables.iter().map(|t| t.to_tsv()).collect();
                ui.ctx().copy_text(text.join("\n"));
                app.status = "Results copied to the clipboard — paste them into Excel or Word".into();
            }
        }
        ui.label(RichText::new("Results update automatically when the data change.").weak());
    });
    ui.add_space(6.0);
    ui.separator();

    let size = 15.0 * zoom;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match out {
        Err(e) => {
            egui::Frame::new().fill(crate::theme::pal().error_bg).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                ui.label(RichText::new(format!("This analysis cannot be computed: {e}")).size(size).color(crate::theme::pal().error_text));
            });
        }
        Ok(o) => {
            for w in &o.warnings {
                // Information (e.g. Prism's own result, excluded values) vs. real warnings.
                let info = w.starts_with("Prism reported") || w.contains("excluded in the data table");
                let pal = crate::theme::pal();
                let (bg, fg, icon) = if info { (pal.selected, pal.accent, "info") } else { (pal.warn_bg, pal.warn_text, "triangle-alert") };
                egui::Frame::new().fill(bg).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
                    ui.set_width(ui.available_width().min(900.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.add(crate::icons::image(icon, size, fg));
                        ui.label(RichText::new(w).size(size).color(fg));
                    });
                });
                ui.add_space(6.0);
            }
            for (k, t) in o.tables.iter().enumerate() {
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(crate::theme::pal().panel)
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(14))
                    .stroke(egui::Stroke::new(1.0_f32, crate::theme::pal().border))
                    .shadow(egui::Shadow { offset: [0, 2], blur: 6, spread: 0, color: Color32::from_black_alpha(18) })
                    .show(ui, |ui| {
                        ui.label(RichText::new(&t.title).size(size * 1.2).strong().color(crate::theme::accent()));
                        ui.add_space(6.0);
                        let kv = t.is_key_value();
                        ui.visuals_mut().faint_bg_color = crate::theme::pal().stripe;
                        egui::Grid::new(("result", id, k)).striped(true).spacing([28.0 * zoom, 7.0 * zoom]).min_col_width(60.0).show(
                            ui,
                            |ui| {
                                if !kv {
                                    for c in &t.columns {
                                        ui.label(RichText::new(c).size(size * 0.95).strong().color(crate::theme::pal().strong));
                                    }
                                    ui.end_row();
                                }
                                for r in &t.rows {
                                    let label = r.first().map(String::as_str).unwrap_or("");
                                    for (j, text) in r.iter().enumerate() {
                                        let col = t.columns.get(j).map(String::as_str).unwrap_or("");
                                        ui.label(cell(text, col, label, size, j == 0));
                                    }
                                    ui.end_row();
                                }
                            },
                        );
                    });
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(zoom_id, zoom));
}
