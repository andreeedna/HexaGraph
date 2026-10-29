//! Graph rendering. A graph is laid out into a backend independent `Scene`
//! of primitives, which can be written as SVG or painted by the GUI.

pub mod axis;
pub mod render;
pub mod scene;
pub mod svg;

pub use render::render;
pub use scene::{Anchor, Color, Element, Hit, Prim, Scene};
pub use svg::to_svg;

/// Qualitative palettes, chosen to be distinguishable with colour blindness.
pub const PALETTES: [(&str, [Color; 8]); 3] = [
    (
        "Hexagon",
        [
            Color::rgb(0x1f, 0x5f, 0xa6),
            Color::rgb(0xe0, 0x6c, 0x1f),
            Color::rgb(0x2a, 0x9d, 0x6f),
            Color::rgb(0xc2, 0x3b, 0x5a),
            Color::rgb(0x7b, 0x5c, 0xb8),
            Color::rgb(0x8a, 0x6a, 0x3c),
            Color::rgb(0x3f, 0xa7, 0xc9),
            Color::rgb(0x6b, 0x6b, 0x6b),
        ],
    ),
    (
        "Okabe-Ito",
        [
            Color::rgb(0x00, 0x72, 0xb2),
            Color::rgb(0xe6, 0x9f, 0x00),
            Color::rgb(0x00, 0x9e, 0x73),
            Color::rgb(0xd5, 0x5e, 0x00),
            Color::rgb(0xcc, 0x79, 0xa7),
            Color::rgb(0x56, 0xb4, 0xe9),
            Color::rgb(0xf0, 0xe4, 0x42),
            Color::rgb(0x00, 0x00, 0x00),
        ],
    ),
    (
        "Greyscale",
        [
            Color::rgb(0x20, 0x20, 0x20),
            Color::rgb(0x70, 0x70, 0x70),
            Color::rgb(0xa8, 0xa8, 0xa8),
            Color::rgb(0x48, 0x48, 0x48),
            Color::rgb(0x90, 0x90, 0x90),
            Color::rgb(0xc8, 0xc8, 0xc8),
            Color::rgb(0x30, 0x30, 0x30),
            Color::rgb(0x60, 0x60, 0x60),
        ],
    ),
];

pub fn palette_color(palette: usize, i: usize) -> Color {
    PALETTES[palette % PALETTES.len()].1[i % 8]
}
