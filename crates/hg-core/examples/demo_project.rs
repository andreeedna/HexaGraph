//! Writes a demo project with every table type, typical analyses and graphs.
//! Usage: cargo run -p hg-core --example demo_project -- examples/demo.hxn

use hg_core::analysis::{AnalysisKind, OneWayTest};
use hg_core::sample::sample_table;
use hg_core::{Project, TableKind};
use hg_stats::nonlinear::Model;
use hg_stats::posthoc::Method;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "demo.hxn".into());
    let mut p = Project { title: "HexaGraph demo".into(), ..Default::default() };
    for kind in TableKind::ALL {
        let id = p.add_table(sample_table(kind));
        match kind {
            TableKind::Column => {
                p.add_analysis(id, AnalysisKind::ColumnStatistics);
                let a = p
                    .add_analysis(id, AnalysisKind::OneWay { test: OneWayTest::Ordinary, comparisons: Some(Method::Tukey), control: None });
                let g = p.add_graph(id, hg_core::GraphKind::ScatterDot);
                let g = p.graphs.iter_mut().find(|x| x.id == g).unwrap();
                g.auto_significance.analysis = Some(a);
                g.y_label = "Activity (U/mL)".into();
            }
            TableKind::XY => {
                p.add_analysis(id, AnalysisKind::NonlinearFit { model: Model::DoseResponseVariableSlope, fixed: Vec::new() });
                let g = p.add_graph(id, hg_core::GraphKind::Xy);
                p.graphs.iter_mut().find(|x| x.id == g).unwrap().y_label = "Response (%)".into();
            }
            TableKind::Grouped => {
                p.add_analysis(id, AnalysisKind::TwoWay { comparisons: Some(Method::Sidak), within_columns: false, repeated: false });
            }
            TableKind::Contingency => {
                p.add_analysis(id, AnalysisKind::Contingency { yates: false });
            }
            TableKind::Survival => {
                p.add_analysis(id, AnalysisKind::Survival { gehan: false });
            }
            TableKind::PartsOfWhole => {
                p.add_analysis(id, AnalysisKind::FractionOfTotal);
            }
        }
    }
    for t in p.tables.clone() {
        if !p.graphs.iter().any(|g| g.table_id == t.id) {
            p.add_graph(t.id, hg_core::GraphKind::for_table(t.kind)[0]);
        }
    }
    p.save(std::path::Path::new(&path)).expect("write project");
    println!("wrote {path}");
}
