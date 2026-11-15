//! Authoring-time tools: everything that turns other formats into Zuri source, and the
//! full build step. None of this ships in a runtime; it is used by the CLI and the editor.

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
/// - every ShaderAsset's WGSL is validated and translated to GLSL and HLSL.
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

/// Compile JSON source into validated `.zur` bytes (see [`compile`]).
pub fn build(json: &str, base_dir: &Path) -> Result<Vec<u8>> {
    let objects = compile(json, base_dir)?;
    zuri_core::File::from_objects(objects.clone())?;
    zuri_core::format::write(&objects, 0)
}
