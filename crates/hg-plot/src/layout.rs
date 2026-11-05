//! Several graphs composed on one page.

use crate::scene::{Anchor, Color, Prim, Scene};
use hg_core::{Layout, Project};

const GAP: f32 = 12.0;

/// Places the graphs of the layout in a grid, each cell as large as the
/// largest graph of its column and row, with an optional panel letter.
pub fn render_layout(project: &Project, l: &Layout) -> Result<Scene, String> {
    let scenes: Vec<Scene> = l
        .graphs
        .iter()
        .filter_map(|&id| project.graph(id))
        .map(|g| {
            crate::render(project, g).unwrap_or_else(|e| {
                let mut s = Scene::new(g.width, g.height);
                s.text((g.width / 2.0, g.height / 2.0), format!("{}: {e}", g.name), 12.0, Anchor::Middle);
                s
            })
        })
        .collect();
    if scenes.is_empty() {
        return Err("tick the graphs to put on this page".into());
    }
    let cols = l.columns.clamp(1, scenes.len());
    let rows = scenes.len().div_ceil(cols);
    let col_w: Vec<f32> = (0..cols).map(|c| scenes.iter().skip(c).step_by(cols).map(|s| s.width).fold(0.0, f32::max)).collect();
    let row_h: Vec<f32> = (0..rows).map(|r| scenes.iter().skip(r * cols).take(cols).map(|s| s.height).fold(0.0, f32::max)).collect();
    let width = col_w.iter().sum::<f32>() + GAP * (cols + 1) as f32;
    let height = row_h.iter().sum::<f32>() + GAP * (rows + 1) as f32;
    let mut page = Scene::new(width, height);
    for (i, s) in scenes.into_iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let dx = GAP * (c + 1) as f32 + col_w[..c].iter().sum::<f32>();
        let dy = GAP * (r + 1) as f32 + row_h[..r].iter().sum::<f32>();
        page.prims.extend(s.prims.into_iter().map(|p| shift(p, dx, dy)));
        if l.letters {
            let letter = char::from(b'A' + (i % 26) as u8).to_string();
            page.prims.push(Prim::Text {
                pos: (dx + 2.0, dy + 18.0),
                text: letter,
                size: 20.0,
                color: Color::BLACK,
                anchor: Anchor::Start,
                angle: 0.0,
                bold: true,
            });
        }
    }
    Ok(page)
}

fn shift(p: Prim, dx: f32, dy: f32) -> Prim {
    let m = |(x, y): (f32, f32)| (x + dx, y + dy);
    match p {
        Prim::Line { from, to, color, width, dashed } => Prim::Line { from: m(from), to: m(to), color, width, dashed },
        Prim::Polyline { points, color, width, dashed } => {
            Prim::Polyline { points: points.into_iter().map(m).collect(), color, width, dashed }
        }
        Prim::Polygon { points, fill, stroke } => Prim::Polygon { points: points.into_iter().map(m).collect(), fill, stroke },
        Prim::Rect { min, max, fill, stroke } => Prim::Rect { min: m(min), max: m(max), fill, stroke },
        Prim::Marker { center, shape, size, fill, stroke, stroke_width } => {
            Prim::Marker { center: m(center), shape, size, fill, stroke, stroke_width }
        }
        Prim::Text { pos, text, size, color, anchor, angle, bold } => Prim::Text { pos: m(pos), text, size, color, anchor, angle, bold },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hg_core::{sample::sample_table, Graph, GraphKind, TableKind};

    #[test]
    fn panels_fill_a_grid() {
        let mut p = Project::default();
        let t = p.add_table(sample_table(TableKind::Column));
        let ids: Vec<u64> = (0..3)
            .map(|i| {
                let id = p.new_id();
                p.graphs.push(Graph::new(id, t, GraphKind::ScatterDot, format!("g{i}")));
                id
            })
            .collect();
        let l = p.add_layout(ids.clone());
        let page = render_layout(&p, p.layout(l).unwrap()).unwrap();
        let g = p.graph(ids[0]).unwrap();
        // Two columns, two rows.
        assert_eq!(page.width, 2.0 * g.width + 3.0 * GAP);
        assert_eq!(page.height, 2.0 * g.height + 3.0 * GAP);
        let letters: Vec<&str> = page
            .prims
            .iter()
            .filter_map(|p| if let Prim::Text { text, size, .. } = p { (*size == 20.0).then_some(text.as_str()) } else { None })
            .collect();
        assert_eq!(letters, ["A", "B", "C"]);
        p.remove_graph(ids[1]);
        assert_eq!(p.layout(l).unwrap().graphs.len(), 2);
    }
}
