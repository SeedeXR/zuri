//! 2D math and path geometry: affine transforms, shape builders, SVG path data,
//! flattening, stroking, trimming and hit testing. No dependencies.

use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

pub fn pt(x: f32, y: f32) -> P {
    P { x, y }
}

impl P {
    fn lerp(self, o: P, t: f32) -> P {
        pt(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
}

/// Affine transform: `x' = a*x + c*y + tx`, `y' = b*x + d*y + ty`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Mat {
    pub const IDENTITY: Mat = Mat { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    /// translate(x, y) * rotate(degrees, clockwise) * scale(sx, sy)
    pub fn trs(x: f32, y: f32, degrees: f32, sx: f32, sy: f32) -> Mat {
        if degrees == 0.0 {
            return Mat { a: sx, b: 0.0, c: 0.0, d: sy, tx: x, ty: y };
        }
        let (s, c) = degrees.to_radians().sin_cos();
        Mat { a: c * sx, b: s * sx, c: -s * sy, d: c * sy, tx: x, ty: y }
    }

    /// `self * o`: apply `o` first, then `self`.
    pub fn mul(&self, o: &Mat) -> Mat {
        Mat {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            tx: self.a * o.tx + self.c * o.ty + self.tx,
            ty: self.b * o.tx + self.d * o.ty + self.ty,
        }
    }

    pub fn apply(&self, p: P) -> P {
        pt(self.a * p.x + self.c * p.y + self.tx, self.b * p.x + self.d * p.y + self.ty)
    }

    pub fn invert(&self) -> Option<Mat> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let i = 1.0 / det;
        Some(Mat {
            a: self.d * i,
            b: -self.b * i,
            c: -self.c * i,
            d: self.a * i,
            tx: (self.c * self.ty - self.d * self.tx) * i,
            ty: (self.b * self.tx - self.a * self.ty) * i,
        })
    }

    /// Largest axis scale; used to pick flattening tolerance and to scale feather radii.
    pub fn max_scale(&self) -> f32 {
        (self.a * self.a + self.b * self.b).max(self.c * self.c + self.d * self.d).sqrt()
    }
}

/// Path element. Quadratics and arcs are converted to cubics on the way in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum El {
    M(P),
    L(P),
    C(P, P, P),
    Z,
}

pub type Path = Vec<El>;

/// A flattened sub-path.
#[derive(Clone, Debug, Default)]
pub struct Contour {
    pub pts: Vec<P>,
    pub closed: bool,
}

const KAPPA: f32 = 0.552_284_8;

pub fn rect(w: f32, h: f32, radius: f32) -> Path {
    let (hw, hh) = (w * 0.5, h * 0.5);
    let r = radius.max(0.0).min(hw.abs()).min(hh.abs());
    if r <= 0.0 {
        return vec![El::M(pt(-hw, -hh)), El::L(pt(hw, -hh)), El::L(pt(hw, hh)), El::L(pt(-hw, hh)), El::Z];
    }
    let k = r * (1.0 - KAPPA);
    vec![
        El::M(pt(-hw + r, -hh)),
        El::L(pt(hw - r, -hh)),
        El::C(pt(hw - k, -hh), pt(hw, -hh + k), pt(hw, -hh + r)),
        El::L(pt(hw, hh - r)),
        El::C(pt(hw, hh - k), pt(hw - k, hh), pt(hw - r, hh)),
        El::L(pt(-hw + r, hh)),
        El::C(pt(-hw + k, hh), pt(-hw, hh - k), pt(-hw, hh - r)),
        El::L(pt(-hw, -hh + r)),
        El::C(pt(-hw, -hh + k), pt(-hw + k, -hh), pt(-hw + r, -hh)),
        El::Z,
    ]
}

pub fn ellipse(w: f32, h: f32) -> Path {
    let (rx, ry) = (w * 0.5, h * 0.5);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    vec![
        El::M(pt(0.0, -ry)),
        El::C(pt(kx, -ry), pt(rx, -ky), pt(rx, 0.0)),
        El::C(pt(rx, ky), pt(kx, ry), pt(0.0, ry)),
        El::C(pt(-kx, ry), pt(-rx, ky), pt(-rx, 0.0)),
        El::C(pt(-rx, -ky), pt(-kx, -ry), pt(0.0, -ry)),
        El::Z,
    ]
}

