//! Built-in example data so new users can explore every table type.

use crate::table::{DataTable, TableKind};

fn fill(t: &mut DataTable, columns: &[&[f64]]) {
    for (d, col) in columns.iter().enumerate() {
        for (r, v) in col.iter().enumerate() {
            t.set(r, crate::table::GridColumn::Value { dataset: d, replicate: 0 }, if v.is_nan() { None } else { Some(*v) });
        }
    }
}

pub fn sample_table(kind: TableKind) -> DataTable {
    match kind {
        TableKind::Column => {
            let mut t = DataTable::new(0, "Enzyme activity", kind, 3, 0, 1);
            for (d, n) in ["Control", "Treated", "Treated + inhibitor"].iter().enumerate() {
                t.datasets[d].title = n.to_string();
            }
            fill(
                &mut t,
                &[
                    &[23.1, 25.4, 21.8, 27.2, 24.5, 26.1, 22.9, 25.0],
                    &[28.4, 30.1, 27.5, 31.6, 29.2, 26.8, 30.9, 29.7],
                    &[24.0, 26.5, 25.1, 27.9, 23.8, 26.2, 25.5, 24.7],
                ],
            );
            t
        }
        TableKind::XY => {
            let mut t = DataTable::new(0, "Dose-response", kind, 2, 0, 2);
            t.x_title = "log[agonist], M".into();
            t.datasets[0].title = "Wild type".into();
            t.datasets[1].title = "Mutant".into();
            let x = [-9.0, -8.5, -8.0, -7.5, -7.0, -6.5, -6.0, -5.5, -5.0];
            let a =
                [[2.1, 3.0], [4.5, 5.9], [10.2, 12.0], [24.8, 22.1], [51.3, 48.0], [76.9, 79.5], [90.4, 88.2], [96.8, 97.5], [98.9, 99.6]];
            let b = [[1.0, 1.8], [2.2, 1.5], [4.1, 3.6], [8.8, 10.1], [19.5, 22.0], [41.2, 38.7], [63.0, 66.1], [80.4, 82.2], [90.1, 88.0]];
            for (r, &xv) in x.iter().enumerate() {
                t.set(r, crate::table::GridColumn::X, Some(xv));
                for k in 0..2 {
                    t.set(r, crate::table::GridColumn::Value { dataset: 0, replicate: k }, Some(a[r][k]));
                    t.set(r, crate::table::GridColumn::Value { dataset: 1, replicate: k }, Some(b[r][k]));
                }
            }
            t
        }
        TableKind::Grouped => {
            let mut t = DataTable::new(0, "Cytokine release", kind, 2, 3, 3);
            t.datasets[0].title = "Vehicle".into();
            t.datasets[1].title = "Drug".into();
            t.row_titles = vec!["Day 1".into(), "Day 3".into(), "Day 7".into()];
            let v = [
                [[12.0, 14.5, 13.1], [11.2, 12.9, 13.4]],
                [[18.4, 20.1, 19.0], [13.5, 14.8, 12.7]],
                [[25.2, 27.9, 26.4], [15.1, 16.9, 14.4]],
            ];
            for r in 0..3 {
                for d in 0..2 {
                    for k in 0..3 {
                        t.set(r, crate::table::GridColumn::Value { dataset: d, replicate: k }, Some(v[r][d][k]));
                    }
                }
            }
            t
        }
        TableKind::Contingency => {
            let mut t = DataTable::new(0, "Treatment outcome", kind, 2, 0, 1);
            t.datasets[0].title = "Recovered".into();
            t.datasets[1].title = "Not recovered".into();
            fill(&mut t, &[&[42.0, 28.0], &[8.0, 22.0]]);
            t.row_titles = vec!["Drug".into(), "Placebo".into()];
            t
        }
        TableKind::Survival => {
            let mut t = DataTable::new(0, "Survival study", kind, 2, 0, 1);
            t.x_title = "Days".into();
            t.datasets[0].title = "Standard".into();
            t.datasets[1].title = "New therapy".into();
            let times = [5.0, 8.0, 12.0, 12.0, 15.0, 20.0, 22.0, 30.0, 3.0, 4.0, 7.0, 9.0, 10.0, 14.0, 16.0, 18.0];
            let ev = [1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0];
            for (r, (&tm, &e)) in times.iter().zip(&ev).enumerate() {
                t.set(r, crate::table::GridColumn::X, Some(tm));
                t.set(r, crate::table::GridColumn::Value { dataset: usize::from(r >= 8), replicate: 0 }, Some(e));
            }
            t
        }
        TableKind::PartsOfWhole => {
            let mut t = DataTable::new(0, "Cell populations", kind, 1, 0, 1);
            t.datasets[0].title = "Count".into();
            fill(&mut t, &[&[48.0, 22.0, 11.0, 14.0, 5.0]]);
            t.row_titles = vec!["T cells".into(), "B cells".into(), "NK cells".into(), "Monocytes".into(), "Other".into()];
            t
        }
    }
}
