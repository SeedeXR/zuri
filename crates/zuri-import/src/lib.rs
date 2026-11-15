//! Authoring-time tools: everything that turns other formats into Zuri source, and the
//! full build step. None of this ships in a runtime; it is used by the CLI and the editor.

mod font;
mod mesh;
mod shader;
mod svg;

pub use mesh::{fbx, gltf, obj};
pub use shader::{translate_shader, Shader};

use serde_json::json;
use std::path::Path;
use zuri_core::format::Value;
use zuri_core::schema::{p, t};
use zuri_core::{Error, Result};

/// Import a file into a complete JSON source document (`{"zuri":1,"children":[...]}`).
///
/// - `.svg` becomes an Artboard of Shapes.
/// - `.obj`, `.gltf`, `.glb`, `.fbx` become a MeshAsset plus an Artboard that shows it in a
///   Model with a looping turntable animation.
pub fn import_file(path: &Path) -> Result<String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("imported").to_string();
    let bytes = std::fs::read(path).map_err(|e| Error(format!("cannot read {}: {e}", path.display())))?;
    let dir = path.parent();
    let mesh = match ext.as_str() {
        "svg" => {
            let (width, height, shapes) = svg::svg(&bytes)?;
            let doc = json!({ "zuri": 1, "children": [{ "type": "Artboard", "name": name, "width": width, "height": height, "children": shapes }] });
            return Ok(serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n");
        }
        "obj" => mesh::obj(&String::from_utf8_lossy(&bytes), dir)?,
        "gltf" | "glb" => mesh::gltf(&bytes, dir)?,
        "fbx" => mesh::fbx(&bytes)?,
        _ => return Err(Error(format!("cannot import \".{ext}\" files. Supported: svg, obj, gltf, glb, fbx"))),
    };
    let doc = json!({ "zuri": 1, "children": [
        { "type": "MeshAsset", "name": name, "bytes": zuri_core::json::base64_encode(&mesh.to_bytes()) },
        { "type": "Artboard", "name": name, "width": 512, "height": 512, "children": [
            { "type": "Model", "name": "model", "asset": name, "x": 256, "y": 256, "width": 480, "height": 480, "rotationX": 15 },
            { "type": "Animation", "name": "turntable", "fps": 60, "duration": 240, "loop": "loop", "children": [
                { "type": "Track", "target": "model", "property": "rotationY", "children": [
                    { "type": "Key", "frame": 0, "value": 0 },
                    { "type": "Key", "frame": 240, "value": 360 } ] } ] } ] } ] });
    Ok(serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n")
}

/// Compile JSON source into objects, doing everything a plain compile cannot:
/// - `"@path"` assets resolve relative to `base_dir`;
/// - a 3D file referenced that way (`.obj`, `.gltf`, `.glb`, `.fbx`) is converted to Zuri's
///   packed mesh on the spot, so the source keeps pointing at the original asset;
/// - every ShaderAsset's WGSL is validated and translated to GLSL and HLSL;
/// - a FontAsset with `glyphs` is cut down to the glyphs the document can show.
pub fn compile(json: &str, base_dir: &Path) -> Result<Vec<zuri_core::format::RawObject>> {
    let read = |rel: &str| -> Result<Vec<u8>> {
        let path = base_dir.join(rel);
        let bytes = std::fs::read(&path).map_err(|e| Error(format!("cannot read {}: {e}", path.display())))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        Ok(match ext.as_str() {
            "obj" => mesh::obj(&String::from_utf8_lossy(&bytes), path.parent())?.to_bytes(),
            "gltf" | "glb" => mesh::gltf(&bytes, path.parent())?.to_bytes(),
            "fbx" => mesh::fbx(&bytes)?.to_bytes(),
            _ => bytes,
        })
    };
    let mut objects = zuri_core::json::compile(json, &read)?;
    subset_fonts(&mut objects);
    for (i, o) in objects.iter_mut().enumerate() {
        if o.type_key != t::SHADER_ASSET {
            continue;
        }
        let Some(wgsl) = o.str(p::WGSL).map(String::from) else { continue };
        let name = o.str(p::NAME).unwrap_or("").to_string();
        let shader = translate_shader(&wgsl).map_err(|e| Error(format!("ShaderAsset '{name}' (object {i}): {e}")))?;
        o.props.retain(|(k, _)| *k != p::GLSL && *k != p::HLSL);
        o.props.push((p::GLSL, Value::Bytes(shader.glsl.into_bytes())));
        o.props.push((p::HLSL, Value::Bytes(shader.hlsl.into_bytes())));
        o.props.sort_by_key(|(k, _)| *k);
    }
    Ok(objects)
}

/// Cut every FontAsset that has `glyphs` down to the characters the document can show in
/// it: the texts set in that font, every string a bind could put into a text (string
/// values, Option names, formats; digits and a number's signs when something is bound to
/// a text), and the characters `glyphs` lists.
fn subset_fonts(objects: &mut [zuri_core::format::RawObject]) {
    let mut bound: String = objects.iter().flat_map(|o| [o.str(p::TEXT_VALUE), o.str(p::FORMAT)]).flatten().collect();
    bound.extend(objects.iter().filter(|o| o.type_key == t::OPTION).filter_map(|o| o.str(p::NAME)));
    if objects.iter().any(|o| o.type_key == t::BIND && o.uint(p::PROPERTY) == Some(p::TEXT as u64)) {
        bound.push_str("0123456789.-");
    }
    for i in 0..objects.len() {
        let o = &objects[i];
        let (t::FONT_ASSET, Some(listed), Some(font)) = (o.type_key, o.str(p::GLYPHS), o.bytes(p::BYTES)) else { continue };
        let texts = objects.iter().filter(|text| text.uint(p::ASSET) == Some(i as u64)).filter_map(|text| text.str(p::TEXT));
        let small = font::subset(font, &texts.chain([bound.as_str(), listed]).collect::<String>());
        let o = &mut objects[i];
        // The built file does not say `glyphs` any more: its font holds what it holds, and
        // subsetting it again after a dump and an edit would silently lose characters.
        o.props.retain(|(k, _)| *k != p::GLYPHS);
        // A font this cannot subset (see font.rs) stays whole.
        if let (Some(small), Some((_, bytes))) = (small, o.props.iter_mut().find(|(k, _)| *k == p::BYTES)) {
            *bytes = Value::Bytes(small);
        }
    }
}

/// Compile JSON source into validated `.zur` bytes (see [`compile`]).
pub fn build(json: &str, base_dir: &Path) -> Result<Vec<u8>> {
    let objects = compile(json, base_dir)?;
    zuri_core::File::from_objects(objects.clone())?;
    zuri_core::format::write(&objects, 0)
}
