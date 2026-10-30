//! Data tables.

use hg_stats::survival::Subjects;
use serde::{Deserialize, Serialize};

pub type Cell = Option<f64>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TableKind {
    /// X values with one or more Y data sets (optionally with replicates).
    XY,
    /// One group per column.
    Column,
    /// Two grouping variables: rows and columns, replicates in sub-columns.
    Grouped,
    /// Counts: outcomes in columns, groups in rows.
    Contingency,
    /// Time in X, event code (1 = event, 0 = censored) per group.
    Survival,
    /// Fractions of a whole.
    PartsOfWhole,
}

impl TableKind {
    pub const ALL: [TableKind; 6] =
        [TableKind::XY, TableKind::Column, TableKind::Grouped, TableKind::Contingency, TableKind::Survival, TableKind::PartsOfWhole];

    pub fn label(self) -> &'static str {
        match self {
            TableKind::XY => "XY",
            TableKind::Column => "Column",
            TableKind::Grouped => "Grouped",
            TableKind::Contingency => "Contingency",
            TableKind::Survival => "Survival",
            TableKind::PartsOfWhole => "Parts of whole",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            TableKind::XY => "Each point has an X and a Y value: dose-response, kinetics, standard curves, correlation.",
            TableKind::Column => "Each column is a group: compare groups with t tests, ANOVA and nonparametric tests.",
            TableKind::Grouped => "Two grouping variables (rows × columns): two-way ANOVA, grouped bar graphs.",
            TableKind::Contingency => "Counts of subjects per outcome: chi-square, Fisher's exact test, relative risk.",
            TableKind::Survival => "Elapsed time and outcome (1 = event, 0 = censored): Kaplan-Meier and log-rank.",
            TableKind::PartsOfWhole => "Values that are fractions of a total: pie charts and percentages.",
        }
    }

    pub fn has_x(self) -> bool {
        matches!(self, TableKind::XY | TableKind::Survival)
    }

    pub fn allows_replicates(self) -> bool {
        matches!(self, TableKind::XY | TableKind::Grouped)
    }

    pub fn x_header(self) -> &'static str {
        match self {
            TableKind::Survival => "Time",
            _ => "X",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSet {
    pub title: String,
    /// values[row][replicate]
    pub values: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridColumn {
    RowTitle,
    X,
    Value { dataset: usize, replicate: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTable {
    pub id: u64,
    pub name: String,
    pub kind: TableKind,
    pub x_title: String,
    pub rows: usize,
    pub replicates: usize,
    pub row_titles: Vec<String>,
    pub x: Vec<Cell>,
    pub datasets: Vec<DataSet>,
    #[serde(default)]
    pub notes: String,
    /// Whether the first column holds row titles.
    #[serde(default = "default_true")]
    pub show_row_titles: bool,
    /// Values excluded from every analysis and graph: (data set, row, replicate).
    /// They stay in the sheet (shown struck through) and can be included again.
    #[serde(default)]
    pub excluded: Vec<(usize, usize, usize)>,
}

pub fn dataset_letter(i: usize) -> String {
    let mut s = String::new();
    let mut n = i + 1;
    while n > 0 {
        n -= 1;
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

impl DataTable {
    pub fn new(id: u64, name: impl Into<String>, kind: TableKind, datasets: usize, rows: usize, replicates: usize) -> Self {
        let replicates = if kind.allows_replicates() { replicates.max(1) } else { 1 };
        let mut t = DataTable {
            id,
            name: name.into(),
            kind,
            x_title: kind.x_header().to_string(),
            rows: 0,
            replicates,
            row_titles: Vec::new(),
            x: Vec::new(),
            datasets: Vec::new(),
            notes: String::new(),
            show_row_titles: matches!(kind, TableKind::Grouped | TableKind::Contingency | TableKind::PartsOfWhole),
            excluded: Vec::new(),
        };
        for i in 0..datasets.max(1) {
            t.add_dataset(format!("Group {}", dataset_letter(i)));
        }
        t.ensure_rows(rows);
        t
    }

    pub fn add_dataset(&mut self, title: impl Into<String>) {
        self.datasets.push(DataSet { title: title.into(), values: vec![vec![None; self.replicates]; self.rows] });
    }

    pub fn remove_dataset(&mut self, i: usize) {
        if self.datasets.len() > 1 && i < self.datasets.len() {
            self.datasets.remove(i);
        }
    }

    /// Changes the kind of table keeping every value. A kind without
    /// replicates gets one row per value (row by row, replicate by
    /// replicate); X values and row titles stay, hidden if the kind has none.
    /// Returns the old number of replicates when the rows were spread out.
    pub fn set_kind(&mut self, kind: TableKind) -> Option<usize> {
        let reps = self.replicates;
        let spread = !kind.allows_replicates() && reps > 1;
        if spread {
            for d in &mut self.datasets {
                d.values = d.values.iter().flatten().map(|v| vec![*v]).collect();
            }
            self.x = self.x.iter().flat_map(|&x| std::iter::once(x).chain(std::iter::repeat_n(None, reps - 1))).collect();
            self.row_titles = self
                .row_titles
                .iter()
                .flat_map(|t| std::iter::once(t.clone()).chain(std::iter::repeat_n(String::new(), reps - 1)))
                .collect();
            self.rows *= reps;
            self.replicates = 1;
            self.excluded = self.excluded.iter().map(|&(d, r, k)| (d, r * reps + k, 0)).collect();
        }
        if self.x_title.is_empty() || self.x_title == self.kind.x_header() {
            self.x_title = kind.x_header().to_string();
        }
        self.kind = kind;
        spread.then_some(reps)
    }

    /// Makes the sizes of a table read from a file consistent, so a damaged or
    /// hand-edited file cannot crash the sheet. Never drops a value: the
    /// table grows to the longest part and the rest is padded with empty cells.
    pub fn normalize(&mut self) {
        let longest_row = self.datasets.iter().flat_map(|d| d.values.iter().map(Vec::len)).max().unwrap_or(0);
        self.replicates = self.replicates.max(longest_row).max(1);
        self.rows = self.datasets.iter().map(|d| d.values.len()).chain([self.rows, self.x.len(), self.row_titles.len()]).max().unwrap_or(0);
        if self.datasets.is_empty() {
            self.add_dataset("Group A");
        }
        let (rows, reps) = (self.rows, self.replicates);
        self.row_titles.resize(rows, String::new());
        self.x.resize(rows, None);
        for d in &mut self.datasets {
            d.values.resize(rows, Vec::new());
            for r in &mut d.values {
                r.resize(reps, None);
            }
        }
    }

    pub fn ensure_rows(&mut self, rows: usize) {
        while self.rows < rows {
            self.rows += 1;
            self.row_titles.push(String::new());
            self.x.push(None);
            for d in &mut self.datasets {
                d.values.push(vec![None; self.replicates]);
            }
        }
    }

    pub fn set_replicates(&mut self, n: usize) {
        if !self.kind.allows_replicates() {
            return;
        }
        let n = n.clamp(1, 100);
        self.replicates = n;
        for d in &mut self.datasets {
            for row in &mut d.values {
                row.resize(n, None);
            }
        }
    }

    pub fn grid_columns(&self) -> Vec<GridColumn> {
        let mut cols = if self.show_row_titles { vec![GridColumn::RowTitle] } else { Vec::new() };
        if self.kind.has_x() {
            cols.push(GridColumn::X);
        }
        for d in 0..self.datasets.len() {
            for r in 0..self.replicates {
                cols.push(GridColumn::Value { dataset: d, replicate: r });
            }
        }
        cols
    }

    pub fn get(&self, row: usize, col: GridColumn) -> Cell {
        match col {
            GridColumn::RowTitle => None,
            GridColumn::X => self.x.get(row).copied().flatten(),
            GridColumn::Value { dataset, replicate } => {
                self.datasets.get(dataset).and_then(|d| d.values.get(row)).and_then(|r| r.get(replicate)).copied().flatten()
            }
        }
    }

    pub fn set(&mut self, row: usize, col: GridColumn, v: Cell) {
        self.ensure_rows(row + 1);
        match col {
            GridColumn::RowTitle => {}
            GridColumn::X => self.x[row] = v,
            GridColumn::Value { dataset, replicate } => {
                // Typing to the right of the last data set creates new ones.
                if v.is_some() {
                    while self.datasets.len() <= dataset {
                        let n = self.datasets.len();
                        self.add_dataset(format!("Group {}", dataset_letter(n)));
                    }
                }
                if let Some(d) = self.datasets.get_mut(dataset) {
                    d.values[row][replicate] = v;
                }
            }
        }
    }

    /// Number of fixed columns (row titles, X) before the data sets.
    pub fn leading_columns(&self) -> usize {
        usize::from(self.show_row_titles) + usize::from(self.kind.has_x())
    }

    /// The column at a grid index, for an unbounded sheet: indices past the
    /// last data set address data sets that do not exist yet.
    pub fn column_at(&self, i: usize) -> GridColumn {
        let mut i = i;
        if self.show_row_titles {
            if i == 0 {
                return GridColumn::RowTitle;
            }
            i -= 1;
        }
        if self.kind.has_x() {
            if i == 0 {
                return GridColumn::X;
            }
            i -= 1;
        }
        let reps = self.replicates.max(1);
        GridColumn::Value { dataset: i / reps, replicate: i % reps }
    }

    /// Text of a cell as typed (row titles included).
    pub fn cell_text(&self, row: usize, col: GridColumn) -> String {
        match col {
            GridColumn::RowTitle => self.row_titles.get(row).cloned().unwrap_or_default(),
            _ => self.get(row, col).map(|v| format!("{v}")).unwrap_or_default(),
        }
    }

    /// Sets a cell from text (numbers are parsed, row titles kept as text).
    pub fn set_text(&mut self, row: usize, col: GridColumn, text: &str) {
        match col {
            GridColumn::RowTitle => {
                if !text.trim().is_empty() || row < self.rows {
                    self.ensure_rows(row + 1);
                    self.row_titles[row] = text.trim().to_string();
                }
            }
            _ => self.set(row, col, parse_number(text)),
        }
    }

    /// Tab separated text of a block of cells (inclusive grid indices).
    pub fn block_text(&self, rows: (usize, usize), cols: (usize, usize)) -> String {
        (rows.0..=rows.1)
            .map(|r| (cols.0..=cols.1).map(|c| self.cell_text(r, self.column_at(c))).collect::<Vec<_>>().join("\t"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Empties a block of cells.
    pub fn clear_block(&mut self, rows: (usize, usize), cols: (usize, usize)) {
        for r in rows.0..=rows.1.min(self.rows.saturating_sub(1)) {
            for c in cols.0..=cols.1 {
                let col = self.column_at(c);
                self.set_text(r, col, "");
            }
        }
    }

    /// Pastes tab/comma/semicolon separated text with its top-left at (row,
    /// grid column index). Data sets are created as needed. Returns the number
    /// of values that were not numbers and were left empty.
    pub fn paste(&mut self, row: usize, col_index: usize, text: &str) -> usize {
        self.paste_with(row, col_index, text, separator(text))
    }

    /// Like `paste`, with the field separator already chosen (`None`: one field per line).
    pub fn paste_with(&mut self, row: usize, col_index: usize, text: &str, sep: Option<char>) -> usize {
        let mut unreadable = 0;
        for (dr, line) in text.lines().enumerate() {
            for (dc, field) in fields(line, sep).into_iter().enumerate() {
                let col = self.column_at(col_index + dc);
                if col != GridColumn::RowTitle && !field.trim().is_empty() && parse_number(field).is_none() {
                    unreadable += 1;
                }
                self.set_text(row + dr, col, field);
            }
        }
        unreadable
    }

    /// Pastes with rows and columns swapped.
    pub fn paste_transposed(&mut self, row: usize, col_index: usize, text: &str) -> usize {
        self.paste_with(row, col_index, &transpose_text(text), Some('\t'))
    }

    /// Inserts an empty row before `row`.
    pub fn insert_row(&mut self, row: usize) {
        self.ensure_rows(row);
        self.rows += 1;
        self.row_titles.insert(row, String::new());
        self.x.insert(row, None);
        for d in &mut self.datasets {
            d.values.insert(row, vec![None; self.replicates]);
        }
        for e in &mut self.excluded {
            if e.1 >= row {
                e.1 += 1;
            }
        }
    }

    /// Deletes rows `first..=last`.
    pub fn delete_rows(&mut self, first: usize, last: usize) {
        let last = last.min(self.rows.saturating_sub(1));
        if first > last || first >= self.rows {
            return;
        }
        self.row_titles.drain(first..=last);
        self.x.drain(first..=last);
        for d in &mut self.datasets {
            d.values.drain(first..=last);
        }
        self.rows -= last - first + 1;
        let n = last - first + 1;
        self.excluded.retain(|e| e.1 < first || e.1 > last);
        for e in &mut self.excluded {
            if e.1 > last {
                e.1 -= n;
            }
        }
    }

    pub fn is_excluded(&self, row: usize, col: GridColumn) -> bool {
        matches!(col, GridColumn::Value { dataset, replicate } if self.excluded.contains(&(dataset, row, replicate)))
    }

    /// Excludes the values of a block, or includes them again when they all are excluded.
    pub fn toggle_excluded(&mut self, rows: (usize, usize), cols: (usize, usize)) {
        let cells: Vec<(usize, usize, usize)> = (rows.0..=rows.1)
            .flat_map(|r| (cols.0..=cols.1).map(move |c| (r, c)))
            .filter_map(|(r, c)| match self.column_at(c) {
                GridColumn::Value { dataset, replicate } if self.get(r, GridColumn::Value { dataset, replicate }).is_some() => {
                    Some((dataset, r, replicate))
                }
                _ => None,
            })
            .collect();
        if cells.iter().all(|c| self.excluded.contains(c)) {
            self.excluded.retain(|e| !cells.contains(e));
        } else {
            for c in cells {
                if !self.excluded.contains(&c) {
                    self.excluded.push(c);
                }
            }
        }
    }

    /// The table as analyses and graphs see it: excluded values removed.
    pub fn analysis_view(&self) -> std::borrow::Cow<'_, DataTable> {
        if self.excluded.is_empty() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut t = self.clone();
        for &(d, r, k) in &self.excluded {
            if let Some(v) = t.datasets.get_mut(d).and_then(|ds| ds.values.get_mut(r)).and_then(|row| row.get_mut(k)) {
                *v = None;
            }
        }
        std::borrow::Cow::Owned(t)
    }

    /// Number of rows holding at least one value (trailing empty rows ignored).
    pub fn used_rows(&self) -> usize {
        (0..self.rows)
            .rev()
            .find(|&r| {
                self.x.get(r).copied().flatten().is_some()
                    || self.datasets.iter().any(|d| d.values[r].iter().any(Option::is_some))
                    || !self.row_titles[r].is_empty()
            })
            .map_or(0, |r| r + 1)
    }

    /// All values of a data set (every row and replicate).
    pub fn column_values(&self, ds: usize) -> Vec<f64> {
        self.datasets[ds].values.iter().flatten().filter_map(|v| *v).collect()
    }

    /// Replicates of one row of a data set.
    pub fn cell_values(&self, ds: usize, row: usize) -> Vec<f64> {
        self.datasets[ds].values[row].iter().filter_map(|v| *v).collect()
    }

    /// Paired columns: rows where every listed data set has a value.
    pub fn complete_rows(&self, ds: &[usize]) -> Vec<Vec<f64>> {
        (0..self.rows)
            .filter_map(|r| ds.iter().map(|&d| self.datasets[d].values[r].first().copied().flatten()).collect::<Option<Vec<f64>>>())
            .collect()
    }

    /// (x, y) pairs for a data set, one per replicate.
    pub fn xy_points(&self, ds: usize) -> Vec<(f64, f64)> {
        (0..self.rows)
            .filter_map(|r| self.x[r].map(|x| (r, x)))
            .flat_map(|(r, x)| self.cell_values(ds, r).into_iter().map(move |y| (x, y)))
            .collect()
    }

    pub fn row_label(&self, row: usize) -> String {
        let t = self.row_titles.get(row).map(String::as_str).unwrap_or("");
        if t.is_empty() {
            format!("Row {}", row + 1)
        } else {
            t.to_string()
        }
    }

    pub fn survival_subjects(&self, ds: usize) -> Subjects {
        let mut s = Subjects::default();
        for r in 0..self.rows {
            if let (Some(t), Some(code)) = (self.x[r], self.datasets[ds].values[r][0]) {
                s.times.push(t);
                s.events.push(code != 0.0);
            }
        }
        s
    }

    /// Contingency counts: rows × data sets.
    pub fn counts(&self) -> Vec<Vec<f64>> {
        (0..self.used_rows()).map(|r| self.datasets.iter().map(|d| d.values[r][0].unwrap_or(0.0)).collect()).collect()
    }

    /// Data sets with at least one value.
    pub fn nonempty_datasets(&self) -> Vec<usize> {
        (0..self.datasets.len()).filter(|&d| !self.column_values(d).is_empty()).collect()
    }
}

/// The field separator of a block of text, chosen once for all its lines:
/// tab, then semicolon, then comma. `None` when every line is one number, such
/// as a single column copied from a spreadsheet with decimal commas ("1,5").
pub fn separator(text: &str) -> Option<char> {
    if text.contains('\t') {
        Some('\t')
    } else if text.contains(';') {
        Some(';')
    } else if text.lines().all(|l| l.trim().is_empty() || parse_number(l).is_some()) {
        None
    } else {
        Some(',')
    }
}

pub fn fields(line: &str, sep: Option<char>) -> Vec<&str> {
    match sep {
        Some(c) => line.split(c).collect(),
        None => vec![line],
    }
}

/// Swaps rows and columns of delimited text.
pub fn transpose_text(text: &str) -> String {
    let sep = separator(text);
    let rows: Vec<Vec<&str>> = text.lines().map(|l| fields(l, sep)).collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    (0..width).map(|c| rows.iter().map(|r| r.get(c).copied().unwrap_or("")).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n")
}

fn default_true() -> bool {
    true
}

pub fn parse_number(s: &str) -> Cell {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok().or_else(|| decimal_point(s).parse().ok()).filter(|v: &f64| v.is_finite())
}

/// "1,5" → "1.5". With both separators the last one is the decimal mark and
/// the other groups thousands: "1.234,5" and "1,234.5" → "1234.5".
fn decimal_point(s: &str) -> String {
    match (s.rfind('.'), s.rfind(',')) {
        (Some(dot), Some(comma)) if comma > dot => s.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => s.replace(',', ""),
        _ => s.replace(',', "."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_and_extract() {
        let mut t = DataTable::new(1, "t", TableKind::Column, 2, 0, 1);
        t.paste(0, 0, "1\t2\n3\t4\n5\t");
        let mut wide = DataTable::new(2, "w", TableKind::Column, 1, 0, 1);
        wide.paste(0, 0, "1,2,3");
        assert_eq!(wide.datasets.len(), 3);
        assert_eq!(t.column_values(0), vec![1.0, 3.0, 5.0]);
        assert_eq!(t.column_values(1), vec![2.0, 4.0]);
        assert_eq!(t.complete_rows(&[0, 1]).len(), 2);
        assert_eq!(dataset_letter(26), "AA");
        assert_eq!(parse_number("1,5"), Some(1.5));
        let mut g = DataTable::new(6, "g", TableKind::Grouped, 1, 2, 2);
        for (r, k, v) in [(0, 0, 1.0), (0, 1, 2.0), (1, 0, 3.0), (1, 1, 4.0)] {
            g.set(r, GridColumn::Value { dataset: 0, replicate: k }, Some(v));
        }
        g.excluded.push((0, 1, 1));
        assert_eq!(g.set_kind(TableKind::Column), Some(2));
        assert_eq!(g.datasets[0].values, vec![vec![Some(1.0)], vec![Some(2.0)], vec![Some(3.0)], vec![Some(4.0)]]);
        assert_eq!((g.rows, g.x.len(), g.row_titles.len(), g.excluded.clone()), (4, 4, 4, vec![(0, 3, 0)]));
        assert_eq!(parse_number("1.234,5"), Some(1234.5));
        assert_eq!(parse_number("1,234.5"), Some(1234.5));
        // One spreadsheet column with decimal commas stays one column.
        let mut it = DataTable::new(4, "it", TableKind::Column, 1, 0, 1);
        assert_eq!(it.paste(0, 0, "1,5\r\n2,25\r\n3,75\r\n"), 0);
        assert_eq!(it.datasets.len(), 1);
        assert_eq!(it.column_values(0), vec![1.5, 2.25, 3.75]);
        let mut mixed = DataTable::new(5, "m", TableKind::Column, 2, 0, 1);
        assert_eq!(mixed.paste(0, 0, "1,5\t2,5\n3,5\nn.d.\t4"), 1);
        assert_eq!(mixed.column_values(0), vec![1.5, 3.5]);
        let mut tr = DataTable::new(3, "tr", TableKind::Column, 1, 0, 1);
        tr.paste_transposed(0, 0, "1\t2\t3");
        assert_eq!(tr.column_values(0), vec![1.0, 2.0, 3.0]);
        assert_eq!(tr.block_text((0, 1), (0, 0)), "1\n2");
        tr.insert_row(0);
        assert_eq!(tr.cell_text(1, GridColumn::Value { dataset: 0, replicate: 0 }), "1");
        tr.delete_rows(0, 1);
        assert_eq!(tr.column_values(0), vec![2.0, 3.0]);
        tr.clear_block((0, 0), (0, 0));
        assert_eq!(tr.column_values(0), vec![3.0]);
    }
}
