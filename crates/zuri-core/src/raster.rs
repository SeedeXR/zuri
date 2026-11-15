//! Reference CPU renderer: exact-area anti-aliased coverage + compositing into
//! premultiplied RGBA8. Deterministic, dependency-free, runs anywhere (including wasm),
//! and is the image every other backend is compared against.
//!
//! Coverage uses a signed-area accumulation buffer (the technique of font-rs / stb_truetype):
//! each edge deposits its signed area per pixel, and a running sum along the row yields
//! exact fractional coverage with no supersampling.
//!
// ponytail: cost is O(pixels in the shape's bounding box) per paint. Fine for UI-sized
// content; for full-screen 4K the upgrade path is sparse strips with GPU compositing
// (see docs/dev/rendering.md), fed by the same contours.

use crate::geom::{bounds, within, Contour, Mat, P};

pub enum Paint<'a> {
    /// Straight (non-premultiplied) 0xAARRGGBB.
    Solid(u32),
    /// `inv` maps device pixels to the space `p0`/`p1` are in. Radial when `radius` is set
    /// (center `p0`). `stops` are (position, 0xAARRGGBB), sorted by position.
    Gradient { stops: &'a [(f32, u32)], inv: Mat, p0: P, p1: P, radius: Option<f32> },
    /// Premultiplied RGBA8 pixels; `inv` maps device pixels to image pixels. With `slice`
    /// the image is nine-sliced into a box, and `inv` maps device pixels into that box:
    /// per axis (x, then y), `(start, end, size)` are the lengths of the two ends that keep
    /// their size and of the box, all in image pixels.
    Image { px: &'a [u8], w: u32, h: u32, inv: Mat, slice: Option<[(f32, f32, f32); 2]> },
}

#[derive(Default)]
pub struct Raster {
    w: usize,
    h: usize,
    scissor: [i32; 4],
    acc: Vec<f32>,
    mask: Vec<u8>,
    tmp: Vec<u8>,
    clips: Vec<Vec<u8>>,
    spare: Vec<Vec<u8>>,
    /// Gradient ramp, premultiplied, 8.8 fixed point (see `build_lut`).
    lut: Vec<[u16; 4]>,
}

#[inline]
fn div255(x: u32) -> u32 {
    (x + 128 + ((x + 128) >> 8)) >> 8
}

/// Straight ARGB -> premultiplied [r, g, b, a], with extra opacity applied.
fn premul(argb: u32, opacity: f32) -> [u8; 4] {
    let a = ((argb >> 24) as f32 * opacity.clamp(0.0, 1.0) + 0.5) as u32;
    let ch = |shift: u32| div255(((argb >> shift) & 0xff) * a) as u8;
    [ch(16), ch(8), ch(0), a as u8]
}

impl Raster {
    /// Start a frame on a `w` x `h` surface: full scissor, no clips.
    pub fn begin(&mut self, w: u32, h: u32) {
        self.w = w as usize;
        self.h = h as usize;
        self.scissor = [0, 0, w as i32, h as i32];
        self.spare.append(&mut self.clips);
    }

    pub fn scissor(&self) -> [i32; 4] {
        self.scissor
    }
    /// Restrict drawing to a pixel rectangle (intersected with the surface).
    pub fn set_scissor(&mut self, s: [i32; 4]) {
        self.scissor = [s[0].max(0), s[1].max(0), s[2].min(self.w as i32), s[3].min(self.h as i32)];
    }

    /// Intersect the current clip with the interior of `cs` (non-zero rule).
    pub fn push_clip(&mut self, cs: &[Contour]) {
        let (w, h) = (self.w, self.h);
        let mut m = self.spare.pop().unwrap_or_default();
        m.clear();
        m.resize(w * h, 0);
        let [x0, y0, x1, y1] = self.scissor;
        if x0 < x1 && y0 < y1 {
            let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
            self.coverage(cs, x0 as f32, y0 as f32, rw, rh, false);
            let prev = self.clips.last();
            for y in 0..rh {
                let row = (y + y0 as usize) * w + x0 as usize;
                for x in 0..rw {
                    let c = self.mask[y * rw + x] as u32;
                    m[row + x] = match prev {
                        Some(p) => div255(c * p[row + x] as u32) as u8,
                        None => c as u8,
                    };
                }
            }
        }
        self.clips.push(m);
    }

    pub fn pop_clip(&mut self) {
        if let Some(m) = self.clips.pop() {
            self.spare.push(m);
        }
    }

    /// Rasterize `cs` into `self.mask` (`rw` x `rh`, origin at device position `ox, oy`).
    fn coverage(&mut self, cs: &[Contour], ox: f32, oy: f32, rw: usize, rh: usize, even_odd: bool) {
        let stride = rw + 2;
        self.acc.clear();
        self.acc.resize(stride * rh, 0.0);
        for c in cs {
            let n = c.pts.len();
            for i in 0..n {
                let (a, b) = (c.pts[i], c.pts[(i + 1) % n]);
                edge(&mut self.acc, rw, rh, P { x: a.x - ox, y: a.y - oy }, P { x: b.x - ox, y: b.y - oy });
            }
        }
        self.mask.clear();
        self.mask.resize(rw * rh, 0);
        for y in 0..rh {
            let mut sum = 0.0f32;
            let (row, out) = (&self.acc[y * stride..y * stride + rw], &mut self.mask[y * rw..(y + 1) * rw]);
            for (a, m) in row.iter().zip(out) {
                sum += a;
                let cov = if even_odd {
                    let t = sum.abs() % 2.0;
                    if t > 1.0 {
                        2.0 - t
                    } else {
                        t
                    }
                } else {
                    sum.abs().min(1.0)
                };
                *m = (cov * 255.0 + 0.5) as u8;
            }
        }
    }

    /// Fill `cs` into the premultiplied RGBA8 surface `px` (`w * h * 4` bytes from `begin`).
    /// `feather` is a blur radius in pixels; `blend` indexes the schema's `blendMode` enum.
    /// With `inner`, the paint is kept inside `cs` and fades inward from its edge: what is
    /// left of `cs` after taking away its feathered self, moved by that many pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(&mut self, px: &mut [u8], cs: &[Contour], even_odd: bool, paint: &Paint, opacity: f32, blend: u32, feather: f32, inner: Option<P>) {
        let Some(b) = bounds(cs) else { return };
        if opacity <= 0.0 || px.len() < self.w * self.h * 4 {
            return;
        }
        // Three box blurs of radius r approximate a gaussian of sigma = feather / 2.
        let sigma = feather.clamp(0.0, 256.0) * 0.5;
        let r = if sigma > 0.0 { (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) * 0.5).round() as i32 } else { 0 };
        let pad = 3 * r;
        // An outward feather reaches `pad` beyond the outline; an inner one stays inside it.
        let reach = if inner.is_some() { 0 } else { pad };
        let s = self.scissor;
        let x0 = (b[0].floor() as i32).saturating_sub(reach).max(s[0]);
        let y0 = (b[1].floor() as i32).saturating_sub(reach).max(s[1]);
        let x1 = (b[2].ceil() as i32).saturating_add(reach).min(s[2]);
        let y1 = (b[3].ceil() as i32).saturating_add(reach).min(s[3]);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        // With feather, coverage just outside the visible region still bleeds in.
        let (rx0, ry0) = (x0 - pad, y0 - pad);
        let (rw, rh) = ((x1 - x0 + 2 * pad) as usize, (y1 - y0 + 2 * pad) as usize);
        // Moving the origin back is moving the outline forward.
        let by = inner.unwrap_or(P { x: 0.0, y: 0.0 });
        self.coverage(cs, rx0 as f32 - by.x, ry0 as f32 - by.y, rw, rh, even_odd);
        if r > 0 {
            self.tmp.clear();
            self.tmp.resize(rw * rh, 0);
            let r = r as usize;
            for horizontal in [true, false] {
                blur_pass(&self.mask, &mut self.tmp, rw, rh, r, horizontal);
                blur_pass(&self.tmp, &mut self.mask, rw, rh, r, horizontal);
                blur_pass(&self.mask, &mut self.tmp, rw, rh, r, horizontal);
                std::mem::swap(&mut self.mask, &mut self.tmp);
            }
        }
        if inner.is_some() {
            std::mem::swap(&mut self.mask, &mut self.tmp);
            self.coverage(cs, rx0 as f32, ry0 as f32, rw, rh, even_odd);
            for (m, soft) in self.mask.iter_mut().zip(&self.tmp) {
                *m = div255(*m as u32 * (255 - *soft as u32)) as u8;
            }
        }

        let solid = match paint {
            Paint::Solid(c) => Some(premul(*c, opacity)),
            Paint::Gradient { stops, .. } => {
                build_lut(&mut self.lut, stops, opacity);
                None
            }
            Paint::Image { .. } => None,
        };
        let op = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
        let clip = self.clips.last();
        let w = self.w;
        for y in y0..y1 {
            let mrow = &self.mask[(y - ry0) as usize * rw + (x0 - rx0) as usize..][..(x1 - x0) as usize];
            let drow = y as usize * w + x0 as usize;
            // Device pixel centre of the first pixel of this row, mapped into paint space.
            let (fx, fy) = (x0 as f32 + 0.5, y as f32 + 0.5);
            let (mut lx, mut ly, dx, dy) = match paint {
                Paint::Gradient { inv, .. } | Paint::Image { inv, .. } => {
                    let p = inv.apply(P { x: fx, y: fy });
                    (p.x, p.y, inv.a, inv.b)
                }
                Paint::Solid(_) => (0.0, 0.0, 0.0, 0.0),
            };
            for (i, m) in mrow.iter().enumerate() {
                let mut m = *m as u32;
                if let Some(c) = clip {
                    m = div255(m * c[drow + i] as u32);
                }
                if m != 0 {
                    let src = match (solid, paint) {
                        (Some(c), _) => c,
                        (_, Paint::Gradient { p0, p1, radius, .. }) => {
                            let t = match radius {
                                Some(r) => ((lx - p0.x).powi(2) + (ly - p0.y).powi(2)).sqrt() / r.max(1e-6),
                                None => {
                                    let (gx, gy) = (p1.x - p0.x, p1.y - p0.y);
                                    ((lx - p0.x) * gx + (ly - p0.y) * gy) / (gx * gx + gy * gy).max(1e-12)
                                }
                            };
                            // Dither: the ramp keeps 8 fractional bits, and where a pixel sits
                            // decides which way it rounds; without it a slow ramp shows as
                            // bands. The threshold steps by an irrational-like amount along
                            // both axes (as interleaved gradient noise does), so every row and
                            // column of an axis-aligned gradient averages right; a Bayer matrix
                            // does not, and draws lines. Both steps are odd, so a row or a
                            // column meets all 256 thresholds. One threshold for all four
                            // channels keeps color <= alpha; a whole value never changes.
                            let d = ((x0 as u32 + i as u32) * 143 + y as u32 * 79) as u16 & 255;
                            self.lut[(t.clamp(0.0, 1.0) * 255.0 + 0.5) as usize].map(|c| ((c + d) >> 8) as u8)
                        }
                        (_, Paint::Image { px: img, w: iw, h: ih, slice, .. }) => {
                            // ponytail: two `stretch` calls per pixel. Work out a row's y once
                            // (when the box is not rotated) if large sliced panels show up in a profile.
                            let (sx, sy) = match slice {
                                Some([x, y]) => (stretch(lx, *x, *iw as f32), stretch(ly, *y, *ih as f32)),
                                None => (lx, ly),
                            };
                            let c = sample(img, *iw as usize, *ih as usize, sx, sy);
                            [div255(c[0] as u32 * op) as u8, div255(c[1] as u32 * op) as u8, div255(c[2] as u32 * op) as u8, div255(c[3] as u32 * op) as u8]
                        }
                        (None, Paint::Solid(_)) => unreachable!(),
                    };
                    let d = &mut px[(drow + i) * 4..(drow + i) * 4 + 4];
                    let s = [div255(src[0] as u32 * m), div255(src[1] as u32 * m), div255(src[2] as u32 * m), div255(src[3] as u32 * m)];
                    if blend == 0 {
                        let inv = 255 - s[3];
                        for ch in 0..4 {
                            d[ch] = (s[ch] + div255(d[ch] as u32 * inv)) as u8;
                        }
                    } else {
                        blend_px(d, s, blend);
                    }
                }
                lx += dx;
                ly += dy;
            }
        }
    }
}

