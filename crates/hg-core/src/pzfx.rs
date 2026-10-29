//! Import of Prism XML project files (`.pzfx`).
//!
//! The format is plain XML: each `<Table>` (or `<HugeTable>`) has a
//! `TableType`, an optional `<RowTitlesColumn>`, an optional `<XColumn>` and
//! one `<YColumn>` per data set whose `<Subcolumn>`s hold `<d>` values. Only
//! the data are imported; analyses and graphs are not.

use crate::table::{DataTable, GridColumn, TableKind};
use crate::Project;

/// A project read from a `.pzfx` file, with notes about what was adapted.
pub struct Imported {
    pub project: Project,
    pub warnings: Vec<String>,
}

/// All the text inside a node (values may be wrapped in formatting tags).
fn text(n: roxmltree::Node) -> String {
    n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>().trim().to_string()
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}

/// Cells of the subcolumns of a column element: (value, excluded).
fn subcolumns(col: roxmltree::Node) -> Vec<Vec<(String, bool)>> {
    col.children()
        .filter(|c| c.has_tag_name("Subcolumn"))
        .map(|sc| sc.children().filter(|d| d.has_tag_name("d")).map(|d| (text(d), d.attribute("Excluded") == Some("1"))).collect())
        .collect()
}

fn table_kind(table_type: &str) -> (TableKind, Option<String>) {
    match table_type {
        "XY" => (TableKind::XY, None),
        "OneWay" => (TableKind::Column, None),
        "TwoWay" => (TableKind::Grouped, None),
        "Contingency" => (TableKind::Contingency, None),
        "Survival" => (TableKind::Survival, None),
        "PartsOfWhole" => (TableKind::PartsOfWhole, None),
        other => (TableKind::Column, Some(format!("tables of type “{other}” are not supported yet and were imported as column tables"))),
    }
}

