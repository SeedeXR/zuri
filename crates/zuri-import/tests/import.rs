//! Importers and the full build step, run on real files: Blender-exported FBX / GLB / OBJ
//! in examples/assets and an inline SVG. Each test goes all the way to pixels.

use std::path::PathBuf;
use zuri_core::model::Mesh;
use zuri_core::{raster, File, Fit, Instance, Raster};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets")
}

fn render(source: &str, w: u32, h: u32, seconds: f32) -> Vec<u8> {
    let bytes = zuri_import::build(source, &assets()).unwrap();
    let file = File::load(&bytes).unwrap();
    let mut inst = Instance::new(&file, 0).unwrap();
    inst.autoplay();
    inst.advance(seconds);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let view = inst.fit(Fit::Contain, 0.5, 0.5, w as f32, h as f32);
    inst.render(&mut Raster::default(), &mut px, w, h, view);
    raster::unpremultiply(&mut px);
    px
}

fn pixel(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    px[((y * w + x) * 4) as usize..][..4].try_into().unwrap()
}

#[test]
fn fbx_glb_and_obj_agree() {
    let read = |name: &str| std::fs::read(assets().join(name)).unwrap();
    let meshes: Vec<Mesh> = vec![
        zuri_import::fbx(&read("rings.fbx")).unwrap(),
        zuri_import::gltf(&read("rings.glb"), None).unwrap(),
        zuri_import::obj(&String::from_utf8(read("rings.obj")).unwrap(), Some(&assets())).unwrap(),
    ];
    for m in &meshes {
        assert_eq!(m.idx.len() / 3, 1646, "same scene, same triangle count");
        assert!(m.pos.len() < 1200, "vertices are welded, got {}", m.pos.len());
        let radius = m.pos.iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f32::max);
        assert!((radius - 1.0).abs() < 1e-4, "normalized to the unit sphere");
        assert_eq!(Mesh::parse(&m.to_bytes()).unwrap(), *m);
        // Three materials survive: orange torus, blue sphere, green cone.
        let mut colors: Vec<[u8; 4]> = m.col.clone();
        colors.sort_unstable();
        colors.dedup();
        assert_eq!(colors.len(), 3, "{colors:?}");
    }
    // All three land Blender's +Z (the cone) on +Y, at the same height.
    let top = |m: &Mesh| m.pos.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
    assert!((top(&meshes[0]) - top(&meshes[1])).abs() < 1e-3 && (top(&meshes[1]) - top(&meshes[2])).abs() < 1e-3);
}

#[test]
fn imported_models_render_the_same_picture() {
    let frames: Vec<Vec<u8>> =
        ["rings.fbx", "rings.glb", "rings.obj"].iter().map(|f| render(&zuri_import::import_file(&assets().join(f)).unwrap(), 256, 256, 0.5)).collect();
    let covered = frames[0].chunks(4).filter(|p| p[3] > 0).count();
    assert!(covered > 256 * 256 / 5, "the model fills a good part of the viewport, got {covered} px");
    for other in &frames[1..] {
        let off = frames[0].iter().zip(other).filter(|(a, b)| a.abs_diff(**b) > 24).count();
        assert!(off * 100 < frames[0].len(), "{off} channel values differ between importers");
    }
    // The turntable animation actually turns the model.
    let later = render(&zuri_import::import_file(&assets().join("rings.glb")).unwrap(), 256, 256, 1.5);
    assert!(frames[1] != later);
}

#[test]
fn svg_shapes_fills_strokes_and_gradients() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100">
      <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
      <rect x="10" y="10" width="80" height="80" fill="#00cc66"/>
      <g transform="translate(100 0)" opacity="0.5"><circle id="dot" cx="50" cy="50" r="30" fill="#000000"/></g>
      <rect x="110" y="80" width="80" height="12" fill="url(#g)" stroke="#ffffff" stroke-width="2"/>
    </svg>"##;
    let dir = std::env::temp_dir().join("zuri-import-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("art.svg"), svg).unwrap();
    let source = zuri_import::import_file(&dir.join("art.svg")).unwrap();
    assert!(source.contains("\"name\": \"dot\""), "ids become names");
    let px = render(&source, 200, 100, 0.0);
    assert_eq!(pixel(&px, 200, 50, 50), [0x00, 0xcc, 0x66, 0xff]);
    assert_eq!(pixel(&px, 200, 150, 50), [0, 0, 0, 128], "group transform and opacity are applied");
    assert_eq!(pixel(&px, 200, 5, 5)[3], 0);
    let (left, right) = (pixel(&px, 200, 116, 86), pixel(&px, 200, 184, 86));
    assert!(left[0] > 200 && left[2] < 60 && right[2] > 200 && right[0] < 60, "gradient runs red to blue: {left:?} {right:?}");
    assert_eq!(pixel(&px, 200, 150, 80)[..3], [255, 255, 255], "stroke is centred on the rectangle edge");
}

