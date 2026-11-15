//! End-to-end behaviour of the runtime, driven through the public API with the example
//! scenes: playback, state machines, listeners, data binding, events, settling, goldens.

use std::path::PathBuf;
use std::sync::Arc;
use zuri_core::{format, json, raster, File, Fit, Instance, Pointer, Raster};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(example: &str) -> Arc<File> {
    compile(&std::fs::read_to_string(root().join("examples").join(example)).unwrap())
}

/// JSON source, with `@` paths read from `examples/`, as a loaded file.
fn compile(src: &str) -> Arc<File> {
    let dir = root().join("examples");
    let objects = json::compile(src, &|rel| std::fs::read(dir.join(rel)).map_err(|e| zuri_core::Error(e.to_string()))).unwrap();
    // Go through real bytes so the binary format is on the path of every test.
    File::load(&format::write(&objects, 0).unwrap()).unwrap()
}

fn frame(inst: &mut Instance, w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let view = inst.fit(Fit::Contain, 0.5, 0.5, w as f32, h as f32);
    inst.render(&mut Raster::default(), &mut px, w, h, view);
    raster::unpremultiply(&mut px);
    px
}

/// Compare against `tests/golden/<name>.png`. Run with `ZURI_BLESS=1` to (re)write goldens.
/// Tolerance absorbs last-bit differences in libm trig between platforms, nothing more.
fn golden(name: &str, w: u32, h: u32, px: &[u8]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(format!("{name}.png"));
    if std::env::var_os("ZURI_BLESS").is_some() {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(&path).unwrap()), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(px).unwrap();
        return;
    }
    let mut reader =
        png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap_or_else(|_| panic!("missing golden {name}.png; run with ZURI_BLESS=1"))))
            .read_info()
            .unwrap();
    let mut want = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut want).unwrap();
    assert_eq!((info.width, info.height), (w, h), "{name}: size changed");
    let off = px.iter().zip(&want).filter(|(a, b)| a.abs_diff(**b) > 3).count();
    assert!(off * 2000 < px.len(), "{name}: {off} channel values differ from the golden by more than 3");
}

fn alpha_sum(px: &[u8]) -> u64 {
    px.chunks(4).map(|p| p[3] as u64).sum()
}

#[test]
fn animation_plays_loops_and_matches_golden() {
    let file = load("hello.json");
    let mut inst = Instance::new(&file, 0).unwrap();
    assert_eq!(inst.animation_names(), ["bounce"]);
    assert_eq!((inst.width(), inst.height()), (400.0, 300.0));
    assert!(inst.play_animation(0) && !inst.play_animation(9));

    assert!(inst.advance(0.0));
    golden("hello_t0", 400, 300, &frame(&mut inst, 400, 300));
    let ball = file.objects.iter().position(|o| o.str(zuri_core::schema::p::NAME) == Some("ball")).unwrap() as u32;
    let y = |inst: &Instance| inst.world_matrix(ball).unwrap().ty;
    assert_eq!(y(&inst), 90.0);
    // 64 steps of 1/64 s sum to exactly one second in binary floating point.
    for _ in 0..64 {
        assert!(inst.advance(1.0 / 64.0), "a looping animation never settles");
    }
    assert!((y(&inst) - 210.0).abs() < 0.01, "end of the pingpong leg, got {}", y(&inst));
    golden("hello_t1", 400, 300, &frame(&mut inst, 400, 300));
    for _ in 0..64 {
        inst.advance(1.0 / 64.0);
    }
    assert!((y(&inst) - 90.0).abs() < 0.01, "pingpong returns, got {}", y(&inst));
}

#[test]
fn rendering_is_deterministic_and_scale_independent() {
    let file = load("hello.json");
    let mut a = Instance::new(&file, 0).unwrap();
    let mut b = Instance::new(&file, 0).unwrap();
    for inst in [&mut a, &mut b] {
        inst.autoplay();
        inst.advance(0.4);
    }
    assert_eq!(frame(&mut a, 200, 150), frame(&mut b, 200, 150));
    // Twice the resolution covers ~4x the pixels: vector content, not a scaled bitmap.
    let (small, big) = (alpha_sum(&frame(&mut a, 200, 150)), alpha_sum(&frame(&mut a, 400, 300)));
    assert!((big as f64 / small as f64 - 4.0).abs() < 0.02, "{small} vs {big}");
}

