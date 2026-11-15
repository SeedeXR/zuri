//! Scene traversal: turns an instance into raster calls.
//!
// ponytail: draws straight into the CPU `Raster`. When the first GPU backend lands, lift the
// calls made here (push_clip / pop_clip / fill) into a trait; this file is the only caller.

use crate::file::{Artboard, Image};
use crate::geom::{self, bounds, pt, Cap, Contour, Join, Mat};
use crate::instance::{Instance, ShaderRequest};
use crate::model;
use crate::raster::{Paint, Raster};
use crate::schema::{self, p, t};
use std::sync::Arc;

/// Flattening tolerance in device pixels.
const TOLERANCE: f32 = 0.2;

enum Source {
    Solid(u32),
    Gradient { stops: Vec<(f32, u32)>, inv: Mat, p0: geom::P, p1: geom::P, radius: Option<f32> },
    Image { img: Arc<Image>, inv: Mat },
}

impl Source {
    fn paint(&self) -> Paint<'_> {
        match self {
            Source::Solid(c) => Paint::Solid(*c),
            Source::Gradient { stops, inv, p0, p1, radius } => Paint::Gradient { stops, inv: *inv, p0: *p0, p1: *p1, radius: *radius },
            Source::Image { img, inv } => Paint::Image { px: &img.px, w: img.w, h: img.h, inv: *inv, slice: None },
        }
    }
}

/// Maps a `w` x `h` box centered on the origin onto an `iw` x `ih` pixel grid.
fn box_to_pixels(w: f32, h: f32, iw: f32, ih: f32) -> Mat {
    let (sx, sy) = (iw / w, ih / h);
    Mat { a: sx, b: 0.0, c: 0.0, d: sy, tx: w * 0.5 * sx, ty: h * 0.5 * sy }
}

impl Instance {
    pub(crate) fn draw(&mut self, r: &mut Raster, px: &mut [u8], view: Mat, opacity: f32) {
        self.view = view;
        for n in 0..self.geo.len() as u32 {
            let ty = self.def().nodes[n as usize].ty;
            if self.geo[n as usize].is_some() {
                continue;
            }
            if ty == t::TEXT_NODE && cfg!(feature = "text") {
                self.lay_text(n);
            } else if schema::is_geometry(ty) || matches!(ty, t::TEXT_NODE | t::RUN) {
                self.geo[n as usize] = Some(self.geometry(n));
            }
        }
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];