/// Regular polygon (`inner == 1`) or star: `n` tips, first tip pointing up.
pub fn star(n: u32, w: f32, h: f32, inner: f32, is_star: bool) -> Path {
    let n = n.clamp(3, 256);
    let count = if is_star { n * 2 } else { n };
    let mut path = Vec::with_capacity(count as usize + 1);
    for i in 0..count {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / count as f32;
        let r = if is_star && i % 2 == 1 { inner.clamp(0.0, 1.0) } else { 1.0 };
        let p = pt(a.cos() * w * 0.5 * r, a.sin() * h * 0.5 * r);
        path.push(if i == 0 { El::M(p) } else { El::L(p) });
    }
    path.push(El::Z);
    path
}

/// Parse SVG path data (`M L H V C S Q T A Z`, absolute and relative).
pub fn parse_svg_path(d: &str) -> Result<Path> {
    let b = d.as_bytes();
    let mut i = 0;
    let mut out = Path::new();
    let (mut cur, mut start) = (P::default(), P::default());
    let mut last_ctrl: Option<(u8, P)> = None; // (b'C' or b'Q', reflected control)
    let mut cmd = 0u8;

    fn skip(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
            *i += 1;
        }
    }
    fn num(b: &[u8], i: &mut usize) -> Result<f32> {
        skip(b, i);
        let (v, len) = decimal(&b[*i..]).filter(|n| n.0.is_finite()).ok_or_else(|| crate::Error(format!("path data: expected a number at offset {}", *i)))?;
        *i += len;
        Ok(v)
    }
    fn flag(b: &[u8], i: &mut usize) -> Result<bool> {
        skip(b, i);
        match b.get(*i) {
            Some(b'0') | Some(b'1') => {
                *i += 1;
                Ok(b[*i - 1] == b'1')
            }
            _ => bail!("path data: expected an arc flag (0 or 1) at offset {}", *i),
        }
    }

    loop {
        skip(b, &mut i);
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() {
            cmd = b[i];
            i += 1;
        } else if cmd == 0 {
            bail!("path data must start with a command, found '{}'", b[i] as char);
        } else if cmd == b'M' {
            cmd = b'L'; // implicit lineto after a moveto
        } else if cmd == b'm' {
            cmd = b'l';
        } else if cmd == b'Z' || cmd == b'z' {
            bail!("path data: expected a command after Z at offset {i}");
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { P::default() };
        let xy = |i: &mut usize| -> Result<P> { Ok(pt(base.x + num(b, i)?, base.y + num(b, i)?)) };
        let upper = cmd.to_ascii_uppercase();
        let mut ctrl = None;
        match upper {
            b'M' => {
                cur = xy(&mut i)?;
                start = cur;
                out.push(El::M(cur));
            }
            b'L' => {
                cur = xy(&mut i)?;
                out.push(El::L(cur));
            }
            b'H' => {
                cur.x = base.x + num(b, &mut i)?;
                out.push(El::L(cur));
            }
            b'V' => {
                cur.y = base.y + num(b, &mut i)?;
                out.push(El::L(cur));
            }
            b'C' | b'S' => {
                let c1 = if upper == b'C' {
                    xy(&mut i)?
                } else {
                    match last_ctrl {
                        Some((b'C', c)) => c,
                        _ => cur,
                    }
                };
                let c2 = xy(&mut i)?;
                let end = xy(&mut i)?;
                out.push(El::C(c1, c2, end));
                ctrl = Some((b'C', pt(2.0 * end.x - c2.x, 2.0 * end.y - c2.y)));
                cur = end;
            }
            b'Q' | b'T' => {
                let q = if upper == b'Q' {
                    xy(&mut i)?
                } else {
                    match last_ctrl {
                        Some((b'Q', c)) => c,
                        _ => cur,
                    }
                };
                let end = xy(&mut i)?;
                out.push(El::C(cur.lerp(q, 2.0 / 3.0), end.lerp(q, 2.0 / 3.0), end));
                ctrl = Some((b'Q', pt(2.0 * end.x - q.x, 2.0 * end.y - q.y)));
                cur = end;
            }
            b'A' => {
                let (rx, ry, rot) = (num(b, &mut i)?, num(b, &mut i)?, num(b, &mut i)?);
                let (large, sweep) = (flag(b, &mut i)?, flag(b, &mut i)?);
                let end = xy(&mut i)?;
                arc_to(&mut out, cur, rx, ry, rot, large, sweep, end);
                cur = end;
            }
            b'Z' => {
                out.push(El::Z);
                cur = start;
            }
            _ => bail!("path data: unknown command '{}'", cmd as char),
        }
        last_ctrl = ctrl;
    }
    Ok(out)
}