/// Deposit the signed area of edge `a -> b` into the accumulation buffer, clipped to
/// `0..=w` horizontally. Edges left of the buffer still contribute winding from x = 0.
fn edge(acc: &mut [f32], w: usize, h: usize, a: P, b: P) {
    if a.y == b.y || !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return;
    }
    let wf = w as f32;
    let inside = |x: f32| (0.0..=wf).contains(&x);
    if inside(a.x) && inside(b.x) {
        return seg(acc, w, h, a, b);
    }
    // Split where the edge crosses x = 0 and x = w; clamp each piece into range.
    let mut ts = [0.0f32, 1.0, 1.0, 1.0];
    let mut n = 1;
    for bound in [0.0, wf] {
        let t = (bound - a.x) / (b.x - a.x);
        if t > 0.0 && t < 1.0 {
            ts[n] = t;
            n += 1;
        }
    }
    if n == 3 && ts[1] > ts[2] {
        ts.swap(1, 2);
    }
    ts[n] = 1.0;
    for i in 0..n {
        let at = |t: f32| P { x: within(a.x + (b.x - a.x) * t, 0.0, wf), y: a.y + (b.y - a.y) * t };
        let (p, q) = (at(ts[i]), at(ts[i + 1]));
        // A piece wholly outside collapses to a vertical edge on the boundary.
        let mid = a.x + (b.x - a.x) * (ts[i] + ts[i + 1]) * 0.5;
        if mid < 0.0 {
            seg(acc, w, h, P { x: 0.0, y: p.y }, P { x: 0.0, y: q.y });
        } else if mid > wf {
            seg(acc, w, h, P { x: wf, y: p.y }, P { x: wf, y: q.y });
        } else {
            seg(acc, w, h, p, q);
        }
    }
}

