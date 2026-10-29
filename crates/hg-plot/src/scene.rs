use hg_core::graph::MarkerShape;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Color { a, ..self }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// Drawing primitives in pixel coordinates (origin top-left, y down).
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Line {
        from: (f32, f32),
        to: (f32, f32),
        color: Color,
        width: f32,
        dashed: bool,
    },
    Polyline {
        points: Vec<(f32, f32)>,
        color: Color,
        width: f32,
        dashed: bool,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        fill: Color,
        stroke: Option<(Color, f32)>,
    },
    Rect {
        min: (f32, f32),
        max: (f32, f32),
        fill: Color,
        stroke: Option<(Color, f32)>,
    },
    Marker {
        center: (f32, f32),
        shape: MarkerShape,
        size: f32,
        fill: Color,
        stroke: Color,
        stroke_width: f32,
    },
    /// `pos` is the text baseline point; `angle` rotates the text
    /// counter-clockwise (degrees) around it.
    Text {
        pos: (f32, f32),
        text: String,
        size: f32,
        color: Color,
        anchor: Anchor,
        angle: f32,
        bold: bool,
    },
}

/// A selectable part of a graph, for interactive editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Element {
    Plot,
    Title,
    XTitle,
    YTitle,
    XAxis,
    YAxis,
    Legend,
    /// A data set (by drawing order, the index used for its color).
    Series(usize),
    /// A significance bracket (index into `graph_brackets`).
    Bracket(usize),
}

impl Element {
    /// Elements that can be dragged to a new position.
    pub fn movable(self) -> bool {
        matches!(self, Element::Title | Element::XTitle | Element::YTitle | Element::Legend)
    }

    pub fn label(self) -> String {
        match self {
            Element::Plot => "Plotting area".into(),
            Element::Title => "Graph title".into(),
            Element::XTitle => "X axis title".into(),
            Element::YTitle => "Y axis title".into(),
            Element::XAxis => "X axis".into(),
            Element::YAxis => "Y axis".into(),
            Element::Legend => "Legend".into(),
            Element::Series(i) => format!("Data set {}", i + 1),
            Element::Bracket(i) => format!("Significance bracket {}", i + 1),
        }
    }
}

/// Where an element is drawn (scene coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub element: Element,
    pub min: (f32, f32),
    pub max: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub background: Color,
    pub prims: Vec<Prim>,
    /// Hit regions, from back to front.
    pub hits: Vec<Hit>,
}

impl Scene {
    pub fn new(width: f32, height: f32) -> Self {
        Scene { width, height, background: Color::WHITE, prims: Vec::new(), hits: Vec::new() }
    }

    pub fn hit(&mut self, element: Element, a: (f32, f32), b: (f32, f32)) {
        self.hits.push(Hit { element, min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) });
    }

    /// The front-most element at a point, with a tolerance in scene units.
    pub fn hit_test(&self, p: (f32, f32), tol: f32) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|h| p.0 >= h.min.0 - tol && p.0 <= h.max.0 + tol && p.1 >= h.min.1 - tol && p.1 <= h.max.1 + tol)
            .copied()
    }

    /// Union of the regions of an element.
    pub fn bounds(&self, e: Element) -> Option<Hit> {
        self.hits.iter().filter(|h| h.element == e).copied().reduce(|a, b| Hit {
            element: e,
            min: (a.min.0.min(b.min.0), a.min.1.min(b.min.1)),
            max: (a.max.0.max(b.max.0), a.max.1.max(b.max.1)),
        })
    }

    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), color: Color, width: f32) {
        self.prims.push(Prim::Line { from, to, color, width, dashed: false });
    }

    pub fn text(&mut self, pos: (f32, f32), text: impl Into<String>, size: f32, anchor: Anchor) {
        self.prims.push(Prim::Text { pos, text: text.into(), size, color: Color::BLACK, anchor, angle: 0.0, bold: false });
    }
}

/// Rough width of a text run, used for layout.
pub fn text_width(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.52
}