        let saved = r.scissor();
        let mut clipped = false;
        if self.u(0, p::CLIP) != 0 {
            let (a, b) = (view.apply(pt(0.0, 0.0)), view.apply(pt(self.width(), self.height())));
            if view.b.abs() < 1e-6 && view.c.abs() < 1e-6 {
                // Axis-aligned: a pixel-snapped scissor is exact enough and much cheaper than a mask.
                let rect = [a.x.min(b.x).round() as i32, a.y.min(b.y).round() as i32, a.x.max(b.x).round() as i32, a.y.max(b.y).round() as i32];
                r.set_scissor([rect[0].max(saved[0]), rect[1].max(saved[1]), rect[2].min(saved[2]), rect[3].min(saved[3])]);
            } else {
                let mut cs = self.contours(0, TOLERANCE);
                geom::transform_contours(&mut cs, &view);
                r.push_clip(&cs);
                clipped = true;
            }
        }
        self.draw_node(ab, 0, r, px, &view, opacity);
        if clipped {
            r.pop_clip();
        }
        r.set_scissor(saved);
    }

    fn draw_node(&mut self, ab: &Artboard, n: u32, r: &mut Raster, px: &mut [u8], view: &Mat, opacity: f32) {
        let node = &ab.nodes[n as usize];
        let opacity = if schema::is_drawable(node.ty) { opacity * self.f(n, p::OPACITY).clamp(0.0, 1.0) } else { opacity };
        if opacity <= 0.0 {
            return;
        }
        let m = view.mul(&self.world[n as usize]);

        let mut clips = 0;
        for &c in &node.children {
            if ab.nodes[c as usize].ty == t::CLIP_NODE {
                if let Some(src) = self.node_ref(c, p::SOURCE) {
                    let sm = view.mul(&self.world[src as usize]);
                    let mut cs = self.contours(src, TOLERANCE / sm.max_scale().max(1e-6));
                    geom::transform_contours(&mut cs, &sm);
                    r.push_clip(&cs);
                    clips += 1;
                }
            }
        }

        match node.ty {
            t::ARTBOARD | t::SHAPE | t::LAYOUT => self.paint_shape(ab, &[n], n, r, px, &m, opacity),
            // Without the text feature a Text has no outline: nothing to paint.
            t::TEXT_NODE if cfg!(feature = "text") => {
                // The Text's paints paint the Runs that have none of their own (all of the
                // text, without Runs); then each Run with paints is painted by those, on top.
                let styled = |run: &u32| ab.nodes[*run as usize].children.iter().any(|g| matches!(ab.nodes[*g as usize].ty, t::FILL | t::STROKE));
                let (mut own, mut plain) = (Vec::new(), Vec::new());
                for run in ab.runs(n) {
                    if styled(&run) { &mut own } else { &mut plain }.push(run);
                }
                if own.is_empty() && plain.is_empty() {
                    plain.push(n);
                }
                self.paint_shape(ab, &plain, n, r, px, &m, opacity);
                for run in own {
                    self.paint_shape(ab, &[run], run, r, px, &m, opacity);
                }
            }
            t::IMAGE => {
                let img = self.file.images.lock().unwrap_or_else(|e| e.into_inner()).get(&self.u(n, p::ASSET)).cloned();
                let (w, h) = self.box_size(n);
                if let (Some(img), Some(inv), true) = (img, m.invert(), w > 0.0 && h > 0.0) {
                    let mut cs = self.contours(n, TOLERANCE);
                    geom::transform_contours(&mut cs, &m);
                    // Nine-slice: the slices are in pixels of the picture as the document
                    // describes it (the asset's width x height), and one such pixel is one
                    // artboard unit. The paint works in pixels of the bitmap the host
                    // decoded, which may be a different size: scale the box into those.
                    let asset = self.file.objects.get(self.u(n, p::ASSET) as usize);
                    let per = |key: u16, decoded: u32| asset.and_then(|a| a.float(key)).filter(|v| *v > 0.0).map_or(1.0, |v| decoded as f32 / v);
                    let (fx, fy) = (per(p::WIDTH, img.w), per(p::HEIGHT, img.h));
                    let s = [p::SLICE_LEFT, p::SLICE_RIGHT, p::SLICE_TOP, p::SLICE_BOTTOM].map(|key| self.f(n, key).max(0.0));
                    let slice = s.iter().any(|v| *v > 0.0).then_some([(s[0] * fx, s[1] * fx, w * fx), (s[2] * fy, s[3] * fy, h * fy)]);
                    // Without slices the box is simply the whole image.
                    let (bw, bh) = if slice.is_some() { (w * fx, h * fy) } else { (img.w as f32, img.h as f32) };
                    let inv = box_to_pixels(w, h, bw, bh).mul(&inv);
                    r.fill(px, &cs, false, &Paint::Image { px: &img.px, w: img.w, h: img.h, inv, slice }, opacity, 0, 0.0, None);
                }
            }
            t::MODEL => {
                if let Some(mesh) = self.file.meshes.get(&self.u(n, p::ASSET)) {
                    let rot = [self.f(n, p::ROTATION_X), self.f(n, p::ROTATION_Y), self.f(n, p::ROTATION_Z)];
                    let (w, h) = self.box_size(n);
                    if let Some(out) = model::render(mesh, rot, self.f(n, p::FOV), w, h, &m, r.scissor()) {
                        let (x0, y0, x1, y1) = (out.x as f32, out.y as f32, (out.x + out.w as i32) as f32, (out.y + out.h as i32) as f32);
                        let cs = [Contour { pts: vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)], closed: true }];
                        let inv = Mat { tx: -x0, ty: -y0, ..Mat::IDENTITY };
                        r.fill(px, &cs, false, &Paint::Image { px: &out.px, w: out.w, h: out.h, inv, slice: None }, opacity, 0, 0.0, None);
                    }
                }
            }
            t::NESTED_ARTBOARD => {
                let mut nested = std::mem::take(&mut self.nested);
                if let Some((_, child)) = nested.iter_mut().find(|(node, _)| *node == n) {
                    // Shader textures and requests live on the root instance (the one the host
                    // talks to): lend the textures down, collect the requests up.
                    // ponytail: instances of one component share a texture per ShaderPaint (the
                    // first one's size); key by instance path if they must differ.
                    std::mem::swap(&mut child.textures, &mut self.textures);
                    child.draw(r, px, m, opacity);
                    std::mem::swap(&mut child.textures, &mut self.textures);
                    for req in child.requests.drain(..) {
                        if !self.requests.iter().any(|q| q.paint == req.paint) {
                            self.requests.push(req);
                        }
                    }
                }
                self.nested = nested;
            }
            _ => {}
        }

        for &c in self.drawn_children(ab, n).iter() {
            self.draw_node(ab, c, r, px, view, opacity);
        }
        for _ in 0..clips {
            r.pop_clip();
        }
    }

    /// Paint the outlines of `outlines`, taken together, with the Fill and Stroke children of `n`, in order.
    #[allow(clippy::too_many_arguments)]
    fn paint_shape(&mut self, ab: &Artboard, outlines: &[u32], n: u32, r: &mut Raster, px: &mut [u8], m: &Mat, opacity: f32) {
        let scale = m.max_scale().max(1e-6);
        let tol = TOLERANCE / scale;
        let mut local = Vec::new();
        for &o in outlines {
            local.extend(self.contours(o, tol));
        }
        // The box the paints work in is that of `n`: the whole Text for the Text's paints,
        // whichever of its Runs they end up painting.
        let lb = if outlines == [n] { bounds(&local) } else { bounds(&self.contours(n, tol)) };
        let (Some(lb), false) = (lb, local.is_empty()) else { return };
        let blend = self.u(n, p::BLEND_MODE);
        let mut filled: Option<Vec<Contour>> = None;
        for &c in &ab.nodes[n as usize].children {
            let ty = ab.nodes[c as usize].ty;
            if ty != t::FILL && ty != t::STROKE {
                continue;
            }
            let feather = self.f(c, p::FEATHER).max(0.0) * scale;
            let inner = ty == t::FILL && self.u(c, p::FEATHER_INNER) != 0;
            // The feather's offset in pixels: it turns and scales with the shape. An outward
            // feather moves by it, paint and all (a drop shadow). An inner one stays where
            // the shape is, and the offset moves what is taken away from it.
            let (fx, fy) = (self.f(c, p::FEATHER_X), self.f(c, p::FEATHER_Y));
            let by = pt(m.a * fx + m.c * fy, m.b * fx + m.d * fy);
            let moves = !inner && (fx != 0.0 || fy != 0.0);
            let pm = if moves { Mat { tx: m.tx + by.x, ty: m.ty + by.y, ..*m } } else { *m };
            let Some(source) = self.source(ab, c, &pm, lb) else { continue };
            if ty == t::FILL {
                let mut moved;
                let dev = if moves {
                    moved = local.clone();
                    geom::transform_contours(&mut moved, &pm);
                    &moved
                } else {
                    filled.get_or_insert_with(|| {
                        let mut dev = local.clone();
                        geom::transform_contours(&mut dev, m);
                        dev
                    })
                };
                r.fill(px, dev, self.u(c, p::FILL_RULE) == 1, &source.paint(), opacity, blend, feather, inner.then_some(by));
            } else {
                let (start, end, offset) = (self.f(c, p::TRIM_START), self.f(c, p::TRIM_END), self.f(c, p::TRIM_OFFSET));
                let mut trimmed = Vec::new();
                let path = if start != 0.0 || end != 1.0 {
                    // `synced` trims each path by itself; otherwise they are one long path.
                    for cs in local.chunks(if self.u(c, p::TRIM_MODE) == 1 { 1 } else { local.len() }) {
                        trimmed.extend(geom::trim(cs, start, end, offset));
                    }
                    &trimmed
                } else {
                    &local
                };
                let dashed;
                let (on, off) = (self.f(c, p::DASH), self.f(c, p::GAP));
                let path = if on > 0.0 {
                    dashed = geom::dash(path, on, if off > 0.0 { off } else { on }, self.f(c, p::DASH_OFFSET));
                    &dashed
                } else {
                    path
                };
                let cap = [Cap::Butt, Cap::Round, Cap::Square][self.u(c, p::CAP).min(2) as usize];
                let join = [Join::Miter, Join::Round, Join::Bevel][self.u(c, p::JOIN).min(2) as usize];
                let mut outline = Vec::new();
                // Stroke in the shape's own space so non-uniform scale stretches the pen too.
                geom::stroke(path, self.f(c, p::THICKNESS), cap, join, tol, &mut outline);
                geom::transform_contours(&mut outline, &pm);
                r.fill(px, &outline, false, &source.paint(), opacity, blend, feather, None);
            }
        }
    }

    /// What a Fill / Stroke paints with. `lb` is the local bounding box of the outline.
    fn source(&mut self, ab: &Artboard, paint: u32, m: &Mat, lb: [f32; 4]) -> Option<Source> {
        let color = self.c(paint, p::COLOR);
        for &g in &ab.nodes[paint as usize].children {
            let ty = ab.nodes[g as usize].ty;
            match ty {
                t::LINEAR_GRADIENT | t::RADIAL_GRADIENT => {
                    let mut stops: Vec<(f32, u32)> = ab.nodes[g as usize]
                        .children
                        .iter()
                        .filter(|s| ab.nodes[**s as usize].ty == t::GRADIENT_STOP)
                        .map(|s| (self.f(*s, p::POSITION), self.c(*s, p::COLOR)))
                        .collect();
                    geom::sort_keyed(&mut stops);
                    let radius = (ty == t::RADIAL_GRADIENT).then(|| self.f(g, p::RADIUS));
                    return Some(Source::Gradient {
                        stops,
                        inv: m.invert()?,
                        p0: pt(self.f(g, p::X0), self.f(g, p::Y0)),
                        p1: pt(self.f(g, p::X1), self.f(g, p::Y1)),
                        radius,
                    });
                }
                t::SHADER_PAINT => {
                    let (bw, bh) = (lb[2] - lb[0], lb[3] - lb[1]);
                    if bw <= 0.0 || bh <= 0.0 {
                        return None;
                    }
                    let obj = ab.nodes[g as usize].obj;
                    let scale = m.max_scale();
                    self.requests.push(ShaderRequest {
                        paint: obj,
                        asset: self.u(g, p::ASSET),
                        width: ((bw * scale).ceil() as u32).clamp(1, 4096),
                        height: ((bh * scale).ceil() as u32).clamp(1, 4096),
                        params: [self.f(g, p::P0), self.f(g, p::P1), self.f(g, p::P2), self.f(g, p::P3)],
                        time: self.time as f32,
                    });
                    // Until the host supplies pixels, fall back to the paint's own color.
                    let Some(img) = self.textures.get(&obj).cloned() else { break };
                    let to_px =
                        Mat { a: img.w as f32 / bw, b: 0.0, c: 0.0, d: img.h as f32 / bh, tx: -lb[0] * img.w as f32 / bw, ty: -lb[1] * img.h as f32 / bh };
                    return Some(Source::Image { inv: to_px.mul(&m.invert()?), img });
                }
                _ => {}
            }
        }
        Some(Source::Solid(color))
    }
}