/// Signed-area accumulation of one edge whose x lies within `0..=w`. Rows are `w + 2` wide.
fn seg(acc: &mut [f32], w: usize, h: usize, p0: P, p1: P) {
    let stride = w + 2;
    let wf = w as f32;
    let (dir, p0, p1) = if p0.y < p1.y { (1.0, p0, p1) } else { (-1.0, p1, p0) };
    let dxdy = (p1.x - p0.x) / (p1.y - p0.y);
    let mut x = p0.x;
    if p0.y < 0.0 {
        x -= p0.y * dxdy;
    }
    let y_start = p0.y.max(0.0) as usize;
    let y_end = (p1.y.ceil().max(0.0) as usize).min(h);
    for y in y_start..y_end {
        let row = y * stride;
        let dy = ((y + 1) as f32).min(p1.y) - (y as f32).max(p0.y);
        let xnext = x + dxdy * dy;
        let d = dy * dir;
        let x0 = within(x.min(xnext), 0.0, wf);
        let x1 = within(x.max(xnext), x0, wf);
        let x0floor = x0.floor();
        let x0i = x0floor as usize;
        let x1ceil = x1.ceil();
        let x1i = x1ceil as usize;
        if x1i <= x0i + 1 {
            let xmf = 0.5 * (x0 + x1) - x0floor;
            acc[row + x0i] += d - d * xmf;
            acc[row + x0i + 1] += d * xmf;
        } else {
            let s = 1.0 / (x1 - x0);
            let x0f = x0 - x0floor;
            let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
            let x1f = x1 - x1ceil + 1.0;
            let am = 0.5 * s * x1f * x1f;
            acc[row + x0i] += d * a0;
            if x1i == x0i + 2 {
                acc[row + x0i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - x0f);
                acc[row + x0i + 1] += d * (a1 - a0);
                for xi in x0i + 2..x1i - 1 {
                    acc[row + xi] += d * s;
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                acc[row + x1i - 1] += d * (1.0 - a2 - am);
            }
            acc[row + x1i] += d * am;
        }
        x = xnext;
    }
}

/// One box-blur pass of radius `r` along rows (`horizontal`) or columns.
fn blur_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len, step, line_step) = if horizontal { (h, w, 1, w) } else { (w, h, w, 1) };
    let div = (2 * r + 1) as u32;
    for l in 0..lines {
        let base = l * line_step;
        let mut sum: u32 = (0..=r.min(len - 1)).map(|i| src[base + i * step] as u32).sum();
        for i in 0..len {
            dst[base + i * step] = ((sum + div / 2) / div) as u8;
            if i + r + 1 < len {
                sum += src[base + (i + r + 1) * step] as u32;
            }
            if i >= r {
                sum -= src[base + (i - r) * step] as u32;
            }
        }
    }
}

