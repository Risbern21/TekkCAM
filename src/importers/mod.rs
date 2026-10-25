// importers/mod.rs — file format parsers (Phase 2)
pub mod gerber;
pub mod excellon;
pub mod dxf;
pub mod svg;

use crate::workspace::layer::{Layer, LayerKind};
use crate::workspace::object::Geometry;

/// Result of parsing a design file.
pub struct ParseOutput {
    pub geometry: Geometry,
    /// Human-readable warnings / info lines for the console.
    pub notes: Vec<String>,
}

/// Read a file from disk, detect its kind, parse it and build a ready-to-use layer.
pub fn load_layer(path: &str, index: usize) -> anyhow::Result<(Layer, Vec<String>)> {
    let mut layer = Layer::from_path(path);
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);

    // Content sniffing for ambiguous extensions (.txt, .art, …)
    if layer.kind == LayerKind::Placeholder {
        if text.contains("%FS") || text.contains("%MO") {
            layer.kind = LayerKind::Gerber;
        } else if text.contains("M48") {
            layer.kind = LayerKind::Drill;
        } else if text.contains("SECTION") && text.contains("ENTITIES") {
            layer.kind = LayerKind::Dxf;
        } else if text.contains("<svg") {
            layer.kind = LayerKind::Svg;
        }
    }

    let out = match layer.kind {
        LayerKind::Gerber => gerber::parse(&text)?,
        LayerKind::Drill => excellon::parse(&text)?,
        LayerKind::Dxf => dxf::parse(&text)?,
        LayerKind::Svg => svg::parse(&text)?,
        LayerKind::Gcode => anyhow::bail!("G-code import is not implemented yet (Phase 4)"),
        LayerKind::Placeholder => anyhow::bail!("Unrecognised file type"),
    };
    layer.geometry = out.geometry;
    layer.color = crate::workspace::layer::palette_color(&layer.kind, index);
    Ok((layer, out.notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_gerber() {
        let out = gerber::parse(include_str!("../../assets/samples/test.gbr")).unwrap();
        assert!(out.geometry.triangles.len() >= 3 * 10);
        let (mn, mx) = out.geometry.bounds().unwrap();
        assert!(mn[0] < 0.1 && mx[0] > 34.9, "bounds {mn:?} {mx:?}");
    }

    #[test]
    fn parses_sample_drill() {
        let out = excellon::parse(include_str!("../../assets/samples/test.drl")).unwrap();
        assert_eq!(out.geometry.drills.len(), 3);
    }

    #[test]
    fn parses_sample_dxf() {
        let out = dxf::parse(include_str!("../../assets/samples/test.dxf")).unwrap();
        assert!(out.geometry.lines.len() > 20);
    }

    #[test]
    fn parses_svg() {
        let svg = r#"<svg width="100mm" height="50mm" viewBox="0 0 100 50"><path d="M10 10 L90 10 A20 20 0 0 1 90 40 Z"/><circle cx="20" cy="30" r="5"/></svg>"#;
        let out = svg::parse(svg).unwrap();
        assert!(out.geometry.lines.len() > 20);
    }

    #[test]
    fn ear_clip_concave_area() {
        let mut g = Geometry::default();
        // L-shape, area 3
        g.add_polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]]);
        let mut area = 0.0_f32;
        for t in g.triangles.chunks(3) {
            area += ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])).abs() * 0.5;
        }
        assert!((area - 3.0).abs() < 1e-4, "area {area}");
    }
}
