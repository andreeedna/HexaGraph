//! Exports graphs as PNG, JPEG, TIFF or SVG.

use hg_plot::Scene;
use resvg::{tiny_skia, usvg};
use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    Tiff,
    Svg,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Png, Format::Jpeg, Format::Tiff, Format::Svg];

    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Tiff => "TIFF",
            Format::Svg => "SVG (vector)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Tiff => "tif",
            Format::Svg => "svg",
        }
    }

    pub fn is_raster(self) -> bool {
        self != Format::Svg
    }
}

/// System fonts, loaded once (it takes a moment).
fn fonts() -> &'static std::sync::Arc<usvg::fontdb::Database> {
    static DB: OnceLock<std::sync::Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        std::sync::Arc::new(db)
    })
}

/// Pixel size of a raster export at `dpi` (the scene is laid out at 72 dpi).
pub fn pixel_size(scene: &Scene, dpi: f32) -> (u32, u32) {
    let s = dpi / 72.0;
    ((scene.width * s).round().max(1.0) as u32, (scene.height * s).round().max(1.0) as u32)
}

/// Rasterizes the scene to straight RGBA pixels.
fn rasterize(scene: &Scene, dpi: f32) -> Result<(u32, u32, Vec<u8>), String> {
    let svg = hg_plot::to_svg(scene);
    let opt = usvg::Options { fontdb: fonts().clone(), ..Default::default() };
    let tree = usvg::Tree::from_str(&svg, &opt).map_err(|e| e.to_string())?;
    let (w, h) = pixel_size(scene, dpi);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("the image is too large")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let s = dpi / 72.0;
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    // The background is opaque, so premultiplied and straight alpha coincide.
    Ok((w, h, pixmap.data().to_vec()))
}

/// Encodes the scene in memory.
pub fn encode(scene: &Scene, format: Format, dpi: f32) -> Result<Vec<u8>, String> {
    if format == Format::Svg {
        return Ok(hg_plot::to_svg(scene).into_bytes());
    }
    let (w, h, rgba) = rasterize(scene, dpi)?;
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("invalid image buffer")?;
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut out = Cursor::new(Vec::new());
    match format {
        Format::Png => rgb.write_to(&mut out, image::ImageFormat::Png),
        Format::Tiff => rgb.write_to(&mut out, image::ImageFormat::Tiff),
        Format::Jpeg => image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95).encode_image(&rgb),
        Format::Svg => unreachable!(),
    }
    .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

pub fn export(scene: &Scene, format: Format, dpi: f32, path: &Path) -> Result<(), String> {
    let bytes = encode(scene, format, dpi)?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_encodes() {
        let mut p = hg_core::Project::default();
        let t = p.add_table(hg_core::sample::sample_table(hg_core::TableKind::Column));
        let g = p.add_graph(t, hg_core::GraphKind::Bar);
        let scene = hg_plot::render(&p, p.graph(g).unwrap()).unwrap();
        for f in Format::ALL {
            let bytes = encode(&scene, f, 150.0).unwrap();
            assert!(bytes.len() > 100, "{f:?}");
        }
        assert_eq!(&encode(&scene, Format::Png, 72.0).unwrap()[1..4], b"PNG");
    }
}
