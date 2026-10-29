//! Import of Prism 10 project files (`.prism`).
//!
//! A `.prism` file is a ZIP archive. `document.json` lists the sheets; each
//! data sheet (`data/sheets/<id>/sheet.json`) names its table and data sets;
//! the values are in `data/tables/<table>/data.csv` (one column per
//! subcolumn, data set after data set) and each data set
//! (`data/sets/<id>.json`) holds its title and cell attributes such as
//! excluded values. Analyses (`analyses/<id>/`) record their kind, inputs,
//! options and the results Prism computed. Graphs are binary and are rebuilt.

use crate::analysis::{Analysis, AnalysisKind, OneWayTest, Options, Tails, TwoGroupTest};
use crate::graph::GraphKind;
use crate::pzfx::Imported;
use crate::table::{DataTable, GridColumn, TableKind};
use crate::Project;
use hg_stats::posthoc::Method;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek};

struct Archive<R: Read + Seek> {
    zip: zip::ZipArchive<R>,
}

impl<R: Read + Seek> Archive<R> {
    fn text(&mut self, path: &str) -> Option<String> {
        let f = self.zip.by_name(path).ok()?;
        let mut s = String::new();
        // shortcut: entries are cut at 512 MB to stop zip bombs; raise it if real projects get that big.
        f.take(1 << 29).read_to_string(&mut s).ok()?;
        Some(s)
    }

    fn json(&mut self, path: &str) -> Option<Value> {
        serde_json::from_str(&self.text(path)?).ok()
    }
}

/// Largest row or subcolumn index read from a file, so a damaged or crafted
/// range such as "0~4000000000" cannot exhaust memory.
const MAX_INDEX: usize = 1 << 20;
/// Largest number of excluded cells read for one data set.
const MAX_EXCLUDED: usize = 1 << 22;

/// Expands a Prism range such as "0", "1~2" or "0,3~5".
fn expand_range(s: &str) -> Vec<usize> {
    s.split(',')
        .filter_map(|part| {
            let mut it = part.trim().split('~');
            let a: usize = it.next()?.trim().parse().ok()?;
            let b: usize = it.next().and_then(|b| b.trim().parse().ok()).unwrap_or(a);
            Some(a.min(MAX_INDEX)..=b.min(MAX_INDEX))
        })
        .flatten()
        .collect()
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// A data set's title (stored as rich text).
fn title_of(set: &Value) -> Option<String> {
    match set.get("title") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(t) => str_of(t, "string").map(str::to_string),
        None => None,
    }
    .filter(|s| !s.trim().is_empty())
}

struct SetInfo {
    uid: String,
    title: Option<String>,
    attributes: Vec<String>,
    subcolumns: usize,
    /// (subcolumn, row) pairs marked as excluded.
    excluded: Vec<(usize, usize)>,
}

fn set_info(uid: &str, set: Option<&Value>, default_subcolumns: usize) -> SetInfo {
    let mut info =
        SetInfo { uid: uid.to_string(), title: None, attributes: Vec::new(), subcolumns: default_subcolumns, excluded: Vec::new() };
    let Some(set) = set else { return info };
    info.title = title_of(set);
    info.attributes = set
        .get("attributes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(ranges) = set.get("replicate ranges").and_then(Value::as_array) {
        let mut max_sub = 0;
        for r in ranges {
            let subs = str_of(r, "range").map(expand_range).unwrap_or_default();
            max_sub = max_sub.max(subs.iter().map(|s| s + 1).max().unwrap_or(0));
            let cells = r.get("replicate").and_then(|rep| rep.get("cellAttributes")).and_then(Value::as_array);
            for c in cells.into_iter().flatten() {
                let excluded =
                    c.get("attributes").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some("EXCLUDED")));
                if excluded {
                    let rows = str_of(c, "rows").map(expand_range).unwrap_or_default();
                    for &sub in &subs {
                        let room = MAX_EXCLUDED.saturating_sub(info.excluded.len());
                        info.excluded.extend(rows.iter().take(room).map(|&row| (sub, row)));
                    }
                }
            }
        }
        if max_sub > 0 {
            info.subcolumns = max_sub;
        }
    }
    info
}

