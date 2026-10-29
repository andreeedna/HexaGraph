//! Paints an `hg_plot::Scene` with egui.

use eframe::egui::{self, epaint::TextShape, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use hg_core::graph::MarkerShape;
use hg_plot::{Anchor, Color, Prim, Scene};

fn c32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Ear-clipping triangulation of a simple polygon (indices into `pts`).
fn triangulate(pts: &[Pos2]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let mut idx: Vec<usize> = if area > 0.0 { (0..n).collect() } else { (0..n).rev().collect() };
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut tris = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 0.0 {
                continue;
            }
            let inside = idx.iter().any(|&j| {
                j != ia && j != ib && j != ic && cross(a, b, pts[j]) > 0.0 && cross(b, c, pts[j]) > 0.0 && cross(c, a, pts[j]) > 0.0
            });
            if !inside {
                tris.push([ia, ib, ic]);
                idx.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break; // degenerate polygon: fall back to a fan for the rest
        }
    }
    for k in 1..idx.len().saturating_sub(1) {
        tris.push([idx[0], idx[k], idx[k + 1]]);
    }
    tris
}

/// Paints the scene so that it fits in `rect`, keeping its aspect ratio.
pub fn paint(painter: &egui::Painter, rect: Rect, scene: &Scene) {
    let s = fit_scale(rect, scene);
    let origin = rect.center() - Vec2::new(scene.width * s, scene.height * s) / 2.0;
    paint_at(painter, origin, s, scene);
}

/// Scale that fits the whole scene in `rect`.
pub fn fit_scale(rect: Rect, scene: &Scene) -> f32 {
    (rect.width() / scene.width).min(rect.height() / scene.height)
}

/// Paints the scene with its top-left corner at `origin`, `s` pixels per point.
pub fn paint_at(painter: &egui::Painter, origin: Pos2, s: f32, scene: &Scene) {
    let p = |q: (f32, f32)| Pos2::new(origin.x + q.0 * s, origin.y + q.1 * s);
    let area = Rect::from_min_size(origin, Vec2::new(scene.width * s, scene.height * s));
    painter.add(egui::Shape::rect_filled(area.translate(Vec2::new(0.0, 3.0)).expand(1.0), 2.0, Color32::from_black_alpha(40)));
    painter.rect_filled(area, 0.0, c32(scene.background));
    for prim in &scene.prims {
        match prim {
            Prim::Line { from, to, color, width, dashed } => {
                let stroke = Stroke::new(width * s, c32(*color));
                if *dashed {
                    painter.extend(Shape::dashed_line(&[p(*from), p(*to)], stroke, 5.0 * s, 4.0 * s));
                } else {
                    painter.line_segment([p(*from), p(*to)], stroke);
                }
            }
            Prim::Polyline { points, color, width, dashed } => {
                let pts: Vec<Pos2> = points.iter().map(|q| p(*q)).collect();
                let stroke = Stroke::new(width * s, c32(*color));
                if *dashed {
                    painter.extend(Shape::dashed_line(&pts, stroke, 5.0 * s, 4.0 * s));
                } else {
                    painter.add(Shape::line(pts, stroke));
                }
            }
            Prim::Polygon { points, fill, stroke } => {
                let pts: Vec<Pos2> = points.iter().map(|q| p(*q)).collect();
                let mut mesh = egui::Mesh::default();
                for &q in &pts {
                    mesh.colored_vertex(q, c32(*fill));
                }
                for [a, b, c] in triangulate(&pts) {
                    mesh.add_triangle(a as u32, b as u32, c as u32);
                }
                painter.add(Shape::mesh(mesh));
                if let Some((c, w)) = stroke {
                    painter.add(Shape::closed_line(pts, Stroke::new(w * s, c32(*c))));
                }
            }
            Prim::Rect { min, max, fill, stroke } => {
                let r = Rect::from_two_pos(p(*min), p(*max));
                let st = stroke.map_or(Stroke::NONE, |(c, w)| Stroke::new(w * s, c32(c)));
                painter.rect(r, 0.0, c32(*fill), st, egui::StrokeKind::Middle);
            }
            Prim::Marker { center, shape, size, fill, stroke, stroke_width } => {
                let (c, r) = (p(*center), size * s);
                let st = Stroke::new(stroke_width * s, c32(*stroke));
                match shape {
                    MarkerShape::Circle => {
                        painter.circle(c, r, c32(*fill), st);
                    }
                    MarkerShape::Square => {
                        painter.rect(Rect::from_center_size(c, Vec2::splat(2.0 * r)), 0.0, c32(*fill), st, egui::StrokeKind::Middle);
                    }
                    MarkerShape::Triangle => {
                        let pts = vec![c + Vec2::new(0.0, -1.15 * r), c + Vec2::new(r, 0.85 * r), c + Vec2::new(-r, 0.85 * r)];
                        painter.add(Shape::convex_polygon(pts, c32(*fill), st));
                    }
                    MarkerShape::TriangleDown => {
                        let pts = vec![c + Vec2::new(0.0, 1.15 * r), c + Vec2::new(-r, -0.85 * r), c + Vec2::new(r, -0.85 * r)];
                        painter.add(Shape::convex_polygon(pts, c32(*fill), st));
                    }
                    MarkerShape::Cross | MarkerShape::Plus => {
                        let line = Stroke::new(1.6 * stroke_width * s, c32(*stroke));
                        let (a, b) = if *shape == MarkerShape::Cross {
                            (Vec2::new(r, r), Vec2::new(r, -r))
                        } else {
                            (Vec2::new(r, 0.0), Vec2::new(0.0, r))
                        };
                        painter.line_segment([c - a, c + a], line);
                        painter.line_segment([c - b, c + b], line);
                    }
                    MarkerShape::Diamond => {
                        let pts =
                            vec![c + Vec2::new(0.0, -1.3 * r), c + Vec2::new(r, 0.0), c + Vec2::new(0.0, 1.3 * r), c + Vec2::new(-r, 0.0)];
                        painter.add(Shape::convex_polygon(pts, c32(*fill), st));
                    }
                }
            }
            Prim::Text { pos, text, size, color, anchor, angle, bold } => {
                let fs = size * s;
                let font = if *bold { FontId::new(fs, crate::theme::bold_family()) } else { FontId::proportional(fs) };
                let galley = painter.layout_no_wrap(text.clone(), font, c32(*color));
                let w = galley.size().x;
                let ascent = fs * 0.82;
                let q = p(*pos);
                // Direction of reading and "up" for a counter-clockwise angle (y grows down).
                let a = angle.to_radians();
                let dir = Vec2::new(a.cos(), -a.sin());
                let up = Vec2::new(-a.sin(), -a.cos());
                let along = match anchor {
                    Anchor::Start => 0.0,
                    Anchor::Middle => w / 2.0,
                    Anchor::End => w,
                };
                let top_left = q - dir * along + up * ascent;
                if *angle == 0.0 {
                    painter.galley(top_left, galley, c32(*color));
                } else {
                    painter.add(TextShape::new(top_left, galley, c32(*color)).with_angle(-a));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_concave_polygon() {
        // An L shape (6 vertices) needs 4 triangles.
        let pts = [(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)].map(|(x, y)| Pos2::new(x, y));
        assert_eq!(triangulate(&pts).len(), 4);
    }
}
