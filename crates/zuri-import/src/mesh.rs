//! 3D importers: OBJ, glTF / GLB and FBX -> Zuri's packed `Mesh`.
//!
//! All three bake node transforms and material base colors into one triangle list with
//! per-vertex position, normal and color. Textures, skinning and animation are not imported.

use std::collections::HashMap;
use std::path::Path;
use zuri_core::model::Mesh;
use zuri_core::{Error, Result};

const GREY: [u8; 4] = [200, 200, 200, 255];

fn color(rgba: [f32; 4]) -> [u8; 4] {
    rgba.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

/// Weld identical vertices (exporters like FBX emit three per triangle), then normalize.
fn done(mesh: Mesh, what: &str) -> Result<Mesh> {
    if mesh.idx.is_empty() {
        return Err(Error(format!("{what}: no triangles found")));
    }
    let has_normals = mesh.nrm.len() == mesh.pos.len();
    let mut out = Mesh::default();
    // Key: bit patterns of position and normal, plus color.
    type Vertex = ([u32; 3], [u32; 3], [u8; 4]);
    let mut seen: HashMap<Vertex, u32> = HashMap::new();
    for i in &mesh.idx {
        let i = *i as usize;
        let (pos, nrm, col) = (mesh.pos[i], if has_normals { mesh.nrm[i] } else { [0.0; 3] }, mesh.col.get(i).copied().unwrap_or(GREY));
        let next = out.pos.len() as u32;
        out.idx.push(*seen.entry((pos.map(f32::to_bits), nrm.map(f32::to_bits), col)).or_insert_with(|| {
            out.pos.push(pos);
            if has_normals {
                out.nrm.push(nrm);
            }
            out.col.push(col);
            next
        }));
    }
    out.finish();
    Ok(out)
}

/// Wavefront OBJ, with `Kd` / `d` colors from the companion `.mtl` when `dir` is given.
pub fn obj(text: &str, dir: Option<&Path>) -> Result<Mesh> {
    let (mut pos, mut nrm) = (Vec::<[f32; 3]>::new(), Vec::<[f32; 3]>::new());
    let mut materials: HashMap<String, [u8; 4]> = HashMap::new();
    let mut current = GREY;
    let mut mesh = Mesh::default();
    let mut normals_complete = true;
    let vec3 = |it: &mut std::str::SplitWhitespace, line: usize| -> Result<[f32; 3]> {
        let mut v = [0.0; 3];
        for c in &mut v {
            *c = it
                .next()
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|f| f.is_finite())
                .ok_or_else(|| Error(format!("OBJ line {line}: expected three numbers")))?;
        }
        Ok(v)
    };
    for (n, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => pos.push(vec3(&mut it, n + 1)?),
            Some("vn") => nrm.push(vec3(&mut it, n + 1)?),
            Some("mtllib") => {
                let Some(text) = dir.zip(it.next()).and_then(|(d, f)| std::fs::read_to_string(d.join(f)).ok()) else { continue };
                let mut name = String::new();
                for l in text.lines() {
                    let mut m = l.split_whitespace();
                    match m.next() {
                        Some("newmtl") => name = m.next().unwrap_or("").to_string(),
                        Some("Kd") => {
                            let c: Vec<f32> = m.filter_map(|s| s.parse().ok()).collect();
                            if c.len() == 3 {
                                materials.entry(name.clone()).or_insert(GREY)[..3].copy_from_slice(&color([c[0], c[1], c[2], 1.0])[..3]);
                            }
                        }
                        Some("d") => {
                            if let Some(a) = m.next().and_then(|s| s.parse::<f32>().ok()) {
                                materials.entry(name.clone()).or_insert(GREY)[3] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("usemtl") => current = it.next().and_then(|m| materials.get(m)).copied().unwrap_or(GREY),
            Some("f") => {
                let mut corners = Vec::new();
                for corner in it {
                    let mut parts = corner.split('/');
                    // Indices are 1-based; negative ones count back from the end.
                    let resolve = |s: Option<&str>, len: usize| -> Option<usize> {
                        let i: i64 = s?.parse().ok()?;
                        let i = if i < 0 { len as i64 + i } else { i - 1 };
                        (i >= 0 && (i as usize) < len).then_some(i as usize)
                    };
                    let p = resolve(parts.next(), pos.len()).ok_or_else(|| Error(format!("OBJ line {}: bad vertex index in \"{corner}\"", n + 1)))?;
                    let _uv = parts.next();
                    let vn = resolve(parts.next(), nrm.len());
                    normals_complete &= vn.is_some();
                    // One vertex per corner; `done` welds the identical ones.
                    corners.push(mesh.pos.len() as u32);
                    mesh.pos.push(pos[p]);
                    mesh.nrm.push(vn.map_or([0.0; 3], |i| nrm[i]));
                    mesh.col.push(current);
                }
                for i in 1..corners.len().saturating_sub(1) {
                    mesh.idx.extend([corners[0], corners[i], corners[i + 1]]);
                }
            }
            _ => {}
        }
    }
    if !normals_complete {
        mesh.nrm.clear(); // finish() derives smooth normals
    }
    done(mesh, "OBJ")
}

type Mat4 = [[f32; 4]; 4];

fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}
fn point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}
fn direction(m: &Mat4, d: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * d[0] + m[1][r] * d[1] + m[2][r] * d[2])
}

/// glTF 2.0: `.glb`, or `.gltf` with embedded or external (`dir`-relative) buffers.
pub fn gltf(bytes: &[u8], dir: Option<&Path>) -> Result<Mesh> {
    let file = gltf::Gltf::from_slice(bytes).map_err(|e| Error(format!("glTF: {e}")))?;
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for buffer in file.document.buffers() {
        buffers.push(match buffer.source() {
            gltf::buffer::Source::Bin => file.blob.clone().ok_or_else(|| Error("glTF: the binary chunk is missing".into()))?,
            gltf::buffer::Source::Uri(uri) => match uri.split_once(";base64,") {
                Some((_, data)) => zuri_core::json::base64_decode(data).ok_or_else(|| Error("glTF: a buffer has invalid base64".into()))?,
                None => {
                    let path = dir.ok_or_else(|| Error(format!("glTF: external buffer \"{uri}\" needs a file path to resolve against")))?.join(uri);
                    std::fs::read(&path).map_err(|e| Error(format!("glTF: cannot read {}: {e}", path.display())))?
                }
            },
        });
    }
    let mut mesh = Mesh::default();
    let scene = file.document.default_scene().or_else(|| file.document.scenes().next());
    let mut stack: Vec<(gltf::Node, Mat4)> =
        scene.iter().flat_map(|s| s.nodes()).map(|n| (n, [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = mat_mul(&parent, &node.transform().matrix());
        stack.extend(node.children().map(|c| (c, world)));
        let Some(m) = node.mesh() else { continue };
        for prim in m.primitives().filter(|p| p.mode() == gltf::mesh::Mode::Triangles) {
            let reader = prim.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
            let Some(positions) = reader.read_positions() else { continue };
            let base = mesh.pos.len() as u32;
            let positions: Vec<[f32; 3]> = positions.collect();
            let tint = prim.material().pbr_metallic_roughness().base_color_factor();
            let colors: Option<Vec<[f32; 4]>> = reader.read_colors(0).map(|c| c.into_rgba_f32().collect());
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            if indices.iter().any(|i| *i as usize >= positions.len()) {
                return Err(Error("glTF: an index points past the end of its vertex data".into()));
            }
            let normals: Vec<[f32; 3]> = match reader.read_normals() {
                Some(n) => n.collect(),
                None => {
                    // Derive smooth normals for just this primitive.
                    let mut part = Mesh { pos: positions.clone(), idx: indices.clone(), ..Default::default() };
                    part.finish();
                    part.nrm
                }
            };
            for (i, p) in positions.iter().enumerate() {
                mesh.pos.push(point(&world, *p));
                mesh.nrm.push(direction(&world, normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0])));
                let c = colors.as_ref().and_then(|c| c.get(i)).copied().unwrap_or([1.0; 4]);
                mesh.col.push(color(std::array::from_fn(|k| c[k] * tint[k])));
            }
            mesh.idx.extend(indices.iter().map(|i| base + i));
        }
    }
    done(mesh, "glTF")
}

/// FBX (binary or ASCII, any version ufbx reads).
pub fn fbx(bytes: &[u8]) -> Result<Mesh> {
    let opts = ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        generate_missing_normals: true,
        ..Default::default()
    };
    let scene = ufbx::load_memory(bytes, opts).map_err(|e| Error(format!("FBX: {}", e.description)))?;
    let mut mesh = Mesh::default();
    let mut tri = Vec::new();
    for node in &scene.nodes {
        let Some(m) = &node.mesh else { continue };
        let world = &node.geometry_to_world;
        for (f, face) in m.faces.iter().enumerate() {
            let material = m.face_material.get(f).and_then(|i| m.materials.get(*i as usize));
            let tint = material.map_or([0.8, 0.8, 0.8, 1.0], |mat| {
                let map = if mat.pbr.base_color.has_value { &mat.pbr.base_color } else { &mat.fbx.diffuse_color };
                let v = map.value_vec4;
                if map.has_value {
                    [v.x as f32, v.y as f32, v.z as f32, 1.0]
                } else {
                    [0.8, 0.8, 0.8, 1.0]
                }
            });
            ufbx::triangulate_face_vec(&mut tri, m, *face);
            for ix in &tri {
                let ix = *ix as usize;
                let p = ufbx::transform_position(world, m.vertex_position[ix]);
                let n = if m.vertex_normal.exists { ufbx::transform_direction(world, m.vertex_normal[ix]) } else { ufbx::Vec3::default() };
                let c = if m.vertex_color.exists {
                    let c = m.vertex_color[ix];
                    [c.x as f32 * tint[0], c.y as f32 * tint[1], c.z as f32 * tint[2], c.w as f32]
                } else {
                    tint
                };
                mesh.idx.push(mesh.pos.len() as u32);
                mesh.pos.push([p.x as f32, p.y as f32, p.z as f32]);
                mesh.nrm.push([n.x as f32, n.y as f32, n.z as f32]);
                mesh.col.push(color(c));
            }
        }
    }
    done(mesh, "FBX")
}
