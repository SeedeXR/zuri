//! Shader translation. Authors write one WGSL function:
//!
//! ```wgsl
//! fn shade(uv: vec2f) -> vec4f {            // uv: 0..1 across the shape, y down
//!     return vec4f(uv, 0.5 + 0.5 * sin(zuri.time), 1.0);   // straight-alpha color
//! }
//! ```
//! with `zuri.time` (seconds), `zuri.resolution` (pixels) and `zuri.params` (the paint's
//! p0..p3) in scope. `zuri build` wraps it into a fragment shader, validates it with naga
//! and stores GLSL ES 3.00 (WebGL2) and HLSL translations next to the source.

use naga::back::{glsl, hlsl};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use zuri_core::{Error, Result};

/// Uniform block every Zuri shader sees. std140: 32 bytes.
const PRELUDE: &str =
    "struct Zuri {\n    resolution: vec2f,\n    time: f32,\n    _pad: f32,\n    params: vec4f,\n}\n@group(0) @binding(0) var<uniform> zuri: Zuri;\n";
const ENTRY: &str =
    "\n@fragment\nfn zuri_fragment(@builtin(position) position: vec4f) -> @location(0) vec4f {\n    return shade(position.xy / zuri.resolution);\n}\n";

#[derive(Debug, Clone)]
pub struct Shader {
    pub glsl: String,
    pub hlsl: String,
}

pub fn translate_shader(wgsl: &str) -> Result<Shader> {
    let full = format!("{PRELUDE}{wgsl}{ENTRY}");
    // Diagnostics quote the offending line, so the prelude's line offset does not confuse.
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| Error(format!("shader does not parse:\n{}", e.emit_to_string(&full))))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| Error(format!("shader is not valid:\n{}", e.emit_to_string(&full))))?;

    let mut glsl_out = String::new();
    let options =
        glsl::Options { version: glsl::Version::Embedded { version: 300, is_webgl: true }, writer_flags: glsl::WriterFlags::empty(), ..Default::default() };
    let pipeline = glsl::PipelineOptions { shader_stage: naga::ShaderStage::Fragment, entry_point: "zuri_fragment".into(), multiview: None };
    glsl::Writer::new(&mut glsl_out, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default())
        .and_then(|mut w| w.write())
        .map_err(|e| Error(format!("shader cannot be translated to GLSL: {e}")))?;

    let mut hlsl_out = String::new();
    let pipeline = hlsl::PipelineOptions { entry_point: Some((naga::ShaderStage::Fragment, "zuri_fragment".into())) };
    hlsl::Writer::new(&mut hlsl_out, &hlsl::Options::default(), &pipeline)
        .write(&module, &info, None)
        .map_err(|e| Error(format!("shader cannot be translated to HLSL: {e}")))?;

    Ok(Shader { glsl: glsl_out, hlsl: hlsl_out })
}
