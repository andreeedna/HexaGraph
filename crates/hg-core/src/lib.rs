//! HexaGraph document model: data tables, analyses, graphs and projects.

pub mod advice;
pub mod analysis;
pub mod format;
pub mod graph;
pub mod prism;
pub mod project;
pub mod pzfx;
pub mod sample;
pub mod table;

pub use analysis::{Analysis, AnalysisKind, AnalysisOutput, ResultTable};
pub use graph::{ErrorBar, Graph, GraphKind, Scale};
pub use project::Project;
pub use table::{DataSet, DataTable, GridColumn, TableKind};
