//! SVG -> Zuri source. usvg resolves the hard parts of SVG (CSS, `use`, units, shapes to
//! paths); what is left is paths with fills, strokes and gradients, which map directly.
//!
//! A group that says something (a transform, an opacity, an id) becomes a Node with that
//! transform as x / y / rotation / scale, so that it can be animated (one path alone in
//! it is a Shape that says the same); a shape sits at the middle of its points, to turn
//! about it.
//!
// ponytail: a Node cannot skew: a group whose matrix does has it put into the points of
// what is in it (and into the groups under it), and keeps only its name and opacity.
// Group opacity fades each shape (exact unless siblings overlap). Not imported yet: clip
// paths, masks, filters, patterns, images, text (unless the SVG has it as outlines),
// dashes. Add these as their Zuri features land.

use serde_json::{json, Map, Value as J};
use usvg::tiny_skia_path::{PathSegment, Point, Transform};
use zuri_core::geom::Mat;
use zuri_core::{Error, Result};

/// A length or a place to three decimals of a pixel as it is drawn, which under a scale
/// of `zoom` is more of them: below a hundredth of a pixel, and the source stays readable.
fn num(v: f32, zoom: f32) -> J {
    let unit = 10f64.powi(3 + zoom.clamp(1.0, 1e4).log10().ceil() as i32);
    // (Nothing added to nothing below nothing is nothing: no "-0.0".)
    json!(((v as f64) * unit).round() / unit + 0.0)
}

/// A property, left out where it is at rest.
fn put(o: &mut Map<String, J>, key: &str, value: J, rest: f64) {
    if value != json!(rest) {
        o.insert(key.into(), value);
    }
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

/// A Fill / Stroke's color or gradient child. `t` maps the paint's space to the shape's.
fn paint(p: &usvg::Paint, opacity: f32, t: &Transform, zoom: f32, out: &mut Map<String, J>) {
    let stops = |g: &usvg::BaseGradient| -> Vec<J> {
        g.stops()
            .iter()
            .map(|s| json!({ "type": "GradientStop", "position": num(s.offset().get(), 1.0), "color": hex(s.color(), s.opacity().get() * opacity) }))
            .collect()
    };
    let n = |v: f32| num(v, zoom);
    match p {
        usvg::Paint::Color(c) => {
            out.insert("color".into(), json!(hex(*c, opacity)));
        }
        usvg::Paint::LinearGradient(g) => {
            let t = t.pre_concat(g.transform());
            let ((x0, y0), (x1, y1)) = (map(&t, g.x1(), g.y1()), map(&t, g.x2(), g.y2()));
            out.insert("children".into(), json!([{ "type": "LinearGradient", "x0": n(x0), "y0": n(y0), "x1": n(x1), "y1": n(y1), "children": stops(g) }]));
        }
        usvg::Paint::RadialGradient(g) => {
            let t = t.pre_concat(g.transform());
            let (cx, cy) = map(&t, g.cx(), g.cy());
            out.insert(
                "children".into(),
                json!([{ "type": "RadialGradient", "x0": n(cx), "y0": n(cy), "radius": n(g.r().get() * mean_scale(&t)), "children": stops(g) }]),
            );
        }
        // Not representable yet: fall back to mid grey so the shape stays visible.
        usvg::Paint::Pattern(_) => {
            out.insert("color".into(), json!("#808080"));
        }
    }
}

/// The box of the paths in `group` that are imported, as `t` places them.
fn extent(group: &usvg::Group, t: Transform, b: &mut [f32; 4]) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => extent(g, t.pre_concat(g.transform()), b),
            usvg::Node::Path(path) if path.is_visible() => {
                let r = path.data().bounds();
                for (x, y) in [(r.left(), r.top()), (r.right(), r.top()), (r.right(), r.bottom()), (r.left(), r.bottom())] {
                    let (x, y) = map(&t, x, y);
                    *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
            }
            _ => {}
        }
    }
}

/// A path as a Shape, its points placed by `t`. With `middle` the Shape is put at the
/// middle of them, to turn about it; without, whoever asks says where it is.
fn shape(path: &usvg::Path, t: &Transform, zoom: f32, middle: bool) -> Map<String, J> {
    let mut o = Map::new();
    o.insert("type".into(), json!("Shape"));
    if !path.id().is_empty() {
        o.insert("name".into(), json!(path.id()));
    }
    let b = path.data().bounds();
    let (cx, cy) = if middle { map(t, b.x() + b.width() * 0.5, b.y() + b.height() * 0.5) } else { (0.0, 0.0) };
    put(&mut o, "x", num(cx, zoom), 0.0);
    put(&mut o, "y", num(cy, zoom), 0.0);
    let t = t.post_translate(-cx, -cy);
    let mut d = String::new();
    for seg in path.data().segments() {
        let mut points = |cmd: char, pts: &[Point]| {
            d.push(cmd);
            for p in pts {
                let (x, y) = map(&t, p.x, p.y);
                d.push_str(&format!("{} {} ", num(x, zoom), num(y, zoom)));
            }
        };
        match seg {
            PathSegment::MoveTo(p) => points('M', &[p]),
            PathSegment::LineTo(p) => points('L', &[p]),
            PathSegment::QuadTo(c, p) => points('Q', &[c, p]),
            PathSegment::CubicTo(c1, c2, p) => points('C', &[c1, c2, p]),
            PathSegment::Close => d.push_str("Z "),
        }
    }
    let mut children = vec![json!({ "type": "Path", "d": d.trim_end() })];
    let fill = path.fill().map(|f| {
        let mut o = Map::new();
        o.insert("type".into(), json!("Fill"));
        if f.rule() == usvg::FillRule::EvenOdd {
            o.insert("fillRule".into(), json!("evenodd"));
        }
        paint(f.paint(), f.opacity().get(), &t, zoom, &mut o);
        J::Object(o)
    });
    let stroke = path.stroke().map(|s| {
        let mut o = Map::new();
        o.insert("type".into(), json!("Stroke"));
        o.insert("thickness".into(), num(s.width().get() * mean_scale(&t), zoom));
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
        paint(s.paint(), s.opacity().get(), &t, zoom, &mut o);
        J::Object(o)
    });
    match path.paint_order() {
        usvg::PaintOrder::FillAndStroke => children.extend(fill.into_iter().chain(stroke)),
        usvg::PaintOrder::StrokeAndFill => children.extend(stroke.into_iter().chain(fill)),
    }
    o.insert("children".into(), J::Array(children));
    o
}