#[test]
fn state_machine_listeners_binding_events_and_settling() {
    let file = load("button.json");
    let mut inst = Instance::new(&file, 0).unwrap();
    assert_eq!(inst.state_machine_names(), ["Button"]);
    inst.autoplay();
    let body = file.objects.iter().position(|o| o.str(zuri_core::schema::p::NAME) == Some("body")).unwrap() as u32;
    let scale = |inst: &Instance| inst.world_matrix(body).unwrap().a;
    let run = |inst: &mut Instance, secs: f32| {
        let mut active = false;
        for _ in 0..(secs * 60.0) as u32 {
            active = inst.advance(1.0 / 60.0);
        }
        active
    };

    assert!(inst.advance(0.0), "first frame always draws");
    frame(&mut inst, 320, 160); // establishes the view that pointer input is mapped through
    assert!(!run(&mut inst, 0.5), "idle button settles: nothing to redraw");
    assert_eq!(scale(&inst), 1.0);
    golden("button_idle", 320, 160, &frame(&mut inst, 320, 160));

    // Hover: listener sets the bool, the transition cross-fades over 0.15 s.
    inst.pointer(Pointer::Move, 160.0, 80.0);
    assert_eq!(inst.number("hover"), Some(1.0));
    inst.advance(0.075);
    assert!(scale(&inst) > 1.02 && scale(&inst) < 1.04, "half-way through the blend, got {}", scale(&inst));
    run(&mut inst, 0.5);
    assert!((scale(&inst) - 1.06).abs() < 1e-4);

    // Press + release inside = click: fires the trigger, bumps the counter, emits the event once.
    inst.pointer(Pointer::Down, 160.0, 80.0);
    run(&mut inst, 0.3);
    assert!((scale(&inst) - 0.95).abs() < 1e-4);
    inst.pointer(Pointer::Up, 160.0, 80.0);
    assert_eq!(inst.number("clicks"), Some(1.0));
    run(&mut inst, 1.0);
    assert_eq!(inst.take_events(), ["clicked"], "one click, one event");
    assert!(inst.take_events().is_empty());
    golden("button_checked", 320, 160, &frame(&mut inst, 320, 160));

    // Press inside, release outside = no click.
    inst.pointer(Pointer::Down, 160.0, 80.0);
    inst.pointer(Pointer::Move, 5.0, 5.0);
    inst.pointer(Pointer::Up, 5.0, 5.0);
    assert_eq!(inst.number("clicks"), Some(1.0));
    assert_eq!(inst.number("hover"), Some(0.0));
    run(&mut inst, 1.0);
    assert_eq!(scale(&inst), 1.0);

    // Data binding: the host recolors the button through the view model.
    let before = frame(&mut inst, 320, 160);
    assert!(inst.set_color("accent", 0xffff3366) && !inst.set_color("nope", 0));
    assert!(inst.advance(0.0), "a property change requests a redraw");
    let after = frame(&mut inst, 320, 160);
    let centre = (80 * 320 + 100) * 4;
    assert_eq!(&before[centre..centre + 3], [0x4f, 0x7c, 0xff]);
    assert_eq!(&after[centre..centre + 3], [0xff, 0x33, 0x66]);
    assert!(!run(&mut inst, 0.2), "and then it settles again");

    // Host-fired trigger behaves like a click on the check layer.
    assert!(inst.fire("pulse"));
    run(&mut inst, 0.1);
    assert_eq!(inst.take_events().len(), 0, "on -> off does not emit");
}

#[test]
fn text_lays_out_binds_and_animates_weight() {
    let file = load("text.json");
    let mut inst = Instance::new(&file, 0).unwrap();
    inst.autoplay();
    inst.advance(0.0);
    // Frame 0 of "rise": the title is transparent, only the body text shows.
    let start = frame(&mut inst, 400, 200);
    for _ in 0..64 {
        inst.advance(1.0 / 64.0);
    }
    let settled = frame(&mut inst, 400, 200);
    golden("text_settled", 400, 200, &settled);
    assert!(settled != start);
    assert!(!inst.advance(0.1), "a finished one-shot animation settles");
    let ink = |px: &[u8]| px.chunks(4).filter(|p| p[1] > 0x80).count(); // bright pixels = glyph ink

    // Data binding rewrites the text; more characters, more ink.
    assert!(inst.set_string("title", "Zuri Zuri"));
    assert!(inst.advance(0.0));
    let longer = frame(&mut inst, 400, 200);
    assert!(ink(&longer) > ink(&settled) + 500, "{} vs {}", ink(&longer), ink(&settled));
    assert_eq!(inst.string("title"), Some("Zuri Zuri"));
}

