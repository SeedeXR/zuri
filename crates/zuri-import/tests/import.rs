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
    for (matrix, scale) in [("0.8660 0.5 -0.3536 0.6124", 0.70715), ("0.0866 0.05 -0.0354 0.0612", 0.0707)] {
        // (Its name has "null" in it, which is a name like another.)
        let written = imported(&format!(r##"<g id="Null 1" transform="matrix({matrix} 0 0)"><rect width="50" height="50"/></g>"##), "Null 1").1;
        assert_eq!((written["rotation"].as_f64().map(f64::round), &written["scaleY"]), (Some(30.0), &serde_json::json!(scale)), "{matrix}");
    }
    // A lean is a lean however small the drawing: it is not said as a turn.
    let small =
        imported(r##"<g id="small" transform="scale(0.001) skewX(10)"><rect width="900" height="900"/><rect width="500" height="500"/></g>"##, "small").1;
    assert_eq!(small, serde_json::json!({ "type": "Node", "name": "small" }));
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
fn a_lottie_animation_comes_in_with_its_layers_and_keys() {
    use serde_json::{json, Value};
    // 200 by 200, 60 frames at 30 a second. Each layer is a square of 20 somewhere of its own.
    fn v(k: impl Into<Value>) -> Value {
        json!({ "a": 0, "k": k.into() })
    }
    let ks = |p: [f64; 2]| json!({ "p": v(p), "a": v([0, 0]), "s": v([100, 100]), "r": v(0), "o": v(100) });
    let fill = |c: [f64; 3]| json!({ "ty": "fl", "c": v(c), "o": v(100) });
    let rect = |at: [f64; 2], size: f64| json!({ "ty": "rc", "p": v(at), "s": v([size, size]), "r": v(0) });
    let layer = |p: [f64; 2], shapes: Value| json!({ "ty": 4, "ip": 0, "op": 60, "st": 0, "ks": ks(p), "shapes": shapes });
    let square = |p: [f64; 2], c: [f64; 3]| layer(p, json!([rect([0.0, 0.0], 20.0), fill(c)]));
    let with = |mut layer: Value, key: &str, value: Value| {
        layer[key] = value;
        layer
    };
    let (red, green, blue) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let line = json!({ "c": false, "v": [[20, 60], [180, 60]], "i": [[0, 0], [0, 0]], "o": [[0, 0], [0, 0]] });
    let diamond = |half: f64| {
        json!([{ "c": true, "v": [[100.0 - half, 20], [100, 20.0 - half], [100.0 + half, 20], [100, 20.0 + half]],
        "i": [[0, 0], [0, 0], [0, 0], [0, 0]], "o": [[0, 0], [0, 0], [0, 0], [0, 0]] }])
    };
    let mut faded = layer([150.0, 140.0], json!([{ "ty": "sr", "sy": 1, "p": v([0, 0]), "pt": v(5), "or": v(15), "ir": v(6), "r": v(0) }, fill(green)]));
    faded["ks"]["o"] = v(25);
    let mut turned = ks([100.0, 100.0]);
    (turned["r"], turned["o"]) = (v(90), v(0));
    let mut anchored = square([80.0, 190.0], [0.0, 1.0, 1.0]);
    anchored["ks"]["a"] = v([-20, 0]);
    // In the precomposition: a yellow square that eases from 30 to 170 along the bottom in
    // 20 frames, waits there for 5 and is at the top. The layer that shows it starts it at frame 30.
    let mut eased = square([0.0, 0.0], [1.0, 1.0, 0.0]);
    eased["ks"]["p"] = json!({ "a": 1, "k": [
        { "t": 0, "s": [30, 170], "o": { "x": [0.333], "y": [0] }, "i": { "x": [0.667], "y": [1] } },
        { "t": 20, "s": [170, 170], "h": 1 }, { "t": 25, "s": [170, 10] } ] });
    let lottie = json!({ "v": "5.7.0", "fr": 30, "ip": 0, "op": 60, "w": 200, "h": 200,
      "assets": [{ "id": "comp", "layers": [eased] }, { "id": "picture", "w": 10, "h": 10, "p": "none.png" }],
      "layers": [
        // The first layer is on top: a red square over a blue one of 40.
        with(square([180.0, 100.0], red), "masksProperties", json!([{ "mode": "a" }])),
        // (Its white stroke comes before its fill: it is drawn over it.)
        layer([180.0, 100.0], json!([rect([0.0, 0.0], 40.0), { "ty": "st", "c": v([1, 1, 1]), "o": v(100), "w": v(6) }, fill(blue)])),
        // A star of five, 15 out and 6 in: a tip is up. Its layer is faded to a quarter.
        faded,
        // A null turned a quarter and faded to nothing, and a green square 50 along it: turned with it, and not faded.
        { "ty": 3, "ind": 7, "ip": 0, "op": 60, "ks": turned },
        with(square([50.0, 0.0], green), "parent", json!(7)),
        // One that comes in at frame 30.
        with(square([30.0, 30.0], blue), "ip", json!(30)),
        { "ty": 0, "refId": "comp", "ip": 0, "op": 60, "st": 30, "w": 200, "h": 200, "ks": ks([0.0, 0.0]) },
        // A white line across, trimmed to its first half.
        // (The trim comes after the group that has the line and its stroke: it cuts what is before it.)
        layer([0.0, 0.0], json!([{ "ty": "gr", "it": [{ "ty": "sh", "ks": v(line) }, { "ty": "st", "c": v([1, 1, 1]), "o": v(100), "w": v(6) },
            with(ks([0.0, 0.0]), "ty", json!("tr"))] }, { "ty": "tm", "s": v(0), "e": v(50), "o": v(0) }])),
        // A fill outside the group that the group of its square is in.
        layer([0.0, 0.0], json!([{ "ty": "gr", "it": [{ "ty": "gr", "it": [rect([160.0, 20.0], 20.0), with(ks([0.0, 0.0]), "ty", json!("tr"))] },
            with(ks([0.0, 0.0]), "ty", json!("tr"))] }, fill([1.0, 0.0, 1.0])])),
        // A matte and what it cuts: the matte is not drawn, the other is (whole). And one that the animation is over before it comes in.
        with(square([130.0, 100.0], red), "td", json!(1)),
        with(square([130.0, 75.0], green), "tt", json!(1)),
        with(with(square([130.0, 100.0], red), "ip", json!(70)), "nm", json!("never")),
        anchored,
        // A diamond that grows from 2 to 15 either way.
        layer([0.0, 0.0], json!([{ "ty": "sh", "ks": { "a": 1, "k": [
            { "t": 0, "s": diamond(2.0), "o": { "x": [0], "y": [0] }, "i": { "x": [1], "y": [1] } }, { "t": 60, "s": diamond(15.0) } ] } }, fill(red)])),
        // Red, then blue from the middle on (two stops at one place: a hard edge), over a square of 40;
        // clear a tenth of the way along, where only the opacity has a stop, and fading out over the last fifth.
        layer([40.0, 120.0], json!([rect([0.0, 0.0], 40.0),
            { "ty": "gf", "t": 1, "s": v([-20, 0]), "e": v([20, 0]), "g": { "p": 4, "k": v([0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.1, 0.0, 0.2, 1.0, 0.8, 1.0, 1.0, 0.0]) }, "o": v(100) }])),
        // A group that does nothing to what is in it: a small red square over a blue one of 30 in a group of its own.
        layer([60.0, 20.0], json!([{ "ty": "gr", "it": [rect([0.0, 0.0], 10.0), fill(red),
            { "ty": "gr", "it": [rect([0.0, 0.0], 30.0), fill(blue), with(ks([0.0, 0.0]), "ty", json!("tr"))] }, with(ks([0.0, 0.0]), "ty", json!("tr"))] }])),
        { "ty": 2, "refId": "picture", "ip": 0, "op": 60, "ks": ks([0.0, 0.0]) },
        { "ty": 0, "refId": "gone", "ip": 0, "op": 60, "ks": ks([0.0, 0.0]) },
        // A solid of 10 at the top left whose color is no color: black.
        { "ty": 1, "sw": 10, "sh": 10, "sc": "red", "ip": 0, "op": 60, "ks": ks([0.0, 0.0]) } ] });
    let dir = std::env::temp_dir().join("zuri-import-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("parts.json"), lottie.to_string()).unwrap();
    let (source, notes) = zuri_import::import_with_notes(&dir.join("parts.json")).unwrap();
    assert_eq!(
        notes,
        [
            "a precomposition that the file does not have, or one more than 16 deep",
            "a solid's color that is not written #rrggbb (it is black)",
            "image layers",
            "masks",
            "track mattes (the matte is left out, and what it cut is drawn whole)"
        ]
    );
    // The diamond is closed, at its start and in its two keys; the line is not. A layer that never shows is not there.
    assert_eq!((source.matches(" Z\"").count(), source.contains("never")), (3, false));

    let at = |frame: f32| render(&source, 200, 200, frame / 30.0);
    let (start, later, end) = (at(15.0), at(35.0), at(54.0));
    let is = |px: &[u8], x: u32, y: u32| pixel(px, 200, x, y).map(|c| (c > 127) as u8);
    let (none, yellow) = ([0, 0, 0, 0], [1, 1, 0, 1]);
    assert_eq!([is(&start, 180, 100), is(&start, 195, 100)], [[1, 0, 0, 1], [0, 0, 1, 1]], "the first layer is on top");
    assert_eq!(is(&start, 162, 115), [1, 1, 1, 1], "a stroke that comes before a fill is over it");
    assert_eq!([is(&start, 150, 128), is(&start, 150, 152)], [[0, 1, 0, 0], none], "a star has a tip up, and its layer is faded");
    assert_eq!([is(&start, 100, 150), is(&start, 150, 100)], [[0, 1, 0, 1], none], "a parent's turn is the child's, its fading is not");
    assert_eq!([is(&start, 30, 30), is(&later, 30, 30)], [none, [0, 0, 1, 1]], "a layer is there from its in point");
    // The precomposition: before it starts, what is in it has not come in; then eased (a
    // straight run would have the square about 65 at this frame, and it is about 48); then held.
    assert_eq!([is(&start, 30, 170), is(&later, 42, 170), is(&later, 70, 170)], [none, yellow, none]);
    assert_eq!([is(&end, 170, 170), is(&end, 170, 42)], [yellow, none], "a held key waits");
    assert_eq!([is(&start, 60, 60), is(&start, 140, 60)], [[1, 1, 1, 1], none], "the stroke is trimmed to half");
    assert_eq!(is(&start, 160, 20), [1, 0, 1, 1], "a fill paints the shapes of the groups before it");
    assert_eq!([is(&start, 130, 100), is(&start, 130, 75)], [none, [0, 1, 0, 1]], "a matte is not drawn");
    assert_eq!(is(&start, 5, 5), [0, 0, 0, 1], "a solid whose color cannot be read is black");
    assert_eq!([is(&start, 105, 190), is(&start, 85, 190)], [[0, 1, 1, 1], none], "the anchor is what is at the position");
    assert_eq!([is(&start, 110, 20), is(&end, 110, 20)], [none, [1, 0, 0, 1]], "a keyed path morphs");
    assert_eq!([is(&start, 37, 120), is(&start, 43, 120)], [[1, 0, 0, 1], [0, 0, 1, 1]], "the gradient is red, then blue, with a hard edge");
    assert_eq!(
        [is(&start, 21, 120), is(&start, 24, 120), is(&start, 30, 120)],
        [[1, 0, 0, 1], [1, 0, 0, 0], [1, 0, 0, 1]],
        "an opacity stop between the colors"
    );
    assert_eq!(is(&start, 58, 120), [0, 0, 1, 0], "a color stop has the opacity that is at its place");
    assert_eq!([is(&start, 60, 20), is(&start, 72, 20)], [[1, 0, 0, 1], [0, 0, 1, 1]], "a group without a transform keeps the order of what is in it");

    // What is written for the rest: a stroke's ends, corners, dashes and trim (all paths
    // at once unless the file says one after another), a fill's rule, a color of 0 to
    // 255 from an old file, and an opacity that is keyed as the alpha of the color. An
    // old file says that a path is closed beside the path, not in it.
    let corner = json!({ "v": [[0, 0], [10, 0], [10, 10]], "i": [[0, 0], [0, 0], [0, 0]], "o": [[0, 0], [0, 0], [0, 0]] });
    let styles = json!({ "fr": 30, "ip": 10, "op": 40, "w": 100, "h": 100, "layers": [layer([0.0, 0.0], json!([{ "ty": "sh", "closed": true, "ks": v(corner) },
        { "ty": "tm", "s": v(25), "e": v(75), "o": v(90), "m": 2 },
        { "ty": "st", "c": v([255, 128, 0]), "o": v(50), "lc": 2, "lj": 3,
          "w": { "a": 1, "k": [{ "t": 10, "s": [3], "e": [5], "h": 1 }, { "t": 20, "s": [5], "h": 1 }, { "t": 30 }, { "t": 40, "s": [7], "e": [9] }, { "t": 50 }] },
          "d": [{ "n": "d", "v": v(4) }, { "n": "g", "v": v(2) }, { "n": "o", "v": v(1) }] },
        { "ty": "fl", "r": 2, "c": v([0, 0, 1]), "o": { "a": 1, "k": [{ "t": 10, "s": [100], "h": 1 }, { "t": 25, "s": [0] }] } } ])),
        layer([0.0, 0.0], json!([{ "ty": "tm", "s": v(25), "e": v(75), "o": v(0) }, rect([50.0, 50.0], 20.0), { "ty": "st", "c": v([1, 1, 1]), "o": v(100), "w": v(2) },
            { "ty": "gf", "t": 2, "s": v([0, 0]), "e": v([3, 4]), "g": { "p": 2, "k": v([0, 1, 1, 1, 1, 0, 0, 0]) }, "o": v(100) }]))] });
    std::fs::write(dir.join("styles.json"), styles.to_string()).unwrap();
    let (source, notes) = zuri_import::import_with_notes(&dir.join("styles.json")).unwrap();
    assert_eq!(notes, ["a trim on a fill (strokes are trimmed)"]);
    let source: Value = serde_json::from_str(&source).unwrap();
    // (The second layer of the file is under the first: it is written first. Its trim comes before its shape, and cuts nothing.)
    let under = &source["children"][0]["children"][0]["children"];
    assert_eq!(under[2], json!({ "type": "Stroke", "color": "#ffffff", "thickness": 2.0 }));
    // (A round gradient is as wide as from its start to its end.)
    assert_eq!(
        under[1],
        json!({ "type": "Fill", "children": [{ "type": "RadialGradient", "radius": 5.0, "children": [
            { "type": "GradientStop", "position": 0.0, "color": "#ffffff" }, { "type": "GradientStop", "position": 1.0, "color": "#000000" }] }] })
    );
    let shape = &source["children"][0]["children"][1]["children"];
    assert_eq!(shape[0], json!({ "type": "Path", "d": "M0 0 C0 0 10 0 10 0 C10 0 10 10 10 10 C10 10 0 0 0 0 Z" }));
    assert_eq!(shape[1], json!({ "type": "Fill", "name": "fill 1", "color": "#0000ff", "fillRule": "evenodd" }));
    assert_eq!(
        shape[2],
        json!({ "type": "Stroke", "name": "stroke 2", "color": "#ff800080", "thickness": 3.0, "cap": "round", "join": "bevel", "dash": 4.0, "gap": 2.0, "dashOffset": 1.0,
            "trimStart": 0.25, "trimEnd": 0.75, "trimOffset": 0.25 })
    );
    // (The animation starts at the file's frame 10: that is its frame 0.)
    let tracks = &source["children"][0]["children"][2]["children"];
    assert_eq!(
        tracks[0],
        json!({ "type": "Track", "target": "fill 1", "property": "color",
            "children": [{ "type": "Key", "frame": 0.0, "colorValue": "#0000ff", "interp": "hold" }, { "type": "Key", "frame": 15.0, "colorValue": "#0000ff00" }] })
    );
    // A key of an old file that has no value is where the one before ends, or where that one is when it holds.
    let widths: Vec<f64> = tracks[1]["children"].as_array().unwrap().iter().map(|key| key["value"].as_f64().unwrap()).collect();
    assert_eq!((tracks[1]["target"].as_str(), widths), (Some("stroke 2"), vec![3.0, 5.0, 5.0, 7.0, 9.0]));
    assert_eq!(source["children"][0]["children"][2]["duration"], 30.0);

    // A precomposition that holds itself four times over is cut short, and said to be.
    let ring = json!({ "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "assets": [{ "id": "c", "layers": vec![json!({ "ty": 0, "refId": "c", "ip": 0, "op": 30 }); 4] }],
        "layers": [{ "ty": 0, "refId": "c", "ip": 0, "op": 30 }] });
    std::fs::write(dir.join("ring.json"), ring.to_string()).unwrap();
    let notes = zuri_import::import_with_notes(&dir.join("ring.json")).unwrap().1;
    assert!(notes.iter().any(|note| note.starts_with("layers beyond")), "{notes:?}");

    std::fs::write(dir.join("other.json"), r#"{ "zuri": 1, "children": [] }"#).unwrap();
    assert!(zuri_import::import_file(&dir.join("other.json")).unwrap_err().0.contains("not a Lottie animation"));
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
