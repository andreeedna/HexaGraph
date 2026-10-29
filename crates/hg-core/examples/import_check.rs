//! Imports a Prism file (.prism or .pzfx) and prints what HexaGraph computes
//! next to what Prism reported, to check an import.
//! Usage: cargo run -p hg-core --example import_check -- <file>

fn main() {
    let path = std::env::args().nth(1).expect("usage: import_check <file.prism|file.pzfx>");
    let imported = if path.ends_with(".pzfx") {
        hg_core::pzfx::import(&std::fs::read_to_string(&path).expect("read file"))
    } else {
        hg_core::prism::import(&std::fs::read(&path).expect("read file"))
    }
    .unwrap_or_else(|e| panic!("{e}"));
    let p = imported.project;
    for t in &p.tables {
        let sets: Vec<String> = t.datasets.iter().enumerate().map(|(d, s)| format!("{} {:?}", s.title, t.column_values(d))).collect();
        println!("table {} ({:?}), excluded {:?}: {}", t.name, t.kind, t.excluded, sets.join("; "));
    }
    for a in &p.analyses {
        let table = p.table(a.table_id).map_or("?", |t| t.name.as_str());
        match p.run_analysis(a.id) {
            Ok(out) => {
                let ours: Vec<String> = out.significance.iter().map(|s| format!("P = {:.6}", s.p)).collect();
                let t_row =
                    out.tables.first().and_then(|t| t.rows.iter().find(|r| r[0] == "t, df")).map(|r| r[1].clone()).unwrap_or_default();
                println!("{} of {table}: HexaGraph {t_row} {} | {}", a.kind.label(), ours.join(", "), a.notes.join(" "));
                if ours.is_empty() {
                    for t in &out.tables {
                        for r in &t.rows {
                            println!("    {}", r.join(" | "));
                        }
                    }
                    for w in &out.warnings {
                        println!("    note: {w}");
                    }
                }
            }
            Err(e) => println!("{} of {table}: error {e}", a.kind.label()),
        }
    }
    for w in imported.warnings {
        println!("note: {w}");
    }
}
