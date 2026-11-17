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

/// An SVG as its imported source, and the Node in it called `name` (its properties).
fn imported(svg: &str, name: &str) -> (String, serde_json::Value) {
    let dir = std::env::temp_dir().join("zuri-import-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("groups.svg");
    std::fs::write(&path, format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">{svg}</svg>"##)).unwrap();
    let source = zuri_import::import_file(&path).unwrap();
    fn find(node: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
        if node["name"] == name {
            return Some(node.clone());
        }
        node["children"].as_array()?.iter().find_map(|child| find(child, name))
    }
    let mut node = find(&serde_json::from_str(&source).unwrap(), name).unwrap_or_else(|| panic!("no {name} in {source}"));
    node.as_object_mut().unwrap().remove("children");
    (source, node)
}

#[test]
fn svg_groups_keep_their_transforms_to_animate() {
    let covered = |source: &str, x: u32, y: u32| pixel(&render(source, 200, 200, 0.0), 200, x, y)[3];
    // A bar turned 30 degrees about its middle: a Node there, with that rotation, and the
    // bar about it. Turned back in the source (as a key would), the bar lies level.
    let (source, spin) = imported(r##"<g id="spin" transform="rotate(30 100 100)"><rect x="60" y="80" width="80" height="40" fill="#ee3333"/></g>"##, "spin");
    assert_eq!(spin, serde_json::json!({ "type": "Node", "name": "spin", "x": 100.0, "y": 100.0, "rotation": 30.0 }));
    assert_eq!([covered(&source, 120, 125), covered(&source, 120, 85)], [255, 0], "turned");
    let level = source.replace(r#""rotation": 30.0"#, r#""rotation": 0"#);
    assert_eq!([covered(&level, 120, 125), covered(&level, 120, 85)], [0, 255], "level");

    // A mirror is a scale below nothing, with no turn; the triangle is on the other side.
    let (source, mirror) =
        imported(r##"<g id="mirror" transform="translate(200 0) scale(-1 1)"><path d="M20 20 L120 40 L40 160 Z" fill="#cc8800"/></g>"##, "mirror");
    assert_eq!(mirror, serde_json::json!({ "type": "Node", "name": "mirror", "x": 130.0, "y": 90.0, "scaleX": -1.0 }));
    assert_eq!([covered(&source, 140, 73), covered(&source, 60, 73)], [255, 0]);

    // A hand of a clock turns about the clock's middle, not its own: the Node is at the
    // point the transform leaves where it is.
    let hand = r##"<g id="hand" transform="rotate(30 100 100)"><rect x="98" y="30" width="4" height="70" fill="#000000"/></g>"##;
    assert_eq!(imported(hand, "hand").1, serde_json::json!({ "type": "Node", "name": "hand", "x": 100.0, "y": 100.0, "rotation": 30.0 }));
    // So it is inside a clock that is a Node itself (at the middle of its face, which is
    // where the hand turns: the hand is at nothing from it). A matrix written to four
    // decimals is still a turn and a scale, and a scale beyond what a number holds is refused.
    let clock = format!(r##"<g id="clock" transform="translate(10 10)"><circle cx="100" cy="100" r="80" fill="#ffffff"/>{hand}</g>"##);
    assert_eq!(imported(&clock, "hand").1, serde_json::json!({ "type": "Node", "name": "hand", "rotation": 30.0 }));
    let written = imported(r##"<g id="m" transform="matrix(0.8660 0.5 -0.3536 0.6124 0 0)"><rect width="50" height="50"/></g>"##, "m").1;
    assert_eq!((written["rotation"].as_f64().map(f64::round), &written["scaleY"]), (Some(30.0), &serde_json::json!(0.70715)));
    let huge = r##"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9"><g id="a" transform="scale(1e20)"><g id="b" transform="scale(1e20)"><rect width="1" height="1"/><rect width="2" height="1"/></g></g></svg>"##;
    let path = std::env::temp_dir().join("zuri-import-test").join("huge.svg");
    std::fs::write(&path, huge).unwrap();
    assert!(zuri_import::import_file(&path).unwrap_err().0.contains("too large"));

    // A group in a group, each with its own; the square is where the two put it (its
    // middle at 40 + 60 cos 20 - 30 sin 20 across and 30 + 60 sin 20 + 30 cos 20 down).
    let nested = r##"<g transform="translate(40 30)"><g id="in" transform="rotate(20) scale(1.5 0.75)"><rect x="10" y="10" width="60" height="60" fill="#33aa33"/></g></g>"##;
    let (source, inner) = imported(nested, "in");
    assert_eq!([&inner["rotation"], &inner["scaleX"], &inner["scaleY"]], [20.0, 1.5, 0.75].map(|v| serde_json::json!(v)).each_ref());
    assert_eq!([covered(&source, 86, 79), covered(&source, 40, 30), covered(&source, 130, 60)], [255, 0, 0]);

    // An element with a transform of its own is one Shape that says it, by its own name;
    // so is what a `use` places. A group that says nothing leaves no Node.
    let elements = r##"<defs><rect id="tile" width="40" height="40" fill="#aa5500"/></defs><use href="#tile" transform="translate(120 120) rotate(45)"/>
      <g><rect id="bar" transform="rotate(30 60 60)" opacity="0.5" x="20" y="50" width="80" height="20" fill="#ee3333"/></g>"##;
    let (source, bar) = imported(elements, "bar");
    assert_eq!(bar, serde_json::json!({ "type": "Shape", "name": "bar", "x": 60.0, "y": 60.0, "rotation": 30.0, "opacity": 0.5 }));
    assert_eq!((source.contains("\"Node\""), source.contains("\"rotation\": 45.0"), covered(&source, 120, 140)), (false, true, 255), "{source}");
    // (The tile's turn leaves a point still that is far from it: it is at its own middle.)
    assert!(source.contains("\"y\": 148.284"), "{source}");
    // A Node is at the middle of all it holds, as the groups inside it place that.
    let (_, pair) = imported(
        r##"<g id="pair" transform="translate(10 10)"><g transform="translate(100 0)"><rect width="20" height="20"/></g><rect width="20" height="20"/></g>"##,
        "pair",
    );
    assert_eq!(pair, serde_json::json!({ "type": "Node", "name": "pair", "x": 70.0, "y": 20.0 }));

    // Under a large scale the points and the widths are written finer: a hairline in a
    // drawing one unit across is still there.
    let hairline = r##"<g id="big" transform="scale(2000)"><path d="M0.01012 0.01049 L0.08038 0.01021 L0.04049 0.08049 Z" fill="none" stroke="#000000" stroke-width="0.0004"/></g>"##;
    let (source, _) = imported(hairline, "big");
    assert!(source.contains("\"thickness\": 0.0004"), "{source}");

    // A Node cannot lean: the lean goes into the points, the Node keeps its name and its
    // opacity, and the picture is the same (the circle's middle is 150 * tan 25 further on).
    // A group inside it has the lean put into its own matrix, which then cannot be said
    // either: its bar is where the lean of what is turned puts it (its middle at 98, 80).
    let leaning = r##"<g id="lean" transform="skewX(25)" opacity="0.5"><circle cx="60" cy="150" r="25" fill="#009999"/>
      <g transform="translate(10 20) rotate(10)"><rect x="20" y="20" width="80" height="60" fill="#990099"/></g></g>"##;
    let (source, lean) = imported(leaning, "lean");
    assert_eq!(lean, serde_json::json!({ "type": "Node", "name": "lean", "opacity": 0.5 }));
    let px = render(&source, 200, 200, 0.0);
    assert_eq!([pixel(&px, 200, 130, 150), pixel(&px, 200, 130, 80)], [[0, 0x99, 0x99, 128], [0x99, 0, 0x99, 128]]);

    // A stroke under an uneven scale is stretched with its shape, as SVG has it: a ring
    // of 6 is 12 wide at its sides and 3 at its top.
    let (source, _) =
        imported(r##"<g id="wide" transform="translate(100 100) scale(2 0.5)"><circle r="30" fill="none" stroke="#0066cc" stroke-width="6"/></g>"##, "wide");
    assert_eq!([covered(&source, 35, 100), covered(&source, 45, 100), covered(&source, 100, 85), covered(&source, 100, 88)], [255, 255, 255, 0]);
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
fn a_subset_font_keeps_the_characters_of_runs_set_in_it() {
    // The Run names no font: it is set in its Text's, and its letters appear nowhere else.
    let scene = |glyphs: &str| {
        format!(
            r##"{{ "zuri": 1, "children": [
              {{ "type": "FontAsset", "name": "Outfit", "bytes": "@Outfit.ttf" {glyphs} }},
              {{ "type": "Artboard", "width": 200, "height": 80, "children": [
                {{ "type": "Text", "asset": "Outfit", "x": 10, "y": 10, "children": [
                  {{ "type": "Fill", "color": "#ffffff" }}, {{ "type": "Run", "text": "Wqk", "size": 40 }} ] }} ] }} ] }}"##
        )
    };
    let (whole, small) = (render(&scene(""), 200, 80, 0.0), render(&scene(r#", "glyphs": """#), 200, 80, 0.0));
    assert!(whole.chunks(4).filter(|p| p[3] > 0).count() > 500 && whole == small);
}

#[test]
fn a_font_listed_whole_is_not_made_bigger() {
    // Every character of the font kept: there is nothing to save, and writing the kerning
    // of all of its glyphs out pair by pair would be larger than how the font holds it.
    let font = std::fs::read(assets().join("Outfit.ttf")).unwrap();
    let face = ttf_parser::Face::parse(&font, 0).unwrap();
    let mut all = String::new();
    for table in face.tables().cmap.unwrap().subtables {
        table.codepoints(|c| all.extend(char::from_u32(c).filter(|c| !c.is_control() && !matches!(c, '"' | '\\'))));
    }
    let scene = |glyphs: &str| {
        format!(
            r##"{{ "zuri": 1, "children": [ {{ "type": "FontAsset", "name": "Outfit", "bytes": "@Outfit.ttf" {glyphs} }},
              {{ "type": "Artboard", "width": 300, "height": 80, "children": [
                {{ "type": "Text", "asset": "Outfit", "text": "AVATAR Toy", "size": 40, "x": 10, "y": 10, "children": [ {{ "type": "Fill", "color": "#ffffff" }} ] }} ] }} ] }}"##
        )
    };
    let (whole, listed) = (scene(""), scene(&format!(r#", "glyphs": "{all}""#)));
    let size = |source: &str| zuri_import::build(source, &assets()).unwrap().len();
    assert!(all.chars().count() > 200 && size(&listed) <= size(&whole), "{} vs {}", size(&listed), size(&whole));
    assert!(render(&whole, 300, 80, 0.0) == render(&listed, 300, 80, 0.0));
    // A good part of the font kept (Latin and its accents): kerned as the whole font is.
    let latin: String = all.chars().filter(|c| (*c as u32) < 0x250).collect();
    let part = scene(&format!(r#", "glyphs": "{latin}""#));
    assert!(size(&part) < size(&whole) && render(&whole, 300, 80, 0.0) == render(&part, 300, 80, 0.0), "{} vs {}", size(&part), size(&whole));
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
