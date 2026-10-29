//! Writes an SVG of every graph type, drawn from the built-in sample data.
//! Usage: cargo run -p hg-plot --example gallery -- <output dir>

use hg_core::graph::GraphKind;
use hg_core::sample::sample_table;
use hg_core::{AnalysisKind, Project, TableKind};
use hg_stats::nonlinear::Model;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "gallery".into());
    std::fs::create_dir_all(&dir).expect("create output dir");
    let mut p = Project::default();
    for kind in TableKind::ALL {
        let id = p.add_table(sample_table(kind));
        let sig = match kind {
            TableKind::XY => {
                p.add_analysis(id, AnalysisKind::NonlinearFit { model: Model::DoseResponseVariableSlope, fixed: Vec::new() });
                None
            }
            TableKind::Column => Some(p.add_analysis(
                id,
                AnalysisKind::OneWay {
                    test: hg_core::analysis::OneWayTest::Ordinary,
                    comparisons: Some(hg_stats::posthoc::Method::Tukey),
                    control: None,
                },
            )),
            _ => None,
        };
        for (gk, transposed) in GraphKind::gallery(kind) {
            let gid = p.add_graph(id, gk);
            let mut g = p.graph(gid).unwrap().clone();
            g.title = gk.gallery_label(transposed, kind);
            g.transpose = transposed;
            g.auto_significance.analysis = sig;
            if kind == TableKind::XY {
                g.y_label = "Response (%)".into();
            } else if kind == TableKind::Column {
                g.y_label = "Activity (U/mL)".into();
            }
            let scene = hg_plot::render(&p, &g).expect("render");
            let path = format!("{dir}/{:?}_{:?}{}.svg", kind, gk, if transposed { "_by_column" } else { "" });
            std::fs::write(&path, hg_plot::to_svg(&scene)).expect("write");
            println!("{path}");
        }
    }
}