/// 256-entry premultiplied color ramp for a gradient, in 8.8 fixed point (255 = 0xff00):
/// the fraction is what `fill` dithers with.
fn build_lut(lut: &mut Vec<[u16; 4]>, stops: &[(f32, u32)], opacity: f32) {
    lut.clear();
    for i in 0..256 {
        let t = i as f32 / 255.0;
        let c = match stops {
            [] => [0; 4],
            [only] => premul(only.1, opacity).map(|v| (v as u16) << 8),
            _ => {
                let hi = stops.iter().position(|s| s.0 >= t).unwrap_or(stops.len() - 1).max(1);
                let (a, b) = (stops[hi - 1], stops[hi]);
                let f = if b.0 > a.0 { ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 1.0 };
                let (pa, pb) = (premul(a.1, opacity), premul(b.1, opacity));
                // Interpolating premultiplied colors avoids dark fringes toward transparent stops.
                std::array::from_fn(|ch| ((pa[ch] as f32 + (pb[ch] as f32 - pa[ch] as f32) * f) * 256.0 + 0.5) as u16)
            }
        };
        lut.push(c);
    }
}

/// Nine-slice along one axis: where position `v` in a box `size` long falls in an image
/// `img` pixels long whose first `a` and last `b` pixels keep their size. Ends that are
/// longer than the image or the box are shortened in proportion, so nothing jumps as a
/// box shrinks. Each of the three parts samples within itself (to its outermost pixel
/// centres), so the stretched middle does not pick up the colour of the end beside it.
fn stretch(v: f32, (a, b, size): (f32, f32, f32), img: f32) -> f32 {
    let fit = (img / (a + b).max(1e-6)).min(1.0);
    let (a, b) = (a * fit, b * fit);
    // How much of each end the box has room for, and the ends as drawn.
    let k = (size / (a + b).max(1e-6)).min(1.0);
    let (da, db) = (a * k, b * k);
    if v < da {
        (v / k).min(a - 0.5)
    } else if v > size - db {
        (img - (size - v) / k).max(img - b + 0.5)
    } else {
        // max then min, not clamp: a middle under one pixel wide has no room between them.
        (a + (v - da) * (img - a - b) / (size - da - db).max(1e-6)).max(a + 0.5).min(img - b - 0.5)
    }
}

