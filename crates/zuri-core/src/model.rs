//! 3D meshes inside a 2D scene: the packed mesh format and a small software rasterizer
//! (perspective projection, z-buffer, per-vertex lighting, 2x2 supersampling).
//!
//! Meshes are converted from OBJ / glTF / FBX at authoring time (`zuri import`); the runtime
//! only ever reads this one packed layout, so it stays small and has no 3D file parsers.
//!
//! ```text
//! "ZMSH"  u32 version (1)  u32 vertex_count  u32 index_count
//! positions  f32 x 3 per vertex
//! normals    f32 x 3 per vertex
//! colors     u8  x 4 per vertex (RGBA, straight alpha)
//! indices    u32 per index (triangles)
//! ```

use crate::geom::{Mat, P};
use crate::Result;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub col: Vec<[u8; 4]>,
    pub idx: Vec<u32>,
}

const MAX_VERTS: usize = 4_000_000;

impl Mesh {
    pub fn parse(b: &[u8]) -> Result<Mesh> {
        let word = |i: usize| b.get(i * 4..i * 4 + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        if b.get(..4) != Some(b"ZMSH") || word(1) != Some(1) {
            bail!("mesh data is not a version 1 ZMSH block");
        }
        let (vc, ic) = (word(2).unwrap_or(0) as usize, word(3).unwrap_or(0) as usize);
        if vc > MAX_VERTS || ic > MAX_VERTS * 3 || ic % 3 != 0 || b.len() != 16 + vc * 28 + ic * 4 {
            bail!("mesh data is truncated or has inconsistent counts");
        }
        let f = |off: usize| f32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        let vec3 = |base: usize, i: usize| [f(base + i * 12), f(base + i * 12 + 4), f(base + i * 12 + 8)];
        let (np, nc, ni) = (16 + vc * 12, 16 + vc * 24, 16 + vc * 28);
        let mesh = Mesh {
            pos: (0..vc).map(|i| vec3(16, i)).collect(),
            nrm: (0..vc).map(|i| vec3(np, i)).collect(),
            col: (0..vc).map(|i| b[nc + i * 4..nc + i * 4 + 4].try_into().unwrap()).collect(),
            idx: (0..ic).map(|i| u32::from_le_bytes(b[ni + i * 4..ni + i * 4 + 4].try_into().unwrap())).collect(),
        };
        if mesh.idx.iter().any(|i| *i as usize >= vc) {
            bail!("mesh index out of range");
        }
        if mesh.pos.iter().chain(&mesh.nrm).flatten().any(|v| !v.is_finite()) {
            bail!("mesh contains non-finite numbers");
        }
        Ok(mesh)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = b"ZMSH".to_vec();
        for w in [1, self.pos.len() as u32, self.idx.len() as u32] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for v in self.pos.iter().chain(&self.nrm).flatten() {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for c in &self.col {
            out.extend_from_slice(c);
        }
        for i in &self.idx {
            out.extend_from_slice(&i.to_le_bytes());
        }
        out
    }

    /// Make the mesh ready to ship: fill in missing normals and colors, then center it and
    /// scale it to fit the unit sphere so every model frames the same way in a `Model` node.
    pub fn finish(&mut self) {
        let n = self.pos.len();
        if self.nrm.len() != n {
            self.nrm = vec![[0.0; 3]; n];
            for tri in self.idx.as_chunks::<3>().0 {
                let [a, b, c] = [self.pos[tri[0] as usize], self.pos[tri[1] as usize], self.pos[tri[2] as usize]];
                let (e1, e2) = (sub(b, a), sub(c, a));
                let face = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                for i in tri {
                    // Area-weighted: the cross product's length is twice the triangle's area.
                    for (n, f) in self.nrm[*i as usize].iter_mut().zip(face) {
                        *n += f;
                    }
                }
            }
        }
        for v in &mut self.nrm {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            *v = if len > 1e-20 { [v[0] / len, v[1] / len, v[2] / len] } else { [0.0, 0.0, 1.0] };
        }
        self.col.resize(n, [200, 200, 200, 255]);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let radius = self.pos.iter().map(|v| sub(*v, center).iter().map(|c| c * c).sum::<f32>()).fold(0.0, f32::max).sqrt();
        let s = if radius > 1e-20 { 1.0 / radius } else { 1.0 };
        for v in &mut self.pos {
            *v = sub(*v, center).map(|c| c * s);
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// A rendered model: premultiplied RGBA8, `w` x `h`, top-left at device pixel `x, y`.
pub struct Rendered {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// Render `mesh` into a `vw` x `vh` viewport centered on the node origin.
/// `rot` is pitch / yaw / roll in degrees, `m` maps node space to device pixels and
/// `scissor` bounds the output.
pub fn render(mesh: &Mesh, rot: [f32; 3], fov: f32, vw: f32, vh: f32, m: &Mat, scissor: [i32; 4]) -> Option<Rendered> {
    const SS: i32 = 2;
    let half_fov = (fov.clamp(1.0, 170.0) * 0.5).to_radians();
    let (sin, cos) = crate::geom::sin_cos(half_fov);
    let (focal, dist) = (cos / sin, 1.0 / sin);
    let half = vw.min(vh) * 0.5;
    if half <= 0.0 || !half.is_finite() || mesh.idx.is_empty() {
        return None;
    }
    let [(sx, cx), (sy, cy), (sz, cz)] = rot.map(|d| crate::geom::sin_cos(d.to_radians()));
    // Roll, then pitch, then yaw.
    let rotate = |v: [f32; 3]| {
        let (x, y) = (v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz);
        let (y, z) = (y * cx - v[2] * sx, y * sx + v[2] * cx);
        [x * cy + z * sy, y, -x * sy + z * cy]
    };
    const LIGHT: [f32; 3] = [-0.37, 0.56, 0.74];
    // Per vertex: device x, y, depth, lit premultiplied-ready color.
    let verts: Vec<([f32; 3], [f32; 4])> = mesh
        .pos
        .iter()
        .zip(&mesh.nrm)
        .zip(&mesh.col)
        .map(|((pos, nrm), col)| {
            let (v, n) = (rotate(*pos), rotate(*nrm));
            let depth = (dist - v[2]).max(1e-3);
            let dev = m.apply(P { x: focal * v[0] / depth * half, y: -focal * v[1] / depth * half });
            let light = 0.32 + 0.78 * (n[0] * LIGHT[0] + n[1] * LIGHT[1] + n[2] * LIGHT[2]).max(0.0);
            let ch = |c: u8| (c as f32 * light).min(255.0);
            ([dev.x, dev.y, depth], [ch(col[0]), ch(col[1]), ch(col[2]), col[3] as f32])
        })
        .collect();

    let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (v, _) in &verts {
        (bx0, by0, bx1, by1) = (bx0.min(v[0]), by0.min(v[1]), bx1.max(v[0]), by1.max(v[1]));
    }
    if !(bx0.is_finite() && by0.is_finite() && bx1.is_finite() && by1.is_finite()) {
        return None;
    }
    let (x0, y0) = ((bx0.floor() as i32).max(scissor[0]), (by0.floor() as i32).max(scissor[1]));
    let (x1, y1) = ((bx1.ceil() as i32).min(scissor[2]), (by1.ceil() as i32).min(scissor[3]));
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let (sw, sh) = (w * SS as usize, h * SS as usize);
    let mut color = vec![[0u8; 4]; sw * sh];
    let mut zbuf = vec![f32::MAX; sw * sh];

    for tri in mesh.idx.as_chunks::<3>().0 {
        let [(a, ca), (b, cb), (c, cc)] = [verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]];
        // Supersampled buffer coordinates.
        let to = |v: [f32; 3]| ((v[0] - x0 as f32) * SS as f32, (v[1] - y0 as f32) * SS as f32);
        let ((ax, ay), (bx, by), (cx, cy)) = (to(a), to(b), to(c));
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if area.abs() < 1e-6 {
            continue;
        }
        let px0 = (ax.min(bx).min(cx).floor().max(0.0)) as usize;
        let py0 = (ay.min(by).min(cy).floor().max(0.0)) as usize;
        let px1 = (ax.max(bx).max(cx).ceil().max(0.0) as usize).min(sw);
        let py1 = (ay.max(by).max(cy).ceil().max(0.0) as usize).min(sh);
        let inv = 1.0 / area;
        for y in py0..py1 {
            for x in px0..px1 {
                let (sxp, syp) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((bx - sxp) * (cy - syp) - (by - syp) * (cx - sxp)) * inv;
                let w1 = ((cx - sxp) * (ay - syp) - (cy - syp) * (ax - sxp)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * sw + x;
                if z < zbuf[i] {
                    zbuf[i] = z;
                    color[i] = std::array::from_fn(|k| (w0 * ca[k] + w1 * cb[k] + w2 * cc[k] + 0.5) as u8);
                }
            }
        }
    }

    // Box-filter down to device resolution, premultiplying as we go.
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let s = color[(y * 2 + dy) * sw + x * 2 + dx];
                let a = s[3] as u32;
                sum = [sum[0] + s[0] as u32 * a, sum[1] + s[1] as u32 * a, sum[2] + s[2] as u32 * a, sum[3] + a];
            }
            let o = &mut px[(y * w + x) * 4..(y * w + x) * 4 + 4];
            o.copy_from_slice(&[(sum[0] / 1020) as u8, (sum[1] / 1020) as u8, (sum[2] / 1020) as u8, (sum[3] / 4) as u8]);
        }
    }
    Some(Rendered { x: x0, y: y0, w: w as u32, h: h as u32, px })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit tetrahedron-ish mesh, enough to exercise parse/serialize/render.
    pub fn tetra() -> Mesh {
        let mut m = Mesh {
            pos: vec![[0.0, 1.0, 0.0], [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [0.0, -1.0, -1.0]],
            nrm: vec![],
            col: vec![[255, 80, 40, 255]; 4],
            idx: vec![0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2],
        };
        m.finish();
        m
    }

    #[test]
    fn bytes_roundtrip_and_reject_corruption() {
        let m = tetra();
        let b = m.to_bytes();
        assert_eq!(Mesh::parse(&b).unwrap(), m);
        assert!(Mesh::parse(&b[..b.len() - 1]).is_err());
        assert!(Mesh::parse(b"nope").is_err());
        let mut bad = b.clone();
        let last = bad.len() - 4;
        bad[last..].copy_from_slice(&99u32.to_le_bytes());
        assert!(Mesh::parse(&bad).is_err());
    }

    #[test]
    fn finish_fits_unit_sphere() {
        let m = tetra();
        let r = m.pos.iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f32::max);
        assert!((r - 1.0).abs() < 1e-5);
        assert_eq!(m.nrm.len(), 4);
    }

    #[test]
    fn renders_inside_viewport_with_opaque_centre() {
        let m = tetra();
        let view = Mat::trs(50.0, 50.0, 0.0, 1.0, 1.0);
        let r = render(&m, [0.0, 30.0, 0.0], 35.0, 100.0, 100.0, &view, [0, 0, 100, 100]).unwrap();
        assert!(r.x >= 0 && r.y >= 0 && r.x + r.w as i32 <= 100 && r.y + r.h as i32 <= 100);
        let at = |x: i32, y: i32| r.px[(((y - r.y) * r.w as i32 + (x - r.x)) * 4 + 3) as usize];
        assert_eq!(at(50, 55), 255);
        assert!(render(&m, [0.0; 3], 35.0, 100.0, 100.0, &view, [0, 0, 0, 0]).is_none());
    }
}