/// Parses the text of a `.pzfx` file.
pub fn import(xml: &str) -> Result<Imported, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not a valid Prism XML file: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("GraphPadPrismFile") {
        return Err("not a Prism XML (.pzfx) file".into());
    }
    let mut project = Project::default();
    let mut warnings = Vec::new();
    let tables = root.children().filter(|n| n.has_tag_name("Table") || n.has_tag_name("HugeTable"));
    for node in tables {
        let name = child(node, "Title").map(text).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("Data {}", project.tables.len() + 1));
        let (kind, note) = table_kind(node.attribute("TableType").unwrap_or("OneWay"));
        if let Some(n) = note {
            warnings.push(format!("“{name}”: {n}."));
        }
        let y_format = node.attribute("YFormat").unwrap_or("replicates");
        if node.attribute("XFormat") == Some("date") {
            warnings.push(format!("“{name}”: the X dates were imported as elapsed time (the numbers Prism stores)."));
        }
        let y_columns: Vec<roxmltree::Node> = node.children().filter(|c| c.has_tag_name("YColumn")).collect();
        let raw = y_format == "replicates" || kind == TableKind::Column;
        let reps = if raw { y_columns.iter().map(|c| subcolumns(*c).len()).max().unwrap_or(1).max(1) } else { 1 };
        if !raw {
            warnings.push(format!("“{name}”: the values were entered as mean and error ({y_format}); only the means were imported."));
        }
        let mut t = DataTable::new(0, name.clone(), kind, y_columns.len().max(1), 0, reps);
        t.replicates = reps;
        for d in &mut t.datasets {
            for row in &mut d.values {
                row.resize(reps, None);
            }
        }
        let mut excluded = 0;
        let mut skipped_x = 0;
        let mut unreadable = 0;
        let mut put = |t: &mut DataTable, row: usize, col: GridColumn, s: &str, excl: bool| {
            if s.is_empty() {
                return;
            }
            let Some(v) = crate::table::parse_number(s) else {
                unreadable += 1;
                return;
            };
            match (excl, col) {
                // Struck-through Y values are kept and marked as excluded, as in Prism.
                (true, GridColumn::Value { dataset, replicate }) => {
                    t.set(row, col, Some(v));
                    t.excluded.push((dataset, row, replicate));
                    excluded += 1;
                }
                // Excluded X values cannot be marked, so they are left out.
                (true, _) => skipped_x += 1,
                (false, _) => t.set(row, col, Some(v)),
            }
        };
        if let Some(rt) = child(node, "RowTitlesColumn") {
            let titles = subcolumns(rt).into_iter().next().unwrap_or_default();
            if titles.iter().any(|(s, _)| !s.is_empty()) {
                t.show_row_titles = true;
                t.ensure_rows(titles.len());
                for (r, (s, _)) in titles.into_iter().enumerate() {
                    t.row_titles[r] = s;
                }
            }
        }
        if let Some(x) = child(node, "XColumn").or_else(|| child(node, "XAdvancedColumn")) {
            if let Some(title) = child(x, "Title").map(text).filter(|s| !s.is_empty()) {
                t.x_title = title;
            }
            if kind.has_x() {
                for (r, (s, excl)) in subcolumns(x).into_iter().next().unwrap_or_default().into_iter().enumerate() {
                    put(&mut t, r, GridColumn::X, &s, excl);
                }
            }
        }
        for (d, col) in y_columns.iter().enumerate() {
            if let Some(title) = child(*col, "Title").map(text).filter(|s| !s.is_empty()) {
                t.datasets[d].title = title;
            }
            let subs = subcolumns(*col);
            let used = if raw { subs.len() } else { 1 };
            for (k, sub) in subs.into_iter().take(used).enumerate() {
                for (r, (s, excl)) in sub.into_iter().enumerate() {
                    put(&mut t, r, GridColumn::Value { dataset: d, replicate: k }, &s, excl);
                }
            }
        }
        if excluded > 0 {
            warnings.push(format!("“{name}”: {excluded} value(s) marked as excluded in Prism are excluded here too (struck through)."));
        }
        if skipped_x > 0 {
            warnings.push(format!("“{name}”: {skipped_x} X value(s) marked as excluded in Prism were left out."));
        }
        if unreadable > 0 {
            warnings.push(format!("“{name}”: {unreadable} value(s) were not numbers (dates or text) and were skipped."));
        }
        let id = project.add_table(t);
        if let Some(((k, tr), _)) = project.table(id).map(crate::graph::recommend).and_then(|r| r.into_iter().next()) {
            let gid = project.add_graph(id, k);
            if let Some(g) = project.graphs.iter_mut().find(|g| g.id == gid) {
                g.transpose = tr;
            }
        }
    }
    if project.tables.is_empty() {
        return Err("the file contains no data tables".into());
    }
    Ok(Imported { project, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> Imported {
        let path = format!("{}/tests/fixtures/pzfx/{name}.pzfx", env!("CARGO_MANIFEST_DIR"));
        import(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn column_table() {
        let p = load("column").project;
        let t = &p.tables[0];
        assert_eq!(t.kind, TableKind::Column);
        assert_eq!(t.datasets[0].title, "Aa");
        assert_eq!(t.column_values(0), vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(t.column_values(1), vec![100.0, 90.0, 80.0, 70.0]);
        assert_eq!(p.graphs.len(), 1);
    }

    #[test]
    fn xy_with_replicates_row_titles_and_exclusions() {
        let p = load("x_y_rep").project;
        let t = &p.tables[0];
        assert_eq!(t.kind, TableKind::XY);
        assert_eq!(t.replicates, 2);
        assert_eq!(t.x_title, "XX");
        assert_eq!(t.row_titles[1], "B");
        assert_eq!(t.datasets[1].title, "Yb");
        assert_eq!(t.cell_values(0, 0), vec![100.0, 50.0]);
        let strike = load("x_y_with_strike");
        assert!(strike.warnings.iter().any(|w| w.contains("excluded")));
        let t = &strike.project.tables[0];
        assert!(!t.excluded.is_empty());
        for &(d, r, k) in &t.excluded {
            assert!(t.datasets[d].values[r][k].is_some(), "excluded values must be kept");
        }
    }

    #[test]
    fn other_table_types() {
        assert_eq!(load("survival").project.tables[0].kind, TableKind::Survival);
        assert_eq!(load("contingency").project.tables[0].kind, TableKind::Contingency);
        assert_eq!(load("parts_of_whole").project.tables[0].kind, TableKind::PartsOfWhole);
        let huge = load("column_hugetable").project;
        assert_eq!(huge.tables[0].replicates, 53);
        let sdn = load("column_sdn");
        assert!(sdn.warnings.iter().any(|w| w.contains("only the means")));
        // Dates are stored as elapsed time: the numbers are kept.
        let date = load("x_date");
        assert!(date.project.tables[0].x.iter().flatten().count() > 10);
        assert!(date.warnings.iter().any(|w| w.contains("dates")));
    }
}