#[test]
fn shaders_translate_and_bad_ones_explain_themselves() {
    let shader = zuri_import::translate_shader("fn shade(uv: vec2f) -> vec4f { return vec4f(uv, sin(zuri.time) * zuri.params.x, 1.0); }").unwrap();
    assert!(shader.glsl.starts_with("#version 300 es"), "{}", shader.glsl);
    assert!(shader.glsl.contains("uniform") && shader.glsl.contains("void main()"));
    assert!(shader.hlsl.contains("zuri_fragment") && shader.hlsl.contains("cbuffer"), "{}", shader.hlsl);

    let err = zuri_import::translate_shader("fn shade(uv: vec2f) -> vec4f { return nope; }").unwrap_err().0;
    assert!(err.contains("nope"), "{err}");
    let err = zuri_import::translate_shader("fn other() {}").unwrap_err().0;
    assert!(err.contains("shade"), "a missing shade() is reported: {err}");
}

#[test]
fn build_embeds_translated_shaders_and_falls_back_on_cpu() {
    let source = std::fs::read_to_string(assets().join("../shader.json")).unwrap();
    let bytes = zuri_import::build(&source, &assets()).unwrap();
    let file = File::load(&bytes).unwrap();
    let asset = file.roots_of(zuri_core::schema::t::SHADER_ASSET)[0] as usize;
    assert!(file.objects[asset].str(zuri_core::schema::p::GLSL).unwrap().contains("#version 300 es"));
    assert!(!file.objects[asset].str(zuri_core::schema::p::HLSL).unwrap().is_empty());

    let mut inst = Instance::new(&file, 0).unwrap();
    inst.autoplay();
    inst.advance(0.0);
    let mut px = vec![0u8; 400 * 240 * 4];
    let view = inst.fit(Fit::Contain, 0.5, 0.5, 400.0, 240.0);
    inst.render(&mut Raster::default(), &mut px, 400, 240, view);
    // No host texture yet: the Fill's own color shows.
    assert_eq!(pixel(&px, 400, 80, 120), [0x5b, 0x3d, 0xf5, 0xff]);
    let request = inst.shader_requests()[0];
    assert_eq!((request.width, request.height, request.params[0]), (320, 160, 1.0));
    assert!(inst.advance(1.0 / 60.0), "the looping pulse animation keeps this scene active");

    // A host supplies pixels (here: solid green) and the next frame uses them, in order
    // with the vector content around it.
    let green: Vec<u8> = [0, 255, 0, 255].repeat((request.width * request.height) as usize);
    assert!(inst.set_shader_texture(request.paint, request.width, request.height, &green));
    inst.render(&mut Raster::default(), &mut px, 400, 240, view);
    assert_eq!(pixel(&px, 400, 80, 120), [0, 255, 0, 255]);
    assert_eq!(pixel(&px, 400, 200, 120), [255, 255, 255, 255], "the white dot still draws on top");
    assert_eq!(pixel(&px, 400, 20, 20), [0x0e, 0x10, 0x16, 0xff], "and the shader is clipped to its shape");
}

#[test]
fn every_example_builds_runs_and_survives_a_dump_and_rebuild() {
    let examples = assets().join("..");
    let mut seen = 0;
    for entry in std::fs::read_dir(&examples).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        seen += 1;
        let bytes = zuri_import::build(&std::fs::read_to_string(&path).unwrap(), &examples).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // dump -> build reproduces the same bytes (assets are embedded after the first build).
        let text = zuri_core::json::decompile(&zuri_core::format::read(&bytes).unwrap()).unwrap();
        assert_eq!(bytes, zuri_import::build(&text, &examples).unwrap(), "{} is not stable through dump + build", path.display());
        let file = File::load(&bytes).unwrap();
        for i in 0..file.artboards.len() {
            let mut inst = Instance::new(&file, i).unwrap();
            inst.autoplay();
            inst.advance(0.1);
            let mut px = vec![0u8; 64 * 64 * 4];
            let view = inst.fit(Fit::Contain, 0.5, 0.5, 64.0, 64.0);
            inst.render(&mut Raster::default(), &mut px, 64, 64, view);
            assert!(px.chunks(4).any(|p| p[3] > 0), "{} drew nothing", path.display());
        }
    }
    assert!(seen >= 5, "expected the example scenes, found {seen}");
}

