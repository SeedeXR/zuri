//! SVG -> Zuri source. usvg resolves the hard parts of SVG (CSS, `use`, units, shapes to
//! paths); what is left is paths with fills, strokes and gradients, which map directly.
//!
// ponytail: transforms are baked into each path's points, so every imported shape sits at
// 0,0 with absolute coordinates. Group opacity is multiplied into each shape (exact unless
// siblings overlap). Not imported yet: clip paths, masks, filters, patterns, images, text
// (unless the SVG has it as outlines), dashes. Add these as their Zuri features land.

use serde_json::{json, Value as J};
use usvg::tiny_skia_path::{PathSegment, Point, Transform};
use zuri_core::{Error, Result};

fn num(v: f32) -> J {
    // Three decimals is below a hundredth of a pixel and keeps the source readable.
    json!(((v as f64) * 1000.0).round() / 1000.0)
}

fn hex(c: usvg::Color, opacity: f32) -> String {
    let a = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    zuri_core::json::format_color(a << 24 | (c.red as u32) << 16 | (c.green as u32) << 8 | c.blue as u32)
}

fn map(t: &Transform, x: f32, y: f32) -> (f32, f32) {
    let mut p = [Point::from_xy(x, y)];
    t.map_points(&mut p);
    (p[0].x, p[0].y)
}

fn mean_scale(t: &Transform) -> f32 {
    (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

/// A Fill / Stroke's color or gradient child. `t` maps the paint's space to the artboard.
fn paint(p: &usvg::Paint, opacity: f32, t: &Transform, out: &mut serde_json::Map<String, J>) {
    let stops = |g: &usvg::BaseGradient| -> Vec<J> {
        g.stops()
            .iter()
            .map(|s| json!({ "type": "GradientStop", "position": num(s.offset().get()), "color": hex(s.color(), s.opacity().get() * opacity) }))
            .collect()
    };
    match p {
        usvg::Paint::Color(c) => {
            out.insert("color".into(), json!(hex(*c, opacity)));
        }
        usvg::Paint::LinearGradient(g) => {
            let t = t.pre_concat(g.transform());
            let ((x0, y0), (x1, y1)) = (map(&t, g.x1(), g.y1()), map(&t, g.x2(), g.y2()));
            out.insert(
                "children".into(),
                json!([{ "type": "LinearGradient", "x0": num(x0), "y0": num(y0), "x1": num(x1), "y1": num(y1), "children": stops(g) }]),
            );
        }
        usvg::Paint::RadialGradient(g) => {
            let t = t.pre_concat(g.transform());
            let (cx, cy) = map(&t, g.cx(), g.cy());
            out.insert(
                "children".into(),
                json!([{ "type": "RadialGradient", "x0": num(cx), "y0": num(cy), "radius": num(g.r().get() * mean_scale(&t)), "children": stops(g) }]),
            );
        }
        // Not representable yet: fall back to mid grey so the shape stays visible.
        usvg::Paint::Pattern(_) => {
            out.insert("color".into(), json!("#808080"));
        }
    }
}

fn walk(group: &usvg::Group, opacity: f32, shapes: &mut Vec<J>) {
    let opacity = opacity * group.opacity().get();
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => walk(g, opacity, shapes),
            usvg::Node::Path(path) if path.is_visible() => {
                let t = path.abs_transform();
                let mut d = String::new();
                for seg in path.data().segments() {
                    let mut put = |cmd: char, pts: &[Point]| {
                        d.push(cmd);
                        for p in pts {
                            let (x, y) = map(&t, p.x, p.y);
                            d.push_str(&format!("{} {} ", num(x), num(y)));
                        }
                    };
                    match seg {
                        PathSegment::MoveTo(p) => put('M', &[p]),
                        PathSegment::LineTo(p) => put('L', &[p]),
                        PathSegment::QuadTo(c, p) => put('Q', &[c, p]),
                        PathSegment::CubicTo(c1, c2, p) => put('C', &[c1, c2, p]),
                        PathSegment::Close => d.push_str("Z "),
                    }
                }
                let mut children = vec![json!({ "type": "Path", "d": d.trim_end() })];
                let fill = path.fill().map(|f| {
                    let mut o = serde_json::Map::new();
                    o.insert("type".into(), json!("Fill"));
                    if f.rule() == usvg::FillRule::EvenOdd {
                        o.insert("fillRule".into(), json!("evenodd"));
                    }
                    paint(f.paint(), f.opacity().get(), &t, &mut o);
                    J::Object(o)
                });
                let stroke = path.stroke().map(|s| {
                    let mut o = serde_json::Map::new();
                    o.insert("type".into(), json!("Stroke"));
                    o.insert("thickness".into(), num(s.width().get() * mean_scale(&t)));
                    o.insert(
                        "cap".into(),
                        json!(match s.linecap() {
                            usvg::LineCap::Butt => "butt",
                            usvg::LineCap::Round => "round",
                            usvg::LineCap::Square => "square",
                        }),
                    );
                    o.insert(
                        "join".into(),
                        json!(match s.linejoin() {
                            usvg::LineJoin::Round => "round",
                            usvg::LineJoin::Bevel => "bevel",
                            _ => "miter",
                        }),
                    );
                    paint(s.paint(), s.opacity().get(), &t, &mut o);
                    J::Object(o)
                });
                match path.paint_order() {
                    usvg::PaintOrder::FillAndStroke => children.extend(fill.into_iter().chain(stroke)),
                    usvg::PaintOrder::StrokeAndFill => children.extend(stroke.into_iter().chain(fill)),
                }
                let mut shape = json!({ "type": "Shape", "children": children });
                if !path.id().is_empty() {
                    shape["name"] = json!(path.id());
                }
                if opacity < 1.0 {
                    shape["opacity"] = num(opacity);
                }
                shapes.push(shape);
            }
            _ => {}
        }
    }
}

/// The children of an Artboard, plus its size.
pub fn svg(data: &[u8]) -> Result<(f32, f32, Vec<J>)> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| Error(format!("SVG: {e}")))?;
    let mut shapes = Vec::new();
    walk(tree.root(), 1.0, &mut shapes);
    Ok((tree.size().width(), tree.size().height(), shapes))
}
