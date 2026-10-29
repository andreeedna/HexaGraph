//! A project file holds tables, analyses and graphs (saved as JSON, *.hxn).

use crate::analysis::{self, Analysis, AnalysisKind, AnalysisOutput};
use crate::graph::{Bracket, BracketKey, Graph, GraphKind};
use crate::table::{DataTable, TableKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub title: String,
    pub tables: Vec<DataTable>,
    pub analyses: Vec<Analysis>,
    pub graphs: Vec<Graph>,
    pub next_id: u64,
    #[serde(default)]
    pub notes: String,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            format_version: FORMAT_VERSION,
            title: "Untitled".into(),
            tables: Vec::new(),
            analyses: Vec::new(),
            graphs: Vec::new(),
            next_id: 1,
            notes: String::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read or write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the file is not a valid HexaGraph project: {0}")]
    Format(#[from] serde_json::Error),
    #[error("the file was written by a newer version of HexaGraph (format {0})")]
    TooNew(u32),
}

impl Project {
    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Adds a data table. Returns its id.
    pub fn add_table(&mut self, mut table: DataTable) -> u64 {
        let id = self.new_id();
        table.id = id;
        self.tables.push(table);
        id
    }

    /// Adds a graph of a table, linked to the curve fits already done on it.
    pub fn add_graph(&mut self, table_id: u64, kind: GraphKind) -> u64 {
        let id = self.new_id();
        let Some(t) = self.table(table_id) else { return id };
        let mut g = Graph::new(id, table_id, kind, t.name.clone());
        g.title = t.name.clone();
        if t.kind == TableKind::Survival {
            g.y_label = "Percent survival".into();
        }
        g.overlay_analyses = self
            .analyses
            .iter()
            .filter(|a| a.table_id == table_id && matches!(a.kind, AnalysisKind::NonlinearFit { .. } | AnalysisKind::LinearRegression))
            .map(|a| a.id)
            .collect();
        self.graphs.push(g);
        id
    }

    pub fn add_analysis(&mut self, table_id: u64, kind: AnalysisKind) -> u64 {
        self.add_analysis_of(table_id, kind, None)
    }

    /// Adds an analysis limited to some data sets (`None` = all).
    pub fn add_analysis_of(&mut self, table_id: u64, kind: AnalysisKind, datasets: Option<Vec<usize>>) -> u64 {
        let id = self.new_id();
        let is_curve = matches!(kind, AnalysisKind::NonlinearFit { .. } | AnalysisKind::LinearRegression);
        self.analyses.push(Analysis {
            id,
            table_id,
            kind,
            datasets,
            options: Default::default(),
            excluded: Vec::new(),
            notes: Vec::new(),
            name: None,
        });
        if is_curve {
            for g in self.graphs.iter_mut().filter(|g| g.table_id == table_id && g.kind == GraphKind::Xy) {
                g.overlay_analyses.push(id);
            }
        }
        id
    }

    pub fn table(&self, id: u64) -> Option<&DataTable> {
        self.tables.iter().find(|t| t.id == id)
    }

    pub fn table_mut(&mut self, id: u64) -> Option<&mut DataTable> {
        self.tables.iter_mut().find(|t| t.id == id)
    }

    /// Name of an analysis as shown to the user.
    pub fn analysis_name(&self, id: u64) -> String {
        self.analysis(id).map_or_else(String::new, |a| a.display_name(self.table(a.table_id).map_or("?", |t| t.name.as_str())))
    }

    pub fn analysis(&self, id: u64) -> Option<&Analysis> {
        self.analyses.iter().find(|a| a.id == id)
    }

    pub fn graph(&self, id: u64) -> Option<&Graph> {
        self.graphs.iter().find(|g| g.id == id)
    }

    /// Results always reflect the current data (analyses are live).
    pub fn run_analysis(&self, id: u64) -> Result<AnalysisOutput, String> {
        let a = self.analysis(id).ok_or("analysis not found")?;
        let table = self.table(a.table_id).ok_or("the data table was deleted")?;
        let view = table.analysis_view();
        let t: &DataTable = &view;
        let mut notes: Vec<String> = Vec::new();
        if !table.excluded.is_empty() {
            notes.push(format!(
                "{} value{} excluded in the data table (struck through) {} not analyzed.",
                table.excluded.len(),
                if table.excluded.len() == 1 { "" } else { "s" },
                if table.excluded.len() == 1 { "is" } else { "are" }
            ));
        }
        notes.extend(a.notes.iter().cloned());
        if a.excluded.is_empty() {
            let mut out = analysis::run(&a.kind, t, a.datasets.as_deref(), &a.options)?;
            out.warnings.splice(0..0, notes);
            return Ok(out);
        }
        // Analyze a copy of the table without the excluded values.
        let mut copy = t.clone();
        let mut gone = Vec::new();
        for &(d, r, k) in &a.excluded {
            if let Some(v) = copy.datasets.get_mut(d).and_then(|ds| ds.values.get_mut(r)).and_then(|row| row.get_mut(k)) {
                if let Some(x) = v.take() {
                    gone.push(format!("{} ({})", crate::format::num(x), t.datasets[d].title));
                }
            }
        }
        let mut out = analysis::run(&a.kind, &copy, a.datasets.as_deref(), &a.options)?;
        out.warnings.splice(0..0, notes);
        if !gone.is_empty() {
            out.warnings.insert(
                0,
                format!("{} value{} excluded from this analysis: {}.", gone.len(), if gone.len() == 1 { "" } else { "s" }, gone.join(", ")),
            );
        }
        Ok(out)
    }

    pub fn remove_table(&mut self, id: u64) {
        self.tables.retain(|t| t.id != id);
        self.analyses.retain(|a| a.table_id != id);
        self.graphs.retain(|g| g.table_id != id);
    }

    pub fn remove_analysis(&mut self, id: u64) {
        self.analyses.retain(|a| a.id != id);
        for g in &mut self.graphs {
            g.overlay_analyses.retain(|&a| a != id);
        }
    }

    /// Removes data set `d` of a table and keeps every reference to the
    /// table's data sets (exclusions, analyses, graph brackets and styles)
    /// pointing at the same data. Analyses that compared the removed data set
    /// are removed too.
    pub fn remove_dataset(&mut self, table_id: u64, d: usize) {
        let Some(t) = self.table_mut(table_id) else { return };
        if t.datasets.len() <= 1 || d >= t.datasets.len() {
            return;
        }
        t.remove_dataset(d);
        let ds = |i: usize| match i.cmp(&d) {
            Ordering::Less => Some(i),
            Ordering::Equal => None,
            Ordering::Greater => Some(i - 1),
        };
        let cells = |v: &[(usize, usize, usize)]| v.iter().filter_map(|&(i, r, k)| Some((ds(i)?, r, k))).collect::<Vec<_>>();
        t.excluded = cells(&t.excluded);
        let mut orphans = Vec::new();
        for a in self.analyses.iter_mut().filter(|a| a.table_id == table_id) {
            a.excluded = cells(&a.excluded);
            if let Some(sets) = &mut a.datasets {
                *sets = sets.iter().filter_map(|&i| ds(i)).collect();
                if sets.is_empty() {
                    orphans.push(a.id);
                }
            }
            let pair = match &mut a.kind {
                AnalysisKind::TwoGroups { a, b, .. } | AnalysisKind::MultipleT { a, b, .. } | AnalysisKind::BlandAltman { a, b } => {
                    Some((a, b))
                }
                AnalysisKind::Roc { controls, patients } => Some((controls, patients)),
                AnalysisKind::OneWay { control, .. } => {
                    if control.is_some_and(|c| c == d) {
                        a.notes.push("The control data set was deleted: all pairs are compared.".into());
                    }
                    *control = control.and_then(ds);
                    None
                }
                _ => None,
            };
            if let Some((x, y)) = pair {
                match (ds(*x), ds(*y)) {
                    (Some(nx), Some(ny)) => (*x, *y) = (nx, ny),
                    _ => orphans.push(a.id),
                }
            }
        }
        for id in orphans {
            self.remove_analysis(id);
        }
        for g in self.graphs.iter_mut().filter(|g| g.table_id == table_id) {
            remap_brackets(g, false, &ds);
            if d < g.colors.len() {
                g.colors.remove(d);
            }
            if d < g.series_styles.len() {
                g.series_styles.remove(d);
            }
        }
    }

    /// Inserts an empty row in a table; exclusions of its analyses and
    /// brackets of its grouped graphs follow the data.
    pub fn insert_row(&mut self, table_id: u64, row: usize) {
        let Some(t) = self.table_mut(table_id) else { return };
        t.insert_row(row);
        self.remap_rows(table_id, &|r| Some(if r >= row { r + 1 } else { r }));
    }

    /// Deletes rows `first..=last` of a table; exclusions of its analyses and
    /// brackets of its grouped graphs follow the data.
    pub fn delete_rows(&mut self, table_id: u64, first: usize, last: usize) {
        let Some(t) = self.table_mut(table_id) else { return };
        let before = t.rows;
        t.delete_rows(first, last);
        let n = before - t.rows;
        if n == 0 {
            return;
        }
        let last = first + n - 1;
        self.remap_rows(table_id, &|r| match r {
            r if r < first => Some(r),
            r if r > last => Some(r - n),
            _ => None,
        });
    }

    /// Moves row references of a table's analyses and graphs (None: the row is gone).
    fn remap_rows(&mut self, table_id: u64, row: &dyn Fn(usize) -> Option<usize>) {
        for a in self.analyses.iter_mut().filter(|a| a.table_id == table_id) {
            a.excluded = a.excluded.iter().filter_map(|&(d, r, k)| Some((d, row(r)?, k))).collect();
        }
        for g in self.graphs.iter_mut().filter(|g| g.table_id == table_id) {
            remap_brackets(g, true, row);
        }
    }

    pub fn remove_graph(&mut self, id: u64) {
        self.graphs.retain(|g| g.id != id);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("projects always serialize")
    }

    pub fn from_json(s: &str) -> Result<Self, ProjectError> {
        let mut p: Project = serde_json::from_str(s)?;
        if p.format_version > FORMAT_VERSION {
            return Err(ProjectError::TooNew(p.format_version));
        }
        p.tables.iter_mut().for_each(DataTable::normalize);
        Ok(p)
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let tmp = path.with_extension("hxn.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }
}

/// Moves the bracket keys of a graph (brackets, hidden comparisons, offsets)
/// after a table edit. `rows` says whether `f` maps table rows or data sets.
/// On grouped graphs the bracket group is a row, or a data set when
/// transposed, and `a`, `b` are the other kind; on column graphs `a`, `b` are
/// data sets.
fn remap_brackets(g: &mut Graph, rows: bool, f: &dyn Fn(usize) -> Option<usize>) {
    let group_moves = g.transpose != rows;
    let key = |(group, a, b): BracketKey| -> Option<BracketKey> {
        match group {
            Some(r) if group_moves => Some((Some(f(r)?), a, b)),
            None if rows => Some((None, a, b)),
            _ => Some((group, f(a)?, f(b)?)),
        }
    };
    g.brackets = g.brackets.drain(..).filter_map(|br| key((br.row, br.a, br.b)).map(|(row, a, b)| Bracket { row, a, b, ..br })).collect();
    g.auto_significance.hidden = g.auto_significance.hidden.iter().filter_map(|&k| key(k)).collect();
    g.bracket_offsets = g.bracket_offsets.iter().filter_map(|&(k, o)| Some((key(k)?, o))).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_all_sample_analyses_run() {
        let mut p = Project::default();
        for kind in TableKind::ALL {
            let id = p.add_table(crate::sample::sample_table(kind));
            for a in AnalysisKind::available(kind) {
                p.add_analysis(id, a);
            }
        }
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
        for a in &p.analyses {
            let out = p.run_analysis(a.id).unwrap_or_else(|e| panic!("{:?} failed: {e}", a.kind));
            assert!(!out.tables.is_empty() || !out.warnings.is_empty(), "{:?} produced nothing", a.kind);
        }
    }

    #[test]
    fn damaged_files_open_without_losing_values() {
        let mut p = Project::default();
        let id = p.add_table(crate::sample::sample_table(TableKind::XY));
        let mut v: serde_json::Value = serde_json::from_str(&p.to_json()).unwrap();
        let rows = v["tables"][0]["rows"].as_u64().unwrap();
        v["tables"][0]["rows"] = (rows + 5).into();
        v["tables"][0]["x"].as_array_mut().unwrap().pop();
        v["tables"][0]["datasets"][0]["values"].as_array_mut().unwrap().pop();
        let back = Project::from_json(&v.to_string()).unwrap();
        let t = back.table(id).unwrap();
        assert_eq!(t.rows as u64, rows + 5);
        assert!(t.x.len() == t.rows && t.datasets.iter().all(|d| d.values.len() == t.rows));
        assert_eq!(
            t.datasets[1].values,
            p.table(id).unwrap().datasets[1].values[..]
                .iter()
                .cloned()
                .chain(std::iter::repeat_n(vec![None; t.replicates], 5))
                .collect::<Vec<_>>()
        );
        let _ = t.block_text((0, t.rows - 1), (0, t.grid_columns().len() - 1));
    }

    #[test]
    fn grouped_brackets_follow_row_edits() {
        let mut p = Project::default();
        let id = p.add_table(crate::sample::sample_table(TableKind::Grouped));
        let g = p.add_graph(id, GraphKind::GroupedBar);
        let bracket = |row| Bracket { row: Some(row), a: 0, b: 1, label: "*".into() };
        p.graphs[0].brackets = vec![bracket(2)];
        p.insert_row(id, 0);
        assert_eq!(p.graph(g).unwrap().brackets, vec![bracket(3)]);
        p.delete_rows(id, 3, 3);
        assert!(p.graph(g).unwrap().brackets.is_empty());
    }

    #[test]
    fn deleting_a_data_set_or_row_keeps_references_on_the_same_data() {
        use crate::analysis::TwoGroupTest;
        use crate::table::GridColumn;
        let mut t = DataTable::new(0, "t", TableKind::Column, 4, 3, 1);
        for d in 0..4 {
            for r in 0..3 {
                t.set(r, GridColumn::Value { dataset: d, replicate: 0 }, Some((10 * d + r) as f64));
            }
        }
        t.excluded.push((2, 0, 0));
        let mut p = Project::default();
        let id = p.add_table(t);
        let ac = p.add_analysis(id, AnalysisKind::TwoGroups { a: 0, b: 2, test: TwoGroupTest::Unpaired });
        let ab = p.add_analysis(id, AnalysisKind::TwoGroups { a: 0, b: 1, test: TwoGroupTest::Unpaired });
        p.analyses[0].excluded.push((2, 1, 0));
        p.remove_dataset(id, 1);
        let t = p.table(id).unwrap();
        assert_eq!(t.datasets[1].title, "Group C");
        assert_eq!(t.excluded, vec![(1, 0, 0)]);
        assert!(matches!(p.analysis(ac).unwrap().kind, AnalysisKind::TwoGroups { a: 0, b: 1, .. }));
        assert_eq!(p.analysis(ac).unwrap().excluded, vec![(1, 1, 0)]);
        assert!(p.analysis(ab).is_none(), "an analysis of the deleted data set goes with it");
        p.insert_row(id, 0);
        assert_eq!(p.analysis(ac).unwrap().excluded, vec![(1, 2, 0)]);
        p.delete_rows(id, 0, 1);
        assert_eq!(p.analysis(ac).unwrap().excluded, vec![(1, 0, 0)]);
    }
}