/// SVG endpoint arc -> cubic segments (W3C SVG implementation notes, F.6).
#[allow(clippy::too_many_arguments)]
fn arc_to(out: &mut Path, p0: P, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool, p1: P) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || p0 == p1 {
        out.push(El::L(p1));
        return;
    }
    let (sin, cos) = rot_deg.to_radians().sin_cos();
    let (dx, dy) = ((p0.x - p1.x) * 0.5, (p0.y - p1.y) * 0.5);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let (cxp, cyp) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let (cx, cy) = (cos * cxp - sin * cyp + (p0.x + p1.x) * 0.5, sin * cxp + cos * cyp + (p0.y + p1.y) * 0.5);
    let a0 = ((y1 - cyp) / ry).atan2((x1 - cxp) / rx);
    let a1 = ((-y1 - cyp) / ry).atan2((-x1 - cxp) / rx);
    let mut sweep_angle = a1 - a0;
    if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f32::consts::TAU;
    } else if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f32::consts::TAU;
    }
    let n = (sweep_angle.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as u32;
    let step = sweep_angle / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |a: f32| -> (P, P) {
        let (s, c) = a.sin_cos();
        let pos = pt(cx + cos * rx * c - sin * ry * s, cy + sin * rx * c + cos * ry * s);
        let tan = pt(-cos * rx * s - sin * ry * c, -sin * rx * s + cos * ry * c);
        (pos, tan)
    };
    let mut a = a0;
    for i in 0..n {
        let (q0, t0) = at(a);
        let (q1, t1) = at(a + step);
        let end = if i == n - 1 { p1 } else { q1 };
        out.push(El::C(pt(q0.x + k * t0.x, q0.y + k * t0.y), pt(q1.x - k * t1.x, q1.y - k * t1.y), end));
        a += step;
    }
}

/// Interpolate two paths. They morph point by point when they have the same commands;
/// otherwise the result snaps from `a` to `b` halfway.
pub fn lerp_path(a: &Path, b: &Path, t: f32) -> Path {
    let same = a.len() == b.len() && a.iter().zip(b).all(|(x, y)| std::mem::discriminant(x) == std::mem::discriminant(y));
    if !same {
        return if t < 0.5 { a.clone() } else { b.clone() };
    }
    a.iter()
        .zip(b)
        .map(|(x, y)| match (x, y) {
            (El::M(p), El::M(q)) => El::M(p.lerp(*q, t)),
            (El::L(p), El::L(q)) => El::L(p.lerp(*q, t)),
            (El::C(p1, p2, p3), El::C(q1, q2, q3)) => El::C(p1.lerp(*q1, t), p2.lerp(*q2, t), p3.lerp(*q3, t)),
            _ => El::Z,
        })
        .collect()
}

/// Flatten `path` (transformed by `m`) into polylines, appending to `out`.
/// `tol` is the maximum deviation, in the units of the output space.
pub fn flatten(path: &[El], m: &Mat, tol: f32, out: &mut Vec<Contour>) {
    let mut cur: Option<Contour> = None;
    let mut last = P::default();
    for el in path {
        match *el {
            El::M(p) => {
                out.extend(cur.take().filter(|c| c.pts.len() > 1));
                last = m.apply(p);
                cur = Some(Contour { pts: vec![last], closed: false });
            }
            El::L(p) => {
                let c = cur.get_or_insert_with(|| Contour { pts: vec![last], closed: false });
                last = m.apply(p);
                c.pts.push(last);
            }
            El::C(c1, c2, p) => {
                let (c1, c2, p) = (m.apply(c1), m.apply(c2), m.apply(p));
                let c = cur.get_or_insert_with(|| Contour { pts: vec![last], closed: false });
                // Wang's formula: segments needed to stay within `tol` of a cubic.
                let dd0 = pt(last.x - 2.0 * c1.x + c2.x, last.y - 2.0 * c1.y + c2.y);
                let dd1 = pt(c1.x - 2.0 * c2.x + p.x, c1.y - 2.0 * c2.y + p.y);
                let mx = (dd0.x * dd0.x + dd0.y * dd0.y).max(dd1.x * dd1.x + dd1.y * dd1.y).sqrt();
                let n = ((0.75 * mx / tol).sqrt().ceil() as usize).clamp(1, 256);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    c.pts.push(pt(w0 * last.x + w1 * c1.x + w2 * c2.x + w3 * p.x, w0 * last.y + w1 * c1.y + w2 * c2.y + w3 * p.y));
                }
                last = p;
            }
            El::Z => {
                if let Some(mut c) = cur.take() {
                    c.closed = true;
                    last = c.pts[0];
                    if c.pts.len() > 1 {
                        out.push(c);
                    }
                }
            }
        }
    }
    out.extend(cur.take().filter(|c| c.pts.len() > 1));
}