fn table_kind(format: &str) -> (TableKind, Option<String>) {
    let f = format.to_ascii_lowercase().replace(['_', '-', ' '], "");
    match f.as_str() {
        "xy" => (TableKind::XY, None),
        "column" | "oneway" => (TableKind::Column, None),
        "grouped" | "twoway" => (TableKind::Grouped, None),
        "contingency" => (TableKind::Contingency, None),
        "survival" => (TableKind::Survival, None),
        "partsofwhole" => (TableKind::PartsOfWhole, None),
        _ => (TableKind::Column, Some(format!("tables of type “{format}” are not supported yet and were imported as column tables"))),
    }
}

/// Splits one CSV line (Prism writes plain numbers; text may be quoted).
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Finds the first `{"pValue": {"value": …}}` in Prism's results, with the
/// statistic next to it, to show what Prism computed.
fn reported(results: &Value) -> Option<String> {
    match results {
        Value::Object(map) => {
            if let Some(p) = map.get("pValue").and_then(|p| p.get("value")).and_then(Value::as_f64) {
                let mut parts = Vec::new();
                for (key, label) in [("t", "t"), ("F", "F"), ("df", "df"), ("chiSquare", "χ²"), ("U", "U")] {
                    if let Some(v) = map.get(key).and_then(Value::as_f64) {
                        parts.push(format!("{label} = {}", crate::format::num(v)));
                    }
                }
                parts.push(format!("P = {}", crate::format::p(p)));
                return Some(parts.join(", "));
            }
            map.values().find_map(reported)
        }
        Value::Array(a) => a.iter().find_map(reported),
        _ => None,
    }
}

/// Maps a Prism analysis to the HexaGraph one, given the data set indices of its inputs.
fn map_analysis(class: &str, design: &str, inputs: &[usize]) -> Option<AnalysisKind> {
    let class = class.to_ascii_uppercase();
    let design = design.to_ascii_uppercase();
    let (a, b) = (inputs.first().copied().unwrap_or(0), inputs.get(1).copied().unwrap_or(1));
    if class.contains("T_TEST") || class.contains("NONPARAMETRIC") && !class.contains("ONE_WAY") {
        let test = if design.contains("WELCH") {
            TwoGroupTest::Welch
        } else if design.contains("MANN") {
            TwoGroupTest::MannWhitney
        } else if design.contains("KOLMOGOROV") {
            TwoGroupTest::KolmogorovSmirnov
        } else if design.contains("WILCOXON") {
            TwoGroupTest::Wilcoxon
        } else if design.contains("PAIRED") && !design.contains("UNPAIRED") {
            TwoGroupTest::Paired
        } else {
            TwoGroupTest::Unpaired
        };
        return Some(AnalysisKind::TwoGroups { a, b, test });
    }
    if class.contains("ONE_WAY") || class.contains("ONEWAY") {
        let test = if design.contains("KRUSKAL") {
            OneWayTest::KruskalWallis
        } else if design.contains("FRIEDMAN") {
            OneWayTest::Friedman
        } else if design.contains("WELCH") || design.contains("BROWN") {
            OneWayTest::Welch
        } else if design.contains("REPEATED") || design.contains("MATCHED") {
            OneWayTest::RepeatedMeasures
        } else {
            OneWayTest::Ordinary
        };
        return Some(AnalysisKind::OneWay { test, comparisons: Some(Method::Tukey), control: None });
    }
    if class.contains("TWO_WAY") || class.contains("TWOWAY") {
        return Some(AnalysisKind::TwoWay { comparisons: Some(Method::Sidak), within_columns: false, repeated: design.contains("REPEATED") && !design.contains("COLUMN") });
    }
    if class.contains("COLUMN_STAT") || class.contains("DESCRIPTIVE") {
        return Some(AnalysisKind::ColumnStatistics);
    }
    if class.contains("NORMAL") {
        return Some(AnalysisKind::Normality);
    }
    if class.contains("CORREL") {
        return Some(AnalysisKind::Correlation { spearman: design.contains("SPEARMAN") });
    }
    if class.contains("LINEAR_REG") {
        return Some(AnalysisKind::LinearRegression);
    }
    if class.contains("NONLIN") {
        return Some(AnalysisKind::NonlinearFit { model: hg_stats::nonlinear::Model::DoseResponseVariableSlope, fixed: Vec::new() });
    }
    if class.contains("SURVIVAL") {
        return Some(AnalysisKind::Survival { gehan: false });
    }
    if class.contains("CONTINGENCY") || class.contains("CHI") || class.contains("FISHER") {
        return Some(AnalysisKind::Contingency { yates: false });
    }
    None
}