#[test]
fn text_runs_are_one_text_in_several_styles() {
    // `plain` is one text; `rich` says the same in Runs (`runs`), painted by `paints`.
    let source = |rich: &str, paints: &str, runs: &str| {
        format!(
            r##"{{ "zuri": 1, "children": [
              {{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }},
              {{ "type": "Artboard", "width": 300, "height": 140, "children": [
                {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "who", "valueType": "string", "value": "world" }} ] }},
                {{ "type": "Text", "name": "plain", "asset": "Outfit", "text": "Hello world", "size": 20, "x": 10, "y": 10, "children": [ {{ "type": "Fill", "color": "#ffffff" }} ] }},
                {{ "type": "Text", "name": "rich", "asset": "Outfit", "x": 10, "y": 50, {rich} "children": [ {{ "type": "Fill", "color": "#ffffff" }} {paints}, {runs} ] }} ] }} ] }}"##
        )
    };
    let scene = |rich: &str, runs: &str| compile(&source(rich, "", runs));
    // A Run in size 20; `more` is the rest of its JSON.
    let run = |text: &str, more: &str| format!(r##"{{ "type": "Run", "text": "{text}", "size": 20 {more} }}"##);
    let red = r##", "name": "accent", "children": [ { "type": "Fill", "color": "#ff0000" } ]"##;
    let play = |file: &Arc<File>| {
        let mut inst = Instance::new(file, 0).unwrap();
        inst.autoplay();
        inst.advance(0.0);
        inst
    };
    // The box of a Text or a Run, relative to where the Text is.
    let size = |inst: &Instance, name: &str| {
        let obj = inst.file().objects.iter().position(|o| o.str(zuri_core::schema::p::NAME) == Some(name)).unwrap() as u32;
        let (b, at) = (inst.bounds(obj).unwrap(), inst.world_matrix(obj).unwrap());
        [b[0] - at.tx, b[1] - at.ty, b[2] - at.tx, b[3] - at.ty].map(|v| (v * 100.0).round() / 100.0)
    };
    let pure = |px: &[u8], want: [u8; 3]| {
        (0..300usize).filter(|x| (50..90usize).any(|y| px[(y * 300 + x) * 4..][..4] == [want[0], want[1], want[2], 255])).collect::<Vec<_>>()
    };

    // Split into Runs (set in the Text's font, as they name none), the text is set
    // exactly as it was in one piece.
    let mut inst = play(&scene("", &format!("{}, {}", run("Hello ", ""), run("world", red))));
    assert_eq!(size(&inst, "rich"), size(&inst, "plain"));
    // A Run with a Fill of its own is painted by it; the others by the Text's.
    let px = frame(&mut inst, 300, 140);
    let (white, reds) = (pure(&px, [255, 255, 255]), pure(&px, [255, 0, 0]));
    assert!(white.len() > 20 && reds.len() > 20 && white.last() < reds.first(), "white then red: {white:?} {reds:?}");
    // A Run has a box of its own (the editor draws it around a selected Run).
    let (accent, whole) = (size(&inst, "accent"), size(&inst, "rich"));
    assert!(accent[0] > 40.0 && accent[2] == whole[2], "{accent:?} in {whole:?}");
    // Its paints go on top of the Text's: a wide outline on the rest of the text does
    // not run over it.
    let reds_under = |stroke: &str| {
        let file = compile(&source("", stroke, &format!("{}, {}", run("Hello", ""), run("world", red))));
        let px = frame(&mut play(&file), 300, 140);
        px.chunks(4).filter(|p| **p == [255, 0, 0, 255]).count()
    };
    let (outlined, bare) = (reds_under(r##", { "type": "Stroke", "color": "#ffffff", "thickness": 10 }"##), reds_under(""));
    assert!(bare > 50 && outlined == bare, "{outlined} vs {bare}");

    // Lines wrap between and inside Runs, and a line is as tall as its largest Run.
    let line = whole[3];
    let wrapped = size(&play(&scene(r#""width": 60,"#, &format!("{}, {}", run("Hello ", ""), run("world", red)))), "rich");
    assert!(wrapped[3] > line + 20.0 && wrapped[2] < 70.0, "{wrapped:?}");
    let big = r##"{ "type": "Run", "name": "big", "text": "world", "size": 40 }"##;
    let tall = size(&play(&scene("", &format!("{}, {big}", run("Hello ", "")))), "rich");
    assert!(tall[3] > line + 10.0 && tall[2] > whole[2] + 20.0, "{tall:?}");

    // A Run's text can be bound and its size animated; the Text is laid out again around it.
    let bound = run("world", r#", "children": [ { "type": "Bind", "source": "who", "property": "text" } ]"#);
    let mut inst = play(&scene("", &format!("{}, {bound}", run("Hello ", ""))));
    let before = size(&inst, "rich");
    assert_eq!(before, size(&inst, "plain"));
    frame(&mut inst, 300, 140);
    inst.set_string("who", "everybody");
    inst.advance(0.0);
    assert!(size(&inst, "rich")[2] > before[2] + 20.0);
    let grow = r##"{ "type": "Animation", "name": "grow", "children": [ { "type": "Track", "target": "accent", "property": "size", "children": [
        { "type": "Key", "frame": 0, "value": 20 }, { "type": "Key", "frame": 60, "value": 40 } ] } ] }, { "type": "Text", "name": "rich""##;
    let animated = source("", "", &format!("{}, {}", run("Hello ", ""), run("world", red))).replacen(r#"{ "type": "Text", "name": "rich""#, grow, 1);
    let mut inst = play(&compile(&animated));
    assert_eq!(size(&inst, "rich"), before);
    frame(&mut inst, 300, 140);
    inst.advance(1.0);
    assert!(size(&inst, "rich")[2] > before[2] + 20.0, "the animated Run grew");

    // Lines of a text in one size are where they always were: line n at n * size *
    // lineHeight below the first, to the last bit (not n additions of size * lineHeight).
    let bottom = |lines: usize| {
        let text = vec!["H"; lines].join("\\n");
        let file = compile(&format!(
            r##"{{ "zuri": 1, "children": [ {{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }},
              {{ "type": "Artboard", "width": 100, "height": 400, "children": [
                {{ "type": "Text", "name": "t", "asset": "Outfit", "text": "{text}", "size": 12, "lineHeight": 1.4 }} ] }} ] }}"##
        ));
        let inst = play(&file);
        inst.bounds(inst.file().objects.iter().position(|o| o.str(zuri_core::schema::p::NAME) == Some("t")).unwrap() as u32).unwrap()[3]
    };
    assert_eq!(bottom(12), bottom(1) + 11.0f32 * 12.0 * 1.4);
}

#[test]
fn letters_are_kerned_with_the_fonts_own_pairs() {
    // The box of a Text set in Outfit at size 50; `more` is the rest of its JSON.
    let bounds = |text: &str, more: &str| {
        let file = compile(&format!(
            r##"{{ "zuri": 1, "children": [ {{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }},
              {{ "type": "Artboard", "width": 400, "height": 200, "children": [
                {{ "type": "Text", "name": "t", "asset": "Outfit", "text": "{text}", "size": 50 {more} }} ] }} ] }}"##
        ));
        let inst = Instance::new(&file, 0).unwrap();
        inst.bounds(inst.file().objects.iter().position(|o| o.str(zuri_core::schema::p::NAME) == Some("t")).unwrap() as u32).unwrap()
    };
    let width = |text: &str, more: &str| bounds(text, more)[2] - bounds(text, more)[0];
    // The same six letters in two orders: only the pairs differ, and the first and last
    // letter are the same, so without kerning both are exactly as wide.
    let (apart, tucked) = (width("AAAVVV", ""), width("AVAVAV", ""));
    assert!(tucked < apart - 5.0, "A and V tuck under each other: {tucked} vs {apart}");
    // Kerning carries across Runs in the same font at the same size, and only those.
    let runs = |first: u32| format!(r#", "children": [ {{ "type": "Run", "text": "AVA", "size": {first} }}, {{ "type": "Run", "text": "VAV", "size": 50 }} ]"#);
    assert_eq!(width("", &runs(50)), tucked);
    // How far the small letters move the large ones: twice as far at twice their size,
    // which a kerning between the two sizes would add a constant to.
    let pen = |size: u32| bounds("", &runs(size))[2] - bounds("VAV", "")[2];
    assert!(pen(5) > 5.0 && (pen(10) - 2.0 * pen(5)).abs() < 0.01, "{} and {}", pen(5), pen(10));
    // A line ends where its last letter ends: the kerning against the space that wrapping
    // dropped does not count (Outfit kerns A against a space).
    let right = r#", "width": 120, "align": "right""#;
    assert_eq!(bounds("VA VA", right)[2], bounds("VA", right)[2]);
}

#[test]
fn hostile_files_error_instead_of_panicking() {
    let file = load("button.json");
    let good = format::write(&file.objects, 0).unwrap();
    // Corrupt every byte in turn: loading and running must never panic.
    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] = bad[i].wrapping_add(0x55);
        if let Ok(f) = File::load(&bad) {
            for a in 0..f.artboards.len() {
                if let Ok(mut inst) = Instance::new(&f, a) {
                    inst.autoplay();
                    inst.advance(0.05);
                    inst.pointer(Pointer::Move, 10.0, 10.0);
                    frame(&mut inst, 32, 16);
                }
            }
        }
    }
}