/// Bilinear sample of a premultiplied RGBA8 image, clamped at the edges.
fn sample(px: &[u8], w: usize, h: usize, x: f32, y: f32) -> [u8; 4] {
    if w == 0 || h == 0 || px.len() < w * h * 4 {
        return [0; 4];
    }
    let (x, y) = (within(x - 0.5, 0.0, (w - 1) as f32), within(y - 0.5, 0.0, (h - 1) as f32));
    let (xi, yi) = (x as usize, y as usize);
    let (xj, yj) = ((xi + 1).min(w - 1), (yi + 1).min(h - 1));
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let at = |x: usize, y: usize, ch: usize| px[(y * w + x) * 4 + ch] as f32;
    std::array::from_fn(|ch| {
        let top = at(xi, yi, ch) + (at(xj, yi, ch) - at(xi, yi, ch)) * fx;
        let bot = at(xi, yj, ch) + (at(xj, yj, ch) - at(xi, yj, ch)) * fx;
        (top + (bot - top) * fy + 0.5) as u8
    })
}

/// The W3C non-separable blend modes (hue, saturation, color, luminosity = 12..=15) of
/// straight colors `cs` over `cb`: they mix the hue, saturation and luminosity of the two.
fn blend_hsl(cs: [f32; 3], cb: [f32; 3], mode: u32) -> [f32; 3] {
    let lum = |c: [f32; 3]| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let sat = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
    let set_lum = |c: [f32; 3], l: f32| {
        let c = c.map(|v| v + l - lum(c));
        let (lo, hi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
        // Pull out-of-range channels back toward the luminosity, keeping the hue.
        c.map(|v| if lo < 0.0 { l + (v - l) * l / (l - lo) } else { v }).map(|v| if hi > 1.0 { l + (v - l) * (1.0 - l) / (hi - l) } else { v })
    };
    let set_sat = |c: [f32; 3], s: f32| {
        let (lo, hi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
        c.map(|v| if hi > lo { (v - lo) * s / (hi - lo) } else { 0.0 })
    };
    match mode {
        12 => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        13 => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        14 => set_lum(cs, lum(cb)),
        _ => set_lum(cb, lum(cs)),
    }
}

/// W3C blend modes on premultiplied pixels. `mode` > 0 indexes `blendMode`.
fn blend_px(d: &mut [u8], s: [u32; 4], mode: u32) {
    let (sa, da) = (s[3] as f32 / 255.0, d[3] as f32 / 255.0);
    let straight = |p: u8, a: f32| if a > 0.0 { (p as f32 / 255.0 / a).min(1.0) } else { 0.0 };
    let hsl = (12..=15).contains(&mode).then(|| blend_hsl([0, 1, 2].map(|ch| straight(s[ch] as u8, sa)), [0, 1, 2].map(|ch| straight(d[ch], da)), mode));
    for ch in 0..3 {
        let (cs_p, cb_p) = (s[ch] as f32 / 255.0, d[ch] as f32 / 255.0);
        let (cs, cb) = (straight(s[ch] as u8, sa), straight(d[ch], da));
        let hard_light = |cb: f32, cs: f32| if cs <= 0.5 { cb * 2.0 * cs } else { cb + (2.0 * cs - 1.0) - cb * (2.0 * cs - 1.0) };
        let b = match mode {
            1 => cb * cs,
            2 => cb + cs - cb * cs,
            3 => hard_light(cs, cb),
            4 => cb.min(cs),
            5 => cb.max(cs),
            6 => {
                if cb == 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            7 => {
                if cb >= 1.0 {
                    1.0
                } else if cs <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            8 => hard_light(cb, cs),
            9 => {
                if cs <= 0.5 {
                    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
                } else {
                    let g = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                    cb + (2.0 * cs - 1.0) * (g - cb)
                }
            }
            10 => (cb - cs).abs(),
            11 => cb + cs - 2.0 * cb * cs,
            _ => hsl.map_or(cs, |c| c[ch]),
        };
        let out = (1.0 - da) * cs_p + (1.0 - sa) * cb_p + sa * da * b;
        d[ch] = (out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    d[3] = ((sa + da - sa * da).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

/// Fill the whole surface with a straight 0xAARRGGBB color.
pub fn clear(px: &mut [u8], argb: u32) {
    px.as_chunks_mut::<4>().0.fill(premul(argb, 1.0));
}

/// Convert straight-alpha RGBA8 to premultiplied in place (what hosts hand in).
pub fn premultiply(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
}

/// Convert premultiplied RGBA8 to straight alpha in place (what Canvas `ImageData`,
/// PNG and most engine texture formats expect).
pub fn unpremultiply(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Contour> {
        vec![Contour { pts: vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)], closed: true }]
    }
    fn surface(w: u32, h: u32) -> (Raster, Vec<u8>) {
        let mut r = Raster::default();
        r.begin(w, h);
        (r, vec![0; (w * h * 4) as usize])
    }
    fn alpha(px: &[u8], w: u32, x: u32, y: u32) -> u8 {
        px[((y * w + x) * 4 + 3) as usize]
    }

    #[test]
    fn coverage_is_exact_area() {
        let (mut r, mut px) = surface(8, 8);
        // Covers pixel (2,2) fully, half of column 3, a quarter of row 4.
        r.fill(&mut px, &square(2.0, 2.0, 3.5, 4.25), false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!(alpha(&px, 8, 2, 2), 255);
        assert_eq!(alpha(&px, 8, 3, 2), 128);
        assert_eq!(alpha(&px, 8, 2, 4), 64);
        assert_eq!(alpha(&px, 8, 3, 4), 32);
        assert_eq!(alpha(&px, 8, 4, 2), 0);
        assert_eq!(alpha(&px, 8, 1, 2), 0);
    }

    #[test]
    fn shapes_crossing_the_surface_edges_are_clipped_not_dropped() {
        let (mut r, mut px) = surface(4, 4);
        r.fill(&mut px, &square(-100.0, -100.0, 100.0, 100.0), false, &Paint::Solid(0xff0000ff), 1.0, 0, 0.0, None);
        assert!(px.chunks(4).all(|p| p == [0, 0, 255, 255]));
        let (mut r, mut px) = surface(4, 4);
        r.fill(&mut px, &square(-10.0, 1.0, 2.0, 3.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!((alpha(&px, 4, 0, 1), alpha(&px, 4, 1, 2), alpha(&px, 4, 2, 1), alpha(&px, 4, 0, 0)), (255, 255, 0, 0));
        let (mut r, mut px) = surface(4, 4);
        r.fill(&mut px, &square(2.0, -50.0, 90.0, 50.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!((alpha(&px, 4, 1, 0), alpha(&px, 4, 2, 0), alpha(&px, 4, 3, 3)), (0, 255, 255));
    }

    #[test]
    fn fill_rules_differ_on_nested_same_direction_squares() {
        let mut cs = square(0.0, 0.0, 8.0, 8.0);
        cs.extend(square(2.0, 2.0, 6.0, 6.0));
        let (mut r, mut px) = surface(8, 8);
        r.fill(&mut px, &cs, false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!(alpha(&px, 8, 4, 4), 255);
        let (mut r, mut px) = surface(8, 8);
        r.fill(&mut px, &cs, true, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!((alpha(&px, 8, 4, 4), alpha(&px, 8, 1, 1)), (0, 255));
    }

    #[test]
    fn source_over_and_opacity() {
        let (mut r, mut px) = surface(2, 1);
        clear(&mut px, 0xff000000);
        r.fill(&mut px, &square(0.0, 0.0, 1.0, 1.0), false, &Paint::Solid(0xffffffff), 0.5, 0, 0.0, None);
        assert_eq!(&px[..4], [128, 128, 128, 255]);
        assert_eq!(&px[4..], [0, 0, 0, 255]);
    }

    #[test]
    fn gradient_clip_scissor_blend_feather() {
        let stops = [(0.0, 0xff000000), (1.0, 0xffffffff)];
        let grad = Paint::Gradient { stops: &stops, inv: Mat::IDENTITY, p0: pt(0.0, 0.0), p1: pt(16.0, 0.0), radius: None };
        let (mut r, mut px) = surface(16, 4);
        r.fill(&mut px, &square(0.0, 0.0, 16.0, 4.0), false, &grad, 1.0, 0, 0.0, None);
        assert!(px[0] < 16 && px[15 * 4] > 239 && (px[8 * 4] as i32 - 136).abs() < 6);

        let (mut r, mut px) = surface(8, 8);
        r.push_clip(&square(0.0, 0.0, 4.0, 8.0));
        r.fill(&mut px, &square(0.0, 0.0, 8.0, 8.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!((alpha(&px, 8, 3, 3), alpha(&px, 8, 4, 3)), (255, 0));
        r.pop_clip();
        r.set_scissor([0, 0, 8, 2]);
        r.fill(&mut px, &square(0.0, 0.0, 8.0, 8.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 0.0, None);
        assert_eq!((alpha(&px, 8, 6, 1), alpha(&px, 8, 6, 2)), (255, 0));

        // multiply: white backdrop x mid grey = mid grey
        let (mut r, mut px) = surface(1, 1);
        clear(&mut px, 0xffffffff);
        r.fill(&mut px, &square(0.0, 0.0, 1.0, 1.0), false, &Paint::Solid(0xff808080), 1.0, 1, 0.0, None);
        assert_eq!(&px[..], [128, 128, 128, 255]);

        // The non-separable modes, against values worked by hand from the W3C formulas.
        let over = |backdrop: u32, source: u32, mode: u32| {
            let (mut r, mut px) = surface(1, 1);
            clear(&mut px, backdrop);
            r.fill(&mut px, &square(0.0, 0.0, 1.0, 1.0), false, &Paint::Solid(source), 1.0, mode, 0.0, None);
            px
        };
        assert_eq!(over(0xff336699, 0xffff0000, 12), [164, 62, 62, 255], "hue of red, saturation and luminosity of the backdrop");
        assert_eq!(over(0xff336699, 0xff808080, 13), [92, 92, 92, 255], "saturation of grey: none");
        assert_eq!(over(0xff808080, 0xffff0000, 14), [255, 74, 74, 255], "color of red at the backdrop's luminosity");
        assert_eq!(over(0xffff0000, 0xff808080, 15), [255, 74, 74, 255], "luminosity of mid grey on red");

        // An inner feather stays inside the outline: strongest at the edge, gone in the
        // middle; moved right, it leans on the left edge.
        let (mut r, mut px) = surface(32, 32);
        r.fill(&mut px, &square(8.0, 8.0, 24.0, 24.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 4.0, Some(pt(0.0, 0.0)));
        let (outside, edge, centre) = (alpha(&px, 32, 7, 16), alpha(&px, 32, 8, 16), alpha(&px, 32, 16, 16));
        assert!(outside == 0 && edge > 90 && centre < 4, "{outside} {edge} {centre}");
        let (mut r, mut px) = surface(32, 32);
        r.fill(&mut px, &square(8.0, 8.0, 24.0, 24.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 4.0, Some(pt(3.0, 0.0)));
        assert!(alpha(&px, 32, 8, 16) > 200 && alpha(&px, 32, 23, 16) < 40, "{} {}", alpha(&px, 32, 8, 16), alpha(&px, 32, 23, 16));

        // feather spreads coverage beyond the hard edge and softens the inside
        let (mut r, mut px) = surface(32, 32);
        r.fill(&mut px, &square(8.0, 8.0, 24.0, 24.0), false, &Paint::Solid(0xffffffff), 1.0, 0, 6.0, None);
        let (outside, edge, centre) = (alpha(&px, 32, 5, 16), alpha(&px, 32, 8, 16), alpha(&px, 32, 16, 16));
        assert!(outside > 0 && outside < edge && edge < centre, "{outside} {edge} {centre}");
    }

    #[test]
    fn slow_gradients_are_dithered_not_banded() {
        // Four grey levels over 256 pixels: undithered, that is four flat 64-pixel bands.
        let stops = [(0.0, 0xff101010), (1.0, 0xff141414)];
        let mean = |px: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>| {
            let n = (xs.len() * ys.len()) as f32;
            ys.flat_map(|y| xs.clone().map(move |x| (x, y))).map(|(x, y)| px[(y * 256 + x) * 4] as f32).sum::<f32>() / n
        };
        for vertical in [false, true] {
            let p1 = if vertical { pt(0.0, 256.0) } else { pt(256.0, 0.0) };
            let grad = Paint::Gradient { stops: &stops, inv: Mat::IDENTITY, p0: pt(0.0, 0.0), p1, radius: None };
            let (mut r, mut px) = surface(256, 256);
            r.fill(&mut px, &square(0.0, 0.0, 256.0, 256.0), false, &grad, 1.0, 0, 0.0, None);
            assert!(px.chunks(4).all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2]));
            for i in 0..256 {
                // Every line across the ramp averages to the ramp's value there: no line stands
                // out from its neighbours, which is what a band edge or a dither pattern would do.
                let got = if vertical { mean(&px, 0..256, i..i + 1) } else { mean(&px, i..i + 1, 0..256) };
                let ideal = 16.0 + 4.0 * (i as f32 + 0.5) / 256.0;
                assert!((got - ideal).abs() < 0.04, "vertical {vertical}, line {i}: {got} vs {ideal}");
            }
            // And along the ramp, small patches do too.
            for i in 0..16 {
                let got = if vertical { mean(&px, 100..108, i * 16..i * 16 + 16) } else { mean(&px, i * 16..i * 16 + 16, 100..108) };
                let ideal = 16.0 + 4.0 * (i as f32 * 16.0 + 8.0) / 256.0;
                assert!((got - ideal).abs() < 0.1, "vertical {vertical}, patch {i}: {got} vs {ideal}");
            }
        }
        // A flat color has nothing to dither.
        let flat = Paint::Gradient { stops: &[(0.0, 0x80336699), (1.0, 0x80336699)], inv: Mat::IDENTITY, p0: pt(0.0, 0.0), p1: pt(8.0, 0.0), radius: None };
        let (mut r, mut px) = surface(8, 8);
        r.fill(&mut px, &square(0.0, 0.0, 8.0, 8.0), false, &flat, 1.0, 0, 0.0, None);
        assert!(px.chunks(4).all(|p| p == &px[..4]));
    }

    #[test]
    fn unpremultiply_restores_straight_color() {
        let mut px = [64, 0, 0, 128];
        unpremultiply(&mut px);
        assert_eq!(px, [128, 0, 0, 128]);
    }
}
