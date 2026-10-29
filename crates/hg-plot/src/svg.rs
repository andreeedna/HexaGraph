use crate::scene::{Anchor, Prim, Scene};
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn fill_attr(c: crate::Color) -> String {
    if c.a == 0 {
        "none".into()
    } else {
        c.hex()
    }
}

fn op(c: crate::Color) -> String {
    if c.a == 255 || c.a == 0 {
        String::new()
    } else {
        format!(" fill-opacity=\"{:.3}\"", c.a as f32 / 255.0)
    }
}

pub fn to_svg(scene: &Scene) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Helvetica, Arial, sans-serif">"#,
        w = scene.width,
        h = scene.height
    );
    let _ = writeln!(s, r#"<rect width="100%" height="100%" fill="{}"/>"#, fill_attr(scene.background));
    for p in &scene.prims {
        match p {
            Prim::Line { from, to, color, width, dashed } => {
                let _ = writeln!(
                    s,
                    r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{}"{}/>"#,
                    from.0,
                    from.1,
                    to.0,
                    to.1,
                    color.hex(),
                    width,
                    if *dashed { r#" stroke-dasharray="5,4""# } else { "" }
                );
            }
            Prim::Polyline { points, color, width, dashed } => {
                let pts: Vec<String> = points.iter().map(|p| format!("{:.2},{:.2}", p.0, p.1)).collect();
                let _ = writeln!(
                    s,
                    r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linejoin="round"{}/>"#,
                    pts.join(" "),
                    color.hex(),
                    width,
                    if *dashed { r#" stroke-dasharray="5,4""# } else { "" }
                );
            }
            Prim::Polygon { points, fill, stroke } => {
                let pts: Vec<String> = points.iter().map(|p| format!("{:.2},{:.2}", p.0, p.1)).collect();
                let st = stroke.map_or(String::new(), |(c, w)| format!(r#" stroke="{}" stroke-width="{}""#, c.hex(), w));
                let _ = writeln!(s, r#"<polygon points="{}" fill="{}"{}{}/>"#, pts.join(" "), fill_attr(*fill), op(*fill), st);
            }
            Prim::Rect { min, max, fill, stroke } => {
                let st = stroke.map_or(String::new(), |(c, w)| format!(r#" stroke="{}" stroke-width="{}""#, c.hex(), w));
                let _ = writeln!(
                    s,
                    r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"{}{}/>"#,
                    min.0,
                    min.1,
                    max.0 - min.0,
                    max.1 - min.1,
                    fill_attr(*fill),
                    op(*fill),
                    st
                );
            }
            Prim::Marker { center, shape, size, fill, stroke, stroke_width } => {
                let (x, y, r) = (center.0, center.1, *size);
                let style =
                    format!(r#"fill="{}"{} stroke="{}" stroke-width="{}""#, fill_attr(*fill), op(*fill), stroke.hex(), stroke_width);
                use hg_core::graph::MarkerShape::*;
                let _ = match shape {
                    Circle => writeln!(s, r#"<circle cx="{x:.2}" cy="{y:.2}" r="{r:.2}" {style}/>"#),
                    Square => {
                        writeln!(s, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" {style}/>"#, x - r, y - r, 2.0 * r, 2.0 * r)
                    }
                    Triangle => writeln!(
                        s,
                        r#"<polygon points="{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}" {style}/>"#,
                        x,
                        y - r * 1.15,
                        x - r,
                        y + r * 0.85,
                        x + r,
                        y + r * 0.85
                    ),
                    TriangleDown => writeln!(
                        s,
                        r#"<polygon points="{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}" {style}/>"#,
                        x,
                        y + r * 1.15,
                        x - r,
                        y - r * 0.85,
                        x + r,
                        y - r * 0.85
                    ),
                    Cross | Plus => {
                        let segs = if *shape == Cross {
                            [(x - r, y - r, x + r, y + r), (x - r, y + r, x + r, y - r)]
                        } else {
                            [(x - r, y, x + r, y), (x, y - r, x, y + r)]
                        };
                        let mut out = Ok(());
                        for (x1, y1, x2, y2) in segs {
                            out = writeln!(
                                s,
                                r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{}" stroke-width="{}"/>"#,
                                stroke.hex(),
                                stroke_width * 1.6
                            );
                        }
                        out
                    }
                    Diamond => writeln!(
                        s,
                        r#"<polygon points="{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}" {style}/>"#,
                        x,
                        y - r * 1.3,
                        x + r,
                        y,
                        x,
                        y + r * 1.3,
                        x - r,
                        y
                    ),
                };
            }
            Prim::Text { pos, text, size, color, anchor, angle, bold } => {
                let a = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let rot =
                    if *angle != 0.0 { format!(r#" transform="rotate({:.1} {:.2} {:.2})""#, -angle, pos.0, pos.1) } else { String::new() };
                let w = if *bold { r#" font-weight="bold""# } else { "" };
                let _ = writeln!(
                    s,
                    r#"<text x="{:.2}" y="{:.2}" font-size="{}" fill="{}" text-anchor="{}"{}{}>{}</text>"#,
                    pos.0,
                    pos.1,
                    size,
                    color.hex(),
                    a,
                    w,
                    rot,
                    esc(text)
                );
            }
        }
    }
    s.push_str("</svg>\n");
    s
}