/// Reads a `.prism` file.
pub fn import(bytes: &[u8]) -> Result<Imported, String> {
    let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "not a Prism project (.prism) file".to_string())?;
    let mut ar = Archive { zip };
    let doc = ar.json("document.json").ok_or("the file has no document.json: it is not a Prism 10 project")?;
    let sheets = doc.get("sheets").ok_or("document.json lists no sheets")?;
    let list = |key: &str| -> Vec<String> {
        sheets
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let mut project = Project::default();
    let mut warnings = Vec::new();
    // Prism sheet id -> (table id in the project, data set uid -> index).
    let mut tables: HashMap<String, (u64, HashMap<String, usize>)> = HashMap::new();
    let mut transposed: Vec<String> = Vec::new();
    let mut excluded: Vec<String> = Vec::new();

    for sheet_id in list("data") {
        let Some(sheet) = ar.json(&format!("data/sheets/{sheet_id}/sheet.json")) else {
            warnings.push(format!("a data sheet ({sheet_id}) could not be read and was skipped"));
            continue;
        };
        let name = str_of(&sheet, "title").unwrap_or("Data").to_string();
        let Some(tab) = sheet.get("table") else { continue };
        let (mut kind, note) = table_kind(str_of(tab, "format").unwrap_or("column"));
        if let Some(n) = note {
            warnings.push(format!("“{name}”: {n}."));
        }
        let data_format = str_of(tab, "dataFormat").unwrap_or("y_replicates").to_string();
        let raw = data_format.contains("replicates") || kind == TableKind::Column;
        let reps = tab.get("replicatesCount").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
        let set_ids: Vec<String> = tab
            .get("dataSets")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        // XY tables keep their X column apart: it comes first in the CSV.
        let x_id = str_of(tab, "xDataSet").map(str::to_string);
        let x_subcolumns = if str_of(tab, "xDataSetFormat").is_some_and(|f| f.contains("error")) { 2 } else { 1 };
        let set_ids: Vec<String> = x_id.iter().cloned().chain(set_ids.into_iter().filter(|id| Some(id) != x_id.as_ref())).collect();
        let sets: Vec<SetInfo> = set_ids
            .iter()
            .map(|id| {
                let v = ar.json(&format!("data/sets/{id}.json"));
                if Some(id) == x_id.as_ref() {
                    let mut info = set_info(id, v.as_ref(), x_subcolumns);
                    info.subcolumns = x_subcolumns;
                    info.attributes.push("_X".to_string());
                    return info;
                }
                let is_x = v
                    .as_ref()
                    .and_then(|v| v.get("attributes"))
                    .and_then(Value::as_array)
                    .is_some_and(|a| a.iter().any(|x| x.as_str().is_some_and(|s| s.contains("_X"))));
                set_info(id, v.as_ref(), if is_x { 1 } else { reps })
            })
            .collect();
        let csv = str_of(tab, "uid").and_then(|uid| ar.text(&format!("data/tables/{uid}/data.csv"))).unwrap_or_default();
        let rows: Vec<Vec<String>> = csv.lines().map(csv_fields).collect();
        // Columns of the CSV: data set after data set, each with its subcolumns.
        let mut col = 0;
        let mut x_values: Vec<Option<f64>> = Vec::new();
        let mut row_titles: Vec<String> = Vec::new();
        let mut x_title: Option<String> = None;
        // (set index, values[row][sub], excluded)
        let mut ys: Vec<(usize, Vec<Vec<Option<f64>>>)> = Vec::new();
        for (si, s) in sets.iter().enumerate() {
            let cell = |r: &Vec<String>, c: usize| r.get(c).map(|v| v.trim().to_string()).unwrap_or_default();
            let is_x = s.attributes.iter().any(|a| a.contains("_X"));
            let is_titles = s.attributes.iter().any(|a| a.contains("ROW") && a.contains("TITLE"));
            if is_titles {
                row_titles = rows.iter().map(|r| cell(r, col)).collect();
            } else if is_x {
                x_values = rows.iter().map(|r| crate::table::parse_number(&cell(r, col))).collect();
                x_title = s.title.clone();
            } else {
                let values =
                    rows.iter().map(|r| (0..s.subcolumns).map(|k| crate::table::parse_number(&cell(r, col + k))).collect()).collect();
                ys.push((si, values));
            }
            col += s.subcolumns;
        }
        // Drop untitled data sets without any value (Prism keeps empty placeholders).
        ys.retain(|(si, v)| sets[*si].title.is_some() || v.iter().flatten().any(Option::is_some));
        if ys.is_empty() {
            warnings.push(format!("“{name}”: the table is empty and was skipped."));
            continue;
        }
        let used_rows = (0..rows.len())
            .rev()
            .find(|&r| ys.iter().any(|(_, v)| v[r].iter().any(Option::is_some)) || x_values.get(r).copied().flatten().is_some())
            .map_or(0, |r| r + 1);
        // A grouped table with a single row is really one group per data set
        // with replicates: import it as a column table (one value per row).
        let transpose = kind == TableKind::Grouped && used_rows <= 1 && reps > 1;
        if transpose {
            kind = TableKind::Column;
            transposed.push(name.clone());
        }
        if !raw {
            warnings.push(format!("“{name}”: the values were entered as mean and error ({data_format}); only the means were imported."));
        }
        let table_reps = if transpose || kind == TableKind::Column || !raw { 1 } else { reps };
        let mut t = DataTable::new(0, name.clone(), kind, ys.len(), 0, table_reps);
        let mut index = HashMap::new();
        for (d, (si, values)) in ys.iter().enumerate() {
            let s = &sets[*si];
            index.insert(s.uid.clone(), d);
            t.datasets[d].title = s.title.clone().unwrap_or_else(|| format!("Group {}", crate::table::dataset_letter(d)));
            for (r, row) in values.iter().enumerate().take(used_rows) {
                for (k, v) in row.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let (rr, kk) = if transpose {
                        (k, 0)
                    } else if raw {
                        (r, k)
                    } else if k == 0 {
                        (r, 0)
                    } else {
                        continue;
                    };
                    t.set(rr, GridColumn::Value { dataset: d, replicate: kk }, Some(*v));
                }
            }
            for &(k, r) in &s.excluded {
                let cell = if transpose { (d, k, 0) } else { (d, r, k) };
                if t.get(cell.1, GridColumn::Value { dataset: cell.0, replicate: cell.2 }).is_some() && !t.excluded.contains(&cell) {
                    t.excluded.push(cell);
                }
            }
        }
        if let Some(title) = x_title.filter(|_| kind.has_x()) {
            t.x_title = title;
        }
        if !transpose {
            for (r, x) in x_values.iter().enumerate().take(used_rows) {
                if kind.has_x() {
                    t.set(r, GridColumn::X, *x);
                }
            }
            if row_titles.iter().any(|s| !s.is_empty()) {
                t.show_row_titles = true;
                t.ensure_rows(row_titles.len().min(used_rows.max(1)));
                for (r, s) in row_titles.into_iter().enumerate().take(t.rows) {
                    t.row_titles[r] = s;
                }
            }
        }
        if !t.excluded.is_empty() {
            excluded.push(format!("{name} ({})", t.excluded.len()));
        }
        let id = project.add_table(t);
        tables.insert(sheet_id, (id, index));
    }
    if project.tables.is_empty() {
        return Err("the file contains no data tables".into());
    }
    if !transposed.is_empty() {
        warnings.push(format!(
            "Grouped tables with a single row were imported as column tables, one replicate per row: {}.",
            transposed.join(", ")
        ));
    }
    if !excluded.is_empty() {
        warnings.push(format!("Values excluded in Prism stay excluded (struck through in the sheet): {}.", excluded.join(", ")));
    }

    // Analyses.
    let mut skipped: Vec<String> = Vec::new();
    for an_id in list("analyses") {
        let Some(sheet) = ar.json(&format!("analyses/{an_id}/sheet.json")) else { continue };
        let title = str_of(&sheet, "title").unwrap_or("analysis").to_string();
        let input_sheet = sheet
            .get("inputSheets")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|s| str_of(s, "uid"))
            .unwrap_or("")
            .to_string();
        let Some((table_id, index)) = tables.get(&input_sheet).cloned() else {
            skipped.push(title);
            continue;
        };
        let inputs: Vec<usize> = sheet
            .get("inputDataSets")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| str_of(s, "uid")).filter_map(|u| index.get(u).copied()).collect())
            .unwrap_or_default();
        let params = ar.json(&format!("analyses/{an_id}/parameters.json"));
        let content = params.as_ref().and_then(|p| p.get("content"));
        let class = str_of(&sheet, "analysisClass").unwrap_or("");
        let design = content.and_then(|c| str_of(c, "experimentalDesign")).unwrap_or("");
        let Some(mut kind) = map_analysis(class, design, &inputs) else {
            skipped.push(title);
            continue;
        };
        // Prism ran no post tests: report the ANOVA table only.
        let post = content.and_then(|c| c.get("postTests")).and_then(|p| str_of(p, "type")).unwrap_or("");
        if let AnalysisKind::TwoWay { comparisons, .. } = &mut kind {
            if post == "NO_TESTS" {
                *comparisons = None;
            }
        }
        let mut options = Options::default();
        if let Some(o) = content.and_then(|c| c.get("options")) {
            if let Some(a) = o.get("significanceLevel").and_then(Value::as_f64) {
                options.alpha = a;
            }
            if str_of(o, "pValueTailType").is_some_and(|t| t.contains("ONE")) {
                options.tails = Tails::Two;
                warnings.push(format!(
                    "“{title}”: Prism used a one-tailed P value; HexaGraph reports two-tailed (change it in the parameters)."
                ));
            }
        }
        let selection = (kind.uses_dataset_selection() && !inputs.is_empty() && inputs.len() < index.len()).then(|| inputs.clone());
        let aid = project.add_analysis_of(table_id, kind, selection);
        if let Some(a) = project.analyses.iter_mut().find(|a: &&mut Analysis| a.id == aid) {
            a.options = options;
            a.name = Some(title.clone());
            if let Some(r) = ar.json(&format!("analyses/{an_id}/results.json")).as_ref().and_then(reported) {
                a.notes.push(format!("Prism reported: {r}."));
            }
        }
    }
    if !skipped.is_empty() {
        warnings.push(format!("Analyses not imported (not available yet): {}.", skipped.join(", ")));
    }

    // Graphs: rebuilt for each table, with the brackets of its first comparison.
    let table_ids: Vec<u64> = project.tables.iter().map(|t| t.id).collect();
    for tid in table_ids {
        let t = project.table(tid).unwrap();
        let choice = if t.kind == TableKind::Column {
            Some((GraphKind::ScatterDot, false))
        } else {
            crate::graph::recommend(t).into_iter().next().map(|r| r.0)
        };
        let Some((kind, tr)) = choice else { continue };
        let replicated = t.replicates > 1;
        let comparison = project
            .analyses
            .iter()
            .find(|a| {
                a.table_id == tid
                    && matches!(
                        a.kind,
                        AnalysisKind::TwoGroups { .. }
                            | AnalysisKind::OneWay { .. }
                            | AnalysisKind::TwoWay { .. }
                            | AnalysisKind::MultipleT { .. }
                    )
            })
            .map(|a| a.id);
        let gid = project.add_graph(tid, kind);
        if let Some(g) = project.graphs.iter_mut().find(|g| g.id == gid) {
            g.transpose = tr;
            g.auto_significance.analysis = comparison;
            g.auto_significance.show_ns = kind != GraphKind::Xy;
            if kind == GraphKind::Xy && replicated {
                // Growth curves: mean ± SEM joined by lines, as Prism draws them.
                g.show_points = false;
                g.connect_points = true;
                g.error_bar = crate::graph::ErrorBar::Sem;
            }
        }
    }
    warnings
        .push("Graphs are stored by Prism in a closed binary format: they were rebuilt with the test results (stars and ANOVA effects).".into());
    Ok(Imported { project, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn huge_ranges_are_capped() {
        assert_eq!(expand_range("0,3~5"), vec![0, 3, 4, 5]);
        assert_eq!(expand_range("0~4000000000").len(), MAX_INDEX + 1);
    }

    /// Builds a small `.prism` archive with the layout Prism 10 uses.
    fn sample() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            let mut put = |path: &str, text: &str| {
                z.start_file(path, opt).unwrap();
                z.write_all(text.as_bytes()).unwrap();
            };
            put("document.json", r#"{"sheets": {"data": ["S1"], "analyses": ["AN1"], "graphs": []}}"#);
            put(
                "data/sheets/S1/sheet.json",
                r#"{"title": "Marker", "table": {"uid": "T1", "format": "grouped", "dataFormat": "y_replicates", "replicatesCount": 3, "dataSets": ["A", "B", "C"]}}"#,
            );
            put(
                "data/sets/A.json",
                r#"{"title": {"string": "Control"}, "attributes": ["DS_ATTR_Y"], "replicate ranges": [
                    {"range": "0", "replicate": {"cellAttributes": [{"rows": "0", "attributes": ["EXCLUDED"]}]}},
                    {"range": "1~2", "replicate": {}}]}"#,
            );
            put(
                "data/sets/B.json",
                r#"{"title": {"string": "Treated"}, "attributes": ["DS_ATTR_Y"], "replicate ranges": [{"range": "0~2", "replicate": {}}]}"#,
            );
            put("data/sets/C.json", r#"{"attributes": ["DS_ATTR_Y"]}"#);
            put("data/tables/T1/data.csv", "9,2.5,3.1,5.2,6.0,5.8,,,\n,,,,,,,,\n");
            put(
                "analyses/AN1/sheet.json",
                r#"{"title": "Unpaired t test of Marker", "analysisClass": "T_TESTS_AND_NONPARAMETRIC",
                "inputDataSets": [{"uid": "A"}, {"uid": "B"}], "inputSheets": [{"uid": "S1"}]}"#,
            );
            put(
                "analyses/AN1/parameters.json",
                r#"{"content": {"experimentalDesign": "UNPAIRED_T_TEST", "options": {"significanceLevel": 0.05}}}"#,
            );
            put(
                "analyses/AN1/results.json",
                r#"{"content": {"unpairedTTestModel": {"unpairedTTest": {"t": 6.1, "df": 3.0, "pValue": {"value": 0.0088}}}}}"#,
            );
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn imports_tables_exclusions_analyses_and_graphs() {
        let imp = import(&sample()).unwrap();
        let p = &imp.project;
        assert_eq!(p.tables.len(), 1);
        let t = &p.tables[0];
        // One-row grouped table -> column table, empty placeholder set dropped.
        assert_eq!(t.kind, TableKind::Column);
        assert_eq!(t.datasets.len(), 2);
        assert_eq!(t.datasets[0].title, "Control");
        assert_eq!(t.column_values(0), vec![9.0, 2.5, 3.1]);
        assert_eq!(t.excluded, vec![(0, 0, 0)]);
        assert_eq!(t.analysis_view().column_values(0), vec![2.5, 3.1]);
        // The t test is imported and runs without the excluded value.
        assert_eq!(p.analyses.len(), 1);
        assert!(matches!(p.analyses[0].kind, AnalysisKind::TwoGroups { a: 0, b: 1, test: TwoGroupTest::Unpaired }));
        assert!(p.analyses[0].notes[0].contains("P = 0.0088"));
        let out = p.run_analysis(p.analyses[0].id).unwrap();
        let ours = out.significance[0].p;
        let expected = hg_stats::ttest::unpaired(&[2.5, 3.1], &[5.2, 6.0, 5.8], false).unwrap().p;
        assert!((ours - expected).abs() < 1e-12);
        // A graph with live brackets.
        assert_eq!(p.graphs.len(), 1);
        assert_eq!(p.graphs[0].auto_significance.analysis, Some(p.analyses[0].id));
    }

    #[test]
    fn xy_table_reads_the_separate_x_column() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            let mut put = |path: &str, text: &str| {
                z.start_file(path, opt).unwrap();
                z.write_all(text.as_bytes()).unwrap();
            };
            put("document.json", r#"{"sheets": {"data": ["S1"], "analyses": [], "graphs": []}}"#);
            put(
                "data/sheets/S1/sheet.json",
                r#"{"title": "Growth", "table": {"uid": "T1", "format": "xy", "dataFormat": "y_replicates", "replicatesCount": 2,
                "dataSets": ["A", "B"], "xDataSetFormat": "y_single", "xDataSet": "X"}}"#,
            );
            put("data/sets/X.json", r#"{"attributes": ["DS_ATTR_X"]}"#);
            put("data/sets/A.json", r#"{"title": {"string": "Control"}, "attributes": ["DS_ATTR_Y"]}"#);
            put("data/sets/B.json", r#"{"title": {"string": "Treated"}, "attributes": ["DS_ATTR_Y"]}"#);
            put("data/tables/T1/data.csv", "0,1,2,3,4\n5,10,12,30,34\n");
            z.finish().unwrap();
        }
        let imp = import(&buf.into_inner()).unwrap();
        let t = &imp.project.tables[0];
        assert_eq!(t.kind, TableKind::XY);
        assert_eq!(t.x[..2], [Some(0.0), Some(5.0)]);
        assert_eq!(t.cell_values(0, 1), vec![10.0, 12.0]);
        assert_eq!(t.cell_values(1, 1), vec![30.0, 34.0]);
        // Replicates are summarized as mean ± SEM joined by lines.
        let g = &imp.project.graphs[0];
        assert!(g.connect_points && !g.show_points);
    }

    #[test]
    fn ranges() {
        assert_eq!(expand_range("0"), vec![0]);
        assert_eq!(expand_range("1~3,7"), vec![1, 2, 3, 7]);
    }
}