pub fn transform_contours(cs: &mut [Contour], m: &Mat) {
    for c in cs {
        for p in &mut c.pts {
            *p = m.apply(*p);
        }
    }
}

/// Bounding box `[x0, y0, x1, y1]`, or `None` when there are no points.
pub fn bounds(cs: &[Contour]) -> Option<[f32; 4]> {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in cs.iter().flat_map(|c| &c.pts) {
        b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
    }
    (b[0] <= b[2] && b[0].is_finite() && b[2].is_finite() && b[1].is_finite() && b[3].is_finite()).then_some(b)
}

/// Non-zero winding point-in-path test (contours are treated as closed).
pub fn contains(cs: &[Contour], p: P) -> bool {
    let mut winding = 0i32;
    for c in cs {
        let n = c.pts.len();
        for i in 0..n {
            let (a, b) = (c.pts[i], c.pts[(i + 1) % n]);
            let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
            if a.y <= p.y {
                if b.y > p.y && side > 0.0 {
                    winding += 1;
                }
            } else if b.y <= p.y && side < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
}

#[derive(Clone, Copy, PartialEq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

/// Push a polygon, oriented so its signed area is positive. Every piece of a stroke must
/// wind the same way or overlapping pieces would cancel under the non-zero rule.
fn push_poly(out: &mut Vec<Contour>, mut pts: Vec<P>) {
    let n = pts.len();
    let area: f32 = (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum();
    if area < 0.0 {
        pts.reverse();
    }
    if area != 0.0 {
        out.push(Contour { pts, closed: true });
    }
}

fn push_disc(out: &mut Vec<Contour>, c: P, r: f32, tol: f32) {
    let n = ((std::f32::consts::PI / (1.0 - (tol / r).min(1.0)).acos().max(0.05)).ceil() as usize).clamp(8, 128);
    let pts = (0..n)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            pt(c.x + a.cos() * r, c.y + a.sin() * r)
        })
        .collect();
    push_poly(out, pts);
}

/// Expand polylines into fillable outlines (fill the result with the non-zero rule).
/// Works in the space of the input, so stroke before applying a non-uniform transform.
pub fn stroke(cs: &[Contour], width: f32, cap: Cap, join: Join, tol: f32, out: &mut Vec<Contour>) {
    let hw = width * 0.5;
    if hw <= 0.0 {
        return;
    }
    for c in cs {
        // Drop repeated points; they have no direction.
        let mut pts: Vec<P> = Vec::with_capacity(c.pts.len());
        for p in &c.pts {
            if pts.last().is_none_or(|l| (l.x - p.x).abs() > 1e-6 || (l.y - p.y).abs() > 1e-6) {
                pts.push(*p);
            }
        }
        if c.closed && pts.len() > 2 && pts[0] == pts[pts.len() - 1] {
            pts.pop();
        }
        let n = pts.len();
        if n < 2 {
            if n == 1 && cap == Cap::Round {
                push_disc(out, pts[0], hw, tol);
            }
            continue;
        }
        let segs = if c.closed { n } else { n - 1 };
        let dir = |i: usize| -> P {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            pt((b.x - a.x) / len, (b.y - a.y) / len)
        };
        for i in 0..segs {
            let (a, b, d) = (pts[i], pts[(i + 1) % n], dir(i));
            let nrm = pt(-d.y * hw, d.x * hw);
            push_poly(out, vec![pt(a.x + nrm.x, a.y + nrm.y), pt(b.x + nrm.x, b.y + nrm.y), pt(b.x - nrm.x, b.y - nrm.y), pt(a.x - nrm.x, a.y - nrm.y)]);
        }
        let joins = if c.closed { 0..n } else { 1..n - 1 };
        for i in joins {
            let (d0, d1, v) = (dir((i + n - 1) % n), dir(i), pts[i]);
            let cross = d0.x * d1.y - d0.y * d1.x;
            let dot = d0.x * d1.x + d0.y * d1.y;
            if cross.abs() < 1e-6 && dot > 0.0 {
                continue; // straight on
            }
            // The gap opens on the outer side of the turn.
            let s = if cross > 0.0 { -hw } else { hw };
            let (o0, o1) = (pt(v.x - d0.y * s, v.y + d0.x * s), pt(v.x - d1.y * s, v.y + d1.x * s));
            // Nearly straight joins (the interior of a flattened curve) get a cheap bevel.
            let kind = if dot > 0.98 && join == Join::Round { Join::Bevel } else { join };
            match kind {
                Join::Round => push_disc(out, v, hw, tol),
                Join::Bevel => push_poly(out, vec![v, o0, o1]),
                Join::Miter => {
                    // Miter length / half width = 1 / cos(theta / 2); limit 4 as in SVG.
                    let cos_half = ((1.0 + dot) * 0.5).max(0.0).sqrt();
                    if cos_half > 0.25 {
                        let (mx, my) = (o0.x + o1.x - 2.0 * v.x, o0.y + o1.y - 2.0 * v.y);
                        let len = (mx * mx + my * my).sqrt();
                        let tip = pt(v.x + mx / len * hw / cos_half, v.y + my / len * hw / cos_half);
                        push_poly(out, vec![v, o0, tip, o1]);
                    } else {
                        push_poly(out, vec![v, o0, o1]);
                    }
                }
            }
        }
        if !c.closed {
            for (v, d) in [(pts[0], dir(0)), (pts[n - 1], dir(n - 2))].into_iter().zip([-1.0f32, 1.0]).map(|((v, d), s)| (v, pt(d.x * s, d.y * s))) {
                match cap {
                    Cap::Butt => {}
                    Cap::Round => push_disc(out, v, hw, tol),
                    Cap::Square => {
                        let nrm = pt(-d.y * hw, d.x * hw);
                        let e = pt(v.x + d.x * hw, v.y + d.y * hw);
                        push_poly(
                            out,
                            vec![pt(v.x + nrm.x, v.y + nrm.y), pt(e.x + nrm.x, e.y + nrm.y), pt(e.x - nrm.x, e.y - nrm.y), pt(v.x - nrm.x, v.y - nrm.y)],
                        );
                    }
                }
            }
        }
    }
}

/// The points of a contour as an explicit open polyline (a closed one repeats its start).
fn open(c: &Contour) -> Vec<P> {
    let mut pts = c.pts.clone();
    if c.closed {
        pts.push(c.pts[0]);
    }
    pts
}

/// The point at fraction `t` of the total length of `cs` (across all contours in order;
/// `t` wraps), and the direction of travel there in degrees.
pub fn point_along(cs: &[Contour], t: f32) -> Option<(P, f32)> {
    // The first time round measures the path; the second stops at the place.
    let (mut at, mut walked) = (f32::INFINITY, 0.0);
    for _ in 0..2 {
        for c in cs {
            let n = c.pts.len();
            for i in 0..n.saturating_sub(usize::from(!c.closed)) {
                let (a, b) = (c.pts[i], c.pts[(i + 1) % n]);
                let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                walked += len;
                if len > 0.0 && at <= walked {
                    return Some((b.lerp(a, (walked - at) / len), (b.y - a.y).atan2(b.x - a.x).to_degrees()));
                }
            }
        }
        // 1 (and 2, 3...) is the end of the path, not its start again.
        at = if t > 0.0 && t.fract() == 0.0 { walked } else { t.rem_euclid(1.0) * walked };
        walked = 0.0;
    }
    None
}

/// Keep the part of the path between `start` and `end` (fractions of total length,
/// measured across all contours in order), shifted by `offset` with wrap-around.
pub fn trim(cs: &[Contour], start: f32, end: f32, offset: f32) -> Vec<Contour> {
    let (lo, hi) = (start.min(end).clamp(0.0, 1.0), start.max(end).clamp(0.0, 1.0));
    if hi - lo >= 1.0 {
        return cs.to_vec();
    }
    if hi - lo <= 0.0 {
        return Vec::new();
    }
    let lines: Vec<Vec<P>> = cs.iter().map(open).collect();
    let seg_len = |a: P, b: P| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    let total: f32 = lines.iter().flat_map(|l| l.windows(2)).map(|w| seg_len(w[0], w[1])).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let off = offset.rem_euclid(1.0);
    let a = ((lo + off) % 1.0) * total;
    let b = a + (hi - lo) * total;
    let ranges = if b > total { vec![(a, total), (0.0, b - total)] } else { vec![(a, b)] };
    let mut out = Vec::new();
    for (from, to) in ranges {
        let mut walked = 0.0;
        for line in &lines {
            let mut piece: Vec<P> = Vec::new();
            for w in line.windows(2) {
                let len = seg_len(w[0], w[1]);
                let (s0, s1) = (walked, walked + len);
                walked = s1;
                if len <= 0.0 || s1 <= from || s0 >= to {
                    continue;
                }
                let (t0, t1) = (((from - s0) / len).max(0.0), ((to - s0) / len).min(1.0));
                if piece.is_empty() {
                    piece.push(w[0].lerp(w[1], t0));
                }
                piece.push(w[0].lerp(w[1], t1));
            }
            if piece.len() > 1 {
                out.push(Contour { pts: piece, closed: false });
            }
        }
    }
    out
}

/// Cut polylines into dashes of length `on` separated by `off`, starting `offset` into the
/// pattern. The pattern restarts on every contour, as in SVG.
pub fn dash(cs: &[Contour], on: f32, off: f32, offset: f32) -> Vec<Contour> {
    let period = on + off;
    if on <= 0.0 || off <= 0.0 || !period.is_finite() {
        return cs.to_vec();
    }
    // Bounds the output for a hostile file asking for microscopic dashes on a huge path.
    const MAX_DASHES: usize = 65_536;
    let mut out = Vec::new();
    for c in cs {
        let pts = open(c);
        let mut phase = offset.rem_euclid(period);
        let mut piece: Vec<P> = if phase < on { vec![pts[0]] } else { Vec::new() };
        for w in pts.windows(2) {
            let len = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
            let mut pos = 0.0;
            while pos < len && out.len() < MAX_DASHES {
                let drawing = phase < on;
                let to_toggle = if drawing { on - phase } else { period - phase };
                let remaining = len - pos;
                let step = to_toggle.min(remaining).max(1e-6);
                pos += step;
                phase += step;
                let p = w[0].lerp(w[1], (pos / len).min(1.0));
                if drawing {
                    piece.push(p);
                }
                if to_toggle <= remaining {
                    if drawing {
                        out.push(Contour { pts: std::mem::take(&mut piece), closed: false });
                    } else {
                        piece.push(p); // a dash starts here
                        phase = 0.0;
                    }
                }
            }
        }
        if piece.len() > 1 {
            out.push(Contour { pts: piece, closed: false });
        }
    }
    out
}

/// The decimal number `text` starts with, as path data writes one (sign, digits, a
/// fraction, an exponent), and how many bytes it takes.
// ponytail: by hand, because `str::parse::<f32>` costs 15 KB of wasm. This rounds twice
// (to f64, then to f32), so a number within about 1e-16 of halfway between two f32s can
// come out one step off, and one that close below the largest f32 reads as infinite
// (which path data refuses). If that ever matters, parse with std again and pay the 15 KB.
fn decimal(text: &[u8]) -> Option<(f32, usize)> {
    let negative = text.first() == Some(&b'-');
    let mut at = usize::from(matches!(text.first(), Some(b'-' | b'+')));
    let (mut digits, mut exp, mut seen, mut fraction) = (0u64, 0i32, false, false);
    while let Some(c) = text.get(at) {
        match c {
            b'0'..=b'9' => {
                seen = true;
                // Digits beyond what a u64 holds only move the exponent.
                if digits < u64::MAX / 10 - 1 {
                    digits = digits * 10 + (c - b'0') as u64;
                    exp -= i32::from(fraction);
                } else {
                    exp += i32::from(!fraction);
                }
            }
            b'.' if !fraction => fraction = true,
            _ => break,
        }
        at += 1;
    }
    if let Some(b'e' | b'E') = text.get(at) {
        let minus = text.get(at + 1) == Some(&b'-');
        at += 1 + usize::from(minus || text.get(at + 1) == Some(&b'+'));
        let (mut e, from) = (0i32, at);
        while let Some(c @ b'0'..=b'9') = text.get(at) {
            e = e.saturating_mul(10).saturating_add((c - b'0') as i32);
            at += 1;
        }
        seen &= at > from; // an exponent needs digits
        exp = exp.saturating_add(if minus { -e } else { e });
    }
    let scale = 10f64.powi(exp.saturating_abs());
    // Zero stays zero whatever its exponent (0 * infinity is not a number).
    let v = if digits == 0 {
        0.0
    } else if exp < 0 {
        digits as f64 / scale
    } else {
        digits as f64 * scale
    };
    seen.then_some((if negative { -v } else { v } as f32, at))
}

/// `v` kept within `lo..=hi`, for a bound that is computed. `f32::clamp` does the same,
/// but panics on reversed bounds with a message that formats floats: with a constant
/// bound that is compiled away, with a computed one it brings 17 KB into the wasm.
pub(crate) fn within(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Stable sort of (key, payload) pairs by key. Every runtime sort goes through this one
/// function on purpose: each `sort_by` call site otherwise compiles its own copy of the
/// standard sort, about 6 KB of wasm apiece.
#[inline(never)]
pub(crate) fn sort_keyed(v: &mut [(f32, u32)]) {
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_reads_what_path_data_writes() {
        let whole = |text: &str| decimal(text.as_bytes()).filter(|n| n.1 == text.len()).map(|n| n.0);
        for text in "0 -0 1 -12.5 +3 .5 5. 1e3 1.5E-2 -2.e+1 0.1 123456.789 16777217 3.4028235e38 1e-45 0.000001 9007199254740993 1.17549435e-38 123456789012345678901234567890 0.30000001192092896 1e99999 1e-99999 0e99999".split(' ') {
            assert_eq!(whole(text).map(f32::to_bits), text.parse::<f32>().ok().map(f32::to_bits), "{text}");
        }
        // Rounded twice (see `decimal`): right at a halfway point it may be one step from std.
        for text in ["1.0000000596046448", "8388608.5000000001", "5.79121334221e-10", "7.0064923216240854e-46"] {
            assert!(whole(text).unwrap().to_bits().abs_diff(text.parse::<f32>().unwrap().to_bits()) <= 1, "{text}");
        }
        // Every float of a wide sample survives being written and read back.
        let mut bits = 0x9e37_79b9u32;
        for _ in 0..200_000 {
            bits = bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = f32::from_bits(bits);
            if v.is_finite() {
                assert_eq!(whole(&v.to_string()), Some(v), "{v}");
                assert_eq!(whole(&format!("{v:e}")), Some(v), "{v:e}");
            }
        }
        for bad in ["", "-", ".", "e5", "1e", "--1", "1e+"] {
            assert_eq!(decimal(bad.as_bytes()), None, "{bad}");
        }
        // It stops where the number does: what follows is the path parser's to read.
        for (text, read) in [("1.2.3", (1.2, 3)), ("1x", (1.0, 1)), ("5e9.91", (5e9, 3)), ("1-2", (1.0, 1))] {
            assert_eq!(decimal(text.as_bytes()), Some(read), "{text}");
        }
    }

    #[test]
    fn matrix_inverse_roundtrips() {
        let m = Mat::trs(10.0, -4.0, 33.0, 2.0, 0.5);
        let p = pt(3.0, 7.0);
        let q = m.invert().unwrap().apply(m.apply(p));
        assert!((q.x - p.x).abs() < 1e-4 && (q.y - p.y).abs() < 1e-4);
        assert!(Mat::trs(0.0, 0.0, 0.0, 0.0, 1.0).invert().is_none());
    }

    #[test]
    fn svg_path_commands() {
        let p = parse_svg_path("M10 10 h20 v20 H10 z m5,5 l1-1 q2 2 4 0 t4 0 c1 1 2 1 3 0 s2-1 3 0 a5 5 0 0 1 10 0").unwrap();
        assert_eq!(p[0], El::M(pt(10.0, 10.0)));
        assert_eq!(p[1], El::L(pt(30.0, 10.0)));
        assert_eq!(p[3], El::L(pt(10.0, 30.0)));
        assert_eq!(p[4], El::Z);
        assert_eq!(p[5], El::M(pt(15.0, 15.0))); // relative to the closed sub-path start
        assert!(matches!(p.last(), Some(El::C(_, _, e)) if (e.x - 40.0).abs() < 1e-3 && (e.y - 14.0).abs() < 1e-3));
        // Compact arc flags and numbers without separators.
        assert!(parse_svg_path("M0 0a1 1 0 01.5.5").is_ok());
        assert!(parse_svg_path("10 10").is_err());
        assert!(parse_svg_path("M 1").is_err());
    }

    #[test]
    fn flatten_circle_stays_within_tolerance() {
        let mut cs = Vec::new();
        flatten(&ellipse(200.0, 200.0), &Mat::IDENTITY, 0.1, &mut cs);
        assert_eq!(cs.len(), 1);
        assert!(cs[0].closed);
        for p in &cs[0].pts {
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 100.0).abs() < 0.15, "radius {r}");
        }
        assert!(contains(&cs, pt(0.0, 0.0)) && !contains(&cs, pt(99.0, 99.0)));
    }

    #[test]
    fn trim_and_stroke() {
        let line = vec![Contour { pts: vec![pt(0.0, 0.0), pt(100.0, 0.0)], closed: false }];
        let t = trim(&line, 0.25, 0.75, 0.0);
        assert_eq!(t.len(), 1);
        assert!((t[0].pts[0].x - 25.0).abs() < 1e-4 && (t[0].pts[1].x - 75.0).abs() < 1e-4);
        assert_eq!(trim(&line, 0.5, 1.0, 0.25).len(), 2); // 75..100 then wraps to 0..25
        let w = trim(&line, 0.5, 1.0, 0.75); // 125..175 is 25..75
        assert!(w.len() == 1 && (w[0].pts[0].x - 25.0).abs() < 1e-3 && (w[0].pts[1].x - 75.0).abs() < 1e-3);
        assert!(trim(&line, 0.3, 0.3, 0.0).is_empty());

        let mut out = Vec::new();
        stroke(&line, 10.0, Cap::Butt, Join::Miter, 0.1, &mut out);
        assert_eq!(bounds(&out), Some([0.0, -5.0, 100.0, 5.0]));
        out.clear();
        stroke(&line, 10.0, Cap::Square, Join::Miter, 0.1, &mut out);
        assert_eq!(bounds(&out), Some([-5.0, -5.0, 105.0, 5.0]));
        assert!(contains(&out, pt(50.0, 4.0)) && !contains(&out, pt(50.0, 6.0)));
    }

    #[test]
    fn dash_cuts_by_arc_length() {
        let line = vec![Contour { pts: vec![pt(0.0, 0.0), pt(50.0, 0.0), pt(50.0, 50.0)], closed: false }];
        let d = dash(&line, 10.0, 10.0, 0.0);
        assert_eq!(d.len(), 5);
        assert!((d[0].pts[0].x, d[0].pts.last().unwrap().x) == (0.0, 10.0));
        // The third dash covers 40..50 and ends exactly at the corner.
        assert!((d[2].pts[0].x - 40.0).abs() < 1e-3 && (d[2].pts.last().unwrap().x - 50.0).abs() < 1e-3);
        assert!((d[3].pts[0].y - 10.0).abs() < 1e-3, "the pattern continues around the corner");
        // Offset shifts the pattern; a dash already under way at the start is kept.
        let shifted = dash(&line, 10.0, 10.0, 5.0);
        assert!((shifted[0].pts.last().unwrap().x - 5.0).abs() < 1e-3 && (shifted[1].pts[0].x - 15.0).abs() < 1e-3);
        assert_eq!(dash(&line, 0.0, 5.0, 0.0).len(), 1, "no dash length means a solid line");
        assert!(dash(&line, 1e-9, 1e-9, 0.0).len() <= 65_536);
    }

    #[test]
    fn morph_requires_matching_structure() {
        let a = parse_svg_path("M0 0 L10 0").unwrap();
        let b = parse_svg_path("M0 0 L20 10").unwrap();
        assert_eq!(lerp_path(&a, &b, 0.5)[1], El::L(pt(15.0, 5.0)));
        let c = parse_svg_path("M0 0 L1 1 L2 2").unwrap();
        assert_eq!(lerp_path(&a, &c, 0.4), a);
        assert_eq!(lerp_path(&a, &c, 0.6), c);
    }
}