#[test]
fn a_font_with_glyphs_is_subset_and_draws_the_same() {
    // The asset folder is the base: the scene refers to "Outfit.ttf" directly.
    let scene = |glyphs: &str, text: &str, weight: u32| {
        format!(
            r##"{{ "zuri": 1, "children": [
              {{ "type": "FontAsset", "name": "Outfit", "bytes": "@Outfit.ttf" {glyphs} }},
              {{ "type": "Artboard", "width": 300, "height": 80, "children": [
                {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "n", "valueType": "number", "value": 42.5 }} ] }},
                {{ "type": "Text", "asset": "Outfit", "text": "{text}", "size": 40, "weight": {weight}, "x": 10, "y": 10, "children": [
                  {{ "type": "Fill", "color": "#ffffff" }} ] }},
                {{ "type": "Text", "asset": "Outfit", "size": 20, "x": 200, "y": 10, "children": [
                  {{ "type": "Fill", "color": "#ffffff" }}, {{ "type": "Bind", "source": "n", "property": "text", "decimals": 1 }} ] }} ] }} ] }}"##
        )
    };
    let size = |source: &str| zuri_import::build(source, &assets()).unwrap().len();
    let ink = |px: &[u8]| px.chunks(4).filter(|p| p[3] > 0).count();
    let (whole, small) = (scene("", "Zuri fónts é", 400), scene(r#", "glyphs": """#, "Zuri fónts é", 400));
    assert!(size(&whole) > 100_000 && size(&small) < 16_000, "{} -> {}", size(&whole), size(&small));

    // The same pixels at the default weight and at another one (the font is variable), for
    // the text and for the bound number, whose digits are kept without being asked for.
    for weight in [400, 800] {
        let (a, b) = (render(&scene("", "Zuri fónts é", weight), 300, 80, 0.0), render(&scene(r#", "glyphs": """#, "Zuri fónts é", weight), 300, 80, 0.0));
        assert!(ink(&a) > 1500 && a == b, "weight {weight}: {} vs {} inked pixels", ink(&a), ink(&b));
    }
    // Text set at run time shows only what was kept: listing a character in `glyphs` keeps it.
    let shown = |glyphs: &str| {
        let source = scene(glyphs, "", 400).replace(
            r##"{ "type": "Fill", "color": "#ffffff" } ] }"##,
            r##"{ "type": "Fill", "color": "#ffffff" }, { "type": "Bind", "source": "s", "property": "text" } ] }"##,
        );
        let source = source.replace(r#""value": 42.5 }"#, r#""value": 42.5 }, { "type": "Property", "name": "s", "valueType": "string" }"#);
        let file = File::load(&zuri_import::build(&source, &assets()).unwrap()).unwrap();
        let mut inst = Instance::new(&file, 0).unwrap();
        assert!(inst.set_string("s", "W"));
        inst.advance(0.0);
        let mut px = vec![0u8; 200 * 80 * 4]; // the left part: the bound number is further right
        inst.render(&mut Raster::default(), &mut px, 200, 80, inst.fit(Fit::None, 0.0, 0.0, 200.0, 80.0));
        ink(&px)
    };
    assert_eq!(shown(r#", "glyphs": """#), 0, "W was not kept");
    assert!(shown(r#", "glyphs": "W""#) > 300 && shown(r#", "glyphs": "W""#) == shown(""));

    // The built file does not ask to be subset again.
    let built = zuri_import::build(&small, &assets()).unwrap();
    let dumped = zuri_core::json::decompile(&zuri_core::format::read(&built).unwrap()).unwrap();
    assert!(!dumped.contains("glyphs") && zuri_import::build(&dumped, &assets()).unwrap() == built);
}