/// What is in `group`, as children of the Node that stands for it. `carry` is what of the
/// groups around could not be said by a Node, and is put into the points here; `zoom` is
/// how much larger than written the Nodes around draw it.
fn walk(group: &usvg::Group, carry: Transform, zoom: f32, shapes: &mut Vec<J>) {
    for node in group.children() {
        match node {
            // A group that says nothing is not kept.
            usvg::Node::Group(g) if g.id().is_empty() && g.opacity().get() >= 1.0 && g.transform().is_identity() => walk(g, carry, zoom, shapes),
            usvg::Node::Group(g) => {
                let mut o = Map::new();
                if !g.id().is_empty() {
                    o.insert("name".into(), json!(g.id()));
                }
                put(&mut o, "opacity", num(g.opacity().get(), 1.0), 1.0);
                // tiny-skia's (sx, ky, kx, sy) are the matrix (a, b, c, d). A Node is a turn
                // after a scale along each axis: a matrix whose two columns are square to
                // each other. One that leans is carried into what the group holds.
                let m = carry.pre_concat(g.transform());
                let (turn, sx, sy) = Mat { a: m.sx, b: m.ky, c: m.kx, d: m.sy, tx: 0.0, ty: 0.0 }.parts();
                // (To a part in ten thousand: a matrix written to four decimals is not a lean.)
                let said = sx.abs() > 1e-6 && (m.sx * m.kx + m.ky * m.sy).abs() <= 1e-4 * sx.abs() * sy;
                let (mut within, mut inner) = (m, zoom);
                if said {
                    // Where the Node is, and what it turns and grows about: the point the
                    // group's own transform leaves where it is (the middle of `rotate(30
                    // 100 100)`), if there is one near what the group holds; else the middle
                    // of that. (usvg's own box of a group takes in what is not imported.)
                    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                    extent(g, Transform::identity(), &mut b);
                    let own = g.transform();
                    let (far, det) = ((b[2] - b[0]).max(b[3] - b[1]), (1.0 - own.sx) * (1.0 - own.sy) - own.kx * own.ky);
                    let still = (((1.0 - own.sy) * own.tx + own.kx * own.ty) / det, ((1.0 - own.sx) * own.ty + own.ky * own.tx) / det);
                    let near = still.0 >= b[0] - far && still.0 <= b[2] + far && still.1 >= b[1] - far && still.1 <= b[3] + far;
                    let (cx, cy) = if near { still } else { ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5) };
                    let (x, y) = map(&m, cx, cy);
                    put(&mut o, "x", num(x, zoom), 0.0);
                    put(&mut o, "y", num(y, zoom), 0.0);
                    // (To five places: a scale of 1.2345 of a thousand pixels is out by half a pixel at three.)
                    put(&mut o, "rotation", num(turn, 100.0), 0.0);
                    put(&mut o, "scaleX", num(sx, 100.0), 1.0);
                    put(&mut o, "scaleY", num(sy, 100.0), 1.0);
                    (within, inner) = (Transform::from_translate(-cx, -cy), zoom * sx.abs().max(sy));
                }
                match g.children() {
                    // One path in a group with no name of its own (an element with a
                    // `transform`): its Shape says what the group says.
                    [usvg::Node::Path(path)] if g.id().is_empty() && path.is_visible() => {
                        let mut lone = shape(path, &within, inner, !said);
                        lone.append(&mut o);
                        shapes.push(J::Object(lone));
                    }
                    _ => {
                        let mut inside = Vec::new();
                        walk(g, within, inner, &mut inside);
                        if !inside.is_empty() {
                            o.insert("type".into(), json!("Node"));
                            o.insert("children".into(), J::Array(inside));
                            shapes.push(J::Object(o));
                        }
                    }
                }
            }
            usvg::Node::Path(path) if path.is_visible() => shapes.push(J::Object(shape(path, &carry, zoom, true))),
            _ => {}
        }
    }
}

/// The children of an Artboard, plus its size.
pub fn svg(data: &[u8]) -> Result<(f32, f32, Vec<J>)> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| Error(format!("SVG: {e}")))?;
    let mut shapes = Vec::new();
    walk(tree.root(), Transform::identity(), 1.0, &mut shapes);
    // A number that is none (a scale beyond what a float holds) is written as `null`,
    // and nothing else here is.
    if J::Array(shapes.clone()).to_string().contains("null") {
        return Err(Error("SVG: a transform or a coordinate is too large to import".into()));
    }
    Ok((tree.size().width(), tree.size().height(), shapes))
}
