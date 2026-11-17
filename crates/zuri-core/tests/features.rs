//! One focused scene per feature, checked through the public API. If a feature is listed
//! as done in docs/parity.md, a test here (or in runtime.rs) drives it.

use std::sync::Arc;
use zuri_core::schema::p;
use zuri_core::{format, json, raster, File, Fit, Instance, Pointer, Raster};

fn load(children: &str) -> Arc<File> {
    let src = format!(r#"{{ "zuri": 1, "children": [ {children} ] }}"#);
    let objects = json::compile(&src, &|_| unreachable!()).unwrap_or_else(|e| panic!("{e}"));
    File::load(&format::write(&objects, 0).unwrap()).unwrap()
}

/// An instance of the first artboard, playing its first state machine or animation.
fn play(file: &Arc<File>) -> Instance {
    let mut inst = Instance::new(file, 0).unwrap();
    inst.autoplay();
    inst.advance(0.0);
    inst
}

/// Render 1:1 (straight alpha) and return a pixel reader.
fn pixels(inst: &mut Instance) -> impl Fn(u32, u32) -> [u8; 4] {
    let (w, h) = (inst.width() as u32, inst.height() as u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let view = inst.fit(Fit::None, 0.5, 0.5, w as f32, h as f32);
    inst.render(&mut Raster::default(), &mut px, w, h, view);
    raster::unpremultiply(&mut px);
    move |x, y| px[((y * w + x) * 4) as usize..][..4].try_into().unwrap()
}

fn object(file: &File, name: &str) -> u32 {
    file.objects.iter().position(|o| o.str(p::NAME) == Some(name)).unwrap_or_else(|| panic!("no object named {name}")) as u32
}

fn x_of(inst: &Instance, name: &str) -> f32 {
    inst.world_matrix(object(inst.file(), name)).unwrap().tx
}

const RED: [u8; 4] = [255, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[test]
fn clip_restricts_a_subtree_to_another_shapes_outline() {
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "Shape", "name": "mask", "x": 50, "y": 50, "children": [ { "type": "Ellipse", "width": 40, "height": 40 } ] },
          { "type": "Node", "children": [
            { "type": "Clip", "source": "mask" },
            { "type": "Shape", "x": 50, "y": 50, "children": [ { "type": "Rectangle", "width": 100, "height": 100 }, { "type": "Fill", "color": "#ff0000" } ] } ] } ] }"##,
    );
    let at = pixels(&mut play(&file));
    assert_eq!(at(50, 50), RED);
    assert_eq!(at(50, 33), RED, "inside the circle's radius of 20");
    assert_eq!(at(50, 27), CLEAR, "outside it");
    assert_eq!(at(5, 5), CLEAR);

    // An Image clips with its box; what has no outline of its own is refused at compile time.
    let boxed = |source: &str| {
        format!(
            r##"{{ "zuri": 1, "children": [ {{ "type": "Artboard", "width": 100, "height": 100, "children": [
              {{ "type": "Image", "name": "frame", "x": 50, "y": 50, "width": 20, "height": 20 }},
              {{ "type": "Node", "name": "group", "children": [
                {{ "type": "Clip", "source": "{source}" }},
                {{ "type": "Shape", "x": 50, "y": 50, "children": [ {{ "type": "Rectangle", "width": 100, "height": 100 }}, {{ "type": "Fill", "color": "#ff0000" }} ] }} ] }} ] }} ] }}"##
        )
    };
    let file = File::load(&format::write(&json::compile(&boxed("frame"), &|_| unreachable!()).unwrap(), 0).unwrap()).unwrap();
    let at = pixels(&mut play(&file));
    assert_eq!((at(50, 50), at(58, 58), at(62, 50)), (RED, RED, CLEAR));
    let err = json::compile(&boxed("group"), &|_| unreachable!()).unwrap_err().0;
    assert!(err.contains("\"group\" is a Node; this needs: Shape, Text, Image, Model"), "{err}");
}

#[test]
fn opacity_multiplies_down_the_tree_and_blend_modes_apply() {
    let file = load(
        r##"{ "type": "Artboard", "width": 40, "height": 20, "children": [
          { "type": "Fill", "color": "#ffffff" },
          { "type": "Node", "opacity": 0.5, "children": [
            { "type": "Shape", "x": 10, "y": 10, "opacity": 0.5, "children": [ { "type": "Rectangle", "width": 20, "height": 20 }, { "type": "Fill", "color": "#000000" } ] } ] },
          { "type": "Shape", "x": 30, "y": 10, "blendMode": "multiply", "children": [ { "type": "Rectangle", "width": 20, "height": 20 }, { "type": "Fill", "color": "#808080" } ] } ] }"##,
    );
    let at = pixels(&mut play(&file));
    // 0.5 * 0.5 = 25 % black over white.
    assert!(at(10, 10)[0].abs_diff(191) <= 1, "{:?}", at(10, 10));
    assert_eq!(at(30, 10), [128, 128, 128, 255], "white x mid grey");
}

#[test]
fn nested_artboards_are_live_instances() {
    let file = load(
        r##"{ "type": "Artboard", "name": "Parent", "width": 100, "height": 100, "children": [
          { "type": "NestedArtboard", "artboard": "Child", "x": 40, "y": 40, "scaleX": 2, "scaleY": 2 } ] },
        { "type": "Artboard", "name": "Child", "width": 20, "height": 20, "children": [
          { "type": "Fill", "color": "#00ff00" },
          { "type": "Shape", "name": "dot", "x": 5, "y": 10, "children": [ { "type": "Rectangle", "width": 4, "height": 4 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "Animation", "name": "slide", "fps": 60, "duration": 60, "children": [
            { "type": "Track", "target": "dot", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 5 }, { "type": "Key", "frame": 60, "value": 15 } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    let at = pixels(&mut inst);
    assert_eq!(at(45, 45), [0, 255, 0, 255], "child background, scaled 2x from (40, 40)");
    assert_eq!(at(50, 60), RED, "child dot at child (5, 10)");
    assert_eq!(at(85, 45), CLEAR, "the child clips to its own 20x20 bounds (40 px here)");
    // The child plays its own animation while the parent advances.
    assert!(inst.advance(1.0));
    let at = pixels(&mut inst);
    assert_eq!(at(70, 60), RED, "dot moved to child x = 15");
    assert_eq!(at(50, 60), [0, 255, 0, 255]);
}

#[test]
fn images_draw_host_decoded_pixels() {
    let file = load(
        r##"{ "type": "ImageAsset", "name": "quad", "bytes": "AAAA", "width": 2, "height": 2 },
        { "type": "Artboard", "width": 40, "height": 40, "children": [
          { "type": "Image", "asset": "quad", "x": 20, "y": 20, "width": 40, "height": 40, "opacity": 0.5 } ] }"##,
    );
    let asset = object(&file, "quad");
    let mut inst = play(&file);
    assert_eq!(pixels(&mut inst)(10, 10), CLEAR, "nothing to draw until the host supplies pixels");
    // 2x2: red, green / blue, white.
    file.set_image(asset, 2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]).unwrap();
    assert!(file.set_image(asset, 2, 2, &[0; 3]).is_err() && file.set_image(asset + 1, 1, 1, &[0; 4]).is_err());
    let at = pixels(&mut inst);
    assert_eq!(at(2, 2), [255, 0, 0, 128], "top-left texel, at the node's 50 % opacity");
    assert_eq!(at(37, 2), [0, 255, 0, 128]);
    assert_eq!(at(2, 37), [0, 0, 255, 128]);
    assert_eq!(at(37, 37), [255, 255, 255, 128]);
}

#[test]
fn nine_sliced_images_keep_their_edges() {
    // A 4 x 4 picture: a one-pixel red frame around a blue middle, drawn 24 x 12. The host
    // decodes it `n` pixels a side.
    let scene = |slices: &str, n: usize| {
        let file = load(&format!(
            r##"{{ "type": "ImageAsset", "name": "panel", "bytes": "AAAA", "width": 4, "height": 4 }},
            {{ "type": "Artboard", "width": 24, "height": 12, "children": [
              {{ "type": "Image", "asset": "panel", "x": 12, "y": 6, "width": 24, "height": 12 {slices} }} ] }}"##
        ));
        let edge = |v: usize| v < n / 4 || v >= n - n / 4;
        let rgba: Vec<u8> = (0..n * n).flat_map(|i| if edge(i % n) || edge(i / n) { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
        file.set_image(object(&file, "panel"), n as u32, n as u32, &rgba).unwrap();
        pixels(&mut play(&file))
    };
    let all = r#", "sliceLeft": 1, "sliceTop": 1, "sliceRight": 1, "sliceBottom": 1"#;
    let (plain, sliced) = (scene("", 4), scene(all, 4));
    // Stretched plainly the frame is 6 wide and 3 high; sliced it stays one pixel all round.
    for (x, y) in [(3, 6), (20, 6), (12, 2), (12, 9)] {
        assert!(plain(x, y)[0] > plain(x, y)[2], "plain {x},{y}: more frame than middle, {:?}", plain(x, y));
        assert_eq!(sliced(x, y), [0, 0, 255, 255], "sliced {x},{y}: the middle, with none of the frame's colour");
    }
    for (x, y) in [(0, 6), (23, 6), (12, 0), (12, 11), (0, 0), (23, 11)] {
        assert_eq!(sliced(x, y), RED, "the frame, at {x},{y}");
    }
    // Slices that do not fit (too long for the picture, a middle under a pixel wide, a box
    // too small) are shortened in proportion: nothing tears, nothing panics.
    for odd in [r#""sliceLeft": 20, "sliceRight": 20"#, r#""sliceLeft": 2.6, "sliceRight": 1"#, r#""sliceLeft": 1, "width": 1.5"#] {
        let at = scene(&format!(", {odd}"), 4);
        assert_eq!(at(12, 0)[..3], RED[..3], "{odd}: the frame's colour (the narrow box covers part of this pixel)");
    }
    // The host may decode the picture at another size (8 x 8 here): the slices are in the
    // asset's pixels, so the frame is still one unit wide.
    let at = scene(all, 8);
    assert_eq!((at(0, 6), at(1, 6), at(12, 0), at(12, 1), at(23, 11)), (RED, [0, 0, 255, 255], RED, [0, 0, 255, 255], RED));
}

#[test]
fn keys_interpolate_hold_morph_paths_and_blend_colors() {
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "Shape", "name": "a", "children": [ { "type": "Path", "name": "outline", "d": "M0 0 L10 0 L10 10 Z" }, { "type": "Fill", "name": "ink", "color": "#000000" } ] },
          { "type": "Animation", "name": "all", "fps": 10, "duration": 10, "children": [
            { "type": "Track", "target": "a", "property": "x", "children": [
              { "type": "Key", "frame": 0, "value": 0, "interp": "hold" }, { "type": "Key", "frame": 5, "value": 50 }, { "type": "Key", "frame": 10, "value": 100 } ] },
            { "type": "Track", "target": "outline", "property": "d", "children": [
              { "type": "Key", "frame": 0, "value": "M0 0 L10 0 L10 10 Z" }, { "type": "Key", "frame": 10, "value": "M0 0 L30 0 L30 30 Z" } ] },
            { "type": "Track", "target": "ink", "property": "color", "children": [
              { "type": "Key", "frame": 0, "value": "#000000" }, { "type": "Key", "frame": 10, "value": "#ff0000" } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    let width = |inst: &Instance| {
        let b = inst.bounds(object(inst.file(), "a")).unwrap();
        b[2] - b[0]
    };
    inst.advance(0.4); // frame 4: x is held at 0, the path is 40 % morphed
    assert_eq!(x_of(&inst, "a"), 0.0);
    assert!((width(&inst) - 18.0).abs() < 1e-3, "{}", width(&inst));
    inst.advance(0.1); // frame 5: the hold ends
    assert_eq!(x_of(&inst, "a"), 50.0);
    inst.advance(0.25); // frame 7.5: linear between 50 and 100
    assert!((x_of(&inst, "a") - 75.0).abs() < 1e-3);
    assert!((width(&inst) - 25.0).abs() < 1e-3);
    let at = pixels(&mut inst);
    assert!(at(90, 5)[0].abs_diff(191) <= 1 && at(90, 5)[3] == 255, "color is 75 % of the way to red: {:?}", at(90, 5));
    assert!(!inst.advance(1.0) || !inst.advance(0.0), "a one-shot animation ends and settles");
    assert_eq!(x_of(&inst, "a"), 100.0);
}

/// Two poses (x = 0 and x = 100) and a view model, for the state-machine tests.
const POSES: &str = r##"
  { "type": "Shape", "name": "s", "children": [ { "type": "Rectangle", "width": 4, "height": 4 }, { "type": "Fill" } ] },
  { "type": "ViewModel", "children": [
    { "type": "Property", "name": "mix", "valueType": "number", "value": 0.25 },
    { "type": "Property", "name": "alarm", "valueType": "bool" },
    { "type": "Property", "name": "go", "valueType": "trigger" } ] },
  { "type": "Animation", "name": "left", "duration": 1, "children": [ { "type": "Track", "target": "s", "property": "x", "children": [ { "type": "Key", "value": 0 } ] } ] },
  { "type": "Animation", "name": "right", "duration": 1, "children": [ { "type": "Track", "target": "s", "property": "x", "children": [ { "type": "Key", "value": 100 } ] } ] },
  { "type": "Animation", "name": "travel", "fps": 10, "duration": 10, "children": [ { "type": "Track", "target": "s", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 0 }, { "type": "Key", "frame": 10, "value": 40 } ] } ] }"##;

#[test]
fn blend_states_mix_animations_along_an_input() {
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "mixer" }} ] }},
            {{ "type": "State", "name": "mixer", "kind": "blend", "input": "mix", "children": [
              {{ "type": "BlendAnimation", "animation": "left", "value": 0 }},
              {{ "type": "BlendAnimation", "animation": "right", "value": 1 }} ] }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    assert!((x_of(&inst, "s") - 25.0).abs() < 1e-3, "{}", x_of(&inst, "s"));
    for (mix, x) in [(0.0, 0.0), (0.8, 80.0), (1.0, 100.0), (7.0, 100.0), (-3.0, 0.0)] {
        inst.set_number("mix", mix);
        inst.advance(0.0);
        assert!((x_of(&inst, "s") - x).abs() < 1e-3, "mix {mix}: {}", x_of(&inst, "s"));
    }
}

#[test]
fn any_state_exit_time_and_triggers() {
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "Event", "name": "arrived" }},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "moving" }} ] }},
            {{ "type": "State", "name": "moving", "animation": "travel", "children": [
              {{ "type": "Transition", "to": "parked", "exitTime": 0.5, "emit": "arrived", "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "parked", "animation": "right" }},
            {{ "type": "State", "name": "anywhere", "kind": "any", "children": [
              {{ "type": "Transition", "to": "panic", "children": [ {{ "type": "Condition", "input": "alarm", "op": "eq", "value": 1 }} ] }} ] }},
            {{ "type": "State", "name": "panic", "animation": "left", "children": [
              {{ "type": "Transition", "to": "moving", "children": [ {{ "type": "Condition", "input": "alarm", "op": "eq", "value": 0 }} ] }} ] }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    // A trigger fired before the exit time is not honoured (and does not linger).
    inst.advance(0.2);
    inst.fire("go");
    inst.advance(0.1);
    assert!((x_of(&inst, "s") - 12.0).abs() < 1e-3, "still travelling: {}", x_of(&inst, "s"));
    inst.advance(0.3); // 0.6 s: past half of the 1 s animation
    assert!(inst.take_events().is_empty());
    inst.fire("go");
    inst.advance(0.0);
    assert_eq!(x_of(&inst, "s"), 100.0);
    assert_eq!(inst.take_events(), ["arrived"]);

    // An `any` transition fires from whatever state is current, and does not re-enter its target.
    inst.set_bool("alarm", true);
    inst.advance(0.0);
    assert_eq!(x_of(&inst, "s"), 0.0);
    inst.advance(0.5);
    assert_eq!(x_of(&inst, "s"), 0.0, "stays in panic while the alarm is on");
    inst.set_bool("alarm", false);
    inst.advance(0.0);
    inst.advance(0.5);
    assert!((x_of(&inst, "s") - 20.0).abs() < 1e-3, "back to travelling from its start: {}", x_of(&inst, "s"));
}

#[test]
fn binds_scale_numbers_and_drive_any_property() {
    let file = load(
        r##"{ "type": "Artboard", "width": 200, "height": 20, "children": [
          { "type": "ViewModel", "children": [
            { "type": "Property", "name": "progress", "valueType": "number", "value": 0.5 },
            { "type": "Property", "name": "shape", "valueType": "string", "value": "M0 0 L8 0 L8 8 Z" } ] },
          { "type": "Shape", "name": "bar", "y": 10, "children": [
            { "type": "Bind", "source": "progress", "property": "x", "mul": 180, "add": 10 },
            { "type": "Path", "name": "glyph", "d": "M0 0 L1 0 L1 1 Z", "children": [ { "type": "Bind", "source": "shape", "property": "d" } ] },
            { "type": "Fill" } ] } ] }"##,
    );
    let mut inst = play(&file);
    assert_eq!(x_of(&inst, "bar"), 100.0, "0.5 * 180 + 10");
    let width = |inst: &Instance| {
        let b = inst.bounds(object(inst.file(), "bar")).unwrap();
        b[2] - b[0]
    };
    assert_eq!(width(&inst), 8.0, "the string property replaced the path data");
    assert!(inst.set_number("progress", 1.0) && inst.set_string("shape", "M0 0 L20 0 L20 20 Z"));
    assert!(inst.advance(0.0));
    assert_eq!((x_of(&inst, "bar"), width(&inst)), (190.0, 20.0));
    assert!(!inst.set_number("progress", f32::NAN), "non-finite input is refused");
}

#[test]
fn a_bind_or_a_track_names_a_property_its_object_has() {
    // A Stroke has no `opacity` (a Shape has): binding it, or keying it, would do nothing.
    let scene = |stroke: &str, track: &str| {
        let src = format!(
            r##"{{ "zuri": 1, "children": [ {{ "type": "Artboard", "children": [
              {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "on", "valueType": "number" }} ] }},
              {{ "type": "Shape", "name": "box", "children": [ {{ "type": "Rectangle", "width": 10, "height": 10 }},
                {{ "type": "Stroke", "name": "line", "children": [ {{ "type": "Bind", "source": "on", "property": "{stroke}" }} ] }} ] }},
              {{ "type": "Animation", "name": "a", "children": [ {{ "type": "Track", "target": "{track}", "property": "opacity", "children": [
                {{ "type": "Key", "frame": 0, "value": 1 }} ] }} ] }} ] }} ] }}"##
        );
        json::compile(&src, &|_| unreachable!())
    };
    scene("thickness", "box").unwrap();
    let bound = scene("opacity", "box").unwrap_err().0;
    assert!(bound.contains(r#"property: Stroke has no property "opacity" to bind. Its properties: name, color, thickness"#), "{bound}");
    let keyed = scene("thickness", "line").unwrap_err().0;
    assert!(keyed.contains(r#"target: Stroke "line" has no property "opacity" to animate. Its properties: name, color, thickness"#), "{keyed}");
    // Nor one that it has but that holds no value: the `target` of a Constraint names another object.
    let named = json::compile(
        r##"{ "zuri": 1, "children": [ { "type": "Artboard", "children": [
          { "type": "ViewModel", "children": [ { "type": "Property", "name": "on", "valueType": "number" } ] },
          { "type": "Shape", "name": "box", "children": [ { "type": "Constraint", "children": [ { "type": "Bind", "source": "on", "property": "target" } ] } ] } ] } ] }"##,
        &|_| unreachable!(),
    )
    .unwrap_err()
    .0;
    assert!(named.contains(r#"property: Constraint cannot bind "target": it names another object"#), "{named}");
}

#[test]
fn geometry_kinds_and_stroke_options() {
    let file = load(
        r##"{ "type": "Artboard", "width": 200, "height": 60, "children": [
          { "type": "Shape", "name": "round", "x": 30, "y": 30, "children": [ { "type": "Rectangle", "width": 40, "height": 40, "radius": 20 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "Shape", "name": "tri", "x": 90, "y": 30, "children": [ { "type": "Polygon", "points": 3, "width": 40, "height": 40 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "Shape", "name": "ring", "x": 150, "y": 30, "children": [
            { "type": "Ellipse", "width": 40, "height": 40 }, { "type": "Ellipse", "width": 20, "height": 20 },
            { "type": "Fill", "color": "#ff0000", "fillRule": "evenodd" } ] },
          { "type": "Shape", "name": "line", "x": 10, "y": 55, "children": [
            { "type": "Path", "d": "M0 0 L100 0" }, { "type": "Stroke", "color": "#ff0000", "thickness": 4, "trimStart": 0.25, "trimEnd": 0.75 } ] } ] }"##,
    );
    let at = pixels(&mut play(&file));
    assert_eq!((at(30, 30), at(12, 12)), (RED, CLEAR), "a radius of half the size makes a circle: the corner is empty");
    // Polygons are inscribed in the width x height ellipse: this one spans y = 10..40.
    assert_eq!((at(90, 35), at(73, 14), at(90, 45)), (RED, CLEAR, CLEAR), "a triangle points up: wide at the bottom, empty top-left");
    assert_eq!((at(150, 30), at(150, 14)), (CLEAR, RED), "even-odd leaves the inner ellipse hollow");
    assert_eq!((at(20, 55), at(60, 55), at(100, 55)), (CLEAR, RED, CLEAR), "the stroke is trimmed to its middle half");
}

#[test]
fn dashed_strokes_follow_the_path_and_shift_with_the_offset() {
    let scene = |offset: f32| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [
              {{ "type": "Shape", "y": 5, "children": [
                {{ "type": "Path", "d": "M0 0 L100 0" }},
                {{ "type": "Stroke", "color": "#ff0000", "thickness": 4, "dash": 10, "dashOffset": {offset} }} ] }} ] }}"##
        ))
    };
    let at = pixels(&mut play(&scene(0.0)));
    assert_eq!((at(5, 5), at(15, 5), at(25, 5), at(35, 5)), (RED, CLEAR, RED, CLEAR), "gap defaults to the dash length");
    let at = pixels(&mut play(&scene(10.0)));
    assert_eq!((at(5, 5), at(15, 5), at(25, 5)), (CLEAR, RED, CLEAR));
}

#[test]
fn nested_artboards_receive_pointer_input_and_their_events_bubble() {
    let file = load(
        r##"{ "type": "Artboard", "name": "Parent", "width": 100, "height": 100, "children": [
          { "type": "NestedArtboard", "artboard": "Child", "x": 50, "y": 50 } ] },
        { "type": "Artboard", "name": "Child", "width": 20, "height": 20, "children": [
          { "type": "Shape", "name": "hit", "x": 10, "y": 10, "children": [ { "type": "Rectangle", "width": 20, "height": 20 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "Event", "name": "tapped" },
          { "type": "StateMachine", "children": [
            { "type": "Layer", "children": [ { "type": "State", "kind": "entry" } ] },
            { "type": "Listener", "target": "hit", "event": "click", "emit": "tapped" } ] } ] }"##,
    );
    let mut inst = play(&file);
    let _ = pixels(&mut inst); // a render gives the child its place on the surface
    for kind in [zuri_core::Pointer::Down, zuri_core::Pointer::Up] {
        inst.pointer(kind, 10.0, 10.0); // outside the child (it occupies 50..70)
    }
    assert!(inst.take_events().is_empty());
    for kind in [zuri_core::Pointer::Down, zuri_core::Pointer::Up] {
        inst.pointer(kind, 60.0, 60.0);
    }
    assert_eq!(inst.take_events(), ["tapped"]);
}

#[test]
fn shader_scenes_settle_unless_the_host_draws_shaders() {
    let file = load(
        r##"{ "type": "ShaderAsset", "name": "s", "wgsl": "fn shade(uv: vec2f) -> vec4f { return vec4f(uv, 0.0, 1.0); }" },
        { "type": "Artboard", "width": 20, "height": 20, "children": [
          { "type": "Shape", "x": 10, "y": 10, "children": [
            { "type": "Rectangle", "width": 20, "height": 20 },
            { "type": "Fill", "color": "#ff0000", "children": [ { "type": "ShaderPaint", "asset": "s" } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    assert_eq!(pixels(&mut inst)(10, 10), RED, "the fallback color shows");
    assert!(!inst.advance(0.1), "a host that never supplies shader pixels is not asked to redraw forever");
    let request = inst.shader_requests()[0];
    assert!(inst.set_shader_texture(request.paint, 1, 1, &[0, 0, 255, 255]));
    assert!(inst.advance(0.1) && inst.advance(0.1), "a host that draws shaders is driven every frame");
    assert_eq!(pixels(&mut inst)(10, 10), [0, 0, 255, 255]);
}

#[test]
fn a_press_never_sticks_and_accessors_are_type_checked() {
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "ViewModel", "children": [
            { "type": "Property", "name": "pressed", "valueType": "bool" },
            { "type": "Property", "name": "tint", "valueType": "color", "value": "#112233" },
            { "type": "Property", "name": "label", "valueType": "string", "value": "hi" },
            { "type": "Property", "name": "go", "valueType": "trigger" } ] },
          { "type": "Shape", "name": "pad", "x": 50, "y": 50, "children": [ { "type": "Rectangle", "width": 40, "height": 40 }, { "type": "Fill" } ] },
          { "type": "StateMachine", "children": [
            { "type": "Layer", "children": [ { "type": "State", "kind": "entry" } ] },
            { "type": "Listener", "target": "pad", "event": "down", "input": "pressed", "action": "set", "value": 1 },
            { "type": "Listener", "event": "up", "input": "pressed", "action": "set", "value": 0 } ] } ] }"##,
    );
    let mut inst = play(&file);
    let _ = pixels(&mut inst);
    use zuri_core::Pointer::*;
    // Released outside the artboard entirely.
    inst.pointer(Down, 50.0, 50.0);
    assert_eq!(inst.number("pressed"), Some(1.0));
    inst.pointer(Up, -10.0, -10.0);
    assert_eq!(inst.number("pressed"), Some(0.0), "an untargeted up listener hears a release anywhere");
    // The pointer leaves the surface mid-press and no release ever arrives.
    inst.pointer(Down, 50.0, 50.0);
    inst.pointer(Exit, 0.0, 0.0);
    assert_eq!(inst.number("pressed"), Some(0.0), "leaving the surface ends the press");
    // Leaving without a press in progress does nothing.
    inst.set_bool("pressed", true);
    inst.pointer(Exit, 0.0, 0.0);
    assert_eq!(inst.number("pressed"), Some(1.0));

    // Each accessor only touches properties of its own type.
    assert!(!inst.set_number("tint", 1.0) && !inst.set_color("pressed", 0) && !inst.set_string("tint", "x") && !inst.fire("label"));
    assert!(inst.number("tint").is_none() && inst.color("label").is_none() && inst.string("pressed").is_none());
    assert!(inst.set_color("tint", 0xff445566) && inst.set_string("label", "yo") && inst.fire("go"));
    assert_eq!((inst.color("tint"), inst.string("label")), (Some(0xff445566), Some("yo")));
}

#[test]
fn hostile_scenes_fail_fast_and_missing_references_point_at_nothing() {
    // A number after Z is not a repeat of Z (this used to loop forever).
    assert!(zuri_core::geom::parse_svg_path("M0 0 L10 10 Z 5").is_err());

    // Eight self-nests per level would be 8^8 instances: refused, not attempted.
    let nests = [r#"{ "type": "NestedArtboard", "artboard": "Self" }"#; 8].join(",");
    let file = load(&format!(r#"{{ "type": "Artboard", "name": "Self", "width": 10, "height": 10, "children": [ {nests} ] }}"#));
    let err = Instance::new(&file, 0).err().expect("refused").to_string();
    assert!(err.contains("nested artboard instances"), "{err}");

    // A NestedArtboard without a target nests nothing (it used to pick object 0: its own artboard).
    let file = load(
        r##"{ "type": "Artboard", "width": 20, "height": 20, "children": [
          { "type": "Shape", "x": 4, "y": 4, "children": [ { "type": "Rectangle", "width": 6, "height": 6 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "NestedArtboard", "x": 10, "y": 10 } ] }"##,
    );
    let at = pixels(&mut play(&file));
    assert_eq!((at(4, 4), at(14, 14)), (RED, CLEAR));
}

#[test]
fn a_segment_after_close_starts_at_the_subpath_start() {
    let file = load(
        r##"{ "type": "Artboard", "width": 20, "height": 30, "children": [
          { "type": "Shape", "children": [ { "type": "Path", "d": "M0 0 L10 0 L10 10 Z L8 28 L0 28 Z" }, { "type": "Fill", "color": "#ff0000" } ] } ] }"##,
    );
    assert_eq!(pixels(&mut play(&file))(1, 24), RED, "second triangle is (0,0) (8,28) (0,28)");
}

#[test]
fn cross_fades_are_linear_in_time_at_any_frame_rate() {
    // `idle` does not animate x, so the fade's start is whatever x was when it fired.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "idle" }} ] }},
            {{ "type": "State", "name": "idle", "children": [
              {{ "type": "Transition", "to": "parked", "blend": 1, "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "parked", "animation": "right" }} ] }} ] }} ] }}"##
    ));
    for steps in [1, 16] {
        let mut inst = play(&file);
        inst.fire("go");
        for _ in 0..steps {
            inst.advance(0.25 / steps as f32);
        }
        assert!((x_of(&inst, "s") - 25.0).abs() < 1e-3, "{steps} steps: {}", x_of(&inst, "s"));
        inst.advance(1.0);
        assert_eq!(x_of(&inst, "s"), 100.0);
    }
}

#[test]
fn a_cross_fade_that_interrupts_another_starts_from_the_picture_as_it_is() {
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "a" }} ] }},
            {{ "type": "State", "name": "a", "animation": "left", "children": [
              {{ "type": "Transition", "to": "b", "blend": 1, "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "b", "animation": "right", "children": [
              {{ "type": "Transition", "to": "a", "blend": 1, "children": [ {{ "type": "Condition", "input": "alarm", "value": 1 }} ] }} ] }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    inst.fire("go");
    inst.advance(0.5);
    assert!((x_of(&inst, "s") - 50.0).abs() < 1e-3, "{}", x_of(&inst, "s"));
    // Half-way to `b`, turn back: x must leave from 50, not jump to b's 100 first.
    inst.set_bool("alarm", true);
    inst.advance(0.01);
    assert!((x_of(&inst, "s") - 49.5).abs() < 1e-3, "{}", x_of(&inst, "s"));
    inst.advance(0.49);
    assert!((x_of(&inst, "s") - 25.0).abs() < 1e-2, "{}", x_of(&inst, "s"));
    inst.advance(1.0);
    assert_eq!(x_of(&inst, "s"), 0.0);

    // Two fades that fire in the same frame are one fade to the last target: `travel` (the
    // state on screen) keeps playing underneath, and `b` is never shown.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "a" }} ] }},
            {{ "type": "State", "name": "a", "animation": "travel", "children": [
              {{ "type": "Transition", "to": "b", "blend": 1, "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "b", "animation": "right", "children": [
              {{ "type": "Transition", "to": "c", "blend": 1, "children": [ {{ "type": "Condition", "input": "alarm", "value": 1 }} ] }} ] }},
            {{ "type": "State", "name": "c", "animation": "left" }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    inst.advance(0.25);
    assert!((x_of(&inst, "s") - 10.0).abs() < 1e-3);
    inst.set_bool("alarm", true);
    inst.fire("go");
    inst.advance(0.5);
    // travel is at 0.75 s (x = 30), half-way faded to left (x = 0).
    assert!((x_of(&inst, "s") - 15.0).abs() < 1e-3, "{}", x_of(&inst, "s"));
}

#[test]
fn looping_playback_keeps_its_place_after_months() {
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 10, "children": [
          { "type": "Shape", "name": "s", "children": [ { "type": "Rectangle", "width": 4, "height": 4 }, { "type": "Fill" } ] },
          { "type": "Animation", "name": "loop", "fps": 60, "duration": 60, "loop": "loop", "children": [
            { "type": "Track", "target": "s", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 0 }, { "type": "Key", "frame": 60, "value": 60 } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    inst.advance(16_000_000.0); // about six months, where an f32 clock cannot hold half a second
    inst.advance(0.5);
    assert!((x_of(&inst, "s") - 30.0).abs() < 1e-3, "{}", x_of(&inst, "s"));
}

#[test]
fn shader_paints_work_inside_nested_artboards_and_late_images_redraw() {
    let file = load(
        r##"{ "type": "ShaderAsset", "name": "s", "wgsl": "fn shade(uv: vec2f) -> vec4f { return vec4f(uv, 0.0, 1.0); }" },
        { "type": "ImageAsset", "name": "quad", "bytes": "AAAA", "width": 1, "height": 1 },
        { "type": "Artboard", "name": "Parent", "width": 40, "height": 20, "children": [
          { "type": "NestedArtboard", "artboard": "Child" }, { "type": "NestedArtboard", "artboard": "Child", "x": 20 } ] },
        { "type": "Artboard", "name": "Child", "width": 20, "height": 20, "children": [
          { "type": "Shape", "x": 5, "y": 5, "children": [
            { "type": "Rectangle", "width": 10, "height": 10 },
            { "type": "Fill", "color": "#ff0000", "children": [ { "type": "ShaderPaint", "asset": "s" } ] } ] },
          { "type": "Image", "asset": "quad", "x": 15, "y": 15, "width": 10, "height": 10 } ] }"##,
    );
    let mut inst = play(&file);
    assert_eq!(pixels(&mut inst)(5, 5), RED, "fallback color until the host draws the shader");
    let _ = pixels(&mut inst);
    let requests = inst.shader_requests().to_vec();
    assert_eq!(requests.len(), 1, "one request per paint, however often it is drawn or nested: {requests:?}");
    assert!(!inst.advance(0.1), "settled");
    assert!(inst.set_shader_texture(requests[0].paint, 1, 1, &[0, 0, 255, 255]));
    assert!(inst.advance(0.1) && inst.advance(0.1), "the root keeps redrawing for a nested shader");
    let at = pixels(&mut inst);
    assert_eq!((at(5, 5), at(25, 5)), ([0, 0, 255, 255], [0, 0, 255, 255]), "both instances show the shader");

    // A shader in a copy that is made later, two levels down, keeps the root redrawing too.
    let file = load(
        r##"{ "type": "ShaderAsset", "name": "s", "wgsl": "fn shade(uv: vec2f) -> vec4f { return vec4f(uv, 0.0, 1.0); }" },
        { "type": "Artboard", "name": "Parent", "width": 20, "height": 20, "children": [ { "type": "NestedArtboard", "name": "card", "artboard": "Card" } ] },
        { "type": "Artboard", "name": "Card", "width": 20, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "n", "valueType": "number" } ] },
          { "type": "NestedArtboard", "artboard": "Child", "children": [ { "type": "Bind", "source": "n", "property": "count" } ] } ] },
        { "type": "Artboard", "name": "Child", "width": 20, "height": 20, "children": [
          { "type": "Shape", "x": 5, "y": 5, "children": [
            { "type": "Rectangle", "width": 10, "height": 10 },
            { "type": "Fill", "color": "#ff0000", "children": [ { "type": "ShaderPaint", "asset": "s" } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    assert!(inst.set_number("card/n", 1.0) && inst.advance(0.0));
    assert_eq!(pixels(&mut inst)(5, 5), RED);
    let requests = inst.shader_requests().to_vec();
    assert!(inst.set_shader_texture(requests[0].paint, 1, 1, &[0, 0, 255, 255]));
    assert!(inst.advance(0.1) && inst.advance(0.1) && inst.advance(0.1), "the root keeps redrawing for a shader in a copy made later");
    assert!(inst.set_number("card/n", 0.0) && inst.advance(0.1) && !inst.advance(0.1), "and settles once the copy is gone");

    // An image decoded after the scene settled still appears.
    let file = load(
        r##"{ "type": "ImageAsset", "name": "quad", "bytes": "AAAA", "width": 1, "height": 1 },
        { "type": "Artboard", "width": 10, "height": 10, "children": [ { "type": "Image", "asset": "quad", "x": 5, "y": 5, "width": 10, "height": 10 } ] }"##,
    );
    let mut inst = play(&file);
    assert!(!inst.advance(0.1), "settled");
    file.set_image(object(&file, "quad"), 1, 1, &[0, 255, 0, 255]).unwrap();
    assert!(inst.advance(0.1), "a late image asks for a redraw");
    assert_eq!(pixels(&mut inst)(5, 5), [0, 255, 0, 255]);
}

#[test]
fn seek_read_back_and_current_states() {
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "idle" }} ] }},
            {{ "type": "State", "name": "idle", "children": [ {{ "type": "Transition", "to": "parked", "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "parked", "animation": "right" }} ] }} ] }} ] }}"##
    ));
    let s = object(&file, "s");
    let mut inst = play(&file);
    assert_eq!(inst.states(), [object(&file, "idle")]);
    assert!(!inst.seek(0.5), "a state machine has no single timeline to seek");
    inst.fire("go");
    inst.advance(0.0);
    assert_eq!(inst.states(), [object(&file, "parked")]);
    assert_eq!(inst.property(s, p::X), Some(100.0), "read back what the animation wrote");

    // `travel` is 10 frames at 10 fps, x 0 -> 40.
    let travel = inst.animation_names().iter().position(|n| *n == "travel").unwrap();
    assert!(inst.play_animation(travel) && inst.states().is_empty());
    assert!(inst.seek(0.5) && inst.advance(0.0));
    assert_eq!(inst.property(s, p::X), Some(20.0));
    assert!(inst.seek(f32::NAN) && inst.advance(0.0), "a bad time is the start, not a crash");
    assert_eq!((inst.property(s, p::X), inst.property(s, p::OPACITY)), (Some(0.0), Some(1.0)), "defaults read back too");
    assert_eq!((inst.property(s, p::COLOR), inst.property_color(s, p::X), inst.property(u32::MAX, p::X)), (None, None, None));
    assert_eq!(inst.property_color(s, p::COLOR), None, "a Shape has no color: its Fill does");
}

#[test]
fn seek_and_declared_events_reach_nested_artboards() {
    let file = load(
        r##"{ "type": "Artboard", "name": "Parent", "width": 100, "height": 20, "children": [
          { "type": "Event", "name": "outer" },
          { "type": "Shape", "name": "p", "children": [ { "type": "Rectangle", "width": 2, "height": 2 }, { "type": "Fill" } ] },
          { "type": "NestedArtboard", "artboard": "Child" },
          { "type": "Animation", "name": "main", "fps": 60, "duration": 120, "children": [
            { "type": "Track", "target": "p", "property": "y", "children": [ { "type": "Key", "frame": 0, "value": 0 }, { "type": "Key", "frame": 120, "value": 12 } ] } ] } ] },
        { "type": "Artboard", "name": "Child", "width": 100, "height": 20, "children": [
          { "type": "Event", "name": "inner" }, { "type": "Event", "name": "outer" },
          { "type": "Shape", "x": 5, "y": 10, "name": "dot", "children": [ { "type": "Rectangle", "width": 4, "height": 4 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "Animation", "name": "slide", "fps": 60, "duration": 60, "children": [
            { "type": "Track", "target": "dot", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 5 }, { "type": "Key", "frame": 60, "value": 65 } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    assert_eq!(inst.event_names(), ["outer", "inner"], "own events, then nested ones, each once");
    // Seeking places the nested artboard at the same time, whatever was shown before.
    for seconds in [0.5, 0.25, 0.5] {
        assert!(inst.seek(seconds) && inst.advance(0.0));
        let x = 5 + (60.0 * seconds) as u32;
        let at = pixels(&mut inst);
        assert_eq!((at(x, 10), at(x + 8, 10)), (RED, CLEAR), "nested dot at x = {x} for t = {seconds}");
    }
}

const BLUE: [u8; 4] = [0, 0, 255, 255];

#[test]
fn the_host_reaches_a_nested_view_model_by_path() {
    // `Main` nests `Card` as `card`, and `Card` nests `Badge` as `badge`. A Card has a
    // number, a color, a string, an enum and a trigger that reports `went`; a Badge a number.
    let file = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 100, "height": 100, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "level", "valueType": "number", "value": 1 } ] },
          { "type": "NestedArtboard", "name": "card", "artboard": "Card" },
          { "type": "NestedArtboard", "name": "other", "artboard": "Card" } ] },
        { "type": "Artboard", "name": "Card", "width": 50, "height": 50, "children": [
          { "type": "ViewModel", "name": "vm", "children": [
            { "type": "Property", "name": "level", "valueType": "number", "value": 2 },
            { "type": "Property", "name": "tint", "valueType": "color", "value": "#102030" },
            { "type": "Property", "name": "title", "valueType": "string", "value": "one" },
            { "type": "Property", "name": "size", "valueType": "enum", "children": [ { "type": "Option", "name": "small" }, { "type": "Option", "name": "large" } ] },
            { "type": "Property", "name": "go", "valueType": "trigger" } ] },
          { "type": "Event", "name": "went" },
          { "type": "NestedArtboard", "name": "badge", "artboard": "Badge" },
          { "type": "StateMachine", "name": "m", "children": [ { "type": "Layer", "children": [
            { "type": "State", "name": "idle", "children": [ { "type": "Transition", "to": "idle", "emit": "went", "children": [ { "type": "Condition", "input": "go" } ] } ] } ] } ] } ] },
        { "type": "Artboard", "name": "Badge", "width": 10, "height": 10, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "count", "valueType": "number", "value": 3 } ] } ] }"##,
    );
    let mut inst = play(&file);
    // Read: the artboard's own, one level down, two levels down.
    assert_eq!((inst.number("level"), inst.number("card/level"), inst.number("card/badge/count")), (Some(1.0), Some(2.0), Some(3.0)));
    assert_eq!((inst.color("card/tint"), inst.string("card/title"), inst.string("card/size")), (Some(0xff10_2030), Some("one"), Some("small")));
    // Written, each kind, into one copy of the Card and not the other.
    assert!(inst.set_number("card/level", 7.0) && inst.set_number("card/badge/count", 9.0) && inst.set_bool("card/level", true));
    assert!(inst.set_color("card/tint", 0xffff_0000) && inst.set_string("card/title", "two") && inst.set_string("card/size", "large"));
    assert_eq!((inst.number("card/level"), inst.number("card/badge/count"), inst.number("level")), (Some(1.0), Some(9.0), Some(1.0)));
    assert_eq!((inst.color("card/tint"), inst.string("card/title"), inst.string("card/size")), (Some(0xffff_0000), Some("two"), Some("large")));
    assert_eq!((inst.number("other/level"), inst.string("other/title")), (Some(2.0), Some("one")));
    // A trigger fired down there is heard there, and what it reports comes up.
    assert!(inst.fire("card/go"));
    inst.advance(0.0);
    inst.advance(0.0);
    assert_eq!(inst.take_events(), ["went"]);
    // What is not there says so: no such artboard, no such property, the wrong kind, a path too long.
    assert!(!inst.set_number("nobody/level", 1.0) && !inst.set_number("card/nothing", 1.0) && !inst.set_color("card/level", 0) && !inst.fire("card/level"));
    assert_eq!((inst.number("card/title"), inst.number("card/badge/count/more"), inst.string("card/")), (None, None, None));
    // What kind a name or a path refers to (number, color, string, enum, trigger are 0, 3, 4, 5, 2), or that it refers to nothing.
    let kinds = ["level", "card/level", "card/tint", "card/title", "card/size", "card/go", "card/badge/count", "card/nothing", "nobody/level"];
    assert_eq!(kinds.map(|name| inst.property_type(name)), [Some(0), Some(0), Some(3), Some(4), Some(5), Some(2), Some(0), None, None]);

    // A path is cut at each `/` and begins with a NestedArtboard's name: the compiler
    // refuses a name with a `/` in it, and two NestedArtboards of one name.
    let refused =
        |children: &str| json::compile(&format!(r#"{{ "zuri": 1, "children": [ {children} ] }}"#), &|_| unreachable!()).expect_err("refused").to_string();
    let slash = refused(
        r#"{ "type": "Artboard", "children": [ { "type": "ViewModel", "children": [ { "type": "Property", "name": "audio/volume", "valueType": "number" } ] } ] }"#,
    );
    assert!(slash.contains(r#"has a "/" in it"#), "{slash}");
    let twice = refused(
        r#"{ "type": "Artboard", "name": "Main", "children": [ { "type": "NestedArtboard", "name": "card", "artboard": "Card" }, { "type": "NestedArtboard", "name": "card", "artboard": "Card" } ] },
           { "type": "Artboard", "name": "Card" }"#,
    );
    assert!(twice.contains(r#"another NestedArtboard is already named "card""#), "{twice}");
}

#[test]
fn a_nested_artboard_with_a_count_is_a_list() {
    // `Main` holds a hugging Layout at (10, 10) around `rows`: a `Row` (40 by 10, a bar
    // painted in its own `tint`, and an empty `Badge` nested in it) repeated `n` times
    // down a column with 4 between them.
    // A press on a Row toggles that Row's `on` and reports `picked`. `theme` goes into
    // every Row's `label`.
    let scene = |rows: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "name": "Main", "width": 100, "height": 100, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [
                {{ "type": "Property", "name": "n", "valueType": "number", "value": 3 }},
                {{ "type": "Property", "name": "theme", "valueType": "string", "value": "dark" }} ] }},
              {{ "type": "Layout", "name": "list", "x": 10, "y": 10, "children": [
                {{ "type": "NestedArtboard", "name": "rows", "artboard": "Row", {rows} "children": [
                  {{ "type": "Bind", "source": "n", "property": "count" }}, {{ "type": "Bind", "source": "theme", "into": "label" }} ] }} ] }} ] }},
            {{ "type": "Artboard", "name": "Row", "width": 40, "height": 10, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [
                {{ "type": "Property", "name": "tint", "valueType": "color", "value": "#ff0000" }},
                {{ "type": "Property", "name": "label", "valueType": "string", "value": "" }},
                {{ "type": "Property", "name": "on", "valueType": "bool" }} ] }},
              {{ "type": "Event", "name": "picked" }},
              {{ "type": "NestedArtboard", "name": "badge", "artboard": "Badge" }},
              {{ "type": "Shape", "name": "bar", "x": 20, "y": 5, "children": [ {{ "type": "Rectangle", "width": 40, "height": 10 }},
                {{ "type": "Fill", "children": [ {{ "type": "Bind", "source": "tint", "property": "color" }} ] }} ] }},
              {{ "type": "StateMachine", "name": "m", "children": [
                {{ "type": "Layer", "children": [ {{ "type": "State", "name": "idle" }} ] }},
                {{ "type": "Listener", "target": "bar", "event": "down", "input": "on", "action": "toggle", "emit": "picked" }} ] }} ] }},
            {{ "type": "Artboard", "name": "Badge", "width": 2, "height": 2, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "dot", "valueType": "number" }} ] }} ] }}"##
        ))
    };
    let column = r#""flow": "column", "gap": 4,"#;
    // The list is as long as the document says before anything has advanced, also
    // where it says so through a Bind (`n` is 3).
    let mut fresh = Instance::new(&scene(column), 0).unwrap();
    assert_eq!((fresh.property_type("rows/2/tint"), fresh.property_type("rows/3/tint")), (Some(3), None));
    // And as long as the host says as soon as it says so: it can write to the new copies at once.
    assert!(fresh.set_number("n", 5.0) && fresh.set_color("rows/4/tint", 0xff00_ff00) && !fresh.set_color("rows/5/tint", 0));
    assert!(fresh.set_number("n", 1.0));
    assert_eq!((fresh.color("rows/0/tint"), fresh.color("rows/1/tint")), (Some(0xffff_0000), None));
    let mut inst = play(&scene(column));
    inst.advance(0.0);
    // Three copies, one under the other: the Layout around them hugs 40 by 3 * 10 + 2 * 4.
    assert_eq!(boxes(&inst, &["list"]), [[10., 10., 50., 48.]]);
    // Each copy has a view model of its own, reached by its position; without one, the first.
    assert!(inst.set_color("rows/1/tint", 0xff00_ff00) && inst.set_color("rows/2/tint", 0xff00_00ff));
    assert_eq!((inst.color("rows/tint"), inst.color("rows/0/tint"), inst.color("rows/1/tint")), (Some(0xffff_0000), Some(0xffff_0000), Some(0xff00_ff00)));
    assert_eq!((inst.color("rows/3/tint"), inst.property_type("rows/3/tint"), inst.property_type("rows/2/tint")), (None, None, Some(3)));
    assert!(!inst.set_color("rows/3/tint", 0) && !inst.set_color("rows/99999999999999999999999/tint", 0));
    // What the artboard binds `into` the component goes into every copy.
    assert_eq!((inst.string("rows/0/label"), inst.string("rows/2/label")), (Some("dark"), Some("dark")));
    inst.advance(0.0);
    {
        let at = pixels(&mut inst);
        assert_eq!(
            [at(30, 15), at(30, 22), at(30, 29), at(30, 43), at(30, 52)],
            [RED, CLEAR, [0, 255, 0, 255], [0, 0, 255, 255], CLEAR],
            "a row, the gap, the next two, nothing"
        );
    }
    // A press lands in the copy that is there, and only that one hears it.
    inst.pointer(Pointer::Down, 30.0, 29.0);
    inst.pointer(Pointer::Up, 30.0, 29.0);
    inst.advance(0.0);
    assert_eq!(inst.take_events(), ["picked"]);
    assert_eq!(["rows/0/on", "rows/1/on", "rows/2/on"].map(|name| inst.number(name)), [Some(0.0), Some(1.0), Some(0.0)]);

    // The list grows and shrinks with `n`: the copies that stay keep what they hold, a
    // new one starts as the document has it (with what is bound into it), and one that
    // went and came back is new.
    let grown = |inst: &mut Instance, n: f32| {
        inst.set_number("n", n);
        inst.advance(0.0);
        boxes(inst, &["list"])[0]
    };
    assert_eq!(grown(&mut inst, 5.0), [10., 10., 50., 76.]);
    // A copy that has not been drawn yet is touched where it will be, and nowhere else:
    // the list starts at (10, 10), so nothing is at (5, 5), and the fifth Row is at 66 down.
    inst.pointer(Pointer::Down, 5.0, 5.0);
    inst.pointer(Pointer::Up, 5.0, 5.0);
    inst.advance(0.0);
    assert_eq!((inst.take_events().len(), inst.number("rows/3/on"), inst.number("rows/4/on")), (0, Some(0.0), Some(0.0)));
    inst.pointer(Pointer::Down, 30.0, 70.0);
    inst.pointer(Pointer::Up, 30.0, 70.0);
    inst.advance(0.0);
    assert_eq!((inst.take_events().len(), inst.number("rows/3/on"), inst.number("rows/4/on")), (1, Some(0.0), Some(1.0)));
    assert_eq!((inst.color("rows/1/tint"), inst.color("rows/4/tint"), inst.string("rows/4/label")), (Some(0xff00_ff00), Some(0xffff_0000), Some("dark")));
    assert_eq!(grown(&mut inst, 2.0), [10., 10., 50., 34.]);
    assert_eq!((inst.color("rows/1/tint"), inst.color("rows/2/tint")), (Some(0xff00_ff00), None));
    assert_eq!(grown(&mut inst, 3.0), [10., 10., 50., 48.]);
    assert_eq!(inst.color("rows/2/tint"), Some(0xffff_0000));
    // A part of one is none, and so is less than none; with none there is nothing to reach.
    assert_eq!(grown(&mut inst, 2.9), [10., 10., 50., 34.]);
    inst.set_number("n", -3.0);
    inst.advance(0.0);
    assert_eq!((inst.color("rows/tint"), inst.color("rows/0/tint"), inst.take_events().len()), (None, None, 0));
    assert_eq!(pixels(&mut inst)(30, 15), CLEAR);
    // What an empty list could report is still declared.
    assert_eq!(inst.event_names(), ["picked"]);
    // More than an instance may hold stops at what it may hold: 4096 nested instances in
    // all, here 2048 Rows with a Badge in each, however often it is asked for.
    for n in [1e9, 2e9, 1e9] {
        inst.set_number("n", n);
        inst.advance(0.0);
        assert_eq!((inst.property_type("rows/2047/badge/dot"), inst.property_type("rows/2048/tint")), (Some(0), None));
    }
    // A list that is short of room does not keep the scene from settling.
    assert!(!inst.advance(0.0), "settled at the cap");

    // Along a row, and with no gap.
    let mut inst = play(&scene(""));
    assert_eq!(boxes(&inst, &["list"]), [[10., 10., 130., 20.]]);
    assert!(inst.set_color("rows/2/tint", 0xff00_00ff));
    inst.advance(0.0);
    let at = pixels(&mut inst);
    assert_eq!([at(30, 15), at(95, 15)], [RED, [0, 0, 255, 255]]);

    // Two lists, `A` and `B`, share what room an instance has (4096 nested instances),
    // and a machine that, when `kick` is fired, makes `A` one longer and fires `go`, which
    // goes into every copy of `A`.
    let two = |a: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "name": "Main", "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [
                {{ "type": "Property", "name": "a", "valueType": "number" }}, {{ "type": "Property", "name": "b", "valueType": "number" }},
                {{ "type": "Property", "name": "kick", "valueType": "trigger" }}, {{ "type": "Property", "name": "go", "valueType": "trigger" }} ] }},
              {{ "type": "NestedArtboard", "name": "A", "artboard": "Item", {a} "children": [
                {{ "type": "Bind", "source": "a", "property": "count" }}, {{ "type": "Bind", "source": "go", "into": "go" }} ] }},
              {{ "type": "NestedArtboard", "name": "B", "artboard": "Item", "children": [ {{ "type": "Bind", "source": "b", "property": "count" }} ] }},
              {{ "type": "StateMachine", "name": "m", "children": [ {{ "type": "Layer", "children": [
                {{ "type": "State", "name": "idle", "children": [ {{ "type": "Transition", "to": "idle", "children": [
                  {{ "type": "Condition", "input": "kick" }}, {{ "type": "Action", "input": "a", "action": "add", "value": 1 }},
                  {{ "type": "Action", "input": "go", "action": "fire" }} ] }} ] }} ] }} ] }} ] }},
            {{ "type": "Artboard", "name": "Item", "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [
                {{ "type": "Property", "name": "t", "valueType": "number" }}, {{ "type": "Property", "name": "go", "valueType": "trigger" }} ] }},
              {{ "type": "Event", "name": "went" }},
              {{ "type": "StateMachine", "name": "m", "children": [ {{ "type": "Layer", "children": [
                {{ "type": "State", "name": "idle", "children": [ {{ "type": "Transition", "to": "idle", "emit": "went", "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }} ] }} ] }} ] }}"##
        ))
    };
    let both = |inst: &mut Instance, a: f32, b: f32| {
        inst.set_number("a", a);
        inst.set_number("b", b);
        inst.advance(0.0);
    };
    let there = |inst: &Instance, names: [&str; 3]| names.map(|name| inst.property_type(name).is_some());
    let mut inst = play(&two(""));
    // The first list takes what room there is; the second still follows its own count,
    // and what it gives up the first takes in the same advance.
    both(&mut inst, 0.0, 5.0);
    both(&mut inst, 5000.0, 5.0);
    assert_eq!(there(&inst, ["A/4090/t", "A/4091/t", "B/4/t"]), [true, false, true]);
    both(&mut inst, 5000.0, 0.0);
    assert_eq!(there(&inst, ["A/4095/t", "A/4096/t", "B/t"]), [true, false, false]);
    // And the other way: the second grows into what the first gave up.
    both(&mut inst, 4000.0, 4000.0);
    assert_eq!(there(&inst, ["A/3999/t", "B/95/t", "B/96/t"]), [true, true, false]);
    both(&mut inst, 10.0, 4000.0);
    assert_eq!(there(&inst, ["A/10/t", "B/3999/t", "B/4000/t"]), [false, true, false]);
    // A trigger that the artboard fires is heard once by every copy, also by the one
    // made in the frame it was fired in.
    let mut inst = play(&two(""));
    both(&mut inst, 3.0, 0.0);
    inst.fire("kick");
    for _ in 0..5 {
        inst.advance(0.0);
    }
    assert_eq!((inst.number("a"), inst.take_events().len()), (Some(4.0), 4));
    // Room that a list further down gives up is taken at the next advance, with nothing set.
    let deep = load(
        r##"{ "type": "Artboard", "name": "Main", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "a", "valueType": "number" } ] },
          { "type": "NestedArtboard", "name": "A", "artboard": "Item", "children": [ { "type": "Bind", "source": "a", "property": "count" } ] },
          { "type": "NestedArtboard", "name": "card", "artboard": "Card" } ] },
        { "type": "Artboard", "name": "Card", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "n", "valueType": "number" } ] },
          { "type": "NestedArtboard", "name": "inner", "artboard": "Item", "children": [ { "type": "Bind", "source": "n", "property": "count" } ] } ] },
        { "type": "Artboard", "name": "Item", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "t", "valueType": "number" } ] } ] }"##,
    );
    let mut inst = play(&deep);
    assert!(inst.set_number("card/n", 4000.0));
    inst.advance(0.0);
    assert!(inst.set_number("a", 4000.0));
    inst.advance(0.0);
    assert_eq!(there(&inst, ["card/inner/3999/t", "A/94/t", "A/95/t"]), [true, true, false]);
    assert!(inst.set_number("card/n", 0.0));
    inst.advance(0.0);
    inst.advance(0.0);
    assert_eq!(there(&inst, ["card/inner/t", "A/3999/t", "A/4000/t"]), [false, true, false]);

    // A count that does not fit when the file is instantiated is refused; a Bind of the
    // count is what counts, whatever is written beside it (`a` is 0).
    let written = load(
        r#"{ "type": "Artboard", "name": "Main", "children": [ { "type": "NestedArtboard", "artboard": "Item", "count": 5000 } ] },
           { "type": "Artboard", "name": "Item" }"#,
    );
    let err = Instance::new(&written, 0).err().expect("refused").to_string();
    assert!(err.contains("nested artboard instances"), "{err}");
    assert_eq!(Instance::new(&two(r#""count": 5000,"#), 0).unwrap().property_type("A/t"), None);
    // Also when it is a list further down that does not fit.
    let below = load(
        r#"{ "type": "Artboard", "name": "Main", "children": [ { "type": "NestedArtboard", "artboard": "Card" } ] },
           { "type": "Artboard", "name": "Card", "children": [ { "type": "NestedArtboard", "artboard": "Item", "count": 5000 } ] },
           { "type": "Artboard", "name": "Item" }"#,
    );
    assert!(Instance::new(&below, 0).is_err());

    // What is bound `into` a component is there before the component's own lists are
    // made: `items` (3) goes into the Card's `n`, which is how long its list is,
    // whatever `n` is in the Card's own document.
    for own in [0, 5000] {
        let fed = load(&format!(
            r##"{{ "type": "Artboard", "name": "Main", "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "items", "valueType": "number", "value": 3 }} ] }},
              {{ "type": "NestedArtboard", "name": "card", "artboard": "Card", "children": [ {{ "type": "Bind", "source": "items", "into": "n" }} ] }} ] }},
            {{ "type": "Artboard", "name": "Card", "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "n", "valueType": "number", "value": {own} }} ] }},
              {{ "type": "NestedArtboard", "name": "rows", "artboard": "Item", "children": [ {{ "type": "Bind", "source": "n", "property": "count" }} ] }} ] }},
            {{ "type": "Artboard", "name": "Item", "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "t", "valueType": "number" }} ] }} ] }}"##
        ));
        let mut inst = Instance::new(&fed, 0).unwrap();
        assert_eq!(there(&inst, ["card/rows/2/t", "card/rows/3/t", "card/rows/t"]), [true, false, true], "{own}");
        // And follows it down there as soon as the host sets it up here.
        assert!(inst.set_number("items", 6.0));
        assert_eq!(there(&inst, ["card/rows/5/t", "card/rows/6/t", "card/rows/t"]), [true, false, true], "{own}");
    }
    // What comes in that way is new to the component: a Listener for a change of it
    // hears the first value, at the first advance.
    let hears = load(
        r##"{ "type": "Artboard", "name": "Main", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "level", "valueType": "number", "value": 70 } ] },
          { "type": "NestedArtboard", "name": "meter", "artboard": "Meter", "children": [ { "type": "Bind", "source": "level", "into": "value" } ] } ] },
        { "type": "Artboard", "name": "Meter", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "value", "valueType": "number" } ] },
          { "type": "Event", "name": "heard" },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "event": "change", "with": "value", "emit": "heard" } ] } ] }"##,
    );
    let mut inst = play(&hears);
    assert_eq!((inst.number("meter/value"), inst.take_events()), (Some(70.0), vec!["heard".to_string()]));
    // An enum set by its Option's name, by path, makes the list it is bound to as long at once.
    let sized = load(
        r##"{ "type": "Artboard", "name": "Main", "children": [ { "type": "NestedArtboard", "name": "card", "artboard": "Card" } ] },
        { "type": "Artboard", "name": "Card", "children": [
          { "type": "ViewModel", "name": "vm", "children": [
            { "type": "Property", "name": "size", "valueType": "enum", "children": [ { "type": "Option", "name": "none" }, { "type": "Option", "name": "some" } ] } ] },
          { "type": "NestedArtboard", "name": "rows", "artboard": "Item", "children": [ { "type": "Bind", "source": "size", "property": "count", "mul": 3 } ] } ] },
        { "type": "Artboard", "name": "Item", "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "t", "valueType": "number" } ] } ] }"##,
    );
    let mut inst = Instance::new(&sized, 0).unwrap();
    assert!(inst.property_type("card/rows/t").is_none() && inst.set_string("card/size", "some"));
    assert_eq!(there(&inst, ["card/rows/2/t", "card/rows/3/t", "card/rows/t"]), [true, false, true]);

    // A NestedArtboard whose name is a number could not be told from a copy's position.
    let refused = json::compile(
        r#"{ "zuri": 1, "children": [ { "type": "Artboard", "name": "Main", "children": [ { "type": "NestedArtboard", "name": "2", "artboard": "Row" } ] },
             { "type": "Artboard", "name": "Row" } ] }"#,
        &|_| unreachable!(),
    )
    .expect_err("refused")
    .to_string();
    assert!(refused.contains("is a number"), "{refused}");
}

#[test]
fn nested_artboards_take_data_and_play_what_they_are_told() {
    let scene = |play: &str, into: &str| {
        format!(
            r##"{{ "zuri": 1, "children": [
            {{ "type": "Artboard", "name": "Parent", "width": 100, "height": 20, "children": [
              {{ "type": "ViewModel", "children": [
                {{ "type": "Property", "name": "level", "valueType": "number", "value": 10 }},
                {{ "type": "Property", "name": "tint", "valueType": "color", "value": "#ff0000" }},
                {{ "type": "Property", "name": "poke", "valueType": "trigger" }} ] }},
              {{ "type": "NestedArtboard", "artboard": "Child", "play": "{play}", "children": [
                {{ "type": "Bind", "source": "level", "into": "{into}" }},
                {{ "type": "Bind", "source": "tint", "into": "paint" }},
                {{ "type": "Bind", "source": "poke", "into": "go" }} ] }} ] }},
            {{ "type": "Artboard", "name": "Child", "width": 100, "height": 20, "children": [
              {{ "type": "ViewModel", "children": [
                {{ "type": "Property", "name": "x", "valueType": "number" }},
                {{ "type": "Property", "name": "paint", "valueType": "color" }},
                {{ "type": "Property", "name": "go", "valueType": "trigger" }} ] }},
              {{ "type": "Event", "name": "poked" }},
              {{ "type": "Shape", "name": "dot", "y": 10, "children": [
                {{ "type": "Rectangle", "width": 10, "height": 10 }},
                {{ "type": "Fill", "children": [ {{ "type": "Bind", "source": "paint", "property": "color" }} ] }},
                {{ "type": "Bind", "source": "x", "property": "x" }} ] }},
              {{ "type": "Animation", "name": "hide", "duration": 1, "children": [
                {{ "type": "Track", "target": "dot", "property": "opacity", "children": [ {{ "type": "Key", "value": 0 }} ] }} ] }},
              {{ "type": "StateMachine", "name": "decoy", "children": [ {{ "type": "Layer", "children": [
                {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "gone" }} ] }},
                {{ "type": "State", "name": "gone", "animation": "hide" }} ] }} ] }},
              {{ "type": "StateMachine", "name": "logic", "children": [ {{ "type": "Layer", "children": [
                {{ "type": "State", "name": "entry2", "kind": "entry", "children": [ {{ "type": "Transition", "to": "idle" }} ] }},
                {{ "type": "State", "name": "idle", "children": [
                  {{ "type": "Transition", "to": "idle2", "emit": "poked", "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
                {{ "type": "State", "name": "idle2" }} ] }} ] }} ] }} ] }}"##
        )
    };
    let load = |play: &str, into: &str| File::load(&format::write(&json::compile(&scene(play, into), &|_| unreachable!()).unwrap(), 0).unwrap());

    // The child plays `logic`, not its first state machine (which hides the dot), and the
    // parent's view model drives the child's.
    let mut inst = play(&load("logic", "x").unwrap());
    let at = pixels(&mut inst);
    assert_eq!((at(10, 10), at(50, 10)), (RED, CLEAR));
    assert!(inst.set_number("level", 50.0) && inst.set_color("tint", 0xff0000ff));
    assert!(inst.advance(0.0), "new data in a nested artboard is a new picture");
    let at = pixels(&mut inst);
    assert_eq!((at(10, 10), at(50, 10)), (CLEAR, BLUE));
    assert!(!inst.advance(0.1), "and then it settles");
    // A trigger fired on the parent fires in the child in the same frame.
    assert!(inst.fire("poke"));
    inst.advance(0.0);
    assert_eq!(inst.take_events(), ["poked"]);

    // Left to itself the child plays its first state machine; an animation can be named too.
    for play_what in ["", "hide"] {
        let mut inst = play(&load(play_what, "x").unwrap());
        inst.advance(0.1);
        assert_eq!(pixels(&mut inst)(10, 10), CLEAR, "play = '{play_what}'");
    }
    // Mistakes are load errors that name the nested artboard.
    let err = |play: &str, into: &str| load(play, into).err().unwrap().0;
    assert!(err("nope", "x").contains("artboard 'Child' has no StateMachine or Animation named 'nope'"), "{}", err("nope", "x"));
    assert!(err("logic", "nope").contains("no view-model Property named 'nope'"), "{}", err("logic", "nope"));
    assert!(err("logic", "paint").contains("different types"), "{}", err("logic", "paint"));
    // Data for a nested artboard has to sit inside one.
    let stray = json::compile(
        r#"{ "children": [ { "type": "Artboard", "children": [
            { "type": "ViewModel", "children": [ { "type": "Property", "name": "v", "valueType": "number" } ] },
            { "type": "Shape", "children": [ { "type": "Bind", "source": "v", "into": "x" } ] } ] } ] }"#,
        &|_| unreachable!(),
    );
    let err = File::from_objects(stray.unwrap()).err().unwrap().0;
    assert!(err.contains("'into' is for a Bind inside a NestedArtboard"), "{err}");
}

#[test]
fn cues_report_events_as_playback_passes_them() {
    // Ten frames a second for one second. `steps` are advances in seconds; the result is
    // what each one reported.
    let run = |looping: &str, cues: &str, steps: &[f32]| -> Vec<Vec<String>> {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 10, "height": 10, "children": [
              {{ "type": "Event", "name": "start" }}, {{ "type": "Event", "name": "late" }},
              {{ "type": "Animation", "name": "a", "fps": 10, "duration": 10, "loop": "{looping}", "children": [ {cues} ] }} ] }}"##
        ));
        let mut inst = play(&file);
        steps.iter().map(|dt| (inst.advance(*dt), inst.take_events()).1).collect()
    };
    let both = r#"{ "type": "Cue", "frame": 0, "emit": "start" }, { "type": "Cue", "frame": 8, "emit": "late" }"#;
    let names = |got: Vec<Vec<String>>| got.iter().map(|e| e.join("+")).collect::<Vec<_>>();
    // Once per lap of a loop; a long step reports a cue once.
    assert_eq!(names(run("loop", both, &[0.3, 0.3, 0.3, 0.3, 5.0])), ["start", "", "late", "start", "start+late"]);
    // Once on the way out and once on the way back.
    assert_eq!(names(run("pingpong", both, &[0.9, 0.4, 0.8])), ["start+late", "late", "start"]);
    // A one-shot passes each frame once, the last frame included.
    let end = r#"{ "type": "Cue", "frame": 10, "emit": "late" }, { "type": "Cue", "frame": 11, "emit": "start" }"#;
    assert_eq!(names(run("oneshot", end, &[0.9, 0.3, 5.0])), ["", "late", ""]);

    // Inside a state machine, a state's animation reports from the moment it is entered.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 10, "height": 10, "children": [
          {{ "type": "Event", "name": "start" }}, {{ "type": "Event", "name": "late" }},
          {{ "type": "Animation", "name": "a", "fps": 10, "duration": 10, "children": [ {both} ] }},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "run" }} ] }},
            {{ "type": "State", "name": "run", "animation": "a", "speed": 2 }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    assert_eq!(inst.take_events(), [] as [&str; 0]);
    inst.advance(0.3);
    assert_eq!(inst.take_events(), ["start"]);
    inst.advance(0.2); // speed 2: the playhead is on frame 10
    assert_eq!(inst.take_events(), ["late"]);

    // In a blend state only the animations that have weight report.
    let blend = |mix: f32| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 10, "height": 10, "children": [
              {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "mix", "valueType": "number", "value": {mix} }} ] }},
              {{ "type": "Event", "name": "start" }}, {{ "type": "Event", "name": "late" }},
              {{ "type": "Animation", "name": "a", "duration": 10, "children": [ {{ "type": "Cue", "emit": "start" }} ] }},
              {{ "type": "Animation", "name": "b", "duration": 10, "children": [ {{ "type": "Cue", "emit": "late" }} ] }},
              {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
                {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "mixed" }} ] }},
                {{ "type": "State", "name": "mixed", "kind": "blend", "input": "mix", "children": [
                  {{ "type": "BlendAnimation", "animation": "a", "value": 0 }}, {{ "type": "BlendAnimation", "animation": "b", "value": 1 }} ] }} ] }} ] }} ] }}"##
        ));
        let mut inst = play(&file);
        inst.advance(0.1);
        inst.take_events().join("+")
    };
    assert_eq!((blend(0.0), blend(0.5), blend(1.0)), ("start".into(), "start+late".into(), "late".into()));

    // A seek is a jump: a nested artboard restarts and runs to the same time, silently.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "name": "Parent", "width": 10, "height": 10, "children": [
          {{ "type": "NestedArtboard", "artboard": "Child" }},
          {{ "type": "Animation", "name": "p", "duration": 600 }} ] }},
        {{ "type": "Artboard", "name": "Child", "width": 10, "height": 10, "children": [
          {{ "type": "Event", "name": "start" }}, {{ "type": "Event", "name": "late" }},
          {{ "type": "Animation", "name": "a", "fps": 10, "duration": 10, "children": [ {both} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    assert!(inst.seek(0.5));
    assert_eq!(inst.take_events(), [] as [&str; 0]);
    inst.advance(0.35);
    assert_eq!(inst.take_events(), ["late"]);
}

#[test]
fn order_rearranges_siblings_and_solo_shows_one_child() {
    let square = |name: &str, color: &str, extra: &str| {
        format!(
            r##"{{ "type": "Shape", "name": "{name}", "x": 10, "y": 10, "children": [
              {{ "type": "Rectangle", "width": 20, "height": 20 }}, {{ "type": "Fill", "color": "{color}" }} {extra} ] }}"##
        )
    };
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 20, "height": 20, "children": [
          {{ "type": "ViewModel", "children": [
            {{ "type": "Property", "name": "front", "valueType": "number" }},
            {{ "type": "Property", "name": "pick", "valueType": "number" }},
            {{ "type": "Property", "name": "reds", "valueType": "number" }},
            {{ "type": "Property", "name": "blues", "valueType": "number" }} ] }},
          {{ "type": "StateMachine", "children": [
            {{ "type": "Listener", "target": "red", "event": "down", "input": "reds", "action": "add", "value": 1 }},
            {{ "type": "Listener", "target": "blue", "event": "down", "input": "blues", "action": "add", "value": 1 }} ] }},
          {{ "type": "Node", "name": "group", "children": [
            {{ "type": "Bind", "source": "pick", "property": "solo" }},
            {}, {} ] }} ] }}"##,
        square("red", "#ff0000", r#", { "type": "Bind", "source": "front", "property": "order" }"#),
        square("blue", "#0000ff", "")
    ));
    let mut inst = play(&file);
    let (red, blue) = (object(inst.file(), "red"), object(inst.file(), "blue"));
    let look = |inst: &mut Instance| (pixels(inst)(10, 10), inst.hit_test(10.0, 10.0));
    assert_eq!(look(&mut inst), (BLUE, Some(blue)), "document order: the later sibling is on top");
    inst.set_number("front", 1.0);
    assert!(inst.advance(0.0));
    assert_eq!(look(&mut inst), (RED, Some(red)), "a higher order draws later, and is what the pointer hits");
    inst.set_number("pick", 2.0);
    inst.advance(0.0);
    assert_eq!(look(&mut inst), (BLUE, Some(blue)), "solo 2: only the second child, whatever the order");
    assert_eq!(inst.bounds(object(inst.file(), "group")), Some([0.0, 0.0, 20.0, 20.0]));
    inst.set_number("pick", 1.0);
    inst.set_number("front", 0.0);
    inst.advance(0.0);
    assert_eq!(look(&mut inst), (RED, Some(red)), "solo 1: only the first");
    // What a solo hides cannot be touched either.
    inst.pointer(Pointer::Down, 10.0, 10.0);
    assert_eq!((inst.number("reds"), inst.number("blues")), (Some(1.0), Some(0.0)));
    inst.set_number("pick", 7.0);
    inst.advance(0.0);
    assert_eq!(look(&mut inst), (CLEAR, None), "solo past the last child: nothing");
    assert_eq!(inst.bounds(object(inst.file(), "group")), None, "and nothing to measure");
}

/// A one-layer state machine over POSES: `entry` goes straight to the first of `states`.
fn machine(first: &str, states: &str) -> Arc<File> {
    load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [
            {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "{first}" }} ] }},
            {states} ] }} ] }} ] }}"##
    ))
}

#[test]
fn transitions_ease_hold_pause_act_and_compare_properties() {
    // `a` travels (x = 40 t); a fade to `b` (x = 100) starts at t = 0.25 and is looked at half-way.
    let half_way = |transition: &str| {
        let file = machine(
            "a",
            &format!(
                r##"{{ "type": "State", "name": "a", "animation": "travel", "children": [
                  {{ "type": "Transition", "to": "b", "blend": 1, {transition} "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
                {{ "type": "State", "name": "b", "animation": "right" }}"##
            ),
        );
        let mut inst = play(&file);
        inst.advance(0.25);
        inst.fire("go");
        inst.advance(0.5);
        let mid = x_of(&inst, "s");
        inst.advance(0.5);
        assert_eq!(x_of(&inst, "s"), 100.0, "{transition}");
        mid
    };
    let near = |got: f32, want: f32| assert!((got - want).abs() < 1e-2, "{got} vs {want}");
    near(half_way(""), 65.0); // travel has reached 30; half of the way to 100
    near(half_way(r#""pause": true,"#), 55.0); // travel stayed at 10
    near(half_way(r#""interp": "hold","#), 30.0); // still all travel
    near(half_way(r#""interp": "cubic", "easeX1": 0, "easeY1": 0, "easeX2": 1, "easeY2": 1,"#), 65.0); // a straight "curve"
    let eased = half_way(r#""interp": "cubic", "easeX1": 0.9, "easeY1": 0, "easeX2": 1, "easeY2": 1,"#);
    assert!(eased > 30.0 && eased < 45.0, "ease-in is still mostly the old state half-way: {eased}");

    // Actions run when a transition fires; a Condition can compare two Properties.
    let file = machine(
        "a",
        r##"{ "type": "State", "name": "a", "children": [
              { "type": "Transition", "to": "b", "children": [
                { "type": "Condition", "input": "mix", "op": "lt", "with": "alarm" },
                { "type": "Action", "input": "mix", "action": "add", "value": 2 },
                { "type": "Action", "input": "alarm", "action": "toggle" } ] } ] },
            { "type": "State", "name": "b", "animation": "right" }"##,
    );
    let mut inst = play(&file);
    inst.advance(0.1);
    assert_eq!((x_of(&inst, "s"), inst.number("mix")), (0.0, Some(0.25)), "0.25 < 0 is false");
    inst.set_bool("alarm", true);
    inst.advance(0.1);
    assert_eq!((x_of(&inst, "s"), inst.number("mix"), inst.number("alarm")), (100.0, Some(2.25), Some(0.0)));

    // A trigger fired by a Transition's Action fires for the whole next frame, so a layer
    // that runs before the one that fired it hears it too.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 100, "height": 10, "children": [ {POSES},
          {{ "type": "StateMachine", "children": [
            {{ "type": "Layer", "children": [
              {{ "type": "State", "name": "entry", "kind": "entry", "children": [ {{ "type": "Transition", "to": "wait" }} ] }},
              {{ "type": "State", "name": "wait", "children": [ {{ "type": "Transition", "to": "moved", "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
              {{ "type": "State", "name": "moved", "animation": "right" }} ] }},
            {{ "type": "Layer", "children": [
              {{ "type": "State", "name": "entry2", "kind": "entry", "children": [ {{ "type": "Transition", "to": "idle" }} ] }},
              {{ "type": "State", "name": "idle", "children": [
                {{ "type": "Transition", "to": "done", "children": [
                  {{ "type": "Condition", "input": "alarm", "value": 1 }}, {{ "type": "Action", "input": "go", "action": "fire" }} ] }} ] }},
              {{ "type": "State", "name": "done" }} ] }} ] }} ] }}"##
    ));
    let mut inst = play(&file);
    inst.set_bool("alarm", true);
    inst.advance(0.1);
    assert_eq!(x_of(&inst, "s"), 0.0, "the first layer had already run");
    inst.advance(0.1);
    assert_eq!(x_of(&inst, "s"), 100.0);

    // Mistakes in Actions and `with` are load errors.
    let broken = |body: &str| {
        let state = format!(r#"{{ "type": "State", "name": "a", "children": [ {{ "type": "Transition", "to": "a", "children": [ {body} ] }} ] }}"#);
        let src = format!(
            r#"{{ "children": [ {{ "type": "Artboard", "children": [ {POSES}, {{ "type": "StateMachine", "children": [ {{ "type": "Layer", "children": [ {state} ] }} ] }} ] }} ] }}"#
        );
        File::from_objects(json::compile(&src, &|_| unreachable!()).unwrap()).err().unwrap().0
    };
    assert!(broken(r#"{ "type": "Action", "action": "add" }"#).contains("an Action needs an 'input'"));
    assert!(broken(r#"{ "type": "Action", "input": "go", "action": "add" }"#).contains("'action' cannot be applied"));
    assert!(broken(r#"{ "type": "Action", "input": "mix", "action": "fire" }"#).contains("'action' cannot be applied"));
    assert!(broken(r#"{ "type": "Condition", "input": "mix", "with": "go" }"#).contains("'with' must be a number, bool or enum"));
}

#[test]
fn chance_picks_among_the_transitions_that_could_fire() {
    let back = r#""children": [ { "type": "Transition", "to": "a", "children": [ { "type": "Condition", "input": "go" } ] } ]"#;
    let file = machine(
        "a",
        &format!(
            r##"{{ "type": "State", "name": "a", "children": [
              {{ "type": "Transition", "to": "b", "chance": 1, "children": [ {{ "type": "Condition", "input": "go" }} ] }},
              {{ "type": "Transition", "to": "never", "chance": 5, "children": [ {{ "type": "Condition", "input": "alarm", "value": 1 }} ] }},
              {{ "type": "Transition", "to": "c", "chance": 3, "children": [ {{ "type": "Condition", "input": "go" }} ] }} ] }},
            {{ "type": "State", "name": "b", {back} }}, {{ "type": "State", "name": "c", {back} }}, {{ "type": "State", "name": "never" }}"##
        ),
    );
    let run = || {
        let mut inst = play(&file);
        let (b, c) = (object(inst.file(), "b"), object(inst.file(), "c"));
        let mut picks = Vec::new();
        for _ in 0..400 {
            inst.fire("go");
            inst.advance(0.0);
            picks.push(inst.states()[0]);
            inst.fire("go");
            inst.advance(0.0); // back to `a`
        }
        assert!(picks.iter().all(|s| *s == b || *s == c), "a transition whose conditions fail is not in the draw");
        (picks.iter().filter(|s| **s == b).count(), picks)
    };
    let (to_b, picks) = run();
    assert!((70..=130).contains(&to_b), "1 : 3 over 400 draws should be near 100, got {to_b}");
    assert_eq!(picks, run().1, "every instance draws the same sequence");
    // And so does a machine that is started again.
    let mut inst = play(&file);
    let first = |inst: &mut Instance| (inst.fire("go"), inst.advance(0.0), inst.states()[0]).2;
    let pick = first(&mut inst);
    inst.play_state_machine(0);
    inst.advance(0.0);
    assert_eq!(first(&mut inst), pick);
}

#[test]
fn direct_states_mix_animations_over_the_design_pose() {
    // `d` holds `right` (x = 100) at the weight `mix` (0.25) over the design pose (x = 0).
    let file = machine(
        "a",
        r##"{ "type": "State", "name": "a", "animation": "travel", "children": [
              { "type": "Transition", "to": "d", "blend": 1, "children": [ { "type": "Condition", "input": "go" } ] } ] },
            { "type": "State", "name": "d", "kind": "direct", "children": [
              { "type": "BlendAnimation", "animation": "right", "input": "mix" },
              { "type": "BlendAnimation", "animation": "left", "value": 0 },
              { "type": "Transition", "to": "a", "blend": 1, "children": [ { "type": "Condition", "input": "go" } ] } ] }"##,
    );
    let mut inst = play(&file);
    let near = |inst: &Instance, want: f32| assert!((x_of(inst, "s") - want).abs() < 0.05, "{} vs {want}", x_of(inst, "s"));
    inst.advance(0.5); // travel: x = 20
    inst.fire("go");
    inst.advance(0.5); // travel plays on underneath (x = 40), half-way to the direct pose (25)
    near(&inst, 32.5);
    inst.advance(0.49);
    inst.advance(0.02); // the fade ends without a jump
    near(&inst, 25.0);
    assert!(!inst.advance(0.1) && !inst.advance(0.1), "a settled direct state is idle");
    near(&inst, 25.0);
    inst.set_number("mix", 1.0);
    assert!(inst.advance(0.1));
    near(&inst, 100.0);
    inst.set_number("mix", 0.0);
    inst.advance(0.1);
    near(&inst, 0.0);
    // Fading out of it: the direct pose (x = 0) is what fades, once.
    inst.set_number("mix", 0.5);
    inst.advance(0.1);
    inst.fire("go");
    inst.advance(0.5); // from 50 toward travel, which restarted and is at x = 20
    near(&inst, 35.0);
}

#[test]
fn fit_modes_and_alignment_place_the_artboard() {
    let file = load(r##"{ "type": "Artboard", "width": 100, "height": 50 }"##);
    let inst = play(&file);
    let placed = |fit: Fit, ax: f32, ay: f32, w: f32, h: f32| {
        let m = inst.fit(fit, ax, ay, w, h);
        [m.a, m.d, m.tx, m.ty]
    };
    // Scale x, scale y, then where the artboard's top left corner lands.
    assert_eq!(placed(Fit::Contain, 0.5, 0.5, 400.0, 400.0), [4.0, 4.0, 0.0, 100.0]);
    assert_eq!(placed(Fit::Contain, 0.5, 1.0, 400.0, 400.0), [4.0, 4.0, 0.0, 200.0], "bottom");
    assert_eq!(placed(Fit::Cover, 0.0, 0.5, 400.0, 400.0), [8.0, 8.0, 0.0, 0.0], "left: the right half overflows");
    assert_eq!(placed(Fit::Cover, 1.0, 0.5, 400.0, 400.0), [8.0, 8.0, -400.0, 0.0]);
    assert_eq!(placed(Fit::FitWidth, 0.5, 0.0, 200.0, 400.0), [2.0, 2.0, 0.0, 0.0]);
    assert_eq!(placed(Fit::FitHeight, 0.5, 0.5, 200.0, 200.0), [4.0, 4.0, -100.0, 0.0]);
    assert_eq!(placed(Fit::ScaleDown, 0.5, 0.5, 400.0, 400.0), [1.0, 1.0, 150.0, 175.0], "room to spare: its own size");
    assert_eq!(placed(Fit::ScaleDown, 0.5, 0.5, 50.0, 50.0), [0.5, 0.5, 0.0, 12.5], "too big: contain");
}

#[test]
fn synced_trim_feather_offset_and_inner_feather() {
    // Two horizontal lines in one shape, the stroke trimmed to its first half.
    let lines = |mode: &str| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 40, "height": 20, "children": [
              {{ "type": "Shape", "children": [
                {{ "type": "Path", "d": "M0 5 L40 5 M0 15 L40 15" }},
                {{ "type": "Stroke", "color": "#ff0000", "thickness": 4, "trimEnd": 0.5, "trimMode": "{mode}" }} ] }} ] }}"##
        ));
        let at = pixels(&mut play(&file));
        [at(10, 5), at(30, 5), at(10, 15), at(30, 15)]
    };
    assert_eq!(lines("sequential"), [RED, RED, CLEAR, CLEAR], "half of the whole: all of the first line");
    assert_eq!(lines("synced"), [RED, CLEAR, RED, CLEAR], "half of each line");

    // A fill moved by its feather offset is a drop shadow; an inner feather stays inside.
    let square = |paint: &str| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 40, "height": 40, "children": [
              {{ "type": "Shape", "x": 20, "y": 20, "scaleX": 2, "children": [
                {{ "type": "Rectangle", "width": 8, "height": 16 }}, {{ "type": "Fill", "color": "#ff0000", {paint} }} ] }} ] }}"##
        ));
        pixels(&mut play(&file))
    };
    let at = square(r#""featherX": 4"#); // the shape is twice as wide, and so is the offset
    assert_eq!((at(14, 20), at(22, 20), at(34, 20), at(37, 20)), (CLEAR, RED, RED, CLEAR));
    // The paint travels with it: a gradient that is blue on the shape's left edge is blue on the shadow's.
    let at = square(
        r##""featherX": 4, "children": [ { "type": "LinearGradient", "x0": -4, "x1": 4, "children": [
            { "type": "GradientStop", "position": 0, "color": "#0000ff" }, { "type": "GradientStop", "position": 1, "color": "#ff0000" } ] } ]"##,
    );
    assert!(at(21, 20)[2] > 230 && at(34, 20)[0] > 230, "{:?} {:?}", at(21, 20), at(34, 20));
    let at = square(r#""feather": 3, "featherInner": true"#);
    assert!(at(10, 20) == CLEAR && at(20, 20)[3] < 16, "nothing outside the outline, next to nothing in the middle: {:?}", at(20, 20));
    assert!(at(12, 20)[3] > 90 && at(27, 20)[3] > 90 && at(20, 12)[3] > 90, "strongest just inside each edge");
    let at = square(r#""feather": 3, "featherInner": true, "featherX": 2"#);
    assert!(at(12, 20)[3] > 200 && at(27, 20)[3] < 30, "leaning on the left edge: {:?} {:?}", at(12, 20), at(27, 20));
}

#[test]
fn constraints_copy_at_a_rate_and_keep_an_offset() {
    // `leader` is at (60, 20), turned 350 degrees (-10 the short way) and scaled 2 by 3.
    let follower = |own: &str, constraint: &str| {
        let inst = play(&load(&format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 120, "children": [
              {{ "type": "Node", "name": "follower", {own} "children": [ {{ "type": "Constraint", "target": "leader", {constraint} }} ] }},
              {{ "type": "Node", "name": "leader", "x": 60, "y": 20, "rotation": 350, "scaleX": 2, "scaleY": 3 }} ] }}"##
        )));
        inst.world_matrix(object(inst.file(), "follower")).unwrap()
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    let place = |own: &str, constraint: &str| {
        let m = follower(own, &format!(r#""constrain": "translation", {constraint}"#));
        (m.tx.round(), m.ty.round())
    };
    let turn = |own: &str, constraint: &str| {
        let m = follower(own, &format!(r#""constrain": "rotation", {constraint}"#));
        m.b.atan2(m.a).to_degrees().round()
    };
    let size = |own: &str, constraint: &str| {
        let m = follower(own, &format!(r#""constrain": "scale", {constraint}"#));
        ((m.a.hypot(m.b) * 100.0).round() / 100.0, (m.c.hypot(m.d) * 100.0).round() / 100.0)
    };
    let here = r#""x": 5, "y": 5,"#;

    // `factor`: what is copied, times it. Half the way, the other way, and still one axis at a time.
    assert_eq!(place(here, r#""factor": 0.5"#), (30., 10.));
    assert_eq!(place(here, r#""factor": -1"#), (-60., -20.));
    assert_eq!(place(here, r#""factor": 0.5, "axis": "x""#), (30., 5.));
    // A rotation is copied as the number the leader's `rotation` makes it, 350 and not the
    // -10 its matrix reads: half of it is 175 (not -5), so a follower at half the rate
    // does not snap back each time its leader passes 180.
    assert_eq!((turn("", r#""factor": 0.5"#), turn("", r#""factor": 2"#), turn("", r#""factor": -1"#)), (175., -20., 10.));
    assert_eq!(size("", r#""factor": 0.5"#), (1., 1.5));
    // `offset`: the follower's own value on top, so it sits that far from the leader (a scale is multiplied).
    assert_eq!(place(here, r#""offset": true"#), (65., 25.));
    assert_eq!(place(here, r#""offset": true, "factor": 0.5"#), (35., 15.));
    assert_eq!(turn(r#""rotation": 30,"#, r#""offset": true"#), 20.);
    assert_eq!(size(r#""scaleX": 3,"#, r#""offset": true"#), (6., 3.));
    // A follower that is flipped stays flipped the way it was: upside down times 2 by 3 is
    // 2 by -3, and flipped both ways is -2 by -3.
    let flipped = follower(r#""scaleY": -1,"#, r#""constrain": "scale", "offset": true"#);
    assert!(near(flipped.a, 2.0) && near(flipped.d, -3.0) && near(flipped.b, 0.0), "{flipped:?}");
    let turned = follower(r#""scaleX": -1, "scaleY": -1,"#, r#""constrain": "scale", "offset": true"#);
    assert!(near(turned.a, -2.0) && near(turned.d, -3.0), "{turned:?}");
    // The limits are on what comes out: twice the leader's x is 120, held at 100; half its
    // turn is 175, held at 100; and a size is limited whichever way it faces.
    assert_eq!(place(here, r#""factor": 2, "max": 100"#), (100., 40.));
    assert_eq!(turn("", r#""factor": 0.5, "max": 100"#), 100.);
    assert_eq!(size("", r#""factor": -1, "max": 1.5"#), (1.5, 1.5));
    // Without either, as before; half strength goes half way to what would be copied; and
    // a `transform` Constraint, which copies everything as it is, reads neither.
    assert_eq!(place(here, r#""strength": 1"#), (60., 20.));
    let half = follower(here, r#""constrain": "translation", "factor": 0.5, "strength": 0.5"#);
    assert!(near(half.tx, 17.5) && near(half.ty, 7.5), "{half:?}");
    let whole = follower(here, r#""constrain": "transform", "factor": 0.5, "offset": true"#);
    assert!(near(whole.tx, 60.0) && near(whole.ty, 20.0), "{whole:?}");
}

#[test]
fn constraints_make_an_object_follow_another() {
    // `leader` and `track` are fixed; `follower` carries one constraint and a red square.
    let scene = |follower: &str, constraint: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 120, "children": [
              {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "mix", "valueType": "number", "value": 1 }} ] }},
              {{ "type": "Node", "name": "follower", {follower} "children": [
                {{ "type": "Constraint", {constraint}, "children": [ {{ "type": "Bind", "source": "mix", "property": "strength" }} ] }},
                {{ "type": "Shape", "name": "mark", "children": [ {{ "type": "Rectangle", "width": 6, "height": 6 }}, {{ "type": "Fill", "color": "#ff0000" }} ] }} ] }},
              {{ "type": "Node", "name": "leader", "x": 60, "y": 20, "rotation": 350, "scaleX": 2, "scaleY": 3 }},
              {{ "type": "Shape", "name": "track", "x": 10, "y": 10, "children": [ {{ "type": "Path", "d": "M0 0 L100 0 L100 100" }} ] }} ] }}"##
        ))
    };
    // Where the follower's child ends up, and the follower's rotation in degrees.
    let placed = |inst: &Instance| {
        let m = inst.world_matrix(object(inst.file(), "mark")).unwrap();
        ((m.tx * 10.0).round() / 10.0, (m.ty * 10.0).round() / 10.0, m.b.atan2(m.a).to_degrees().round())
    };
    let at = |follower: &str, constraint: &str| placed(&play(&scene(follower, constraint)));

    assert_eq!(at(r#""x": 5, "y": 5,"#, r#""constrain": "translation", "target": "leader""#), (60.0, 20.0, 0.0));
    assert_eq!(at(r#""rotation": 20,"#, r#""constrain": "rotation", "target": "leader""#), (0.0, 0.0, -10.0), "350 degrees, the short way");
    let m = play(&scene("", r#""constrain": "transform", "target": "leader""#));
    let w = m.world_matrix(object(m.file(), "follower")).unwrap();
    assert!((w.a.hypot(w.b) - 2.0).abs() < 1e-4 && (w.tx, w.ty) == (60.0, 20.0), "{w:?}");

    // A mirrored follower stays mirrored; a skewed one is not rebuilt, so a constraint
    // that is barely on barely changes it.
    let m = play(&scene(r#""rotation": 20, "scaleX": -1,"#, r#""constrain": "rotation", "target": "leader""#));
    let w = m.world_matrix(object(m.file(), "follower")).unwrap();
    assert!(w.a * w.d - w.b * w.c < 0.0 && ((-w.b).atan2(-w.a).to_degrees() + 10.0).abs() < 1e-3, "{w:?}");
    let skewed = |mix: f32| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 10, "height": 10, "children": [
              {{ "type": "Node", "name": "leader", "rotation": 90 }},
              {{ "type": "Node", "scaleX": 2, "children": [ {{ "type": "Node", "name": "follower", "rotation": 30, "children": [
                {{ "type": "Constraint", "constrain": "rotation", "target": "leader", "strength": {mix} }} ] }} ] }} ] }}"##
        ));
        let inst = play(&file);
        let w = inst.world_matrix(object(inst.file(), "follower")).unwrap();
        [w.a, w.b, w.c, w.d]
    };
    let (off, barely) = (skewed(0.0), skewed(0.001));
    assert!(off.iter().zip(&barely).all(|(a, b)| (a - b).abs() < 0.01), "{off:?} {barely:?}");

    // Distance from the leader at (60, 20).
    assert_eq!(at(r#""x": 60, "y": 120,"#, r#""constrain": "distance", "target": "leader", "distance": 30"#), (60.0, 50.0, 0.0), "no further than 30");
    assert_eq!(at(r#""x": 60, "y": 30,"#, r#""constrain": "distance", "target": "leader", "distance": 30"#), (60.0, 30.0, 0.0), "already inside");
    assert_eq!(at(r#""x": 60, "y": 30,"#, r#""constrain": "distance", "target": "leader", "distance": 30, "limit": "further""#), (60.0, 50.0, 0.0));
    assert_eq!(at(r#""x": 60, "y": 120,"#, r#""constrain": "distance", "target": "leader", "distance": 30, "limit": "exact""#), (60.0, 50.0, 0.0));

    // One axis of a copy, and limits on what is copied. The leader is at (60, 20), turned 350:
    // a rotation is limited as the number the leader's own `rotation` makes it (350, not
    // the -10 its matrix reads), so a limit under it holds and one over it does nothing.
    let copy = |what: &str, more: &str| at(r#""x": 5, "y": 5, "rotation": 20,"#, &format!(r#""constrain": "{what}", "target": "leader", {more}"#));
    assert_eq!((copy("translation", r#""axis": "x""#), copy("translation", r#""axis": "y""#)), ((60.0, 5.0, 20.0), (5.0, 20.0, 20.0)));
    assert_eq!((copy("translation", r#""max": 40"#), copy("translation", r#""axis": "x", "min": 80"#)), ((40.0, 20.0, 20.0), (80.0, 5.0, 20.0)));
    assert_eq!(
        (copy("rotation", r#""max": 340"#), copy("rotation", r#""max": -30"#), copy("rotation", r#""min": -5"#)),
        ((5.0, 5.0, -20.0), (5.0, 5.0, -30.0), (5.0, 5.0, -10.0))
    );
    let wide = |more: &str| {
        let inst = play(&scene("", &format!(r#""constrain": "scale", "target": "leader", {more}"#)));
        let w = inst.world_matrix(object(inst.file(), "follower")).unwrap();
        ((w.a.hypot(w.b) * 10.0).round() / 10.0, (w.c.hypot(w.d) * 10.0).round() / 10.0)
    };
    // The leader is twice as wide and three times as tall.
    assert_eq!((wide(r#""axis": "both""#), wide(r#""axis": "y""#), wide(r#""axis": "x""#)), ((2.0, 3.0), (1.0, 3.0), (2.0, 1.0)));
    assert_eq!((wide(r#""max": 1.5"#), wide(r#""min": 2.5"#)), ((1.5, 1.5), (2.5, 3.0)));
    // Limits that cross: `max` wins, wherever the leader is. And a mirrored leader stays
    // mirrored under a limit, which is on its size.
    assert_eq!(copy("translation", r#""axis": "x", "min": 80, "max": 40"#), (40.0, 5.0, 20.0));
    let mirrored = load(
        r#"{ "type": "Artboard", "width": 10, "height": 10, "children": [
          { "type": "Node", "name": "leader", "scaleX": -2 },
          { "type": "Node", "name": "follower", "children": [ { "type": "Constraint", "constrain": "scale", "target": "leader", "max": 1.5 } ] } ] }"#,
    );
    let inst = play(&mirrored);
    assert_eq!(inst.world_matrix(object(inst.file(), "follower")).unwrap().a, -1.5);

    // Along the track's outline: right for 100, then down for 100, from (10, 10).
    let along = |position: f32, orient: bool| at("", &format!(r#""constrain": "followPath", "target": "track", "position": {position}, "orient": {orient}"#));
    assert_eq!(along(0.25, true), (60.0, 10.0, 0.0));
    assert_eq!(along(0.75, true), (110.0, 60.0, 90.0), "facing down the second leg");
    assert_eq!(along(0.75, false), (110.0, 60.0, 0.0));
    assert_eq!((along(1.0, false), along(1.25, false)), ((110.0, 110.0, 0.0), (60.0, 10.0, 0.0)), "1 is the end; past it wraps");

    // Strength blends, and what is inside the constrained object is drawn where it went.
    let mut inst = play(&scene(r#""x": 20, "y": 100,"#, r#""constrain": "translation", "target": "leader""#));
    assert_eq!(pixels(&mut inst)(60, 20), RED);
    inst.set_number("mix", 0.5);
    assert!(inst.advance(0.0));
    assert_eq!(placed(&inst), (40.0, 60.0, 0.0));
    assert_eq!((pixels(&mut inst)(40, 60), pixels(&mut inst)(60, 20)), (RED, CLEAR));
    inst.set_number("mix", 0.0);
    inst.advance(0.0);
    assert_eq!(placed(&inst), (20.0, 100.0, 0.0));
    assert!(!inst.advance(0.1), "a constrained scene still settles");
}

#[test]
fn enums_binds_that_write_back_formatted_text_and_change_listeners() {
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 20, "children": [
          { "type": "ViewModel", "children": [
            { "type": "Property", "name": "mode", "valueType": "enum", "value": 1, "children": [
              { "type": "Option", "name": "idle" }, { "type": "Option", "name": "busy" }, { "type": "Option", "name": "done" } ] },
            { "type": "Property", "name": "where", "valueType": "number" },
            { "type": "Property", "name": "shared", "valueType": "number", "value": 7 },
            { "type": "Property", "name": "seen", "valueType": "number" },
            { "type": "Property", "name": "modeName", "valueType": "string" } ] },
          { "type": "Text", "children": [
            { "type": "Bind", "source": "mode", "property": "text" },
            { "type": "Bind", "source": "modeName", "property": "text", "direction": "toSource" } ] },
          { "type": "Event", "name": "modeChanged" },
          { "type": "Node", "name": "mover", "children": [
            { "type": "Bind", "source": "where", "property": "x", "direction": "toSource" },
            { "type": "Bind", "source": "shared", "property": "y", "direction": "twoWay" } ] },
          { "type": "Animation", "name": "slide", "fps": 10, "duration": 10, "children": [
            { "type": "Track", "target": "mover", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 0 }, { "type": "Key", "frame": 10, "value": 50 } ] },
            { "type": "Track", "target": "mover", "property": "y", "children": [ { "type": "Key", "frame": 5, "value": 7 }, { "type": "Key", "frame": 10, "value": 20, "interp": "hold" } ] } ] },
          { "type": "StateMachine", "children": [
            { "type": "Layer", "children": [
              { "type": "State", "name": "entry", "kind": "entry", "children": [ { "type": "Transition", "to": "go" } ] },
              { "type": "State", "name": "go", "animation": "slide" } ] },
            { "type": "Listener", "event": "change", "with": "mode", "emit": "modeChanged", "children": [
              { "type": "Action", "input": "seen", "action": "add", "value": 1 } ] } ] } ] }"##,
    );
    let mut inst = play(&file);
    // An enum is a number with names: set and read either way; binds to text give the name.
    assert_eq!(inst.properties().iter().find(|p| p.0 == "mode").map(|p| p.1), Some(5));
    assert_eq!((inst.number("mode"), inst.string("mode")), (Some(1.0), Some("busy")));
    assert!(inst.set_string("mode", "done") && !inst.set_string("mode", "nope"));
    assert_eq!((inst.number("mode"), inst.string("mode")), (Some(2.0), Some("done")));
    assert!(inst.set_number("mode", 0.0) && !inst.set_number("mode", 3.0) && !inst.set_number("mode", 0.5));
    assert_eq!(inst.string("mode"), Some("idle"));
    inst.advance(0.0);
    assert_eq!(inst.string("modeName"), Some("idle"), "bound to a text, an enum is its Option's name");
    assert!(inst.set_string("mode", "busy"));

    // A `change` listener hears it on the next frame, once, and not when nothing changed.
    assert_eq!((inst.take_events().len(), inst.number("seen")), (1, Some(1.0)), "the set_number above, heard by the advance after it");
    inst.advance(0.0);
    assert_eq!((inst.take_events(), inst.number("seen")), (vec!["modeChanged".to_string()], Some(2.0)), "and the set_string after it");
    inst.advance(0.1);
    assert_eq!((inst.take_events().len(), inst.number("seen")), (0, Some(2.0)));

    // toSource: the animated x (50 per second) can be read from the view model.
    assert!((inst.number("where").unwrap() - 5.0).abs() < 1e-3, "{:?}", inst.number("where"));
    // twoWay: the Property set y at first (7); the host can still set it; and when the
    // animation moves y (a hold key to 20 at one second) the Property follows.
    assert_eq!(inst.property(object(inst.file(), "mover"), p::Y), Some(7.0));
    inst.set_number("shared", 9.0);
    inst.advance(0.1);
    assert_eq!(inst.property(object(inst.file(), "mover"), p::Y), Some(9.0), "the Property changed: it wins this frame");
    inst.advance(1.0);
    assert_eq!((inst.property(object(inst.file(), "mover"), p::Y), inst.number("shared")), (Some(20.0), Some(20.0)));
}

#[test]
fn view_model_mistakes_are_load_errors() {
    let err = |children: &str| {
        let src = format!(r#"{{ "children": [ {{ "type": "Artboard", "name": "A", "children": [ {children} ] }} ] }}"#);
        File::from_objects(json::compile(&src, &|_| unreachable!()).unwrap_or_else(|e| panic!("{e}"))).err().map(|e| e.0).unwrap_or_default()
    };
    let vm = |props: &str| format!(r#"{{ "type": "ViewModel", "children": [ {props} ] }}"#);
    let mode = |value: &str| {
        format!(
            r#"{{ "type": "Property", "name": "mode", "valueType": "enum", "value": {value}, "children": [ {{ "type": "Option", "name": "idle" }}, {{ "type": "Option", "name": "busy" }} ] }}"#
        )
    };
    // An enum starts on one of its Options, given by position or by name.
    assert_eq!(err(&vm(&mode("1"))), "");
    assert_eq!(err(&vm(&mode(r#""busy""#))), "");
    for bad in ["2", "0.5", r#""nope""#] {
        assert!(err(&vm(&mode(bad))).contains("must be one of its Options"), "{bad}");
    }
    assert!(err(&vm(r#"{ "type": "Property", "name": "e", "valueType": "enum" }"#)).contains("must be one of its Options"));
    // Option names are their enum's own: they do not clash with objects, or with another enum's.
    let shape = r#"{ "type": "Shape", "name": "busy" }, { "type": "Animation", "children": [ { "type": "Track", "target": "busy", "property": "x" } ] }"#;
    assert_eq!(err(&format!("{}, {shape}", vm(&format!("{}, {}", mode("0"), mode("1").replace("mode", "other"))))), "");
    // A Bind that writes back needs a Property of the kind the object's property holds.
    let bind = |ty: &str, prop: &str| {
        err(&format!(
            r#"{}, {{ "type": "Text", "children": [ {{ "type": "Bind", "source": "v", "property": "{prop}", "direction": "toSource" }} ] }}"#,
            vm(&format!(r#"{{ "type": "Property", "name": "v", "valueType": "{ty}" }}"#))
        ))
    };
    assert_eq!((bind("number", "x"), bind("bool", "x"), bind("string", "text")), (String::new(), String::new(), String::new()));
    assert!(bind("number", "text").contains("needs a string Property"));
    assert!(bind("string", "x").contains("needs a number or bool Property"));
    assert!(bind("trigger", "x").contains("needs a number or bool Property"));
}

#[test]
fn binds_keep_a_number_in_range_and_make_it_whole() {
    // `level` in the view model drives `bar`'s x through a Bind with `convert`.
    let x = |convert: &str, level: f32| {
        let mut inst = play(&load(&format!(
            r##"{{ "type": "Artboard", "width": 200, "height": 20, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "level", "valueType": "number" }} ] }},
              {{ "type": "Shape", "name": "bar", "children": [ {{ "type": "Bind", "source": "level", "property": "x", {convert} }} ] }} ] }}"##
        )));
        inst.set_number("level", level);
        inst.advance(0.0);
        x_of(&inst, "bar")
    };
    // A range: 0..60 becomes 10..130, kept within 20..100. With the limits crossed, `max` wins.
    let range = r#""mul": 2, "add": 10, "min": 20, "max": 100"#;
    assert_eq!([0.0, 30.0, 60.0].map(|level| x(range, level)), [20.0, 70.0, 100.0]);
    assert_eq!(x(r#""min": 50, "max": 20"#, 35.0), 20.0);
    // Whole numbers: the nearest (a half goes away from 0, as it does in a number written
    // as text), the next down, the next up, also under 0; and a large whole number, which
    // adding a half would nudge, stays what it is.
    let whole = |round: &str, level: f32| x(&format!(r#""round": "{round}""#), level);
    assert_eq!([3.4, 3.5, -3.5, -3.4, 8388609.0, 0.49999997].map(|level| whole("nearest", level)), [3.0, 4.0, -4.0, -3.0, 8388609.0, 0.0]);
    assert_eq!([3.9, -3.1].map(|level| whole("down", level)), [3.0, -4.0]);
    assert_eq!([3.1, -3.9].map(|level| whole("up", level)), [4.0, -3.0]);
    assert_eq!(whole("none", 3.4), 3.4);
    // In that order: scaled, limited, then made whole (0.2 x 10 is 2, held at 2.5, which rounds to 3).
    assert_eq!(x(r#""mul": 10, "min": 2.5, "round": "nearest""#, 0.2), 3.0);
    // Without limits a number goes through, however large; a limit as far off as the
    // default (a billion) is none either, which is what an editor writes for "no limit".
    assert_eq!((x(r#""mul": 1"#, 3e12), x(r#""max": 1000000000"#, 3e12), x(r#""max": 500000000"#, 3e12)), (3e12, 3e12, 5e8));
    // A Bind that also writes back to the view model hands the value on as it is: 30.5 stays
    // 30.5 whatever limit and rounding it carries.
    let mut both = play(&load(
        r##"{ "type": "Artboard", "width": 20, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "level", "valueType": "number" } ] },
          { "type": "Shape", "name": "bar", "children": [ { "type": "Bind", "source": "level", "property": "x", "direction": "twoWay", "max": 5, "round": "up" } ] } ] }"##,
    ));
    both.set_number("level", 30.5);
    both.advance(0.0);
    assert_eq!(x_of(&both, "bar"), 30.5);

    // The same conversions on the way to a text, a whole-number property and a switch:
    // 7.6 held at 5 and written with a digit; 2.1 rounded up to show child 3 (cut off, it
    // would be 2); and a
    // Bool turned round by `"mul": -1, "add": 1`.
    let mut inst = play(&load(
        r##"{ "type": "Artboard", "width": 20, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [
            { "type": "Property", "name": "level", "valueType": "number", "value": 7.6 },
            { "type": "Property", "name": "which", "valueType": "number", "value": 2.1 },
            { "type": "Property", "name": "off", "valueType": "bool", "value": 1 },
            { "type": "Property", "name": "out", "valueType": "string" } ] },
          { "type": "Text", "children": [
            { "type": "Bind", "source": "level", "property": "text", "max": 5, "decimals": 1 },
            { "type": "Bind", "source": "out", "property": "text", "direction": "toSource" } ] },
          { "type": "Node", "name": "pick", "children": [ { "type": "Bind", "source": "which", "property": "solo", "round": "up" } ] },
          { "type": "Layout", "name": "row", "children": [ { "type": "Bind", "source": "off", "property": "wrap", "mul": -1, "add": 1 } ] } ] }"##,
    ));
    inst.advance(0.0);
    let read = |inst: &Instance, name: &str, key: u16| inst.property(object(inst.file(), name), key);
    assert_eq!((inst.string("out"), read(&inst, "pick", p::SOLO), read(&inst, "row", p::WRAP)), (Some("5.0"), Some(3.0), Some(0.0)));
    inst.set_bool("off", false);
    inst.advance(0.0);
    assert_eq!(read(&inst, "row", p::WRAP), Some(1.0));
}

#[test]
fn numbers_bound_to_text_are_formatted() {
    // Text needs a font to draw, but a bind writes the string either way: read it back
    // through a second, to-source bind into a string Property.
    let scene = |format: &str, value: f32| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 10, "height": 10, "children": [
              {{ "type": "ViewModel", "children": [
                {{ "type": "Property", "name": "n", "valueType": "number", "value": {value} }},
                {{ "type": "Property", "name": "out", "valueType": "string" }} ] }},
              {{ "type": "Text", "name": "t", "children": [
                {{ "type": "Bind", "source": "n", "property": "text", {format} }},
                {{ "type": "Bind", "source": "out", "property": "text", "direction": "toSource" }} ] }} ] }}"##
        ));
        let mut inst = play(&file);
        inst.advance(0.0);
        inst.string("out").unwrap().to_string()
    };
    assert_eq!(scene(r#""decimals": 0"#, 41.5), "42");
    assert_eq!(scene(r#""decimals": 2"#, 1.23456), "1.23");
    assert_eq!(scene(r#""decimals": 3"#, -0.0421), "-0.042");
    assert_eq!(scene(r#""decimals": 1, "mul": 100, "format": "{}%""#, 0.256), "25.6%");
    assert_eq!(scene(r#""format": "Score: {} pts""#, 7.0), "Score: 7 pts");
    // Far beyond what a text needs: no panic, and the sign survives.
    assert!(scene(r#""decimals": 9"#, -1e30).starts_with("-9000000000."));
    assert_eq!(scene(r#""decimals": 1"#, 1e30), "900000000000000000.0");
}

#[test]
fn key_listeners_react_to_the_keys_the_host_reports() {
    let keys = r##"{ "type": "ViewModel", "children": [
            { "type": "Property", "name": "presses", "valueType": "number" },
            { "type": "Property", "name": "held", "valueType": "bool" },
            { "type": "Property", "name": "any", "valueType": "number" } ] },
          { "type": "Event", "name": "submitted" },
          { "type": "StateMachine", "children": [
            { "type": "Listener", "event": "keyDown", "key": "Enter", "input": "presses", "action": "add", "value": 1, "emit": "submitted" },
            { "type": "Listener", "event": "keyUp", "key": " ", "input": "held", "action": "set", "value": 1 },
            { "type": "Listener", "event": "keyDown", "input": "any", "action": "add", "value": 1 } ] }"##;
    let file = load(&format!(
        r##"{{ "type": "Artboard", "name": "Parent", "width": 10, "height": 10, "children": [
          {{ "type": "Node", "name": "group", "solo": 1, "children": [ {{ "type": "NestedArtboard", "artboard": "Child" }}, {{ "type": "Node" }} ] }}, {keys} ] }},
        {{ "type": "Artboard", "name": "Child", "width": 10, "height": 10, "children": [ {keys} ] }}"##
    ));
    let mut inst = play(&file);
    inst.advance(0.0);
    assert!(!inst.advance(0.0), "settled");
    // The parent's Listener and the nested artboard's both hear Enter; both report the event.
    assert!(inst.key("Enter", true));
    assert_eq!((inst.number("presses"), inst.number("any"), inst.take_events()), (Some(1.0), Some(1.0), vec!["submitted".to_string(); 2]));
    assert!(inst.advance(0.0), "a key a Listener reacted to is a change");
    // Releasing Enter: nothing listens for that. A key nobody names still reaches the any-key Listener.
    assert!(!inst.key("Enter", false) && inst.key("x", true) && !inst.key("x", false));
    assert_eq!((inst.number("presses"), inst.number("any")), (Some(1.0), Some(2.0)));
    // A keyUp Listener hears the release, not the press (the any-key one hears the press).
    assert!(inst.key(" ", true) && inst.number("held") == Some(0.0));
    assert!(inst.key(" ", false) && inst.number("held") == Some(1.0));

    // A nested artboard that a solo hides does not hear keys.
    let hidden = load(&format!(
        r##"{{ "type": "Artboard", "name": "Parent", "width": 10, "height": 10, "children": [
          {{ "type": "Node", "solo": 2, "children": [ {{ "type": "NestedArtboard", "artboard": "Child" }}, {{ "type": "Node" }} ] }} ] }},
        {{ "type": "Artboard", "name": "Child", "width": 10, "height": 10, "children": [ {keys} ] }}"##
    ));
    let mut inst = play(&hidden);
    assert!(!inst.key("Enter", true) && inst.take_events().is_empty());
    // Mistakes are load errors; a key without a name is not a key.
    assert!(!inst.key("", true));
    for bad in [r#""event": "click", "key": "Enter""#, r#""event": "keyDown", "key": """#, r#""event": "keyDown", "target": "n""#] {
        let src = format!(
            r#"{{ "children": [ {{ "type": "Artboard", "children": [ {{ "type": "Node", "name": "n" }}, {{ "type": "StateMachine", "children": [ {{ "type": "Listener", {bad} }} ] }} ] }} ] }}"#
        );
        let err = File::from_objects(json::compile(&src, &|_| unreachable!()).unwrap()).err().unwrap().0;
        assert!(err.contains("is for a keyDown or keyUp Listener"), "{bad}: {err}");
    }
    // Without a state machine playing there are no Listeners (the Child artboard, not started).
    let mut idle = Instance::new(&file, 1).unwrap();
    assert!(!idle.key("Enter", true));
}

#[test]
fn bones_chain_from_end_to_end_and_ik_reaches_for_a_target() {
    // An arm: `upper` (30 long) from (10, 50), `lower` (20 long) at its end, a `hand` Node
    // at the far end of `lower`, and a `sleeve` Shape riding on `upper`.
    let arm = |pose: &str, constraint: &str, target: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 120, "children": [
              {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "mix", "valueType": "number", "value": 1 }} ] }},
              {{ "type": "Node", "name": "goal", {target} }},
              {{ "type": "Bone", "name": "upper", "x": 10, "y": 50, "length": 30, {pose} "children": [
                {{ "type": "Shape", "name": "sleeve", "x": 15, "children": [ {{ "type": "Rectangle", "width": 4, "height": 4 }}, {{ "type": "Fill", "color": "#ff0000" }} ] }},
                {{ "type": "Bone", "name": "lower", "length": 20, "rotation": 90, "children": [
                  {{ "type": "Node", "name": "hand", "x": 20 }} {constraint} ] }} ] }} ] }}"##
        ))
    };
    let at = |inst: &Instance, name: &str| {
        let m = inst.world_matrix(object(inst.file(), name)).unwrap();
        ((m.tx * 10.0).round() / 10.0, (m.ty * 10.0).round() / 10.0)
    };
    let ik = |bones: u32| {
        format!(
            r#", {{ "type": "Constraint", "constrain": "ik", "target": "goal", "bones": {bones}, "children": [ {{ "type": "Bind", "source": "mix", "property": "strength" }} ] }}"#
        )
    };

    // No constraint: the lower bone starts where the upper one ends, and points down.
    let inst = play(&arm("", "", r#""x": 0, "y": 0"#));
    assert_eq!((at(&inst, "lower"), at(&inst, "hand"), at(&inst, "sleeve")), ((40.0, 50.0), (40.0, 70.0), (25.0, 50.0)));
    let turned = play(&arm(r#""rotation": 90,"#, "", r#""x": 0, "y": 0"#));
    assert_eq!((at(&turned, "lower"), at(&turned, "hand")), ((10.0, 80.0), (-10.0, 80.0)), "the whole arm turns with the upper bone");

    // Two bones reach a target within their span (50): the hand lands on it, and what
    // rides on the upper bone went with it.
    let mut inst = play(&arm("", &ik(2), r#""x": 30, "y": 20"#));
    let (hand, sleeve) = (at(&inst, "hand"), at(&inst, "sleeve"));
    assert!((hand.0 - 30.0).abs() < 0.5 && (hand.1 - 20.0).abs() < 0.5, "{hand:?}");
    assert!(sleeve != (25.0, 50.0) && ((sleeve.0 - 10.0).hypot(sleeve.1 - 50.0) - 15.0).abs() < 0.1, "still 15 along the upper bone: {sleeve:?}");
    assert!(pixels(&mut inst)(sleeve.0 as u32, sleeve.1 as u32)[3] > 0, "and is drawn there");
    assert!(!inst.advance(0.1), "a solved pose settles");
    // Strength blends the pose in; at 0 the arm is as animated.
    inst.set_number("mix", 0.0);
    inst.advance(0.0);
    assert_eq!(at(&inst, "hand"), (40.0, 70.0));
    inst.set_number("mix", 0.5);
    inst.advance(0.0);
    let half = at(&inst, "hand");
    assert!(half != (40.0, 70.0) && (half.0 - 30.0).abs() + (half.1 - 20.0).abs() > 1.0, "part of the way: {half:?}");

    // Out of reach, the arm points straight at the target.
    let far = at(&play(&arm("", &ik(2), r#""x": 210, "y": 50"#)), "hand");
    assert!((far.0 - 60.0).abs() < 0.5 && (far.1 - 50.0).abs() < 0.5, "{far:?}");
    // One bone: only the lower bone turns (toward a target straight above its start).
    let inst = play(&arm("", &ik(1), r#""x": 40, "y": 0"#));
    assert_eq!((at(&inst, "lower"), at(&inst, "hand")), ((40.0, 50.0), (40.0, 30.0)));
    // Every target within reach is reached, whichever way the arm has to fold: on the
    // line of the upper bone, at the elbow's place, close to the shoulder. (An iterating
    // solver stalls on the first two.)
    for goal in [(45, 50), (40, 50), (20, 50), (10, 20), (30, 90), (55, 55), (-20, 30)] {
        let hand = at(&play(&arm("", &ik(2), &format!(r#""x": {}, "y": {}"#, goal.0, goal.1))), "hand");
        assert!((hand.0 - goal.0 as f32).abs() < 0.3 && (hand.1 - goal.1 as f32).abs() < 0.3, "{goal:?}: {hand:?}");
    }
    // A longer chain: four bones of 20, straight, reach anywhere within 80.
    let tail = |goal: (i32, i32)| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 200, "height": 200, "children": [
              {{ "type": "Node", "name": "goal", "x": {}, "y": {} }},
              {{ "type": "Bone", "x": 100, "y": 100, "length": 20, "children": [ {{ "type": "Bone", "length": 20, "children": [
                {{ "type": "Bone", "length": 20, "children": [ {{ "type": "Bone", "length": 20, "children": [
                  {{ "type": "Node", "name": "end", "x": 20 }},
                  {{ "type": "Constraint", "constrain": "ik", "target": "goal", "bones": 4 }} ] }} ] }} ] }} ] }} ] }}"##,
            goal.0, goal.1
        ));
        at(&play(&file), "end")
    };
    for goal in [(150, 100), (100, 30), (60, 140), (110, 105), (100, 100)] {
        let end = tail(goal);
        assert!((end.0 - goal.0 as f32).abs() < 0.3 && (end.1 - goal.1 as f32).abs() < 0.3, "{goal:?}: {end:?}");
    }
    // A bone without a length has nothing to point with: it is left alone, and the
    // chain above it still solves.
    let stub = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "Node", "name": "goal", "x": 40, "y": 30 },
          { "type": "Bone", "name": "upper", "x": 10, "y": 50, "length": 30, "children": [
            { "type": "Bone", "name": "wrist", "rotation": 33, "children": [ { "type": "Constraint", "constrain": "ik", "target": "goal" } ] } ] } ] }"##,
    );
    let inst = play(&stub);
    let wrist = inst.world_matrix(object(inst.file(), "wrist")).unwrap();
    let upper = inst.world_matrix(object(inst.file(), "upper")).unwrap();
    assert!((upper.b.atan2(upper.a).to_degrees() + 33.69).abs() < 0.05, "the upper bone points at the target");
    assert!((wrist.b.atan2(wrist.a).to_degrees() - (33.0 - 33.69)).abs() < 0.05, "the wrist keeps its own rotation");
    // An `ik` Constraint anywhere but in a Bone is a mistake, said at load.
    let misplaced = json::compile(
        r#"{ "children": [ { "type": "Artboard", "children": [ { "type": "Node", "name": "n", "children": [ { "type": "Constraint", "constrain": "ik", "target": "n" } ] } ] } ] }"#,
        &|_| unreachable!(),
    );
    assert!(File::from_objects(misplaced.unwrap()).err().unwrap().0.contains("goes inside the Bone"));
    // In a mirrored arm the bones still turn the right way.
    let mirrored = at(&play(&arm(r#""scaleY": -1,"#, &ik(2), r#""x": 30, "y": 20"#)), "hand");
    assert!((mirrored.0 - 30.0).abs() < 0.5 && (mirrored.1 - 20.0).abs() < 0.5, "{mirrored:?}");
}

#[test]
fn skinned_paths_bend_with_their_bones() {
    // Two bones in a line from (10, 60), 50 long each, and a strip drawn along them. Its
    // ends are held by one bone each and the middle by both. `skins` is the Path's children.
    let strip = |d: &str, skins: &str| {
        format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 120, "children": [
              {{ "type": "Bone", "name": "upper", "x": 10, "y": 60, "length": 50, "children": [ {{ "type": "Bone", "name": "lower", "length": 50 }} ] }},
              {{ "type": "Shape", "name": "strip", "x": 5, "children": [
                {{ "type": "Path", "name": "outline", "x": -5, "d": "{d}", "children": [ {skins} ] }},
                {{ "type": "Fill", "color": "#ff0000" }} ] }},
              {{ "type": "Animation", "name": "bend", "children": [
                {{ "type": "Track", "target": "lower", "property": "rotation", "children": [ {{ "type": "Key", "frame": 0, "value": 0 }}, {{ "type": "Key", "frame": 60, "value": 90 }} ] }} ] }} ] }}"##
        )
    };
    let skin = |bone: &str, weights: &str| format!(r#"{{ "type": "Skin", "target": "{bone}", "weights": "{weights}" }}"#);
    let d = "M10 55 L60 55 L110 55 L110 65 L60 65 L10 65 Z";
    let file = load(&strip(d, &format!("{}, {}", skin("upper", "1 0.5 0 0 0.5 1"), skin("lower", "0 0.5 1 1 0.5 0"))));
    let bounds = |inst: &Instance| inst.bounds(object(inst.file(), "strip")).unwrap().map(|v| (v * 10.0).round() / 10.0);

    // In the pose the document describes, the path is as it was drawn.
    let mut inst = play(&file);
    assert_eq!(bounds(&inst), [10.0, 55.0, 110.0, 65.0]);
    assert!(pixels(&mut inst)(100, 60)[3] > 0);
    // The lower bone turns down: its end of the strip goes with it, the far end stays, and
    // the middle, held by both, lands between where each bone would take it.
    inst.advance(1.0);
    assert_eq!(bounds(&inst), [10.0, 55.0, 65.0, 110.0]);
    let at = pixels(&mut inst);
    assert!(at(60, 100)[3] > 0 && at(30, 60)[3] > 0 && at(100, 60)[3] == 0, "drawn where the bones are now");
    // Half-way through the turn the strip is half-way too (it is not drawn from a stale outline).
    let mut half = play(&file);
    half.advance(0.5);
    let b = bounds(&half);
    assert!(b[2] > 70.0 && b[2] < 110.0 && b[3] > 70.0 && b[3] < 110.0, "{b:?}");

    // One weight per point, and a curve has three points.
    let compiled = |d: &str, skins: &str| json::compile(&format!(r#"{{ "zuri": 1, "children": [ {} ] }}"#, strip(d, skins)), &|_| unreachable!());
    let refused = |d: &str, skins: &str| File::from_objects(compiled(d, skins).unwrap()).err().map(|e| e.0).unwrap_or_default();
    assert!(refused(d, &skin("upper", "1 0")).contains("which has 6"));
    assert!(refused(d, &skin("upper", "1 1 1 1 1 x")).contains("one number from 0 to 1 per point"));
    // Without weights a bone holds every point alike: the whole strip turns with `lower`, about (60, 60).
    let mut rigid = play(&File::from_objects(compiled(d, r#"{ "type": "Skin", "target": "lower" }"#).unwrap()).unwrap());
    rigid.advance(1.0);
    assert_eq!(bounds(&rigid), [55.0, 10.0, 65.0, 110.0]);
    assert_eq!(refused("M0 0 C1 1 2 2 3 3 Z", &skin("upper", "1 1 1 1")), "");
    assert!(refused("M0 0 C1 1 2 2 3 3 Z", &skin("upper", "1 1")).contains("which has 4"));
    // Weights are from 0 to 1, and are checked before the Skin has a target.
    assert!(refused(d, &skin("upper", "1 1 1 1 1 -1")).contains("from 0 to 1"));
    assert!(refused(d, r#"{ "type": "Skin", "weights": "1 2 x" }"#).contains("from 0 to 1"));
    assert_eq!(refused(d, r#"{ "type": "Skin", "weights": "1 1 1 1 1 1" }"#), "", "not bound yet");

    let both = format!("{}, {}", skin("upper", "1 0.5 0 0 0.5 1"), skin("lower", "0 0.5 1 1 0.5 0"));
    // The scene with one piece of its source swapped for another.
    let scene = |skins: &str, from: &str, to: &str| load(&strip(d, skins).replacen(from, to, 1));
    // A Constraint that follows the strip's outline reads it as the bones have it now,
    // whether or not a frame was drawn in between.
    let rider = r#"{ "type": "Node", "name": "rider", "children": [ { "type": "Constraint", "constrain": "followPath", "target": "strip", "position": 0.3 } ] }, { "type": "Animation""#;
    let followed = scene(&both, r#"{ "type": "Animation""#, rider);
    let (mut drawn, mut fresh) = (play(&followed), play(&followed));
    for _ in 0..4 {
        let _ = pixels(&mut drawn); // a frame is drawn
        drawn.advance(0.25);
    }
    fresh.advance(1.0);
    let rider_at = |inst: &Instance| inst.world_matrix(object(inst.file(), "rider")).map(|m| (m.tx.round(), m.ty.round()));
    assert_eq!(rider_at(&drawn), rider_at(&fresh));
    assert_ne!(rider_at(&fresh), rider_at(&play(&followed)), "it moved with the bend");
    // A point that a keyed `d` adds is held too, when the bone holds every point alike.
    let longer = r#"{ "type": "Track", "target": "outline", "property": "d", "children": [ { "type": "Key", "frame": 0, "textValue": "M10 55 L60 55 L110 55 L110 65 L60 65 L10 65 L10 60 Z" } ] }, { "type": "Track", "target": "lower""#;
    let mut keyed = play(&scene(r#"{ "type": "Skin", "target": "lower" }"#, r#"{ "type": "Track", "target": "lower""#, longer));
    keyed.advance(1.0);
    assert_eq!(bounds(&keyed), [55.0, 10.0, 65.0, 110.0]);
    // A bone with no size in the document's pose cannot hold anything: the strip stays as drawn.
    let flat = play(&scene(&both, r#""name": "upper","#, r#""name": "upper", "scaleX": 0,"#));
    assert_eq!(bounds(&flat), [10.0, 55.0, 110.0, 65.0]);

    // What holds a path is a Bone.
    assert!(compiled(d, &skin("strip", "1 1 1 1 1 1")).unwrap_err().0.contains("Bone"));
}

#[test]
fn elastic_keys_swing_past_the_next_key_and_land_on_it() {
    // `dot` goes from 0 to 100 in a second; `key` is what the first Key says about how.
    let at = |key: &str, seconds: f32| {
        let mut inst = play(&load(&format!(
            r##"{{ "type": "Artboard", "width": 200, "height": 20, "children": [
              {{ "type": "Shape", "name": "dot" }},
              {{ "type": "Animation", "name": "move", "fps": 60, "duration": 60, "children": [
                {{ "type": "Track", "target": "dot", "property": "x", "children": [
                  {{ "type": "Key", "frame": 0, "value": 0, {key} }}, {{ "type": "Key", "frame": 60, "value": 100 }} ] }} ] }} ] }}"##
        )));
        inst.advance(seconds);
        (x_of(&inst, "dot") * 10.0).round() / 10.0
    };
    // As it comes: one swing takes half the way (`period` 0.5), so a quarter of the way in
    // it is furthest past the end, by 0.75^6 of the distance; then under it, and there at the end.
    let elastic = r#""interp": "elastic""#;
    assert_eq!((at(elastic, 0.25), at(elastic, 1.0)), (117.8, 100.0));
    assert!(at(elastic, 0.5) < 100.0);
    // `amplitude` 2 and `period` 0.3, swinging on leaving: three quarters of the way it is
    // 2 x 0.75^6 short of where arriving would be a quarter of the way in.
    assert_eq!(at(r#""interp": "elasticIn", "amplitude": 2, "period": 0.3"#, 0.75), 35.6);
    // Both ways is half the distance each: the middle is the middle, and a swing still
    // takes `period` of the whole way, so a quarter in it is 0.5^7 of the distance under the start.
    let both = r#""interp": "elasticInOut""#;
    assert_eq!((at(both, 0.5), at(both, 0.25)), (50.0, -0.8));
    // A period under 0.05 is taken as 0.05 (twenty swings are plenty): half a swing in,
    // it is 0.975^6 of the distance past the end.
    assert_eq!((at(r#""interp": "elastic", "period": 0"#, 0.025), at(r#""interp": "elastic", "period": 0.05"#, 0.025)), (185.9, 185.9));

    // A Transition's cross-fade has nothing to swing past: the compiler says which it takes.
    let fade = r#"{ "zuri": 1, "children": [ { "type": "Artboard", "children": [ { "type": "StateMachine", "children": [ { "type": "Layer", "children": [
      { "type": "State", "name": "a", "children": [ { "type": "Transition", "to": "a", "interp": "elastic" } ] } ] } ] } ] } ] }"#;
    let error = json::compile(fade, &|_| unreachable!()).expect_err("refused").to_string();
    assert!(error.contains("expected one of: linear, hold, cubic, got"), "{error}");
}

#[test]
fn joysticks_scrub_an_animation_on_each_axis() {
    // `dot` goes from x 10 to 110 in `across` and from y 20 to 80 in `down` (60 frames
    // each). The joystick's handleX scrubs the first and its handleY the second; `h` in
    // the view model is bound to handleX, `seen` reads `dot`'s x back, and `sweep` keys
    // handleY from -1 to 1 in a second. `first` goes before the other animations.
    let scene = |stick: &str, first: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 100, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [
                {{ "type": "Property", "name": "h", "valueType": "number" }}, {{ "type": "Property", "name": "seen", "valueType": "number" }} ] }},
              {{ "type": "Shape", "name": "dot", "x": 1, "y": 2, "children": [
                {{ "type": "Ellipse", "width": 4, "height": 4 }}, {{ "type": "Bind", "source": "seen", "property": "x", "direction": "toSource" }} ] }},
              {first}
              {{ "type": "Animation", "name": "across", "fps": 60, "duration": 60, "children": [
                {{ "type": "Track", "target": "dot", "property": "x", "children": [ {{ "type": "Key", "frame": 0, "value": 10 }}, {{ "type": "Key", "frame": 60, "value": 110 }} ] }} ] }},
              {{ "type": "Animation", "name": "down", "duration": 60, "children": [
                {{ "type": "Track", "target": "dot", "property": "y", "children": [ {{ "type": "Key", "frame": 0, "value": 20 }}, {{ "type": "Key", "frame": 60, "value": 80 }} ] }} ] }},
              {{ "type": "Joystick", "name": "stick", {stick} "children": [ {{ "type": "Bind", "source": "h", "property": "handleX" }} ] }} ] }}"##
        ))
    };
    let sweep = r#"{ "type": "Animation", "name": "sweep", "fps": 60, "duration": 60, "children": [
      { "type": "Track", "target": "stick", "property": "handleY", "children": [ { "type": "Key", "frame": 0, "value": -1 }, { "type": "Key", "frame": 60, "value": 1 } ] } ] },"#;
    let at = |inst: &Instance| {
        let m = inst.world_matrix(object(inst.file(), "dot")).unwrap();
        (m.tx.round(), m.ty.round())
    };
    let both = r#""xAnimation": "across", "yAnimation": "down","#;

    // Steered through the view model. The animations a joystick scrubs are poses, so
    // nothing plays of itself: a handle at rest (0, 0) is the middle of both.
    let mut inst = play(&scene(both, ""));
    assert_eq!(at(&inst), (60., 50.));
    // 1 is the animation's end, -1 its start, and beyond is held there. A Property bound
    // to what the joystick poses has the posed value in the same frame.
    for (h, x) in [(1.0, 110.), (-1.0, 10.), (0.5, 85.), (7.0, 110.)] {
        inst.set_number("h", h);
        inst.advance(0.0);
        assert_eq!((at(&inst), inst.number("seen")), ((x, 50.), Some(x)), "h = {h}");
    }
    assert!(!inst.advance(0.1), "a joystick that is not moved leaves the scene settled");
    // One of its animations played on purpose (a timeline being edited, say) is left to
    // play; the other axis goes on posing.
    // (The handle is at the animation's start, where the joystick would hold `dot` at 10.)
    inst.set_number("h", -1.0);
    inst.play_animation(0);
    inst.advance(0.5);
    assert_eq!(at(&inst), (60., 50.));
    inst.advance(0.5);
    assert_eq!(at(&inst), (110., 50.));

    // Keyed: the animation that plays moves the handle, and the joystick poses after it.
    let mut inst = play(&scene(both, sweep));
    assert_eq!(at(&inst), (60., 20.));
    inst.advance(0.5);
    assert_eq!(at(&inst), (60., 50.));
    inst.advance(0.5);
    assert_eq!(at(&inst), (60., 80.));

    // One axis is enough: the other leaves the scene alone (y stays where the document
    // put it; nothing is played here, or `down`, which no joystick scrubs now, would be).
    let mut inst = Instance::new(&scene(r#""xAnimation": "across","#, ""), 0).unwrap();
    inst.set_number("h", -1.0);
    inst.advance(0.0);
    assert_eq!(at(&inst), (10., 2.));

    // A handle that overflows (3e38 x 3e38) into an animation without length asks for a
    // frame that is not a number: the first key stands in.
    let mut inst = play(&load(
        r##"{ "type": "Artboard", "width": 10, "height": 10, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "h", "valueType": "number", "value": 3e38 } ] },
          { "type": "Shape", "name": "dot" },
          { "type": "Animation", "name": "still", "duration": 0, "children": [
            { "type": "Track", "target": "dot", "property": "x", "children": [ { "type": "Key", "frame": 0, "value": 3 }, { "type": "Key", "frame": 10, "value": 9 } ] } ] },
          { "type": "Joystick", "xAnimation": "still", "children": [ { "type": "Bind", "source": "h", "property": "handleX", "mul": 3e38 } ] } ] }"##,
    ));
    inst.advance(0.0);
    assert_eq!(at(&inst).0, 3.);

    // An animation that a joystick scrubs cannot move a joystick's handle: it would chase itself.
    let chasing = format!(
        r##"{{ "zuri": 1, "children": [ {{ "type": "Artboard", "children": [ {sweep}
          {{ "type": "Joystick", "name": "stick", "xAnimation": "sweep" }} ] }} ] }}"##
    );
    let objects = json::compile(&chasing, &|_| unreachable!()).unwrap();
    let error = File::load(&format::write(&objects, 0).unwrap()).err().expect("refused");
    assert!(error.to_string().contains("cannot move a Joystick's handle"), "{error}");
}

/// The bounds of the named objects, to the nearest unit.
fn boxes(inst: &Instance, names: &[&str]) -> Vec<[f32; 4]> {
    names.iter().map(|name| inst.bounds(object(inst.file(), name)).unwrap().map(f32::round)).collect()
}

#[test]
fn layouts_arrange_what_is_inside_them() {
    // A Layout at (10, 10) holding `a` (20 x 10), `mid` (a Layout of its own) and `b`
    // (30 x 40). `outer` and `mid` are the properties of the two Layouts.
    let scene = |outer: &str, mid: &str, a: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 240, "height": 120, "children": [
              {{ "type": "Layout", "name": "box", "x": 10, "y": 10, "gap": 10, "padding": 5, {outer} "children": [
                {{ "type": "Fill", "color": "#ff0000" }},
                {{ "type": "Shape", "name": "a", {a} "children": [ {{ "type": "Rectangle", "width": 20, "height": 10 }}, {{ "type": "Fill", "color": "#00ff00" }} ] }},
                {{ "type": "Layout", "name": "mid", {mid} "children": [ {{ "type": "Fill", "color": "#0000ff" }} ] }},
                {{ "type": "Node", "name": "group", "children": [
                  {{ "type": "Shape", "name": "b", "x": 100, "children": [ {{ "type": "Rectangle", "width": 30, "height": 40 }} ] }} ] }} ] }},
              {{ "type": "Animation", "name": "widen", "children": [ {{ "type": "Track", "target": "box", "property": "gap", "children": [
                {{ "type": "Key", "frame": 0, "value": 10 }}, {{ "type": "Key", "frame": 60, "value": 30 }} ] }} ] }} ] }}"##
        ))
    };
    let at = |outer: &str, mid: &str, names: &[&str]| boxes(&play(&scene(outer, mid, "")), names);

    // A row: side by side, a gap apart, inside the padding. Without a size of its own
    // the Layout is as large as what it holds (the empty Layout takes no room, but its
    // gaps), and a group counts for what is in it, wherever that sits in the group.
    assert_eq!(at("", "", &["box", "a", "b"]), [[10., 10., 90., 60.], [15., 15., 35., 25.], [55., 15., 85., 55.]]);
    // A column: one under the other.
    assert_eq!(at(r#""flow": "column","#, "", &["box", "a", "b"]), [[10., 10., 50., 90.], [15., 15., 35., 25.], [15., 45., 45., 85.]]);
    // Across the flow: in the middle, at the end.
    assert_eq!(at(r#""alignItems": "center","#, "", &["a"]), [[15., 30., 35., 40.]]);
    assert_eq!(at(r#""alignItems": "end","#, "", &["a"]), [[15., 45., 35., 55.]]);

    // With a width of its own there is room to spare (220 - 10 - 70 = 140): `justify` says where it goes.
    let wide = |more: &str| at(&format!(r#""width": 220, {more}"#), "", &["a", "b"]);
    assert_eq!(wide(""), [[15., 15., 35., 25.], [55., 15., 85., 55.]]);
    assert_eq!(wide(r#""justify": "center","#), [[85., 15., 105., 25.], [125., 15., 155., 55.]]);
    assert_eq!(wide(r#""justify": "end","#), [[155., 15., 175., 25.], [195., 15., 225., 55.]]);
    assert_eq!(wide(r#""justify": "spaceBetween","#), [[15., 15., 35., 25.], [195., 15., 225., 55.]]);
    // A Layout that grows takes the room to spare; `stretch` makes it as tall as the row.
    let grown = at(r#""width": 220, "alignItems": "stretch","#, r#""grow": 1, "height": 6,"#, &["mid", "b"]);
    assert_eq!(grown, [[45., 15., 185., 55.], [195., 15., 225., 55.]]);
    assert_eq!(at(r#""width": 220,"#, r#""grow": 1, "height": 6,"#, &["mid"]), [[45., 15., 185., 21.]], "its own height without `stretch`");

    // A child's x / y move it from its place; the others stay.
    let nudged = play(&scene("", "", r#""x": 3, "y": -2,"#));
    assert_eq!(boxes(&nudged, &["a", "b"]), [[18., 13., 38., 23.], [55., 15., 85., 55.]]);

    // The Layout paints its box, the pointer finds it, and what is inside is on top.
    let mut inst = play(&scene("", "", ""));
    let px = pixels(&mut inst);
    assert_eq!((px(12, 12), px(25, 20), px(5, 5)), (RED, [0, 255, 0, 255], CLEAR));
    let named = |inst: &Instance, x: f32, y: f32| inst.hit_test(x, y).and_then(|obj| inst.file().objects[obj as usize].str(p::NAME).map(String::from));
    assert_eq!((named(&inst, 12.0, 12.0), named(&inst, 25.0, 20.0), named(&inst, 5.0, 5.0)), (Some("box".into()), Some("a".into()), None));

    // Its properties animate, and what it holds moves with them.
    inst.advance(1.0);
    assert_eq!(boxes(&inst, &["box", "b"]), [[10., 10., 130., 60.], [95., 15., 125., 55.]]);
    assert!(!inst.advance(0.1), "a laid-out scene settles");

    // A Layout inside that is scaled or turned takes the room it shows at, and what
    // comes after it starts where it ends. Scaled, it grows by what is left in the
    // parent's measure; turned, it keeps its size.
    assert_eq!(at("", r#""width": 20, "height": 6, "scaleX": 2,"#, &["mid", "b"]), [[45., 15., 85., 21.], [95., 15., 125., 55.]]);
    assert_eq!(at("", r#""width": 20, "height": 6, "rotation": 90,"#, &["mid", "b"]), [[45., 15., 51., 35.], [61., 15., 91., 55.]]);
    assert_eq!(at(r#""width": 220,"#, r#""grow": 1, "height": 6, "scaleX": 2,"#, &["mid", "b"]), [[45., 15., 185., 21.], [195., 15., 225., 55.]]);
    assert_eq!(at(r#""width": 220,"#, r#""grow": 1, "width": 20, "height": 6, "rotation": 90,"#, &["mid"]), [[45., 15., 51., 35.]]);
    // A size cannot be less than nothing, whatever padding is asked for.
    assert_eq!(at("", r#""padding": -10,"#, &["a", "b"]), [[15., 15., 35., 25.], [55., 15., 85., 55.]]);

    // A least and a most. The empty Layout is 20 by 6 at least, which pushes `b` along
    // and widens the box; one with a width of 50 is 20 at most; where they cross, the most wins.
    assert_eq!(at("", r#""minWidth": 20, "minHeight": 6,"#, &["box", "mid", "b"]), [[10., 10., 110., 60.], [45., 15., 65., 21.], [75., 15., 105., 55.]]);
    assert_eq!(at("", r#""width": 50, "height": 6, "maxWidth": 20,"#, &["mid"]), [[45., 15., 65., 21.]]);
    assert_eq!(at("", r#""height": 6, "minWidth": 80, "maxWidth": 40,"#, &["mid"]), [[45., 15., 85., 21.]]);
    // A most of 0 is 0, not "no limit": a Layout keyed down to it folds away.
    assert_eq!(at("", r#""width": 20, "height": 6, "maxHeight": 0,"#, &["mid"]), [[45., 15., 65., 15.]]);
    // What `grow` and `stretch` give is held to them too: 50 of the 140 to spare, 10 of the 40 across.
    assert_eq!(at(r#""width": 220,"#, r#""grow": 1, "height": 6, "maxWidth": 50,"#, &["mid", "b"]), [[45., 15., 95., 21.], [105., 15., 135., 55.]]);
    assert_eq!(at(r#""width": 220, "alignItems": "stretch","#, r#""grow": 1, "height": 6, "maxHeight": 10,"#, &["mid"]), [[45., 15., 185., 25.]]);
    // A Layout that fits its content is larger where its least says: 100 wide has 20 to
    // spare for `justify`, and 80 tall has 60 across for `alignItems`.
    assert_eq!(at(r#""minWidth": 100, "justify": "end","#, "", &["a"]), [[35., 15., 55., 25.]]);
    assert_eq!(at(r#""minHeight": 80, "alignItems": "end","#, "", &["a"]), [[15., 75., 35., 85.]]);
}

#[test]
fn layouts_wrap_onto_lines_and_pad_each_side() {
    // A Layout at (10, 10) with a gap of 10 and a padding of 5, holding four Shapes 30
    // wide: `a` (10 tall), `b` (20), `c` and `d` (10). `more` goes between `a` and `b`.
    let shape = |name: &str, height: u32| {
        format!(
            r##"{{ "type": "Shape", "name": "{name}", "children": [ {{ "type": "Rectangle", "width": 30, "height": {height} }}, {{ "type": "Fill", "color": "#00ff00" }} ] }}"##
        )
    };
    let (a, rest) = (shape("a", 10), format!("{}, {}, {}", shape("b", 20), shape("c", 10), shape("d", 10)));
    let scene = |outer: &str, more: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 240, "height": 120, "children": [
              {{ "type": "Layout", "name": "box", "x": 10, "y": 10, "gap": 10, "padding": 5, {outer} "children": [ {a}, {more} {rest} ] }} ] }}"##
        ))
    };
    // The same Layout, wrapping, as the first child of a `card` at (10, 10) with a padding of 5; `after` (30 x 10) comes after it.
    let inside = |card: &str, outer: &str, names: &[&str]| {
        let file = load(&format!(
            r##"{{ "type": "Artboard", "width": 240, "height": 120, "children": [
              {{ "type": "Layout", "name": "card", "x": 10, "y": 10, "padding": 5, {card} "children": [
                {{ "type": "Layout", "name": "box", "gap": 10, "padding": 5, "wrap": true, {outer} "children": [ {a}, {rest} ] }},
                {} ] }} ] }}"##,
            shape("after", 10)
        ));
        boxes(&play(&file), names)
    };
    let at = |outer: &str, names: &[&str]| boxes(&play(&scene(outer, "")), names);
    let all = ["box", "a", "b", "c", "d"];

    // One line, however little room there is: what does not fit sticks out (the bounds of the box include it).
    assert_eq!(
        at(r#""width": 100,"#, &all),
        [[10., 10., 165., 40.], [15., 15., 45., 25.], [55., 15., 85., 35.], [95., 15., 125., 25.], [135., 15., 165., 25.]]
    );
    // Wrapping, 90 of room: `a` and `b` fit, `c` starts a line under the taller of them, a gap below.
    let wrapped = [[10., 10., 110., 60.], [15., 15., 45., 25.], [55., 15., 85., 35.], [15., 45., 45., 55.], [55., 45., 85., 55.]];
    assert_eq!(at(r#""width": 100, "wrap": true,"#, &all), wrapped);
    // A child that just fits stays on the line (4 x 30 + 3 x 10 + 2 x 5 = 160), and one more unit of gap breaks it.
    assert_eq!(at(r#""width": 160, "wrap": true,"#, &["box", "d"]), [[10., 10., 170., 40.], [135., 15., 165., 25.]]);
    assert_eq!(at(r#""width": 160, "wrap": true, "gap": 11,"#, &["d"]), [[15., 46., 45., 56.]]);
    // Without a width of its own it is as wide as its children, on one line.
    assert_eq!(at(r#""wrap": true,"#, &["box", "d"]), [[10., 10., 170., 40.], [135., 15., 165., 25.]]);
    // With a most it is no wider than that, and its lines break there as at a width of its
    // own; so do those of one whose own width is over its most.
    assert_eq!(at(r#""maxWidth": 100, "wrap": true,"#, &all), wrapped);
    assert_eq!(at(r#""width": 200, "maxWidth": 100, "wrap": true,"#, &all), wrapped);
    // `justify` and `alignItems` work line by line: each line has 20 to spare, and the first is 20 tall.
    assert_eq!(at(r#""width": 100, "wrap": true, "justify": "end", "alignItems": "center","#, &["a", "c"]), [[35., 20., 65., 30.], [35., 45., 65., 55.]]);
    // A column wraps to the right: 50 of room holds `a` and `b` (10 + 10 + 20). Its most
    // is as tall as it gets, so that does the same.
    let columns = [[10., 10., 90., 70.], [15., 15., 45., 25.], [15., 35., 45., 55.], [55., 15., 85., 25.], [55., 35., 85., 45.]];
    assert_eq!(at(r#""flow": "column", "height": 60, "wrap": true,"#, &all), columns);
    assert_eq!(at(r#""flow": "column", "maxHeight": 60, "wrap": true,"#, &all), columns);
    // A Layout that grows takes what its own line has to spare (90 - 60 - 2 gaps), and `stretch` makes it as tall as that line.
    let mid = |outer: &str| boxes(&play(&scene(outer, r#"{ "type": "Layout", "name": "mid", "grow": 1, "height": 6 },"#)), &["mid", "b", "c"]);
    assert_eq!(mid(r#""width": 100, "wrap": true,"#), [[55., 15., 65., 21.], [75., 15., 105., 35.], [15., 45., 45., 55.]]);
    assert_eq!(mid(r#""width": 100, "wrap": true, "alignItems": "stretch","#)[0], [55., 15., 65., 35.]);

    // With one line or several, a line is as tall as its tallest child and the lines start at the top:
    // nothing jumps when `d` goes to a second line. `a` ends where `b` does, 20 under the padding.
    let low = |width: u32| at(&format!(r#""width": {width}, "height": 100, "wrap": true, "alignItems": "end","#), &["a"]);
    assert_eq!((low(160), low(159)), (vec![[15., 25., 45., 35.]], vec![[15., 25., 45., 35.]]));
    // A line that holds something is full when the next child does not fit, also when what it holds takes
    // no room: 15 of room has the empty Layout, and `wide` goes on a line of its own, a gap (-10) from it.
    let pulled = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "Layout", "x": 10, "y": 20, "width": 15, "wrap": true, "gap": -10, "children": [
            { "type": "Layout" },
            { "type": "Shape", "name": "wide", "children": [ { "type": "Rectangle", "width": 30, "height": 10 }, { "type": "Fill", "color": "#00ff00" } ] } ] } ] }"##,
    );
    assert_eq!(boxes(&play(&pulled), &["wide"]), [[10., 10., 40., 20.]]);

    // The lines break at the size a parent gives. A column 100 wide stretches the row to 90, which leaves
    // 80 of room: two lines. The column is as tall as the row then is, and `after` goes under it.
    assert_eq!(
        inside(r#""width": 100, "flow": "column", "alignItems": "stretch","#, "", &["card", "box", "a", "c", "d", "after"]),
        [[10., 10., 110., 80.], [15., 15., 105., 65.], [20., 20., 50., 30.], [20., 50., 50., 60.], [60., 50., 90., 60.], [15., 65., 45., 75.]]
    );
    // A row 200 wide lets it grow from 40 to 160 (190 less `after`), where all four fit on one line again.
    assert_eq!(
        inside(r#""width": 200,"#, r#""width": 40, "grow": 1,"#, &["card", "box", "d", "after"]),
        [[10., 10., 210., 50.], [15., 15., 175., 45.], [140., 20., 170., 30.], [175., 15., 205., 25.]]
    );

    // A side's padding is added to `padding` (5 here): 20 on the left and none at the bottom, none at the top and 30 on the right.
    assert_eq!(at(r#""paddingLeft": 15, "paddingBottom": -5,"#, &["box", "a"]), [[10., 10., 185., 35.], [30., 15., 60., 25.]]);
    assert_eq!(at(r#""paddingTop": -5, "paddingRight": 25,"#, &["box", "a"]), [[10., 10., 195., 35.], [15., 10., 45., 20.]]);
    // The room to spare is what the two sides leave: 200 - 20 - 5 - 150 along, 40 - 5 - 0 across.
    assert_eq!(
        at(r#""width": 200, "height": 40, "paddingLeft": 15, "paddingBottom": -5, "justify": "end", "alignItems": "end","#, &["a"]),
        [[55., 40., 85., 50.]]
    );

    // A Clip that names the Layout cuts what sticks out of its box: `c` ends at 110.
    let cut = |more: &str| {
        let mut inst = play(&scene(r#""width": 100,"#, more));
        let px = pixels(&mut inst);
        (px(100, 20), px(118, 20))
    };
    assert_eq!(cut(""), ([0, 255, 0, 255], [0, 255, 0, 255]));
    assert_eq!(cut(r#"{ "type": "Clip", "source": "box" },"#), ([0, 255, 0, 255], CLEAR));
}

#[test]
fn layouts_scroll_what_does_not_fit() {
    // A column at (10, 10), 60 by 50 with a padding of 5 and a gap of 5, holding four
    // rows 50 by 20: 95 of content in 40 of room, so 55 to scroll. `pos` in the view
    // model is bound both ways to how far it has gone; a press on `d`, and one anywhere
    // on the list, are reported.
    let scene = |list: &str| {
        let row = |name: &str| {
            format!(
                r##"{{ "type": "Shape", "name": "{name}", "children": [ {{ "type": "Rectangle", "width": 50, "height": 20 }}, {{ "type": "Fill", "color": "#00ff00" }} ] }}"##
            )
        };
        load(&format!(
            r##"{{ "type": "Artboard", "width": 80, "height": 120, "children": [
              {{ "type": "ViewModel", "name": "vm", "children": [ {{ "type": "Property", "name": "pos", "valueType": "number" }} ] }},
              {{ "type": "Event", "name": "touched" }}, {{ "type": "Event", "name": "inside" }},
              {{ "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "padding": 5, "gap": 5, {list} "children": [
                {{ "type": "Bind", "source": "pos", "property": "scrolled", "direction": "twoWay" }}, {}, {}, {}, {} ] }},
              {{ "type": "StateMachine", "name": "m", "children": [
                {{ "type": "Layer", "children": [ {{ "type": "State", "name": "idle" }} ] }},
                {{ "type": "Listener", "target": "d", "event": "down", "emit": "touched" }},
                {{ "type": "Listener", "target": "list", "event": "down", "emit": "inside" }} ] }} ] }}"##,
            row("a"),
            row("b"),
            row("c"),
            row("d")
        ))
    };
    let column = r#""flow": "column", "height": 50, "scroll": true,"#;
    let scrolled = |by: f32| {
        let mut inst = play(&scene(column));
        inst.set_number("pos", by);
        inst.advance(0.0);
        boxes(&inst, &["a", "d"])
    };
    // Not scrolled, the rows start under the padding and run out of the box; scrolled,
    // they move back by that much, no further than what does not fit, and not the other way.
    assert_eq!(scrolled(0.0), [[15., 15., 65., 35.], [15., 90., 65., 110.]]);
    assert_eq!(scrolled(30.0), [[15., -15., 65., 5.], [15., 60., 65., 80.]]);
    assert_eq!(scrolled(999.0), [[15., -40., 65., -20.], [15., 35., 65., 55.]]);
    assert_eq!(scrolled(-10.0), scrolled(0.0));
    // The property itself is kept to that range, so a host that adds to it (a wheel) never
    // has to unwind it: what it reads back is where the content is.
    let mut inst = play(&scene(column));
    for (set, held) in [(999.0, 55.0), (-10.0, 0.0), (30.0, 30.0)] {
        inst.set_number("pos", set);
        inst.advance(0.0);
        inst.advance(0.0);
        assert_eq!(inst.number("pos"), Some(held), "set to {set}");
    }

    // What is outside the box is not drawn: `c` (65 to 85) is under it. Without `scroll` it is.
    let below = |list: &str| pixels(&mut play(&scene(list)))(30, 70);
    assert_eq!((below(column), below(r#""flow": "column", "height": 50,"#)), (CLEAR, [0, 255, 0, 255]));

    // A drag: the pointer takes hold inside the box and, 8 pixels on, the rows go with it
    // (20 up moves them 12), then as far as they go and no further; let go, the pointer
    // moves alone.
    let mut inst = play(&scene(column));
    drop(pixels(&mut inst)); // drawn once: the pointer is placed by the last view
    let drag = |inst: &mut Instance, kind: Pointer, y: f32| {
        inst.pointer(kind, 30.0, y);
        inst.advance(0.0);
        (boxes(inst, &["a"])[0][1], inst.number("pos").unwrap())
    };
    assert_eq!(drag(&mut inst, Pointer::Down, 30.0), (15.0, 0.0));
    assert_eq!(drag(&mut inst, Pointer::Move, 10.0), (3.0, 12.0));
    assert_eq!(drag(&mut inst, Pointer::Move, -200.0), (-40.0, 55.0));
    assert_eq!(drag(&mut inst, Pointer::Up, -200.0), (-40.0, 55.0));
    assert_eq!(drag(&mut inst, Pointer::Move, 30.0), (-40.0, 55.0));
    // `d` is in the box now, at 35 to 55, and can be pressed; where it would be without
    // the scrolling (90 to 110, outside the box) nothing was there to press.
    inst.take_events();
    inst.pointer(Pointer::Down, 30.0, 45.0);
    assert_eq!(inst.take_events(), ["touched", "inside"]);
    let mut inst = play(&scene(column));
    drop(pixels(&mut inst)); // drawn once: the pointer is placed by the last view
    inst.pointer(Pointer::Down, 30.0, 100.0);
    assert!(inst.take_events().is_empty(), "a row scrolled out of the box is not pressed, and nor is the list through it");

    // A Layout that fits its content has nowhere to go: a drag leaves it.
    let mut inst = play(&scene(r#""flow": "column", "scroll": true,"#));
    drop(pixels(&mut inst)); // drawn once: the pointer is placed by the last view
    drag(&mut inst, Pointer::Down, 30.0);
    assert_eq!(drag(&mut inst, Pointer::Move, 10.0), (15.0, 0.0));
    // One that wraps scrolls its lines: a row 60 wide holds one of these per line, in 20 of
    // room across, and they go up.
    let mut inst = play(&scene(r#""height": 30, "wrap": true, "scroll": true,"#));
    inst.set_number("pos", 20.0);
    inst.advance(0.0);
    assert_eq!(boxes(&inst, &["a", "b"]), [[15., -5., 65., 15.], [15., 20., 65., 40.]]);
    // As far as they go: 95 of lines in 20 of room leaves 75.
    inst.set_number("pos", 999.0);
    inst.advance(0.0);
    assert_eq!(boxes(&inst, &["a", "d"]), [[15., -60., 65., -40.], [15., 15., 65., 35.]]);
}

#[test]
fn the_scene_has_one_pointer_and_one_button() {
    // A square at (20, 20) to (60, 60) that reports a click on it, the pointer coming
    // onto it and leaving it; the artboard reports every release.
    let file = load(
        r##"{ "type": "Artboard", "width": 100, "height": 100, "children": [
          { "type": "Event", "name": "clicked" }, { "type": "Event", "name": "released" },
          { "type": "Event", "name": "entered" }, { "type": "Event", "name": "left" },
          { "type": "Shape", "name": "a", "x": 40, "y": 40, "children": [ { "type": "Rectangle", "width": 40, "height": 40 }, { "type": "Fill", "color": "#00ff00" } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "a", "event": "click", "emit": "clicked" },
            { "type": "Listener", "target": "a", "event": "enter", "emit": "entered" },
            { "type": "Listener", "target": "a", "event": "exit", "emit": "left" },
            { "type": "Listener", "event": "up", "emit": "released" } ] } ] }"##,
    );
    // What a host with several pointers sends: (pointer, button, kind, on the square or beside it).
    // Returns which of them were the scene's, and what the scene reported.
    use Pointer::{Down, Exit, Move, Up};
    let feed = |inst: &mut Instance, events: &[(u32, u32, Pointer, bool)]| {
        inst.take_events();
        let taken: Vec<bool> =
            events.iter().map(|(pointer, button, kind, on)| inst.pointer_of(*pointer, *button, *kind, if *on { 40.0 } else { 5.0 }, 40.0)).collect();
        inst.advance(0.0);
        (taken, inst.take_events())
    };
    let heard = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();

    // A second pointer that presses and lets go beside a press is not the scene's: the first one's click counts.
    let mut inst = play(&file);
    assert_eq!(
        feed(&mut inst, &[(1, 0, Down, true), (2, 0, Down, false), (2, 0, Up, false), (1, 0, Up, true)]),
        (vec![true, false, false, true], heard(&["entered", "clicked", "released"]))
    );
    // Nor is it when it stays on after the first has let go: it moves and lets go unheard.
    // Then it can press on its own.
    assert_eq!(
        feed(&mut inst, &[(1, 0, Down, true), (2, 0, Down, false), (1, 0, Up, true), (2, 0, Move, false), (2, 0, Up, false)]),
        (vec![true, false, true, false, false], heard(&["clicked", "released"]))
    );
    assert_eq!(feed(&mut inst, &[(2, 0, Down, true), (2, 0, Up, true)]), (vec![true, true], heard(&["clicked", "released"])));
    // One whose release beside a press the host never reported presses anew all the same.
    assert_eq!(
        feed(&mut inst, &[(1, 0, Down, true), (2, 0, Down, false), (1, 0, Up, true), (2, 0, Down, true), (2, 0, Up, true)]).0,
        [true, false, true, true, true]
    );
    // One that pressed and let go beside a press is the scene's again once the press is over: it leaves the square.
    assert_eq!(
        feed(&mut inst, &[(5, 0, Down, true), (1, 0, Down, false), (1, 0, Up, false), (5, 0, Up, true), (1, 0, Move, false)]),
        (vec![true, false, false, true, true], heard(&["clicked", "released", "left"]))
    );

    // Another button does not press, and its release is no release: not on its own, and
    // not in the middle of a press, which goes on until the primary button is let go.
    assert_eq!(feed(&mut inst, &[(1, 2, Down, true), (1, 2, Up, true)]), (vec![false, false], heard(&[])));
    assert_eq!(
        feed(&mut inst, &[(1, 0, Down, true), (1, 2, Down, true), (1, 2, Up, true), (1, 0, Up, true)]),
        (vec![true, false, false, true], heard(&["entered", "clicked", "released"]))
    );

    // The pointer that is pressing leaves: the press is over (the artboard hears its
    // release), and another pointer can press.
    assert_eq!(
        feed(&mut inst, &[(1, 0, Down, true), (1, 0, Exit, false), (2, 0, Down, true), (2, 0, Up, true)]),
        (vec![true, true, true, true], heard(&["left", "released", "entered", "clicked", "released"]))
    );
    // A host with one pointer has pointer 0, button 0.
    let mut inst = play(&file);
    inst.pointer(Down, 40.0, 40.0);
    assert_eq!(feed(&mut inst, &[(7, 0, Move, true), (0, 0, Up, true)]), (vec![false, true], heard(&["clicked", "released"])));
}

#[test]
fn a_drag_that_scrolls_is_not_a_tap() {
    // A column at (10, 10), 60 by 50 with a padding of 5 and a gap of 5, holding `a` and
    // `b` (50 by 20 each) and a list of two Rows as tall: 95 of content in 40 of room.
    // A click on `a` is reported, and so are every release and the pointer coming onto
    // `a`; `held` is whether `a` is pressed; a click on a Row toggles that Row's `on`.
    let file = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [
            { "type": "Property", "name": "pos", "valueType": "number" }, { "type": "Property", "name": "held", "valueType": "bool" } ] },
          { "type": "Event", "name": "clicked" }, { "type": "Event", "name": "released" }, { "type": "Event", "name": "entered" },
          { "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "height": 50, "padding": 5, "gap": 5, "flow": "column", "scroll": true, "children": [
            { "type": "Bind", "source": "pos", "property": "scrolled", "direction": "twoWay" },
            { "type": "Shape", "name": "a", "children": [ { "type": "Rectangle", "width": 50, "height": 20 }, { "type": "Fill", "color": "#00ff00" } ] },
            { "type": "Shape", "name": "b", "children": [ { "type": "Rectangle", "width": 50, "height": 20 }, { "type": "Fill", "color": "#00ff00" } ] },
            { "type": "NestedArtboard", "name": "rows", "artboard": "Row", "count": 2, "flow": "column", "gap": 5 } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "a", "event": "click", "emit": "clicked" },
            { "type": "Listener", "target": "a", "event": "enter", "emit": "entered" },
            { "type": "Listener", "target": "a", "event": "down", "input": "held", "action": "set", "value": 1 },
            { "type": "Listener", "target": "a", "event": "up", "input": "held", "action": "set", "value": 0 },
            { "type": "Listener", "event": "up", "emit": "released" } ] } ] },
        { "type": "Artboard", "name": "Row", "width": 50, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "on", "valueType": "bool" } ] },
          { "type": "Shape", "name": "bar", "x": 25, "y": 10, "children": [ { "type": "Rectangle", "width": 50, "height": 20 }, { "type": "Fill", "color": "#ff0000" } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "bar", "event": "click", "input": "on", "action": "toggle" } ] } ] }"##,
    );
    // A press at the first place that goes to each of the others and is let go at the
    // last, with a frame after each event as a host has (the content moves in a frame).
    // The places are in the artboard's units, and `zoom` surface pixels to one of those.
    fn drag(inst: &mut Instance, zoom: f32, path: &[(f32, f32)]) -> Vec<String> {
        inst.take_events();
        let kinds = [Pointer::Down].into_iter().chain(path[1..].iter().map(|_| Pointer::Move)).chain([Pointer::Up]);
        for (kind, (x, y)) in kinds.zip(path.iter().chain(path.last())) {
            inst.pointer(kind, x * zoom, y * zoom);
            inst.advance(0.0);
        }
        inst.take_events()
    }
    // The same down the list at 30 across, and how far the list has gone after.
    let press = |inst: &mut Instance, path: &[f32]| (drag(inst, 1.0, &path.iter().map(|y| (30.0, *y)).collect::<Vec<_>>()), inst.number("pos").unwrap());
    let heard = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    // A tap on `a` (15 to 35 down) clicks it. So does a press that slips less than 8
    // pixels of the surface, and the list stays where it is; and one that goes further
    // against the end of the list, which cannot go that way.
    let mut inst = play(&file);
    assert_eq!(press(&mut inst, &[25.0]), (heard(&["entered", "clicked", "released"]), 0.0));
    assert_eq!(press(&mut inst, &[25.0, 18.0]), (heard(&["clicked", "released"]), 0.0));
    assert_eq!(press(&mut inst, &[20.0, 32.0]), (heard(&["clicked", "released"]), 0.0));
    // Against the end the 8 pixels are not spent: a little way back is still no scroll.
    assert_eq!(press(&mut inst, &[20.0, 32.0, 25.0]), (heard(&["clicked", "released"]), 0.0));
    // One that goes further is a scroll: the list goes with it from those 8 pixels on
    // (12 gone, 4 scrolled), the press is let go of once (when it became a scroll) and
    // nothing is clicked; `a` hears that it is no longer held. Let go, the pointer is
    // on `a` again (at 11 to 31 now), which it enters.
    let mut inst = play(&file);
    assert_eq!(press(&mut inst, &[25.0, 23.0, 15.0, 13.0]), (heard(&["entered", "released", "entered"]), 4.0));
    assert_eq!(inst.number("held"), Some(0.0));
    // Once it goes with the pointer it stays with it, back to within the 8 pixels too:
    // from 20, 12 up scrolls 4 more, and 8 of those back down takes 8 off again.
    let mut back = play(&file);
    back.set_number("pos", 20.0);
    back.advance(0.0);
    assert_eq!(press(&mut back, &[40.0, 28.0, 36.0]).1, 16.0);
    // So it does when the pointer leaves the surface in the middle of a press on it.
    let mut left = play(&file);
    left.pointer(Pointer::Down, 30.0, 25.0);
    assert_eq!(left.number("held"), Some(1.0));
    left.pointer(Pointer::Exit, 0.0, 0.0);
    assert_eq!(left.number("held"), Some(0.0));
    // And that was the release: if the pointer comes back and is let go there, neither
    // `a` nor the artboard is told a second time.
    left.take_events();
    left.pointer(Pointer::Down, 30.0, 25.0);
    left.pointer(Pointer::Exit, 0.0, 0.0);
    assert_eq!(left.take_events(), ["entered", "released"]);
    left.pointer(Pointer::Move, 30.0, 25.0);
    left.pointer(Pointer::Up, 30.0, 25.0);
    assert_eq!(left.take_events(), ["entered"]);
    // And it leaves and enters it as before.
    inst.pointer(Pointer::Move, 30.0, 70.0);
    inst.pointer(Pointer::Move, 30.0, 20.0);
    assert_eq!(inst.take_events(), ["entered"]);
    // A press that begins before the last was let go (a release that never came) is a
    // new press: `a` is at 8 to 28 after a scroll of 7.
    let mut inst = play(&file);
    for (kind, y) in [(Pointer::Down, 25.0), (Pointer::Move, 10.0)] {
        inst.pointer(kind, 30.0, y);
        inst.advance(0.0);
    }
    assert_eq!(press(&mut inst, &[15.0]), (heard(&["entered", "clicked", "released"]), 7.0));
    // The same in what is nested: scrolled to the end, the Rows are at 10 to 30 and 35 to
    // 55. A tap toggles one; a drag that began on the other scrolls and leaves it.
    let mut inst = play(&file);
    inst.set_number("pos", 55.0);
    inst.advance(0.0);
    assert_eq!(press(&mut inst, &[20.0]).1, 55.0);
    assert_eq!(press(&mut inst, &[45.0, 55.0]).1, 53.0);
    assert_eq!((inst.number("rows/0/on"), inst.number("rows/1/on")), (Some(1.0), Some(0.0)));
    // The host says how many pixels: with 2, a press that goes 5 is a scroll of 3. What
    // is no number of pixels is not taken.
    let mut inst = play(&file);
    inst.set_drag_threshold(2.0);
    inst.set_drag_threshold(-1.0);
    inst.set_drag_threshold(f32::NAN);
    inst.set_drag_threshold(f32::INFINITY);
    assert_eq!(press(&mut inst, &[25.0, 20.0]), (heard(&["entered", "released", "entered"]), 3.0));
    inst.set_drag_threshold(0.0);
    assert_eq!(press(&mut inst, &[25.0, 24.0]).1, 4.0);
    // The 8 pixels are of the surface, however large the artboard is drawn. At a quarter
    // of its size a slip of 20 units is 5 pixels, a tap; at four times, 3 units are 12
    // pixels, a scroll (of the 4 pixels past the 8: one unit).
    for (zoom, slip, went) in [(0.25, 20.0, 0.0), (4.0, 3.0, 1.0)] {
        let mut inst = play(&file);
        let (w, h) = (80.0 * zoom, 120.0 * zoom);
        let view = inst.fit(Fit::Contain, 0.5, 0.5, w, h);
        inst.render(&mut Raster::default(), &mut vec![0; (w * h * 4.0) as usize], w as u32, h as u32, view);
        drag(&mut inst, zoom, &[(30.0, 45.0), (30.0, 45.0 - slip)]);
        assert_eq!(inst.number("pos"), Some(went), "{zoom}");
    }

    // A list of four Strips, each of which scrolls sideways (80 of content in 50) and
    // toggles its `on` at a click; the artboard reports a click anywhere on it.
    let strips = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "pos", "valueType": "number" } ] },
          { "type": "Event", "name": "clicked" },
          { "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "height": 50, "padding": 5, "flow": "column", "scroll": true, "children": [
            { "type": "Bind", "source": "pos", "property": "scrolled", "direction": "twoWay" },
            { "type": "NestedArtboard", "name": "rows", "artboard": "Strip", "count": 4, "flow": "column", "gap": 5 } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "event": "click", "emit": "clicked" } ] } ] },
        { "type": "Artboard", "name": "Strip", "width": 50, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [
            { "type": "Property", "name": "sx", "valueType": "number" }, { "type": "Property", "name": "on", "valueType": "bool" } ] },
          { "type": "Layout", "name": "strip", "width": 50, "height": 20, "scroll": true, "children": [
            { "type": "Bind", "source": "sx", "property": "scrolled", "direction": "twoWay" },
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 40, "height": 20 }, { "type": "Fill", "color": "#00ff00" } ] },
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 40, "height": 20 }, { "type": "Fill", "color": "#0000ff" } ] } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "strip", "event": "click", "input": "on", "action": "toggle" } ] } ] }"##,
    );
    let state = |inst: &Instance| [inst.number("rows/0/sx").unwrap(), inst.number("rows/0/on").unwrap(), inst.number("pos").unwrap()];
    // A tap on the first Strip (15 to 35 down) is a click there and on the artboard.
    let mut inst = play(&strips);
    assert_eq!((drag(&mut inst, 1.0, &[(40.0, 25.0)]), state(&inst)), (heard(&["clicked"]), [0.0, 1.0, 0.0]));
    // A press that moves a Strip is a scroll for the artboard around it too: no click anywhere.
    let mut inst = play(&strips);
    assert_eq!((drag(&mut inst, 1.0, &[(40.0, 25.0), (20.0, 25.0)]), state(&inst)), (heard(&[]), [12.0, 0.0, 0.0]));
    // The host's number of pixels holds for what is nested too.
    let mut inst = play(&strips);
    inst.set_drag_threshold(2.0);
    assert_eq!((drag(&mut inst, 1.0, &[(40.0, 25.0), (35.0, 25.0)]), state(&inst)), (heard(&[]), [3.0, 0.0, 0.0]));
    // One that goes sideways and up moves the Strip and the list, each its own way.
    let mut inst = play(&strips);
    assert_eq!((drag(&mut inst, 1.0, &[(45.0, 28.0), (20.0, 17.0)]), state(&inst)), (heard(&[]), [17.0, 0.0, 3.0]));
    // A Strip that is held is followed wherever the pointer goes: out of the list's box
    // (which ends at 60 down) it goes on, by 10 more for 10 more to the left.
    let mut inst = play(&strips);
    drag(&mut inst, 1.0, &[(45.0, 25.0), (30.0, 25.0), (30.0, 100.0), (20.0, 100.0)]);
    assert_eq!(inst.number("rows/0/sx"), Some(17.0));
    // One that is scaled to nothing while it is pressed still hears that it was let go
    // (the pointer cannot be anywhere on it: for it, it has left).
    let shrunk = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "size", "valueType": "number", "value": 1 } ] },
          { "type": "NestedArtboard", "name": "key", "artboard": "Key", "x": 10, "y": 10, "children": [ { "type": "Bind", "source": "size", "property": "scaleX" } ] } ] },
        { "type": "Artboard", "name": "Key", "width": 50, "height": 20, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "held", "valueType": "bool" } ] },
          { "type": "Shape", "name": "face", "x": 25, "y": 10, "children": [ { "type": "Rectangle", "width": 50, "height": 20 } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "face", "event": "down", "input": "held", "action": "set", "value": 1 },
            { "type": "Listener", "target": "face", "event": "up", "input": "held", "action": "set", "value": 0 } ] } ] }"##,
    );
    let mut inst = play(&shrunk);
    inst.pointer(Pointer::Down, 30.0, 20.0);
    assert_eq!(inst.number("key/held"), Some(1.0));
    inst.set_number("size", 0.0);
    inst.advance(0.0);
    inst.pointer(Pointer::Up, 30.0, 20.0);
    assert_eq!(inst.number("key/held"), Some(0.0));

    // A key in a box that hides what is outside it, which reports each `up` on its face.
    // A press that leaves the box is let go for the key, once; and that it was does
    // not last into the next press, which begins outside and is let go on the key.
    let boxed = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "Layout", "name": "box", "x": 10, "y": 10, "width": 60, "height": 50, "scroll": true, "children": [
            { "type": "NestedArtboard", "name": "key", "artboard": "Key" } ] } ] },
        { "type": "Artboard", "name": "Key", "width": 50, "height": 20, "children": [
          { "type": "Event", "name": "lifted" },
          { "type": "Shape", "name": "face", "x": 25, "y": 10, "children": [ { "type": "Rectangle", "width": 50, "height": 20 } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "face", "event": "up", "emit": "lifted" } ] } ] }"##,
    );
    let mut inst = play(&boxed);
    assert_eq!(drag(&mut inst, 1.0, &[(30.0, 20.0), (30.0, 100.0)]), heard(&["lifted"]));
    assert_eq!(drag(&mut inst, 1.0, &[(30.0, 100.0), (30.0, 20.0)]), heard(&["lifted"]));

    // A list in a list that scrolls the same way: the one around it is at its start and
    // cannot follow a drag downwards; the one inside (at 20) does.
    let inner = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "pos", "valueType": "number" } ] },
          { "type": "Layout", "name": "outer", "x": 10, "y": 10, "width": 60, "height": 50, "flow": "column", "scroll": true, "children": [
            { "type": "Bind", "source": "pos", "property": "scrolled", "direction": "twoWay" },
            { "type": "NestedArtboard", "name": "panel", "artboard": "Panel" },
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 60, "height": 40 }, { "type": "Fill", "color": "#00ff00" } ] } ] } ] },
        { "type": "Artboard", "name": "Panel", "width": 60, "height": 40, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "ip", "valueType": "number", "value": 20 } ] },
          { "type": "Layout", "name": "inner", "width": 60, "height": 40, "flow": "column", "scroll": true, "children": [
            { "type": "Bind", "source": "ip", "property": "scrolled", "direction": "twoWay" },
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 60, "height": 40 }, { "type": "Fill", "color": "#ff0000" } ] },
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 60, "height": 40 }, { "type": "Fill", "color": "#0000ff" } ] } ] } ] }"##,
    );
    let mut inst = play(&inner);
    inst.advance(0.0);
    drag(&mut inst, 1.0, &[(30.0, 20.0), (30.0, 35.0)]);
    assert_eq!((inst.number("panel/ip"), inst.number("pos")), (Some(13.0), Some(0.0)));

    // A Layout whose `scrolled` a Property is bound to is where the Property says: a
    // press that goes along it is not a scroll, and clicks what it began on.
    let fixed = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "pos", "valueType": "number" } ] },
          { "type": "Event", "name": "clicked" },
          { "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "height": 50, "flow": "column", "scroll": true, "children": [
            { "type": "Bind", "source": "pos", "property": "scrolled" },
            { "type": "Shape", "name": "tall", "children": [ { "type": "Rectangle", "width": 60, "height": 90 }, { "type": "Fill", "color": "#00ff00" } ] } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "tall", "event": "click", "emit": "clicked" } ] } ] }"##,
    );
    let mut inst = play(&fixed);
    assert_eq!(drag(&mut inst, 1.0, &[(30.0, 40.0), (30.0, 25.0)]), heard(&["clicked"]));
}

#[test]
fn scrolling_layouts_nest_and_hide_what_they_hold() {
    let block = |name: &str, width: u32| {
        format!(r##"{{ "type": "Shape", "name": "{name}", "children": [ {{ "type": "Rectangle", "width": {width}, "height": 20 }} ] }}"##)
    };
    // A row that scrolls sideways (90 of content in 50) as the first of four rows in a
    // column that scrolls up and down (95 in 40). A press on the row takes hold of both,
    // and each goes its own way with the pointer once it has gone 8 pixels that way, by
    // what it goes beyond those: 10 to the left and 5 up moves the row alone (by 2), 10
    // up the column too.
    let file = load(&format!(
        r##"{{ "type": "Artboard", "width": 80, "height": 120, "children": [
          {{ "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "height": 50, "flow": "column", "padding": 5, "gap": 5, "scroll": true, "children": [
            {{ "type": "Layout", "name": "strip", "width": 50, "scroll": true, "children": [ {}, {}, {} ] }}, {}, {}, {} ] }} ] }}"##,
        block("s1", 30),
        block("s2", 30),
        block("s3", 30),
        block("b", 50),
        block("c", 50),
        block("d", 50)
    ));
    let mut inst = play(&file);
    drop(pixels(&mut inst));
    let gone = |inst: &Instance| ["strip", "list"].map(|name| inst.property(object(inst.file(), name), p::SCROLLED).unwrap());
    inst.pointer(Pointer::Down, 30.0, 25.0);
    inst.pointer(Pointer::Move, 20.0, 20.0);
    inst.advance(0.0);
    assert_eq!(gone(&inst), [2.0, 0.0]);
    inst.pointer(Pointer::Move, 20.0, 15.0);
    inst.advance(0.0);
    assert_eq!(gone(&inst), [2.0, 2.0]);
    // A press under the row (on `b`, which the column has taken 2 up) holds the column alone.
    inst.pointer(Pointer::Up, 20.0, 15.0);
    inst.pointer(Pointer::Down, 30.0, 40.0);
    inst.pointer(Pointer::Move, 0.0, 30.0);
    inst.advance(0.0);
    assert_eq!(gone(&inst), [2.0, 4.0]);

    // Rows that are components: one that is scrolled out of the box does not get the
    // press (it would report `pressed`); scrolled in, it does.
    let rows = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 80, "height": 120, "children": [
          { "type": "ViewModel", "name": "vm", "children": [ { "type": "Property", "name": "pos", "valueType": "number" } ] },
          { "type": "Layout", "name": "list", "x": 10, "y": 10, "width": 60, "height": 50, "flow": "column", "padding": 5, "gap": 5, "scroll": true, "children": [
            { "type": "Bind", "source": "pos", "property": "scrolled" },
            { "type": "NestedArtboard", "artboard": "Row" }, { "type": "NestedArtboard", "artboard": "Row" },
            { "type": "NestedArtboard", "artboard": "Row" }, { "type": "NestedArtboard", "artboard": "Row" } ] },
          { "type": "StateMachine", "name": "m", "children": [ { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] } ] } ] },
        { "type": "Artboard", "name": "Row", "width": 50, "height": 20, "children": [
          { "type": "Event", "name": "pressed" },
          { "type": "Shape", "name": "face", "x": 25, "y": 10, "children": [ { "type": "Rectangle", "width": 50, "height": 20 } ] },
          { "type": "StateMachine", "name": "m", "children": [
            { "type": "Layer", "children": [ { "type": "State", "name": "idle" } ] },
            { "type": "Listener", "target": "face", "event": "down", "emit": "pressed" } ] } ] }"##,
    );
    let mut inst = play(&rows);
    drop(pixels(&mut inst));
    // The fourth row is laid out at 90 to 110, under the box (which ends at 60).
    inst.pointer(Pointer::Down, 30.0, 100.0);
    inst.advance(0.0);
    assert!(inst.take_events().is_empty());
    inst.pointer(Pointer::Up, 30.0, 100.0);
    inst.set_number("pos", 55.0);
    inst.advance(0.0);
    inst.pointer(Pointer::Down, 30.0, 45.0);
    inst.advance(0.0);
    assert_eq!(inst.take_events(), ["pressed"]);
}

#[test]
fn layouts_measure_what_is_drawn_and_what_changes_inside() {
    let named = |inst: &Instance, name: &str| inst.bounds(object(inst.file(), name)).unwrap().map(f32::round);
    // A group that shows one of its children at a time takes the room of that one.
    let solo = |n: u32| {
        play(&load(&format!(
            r##"{{ "type": "Artboard", "width": 200, "height": 60, "children": [
              {{ "type": "Layout", "name": "box", "gap": 10, "children": [
                {{ "type": "Node", "solo": {n}, "children": [
                  {{ "type": "Shape", "children": [ {{ "type": "Rectangle", "width": 16, "height": 16 }} ] }},
                  {{ "type": "Shape", "children": [ {{ "type": "Rectangle", "width": 120, "height": 16 }} ] }} ] }},
                {{ "type": "Shape", "name": "after", "children": [ {{ "type": "Rectangle", "width": 10, "height": 10 }} ] }} ] }} ] }}"##
        )))
    };
    assert_eq!((named(&solo(1), "after")[0], named(&solo(2), "after")[0], named(&solo(0), "after")[0]), (26.0, 130.0, 130.0));

    // A nested artboard takes the room of its artboard.
    let file = load(
        r##"{ "type": "Artboard", "name": "Main", "width": 200, "height": 60, "children": [
            { "type": "Layout", "gap": 10, "children": [
              { "type": "NestedArtboard", "artboard": "Chip" },
              { "type": "Shape", "name": "after", "children": [ { "type": "Rectangle", "width": 10, "height": 10 } ] } ] } ] },
          { "type": "Artboard", "name": "Chip", "width": 20, "height": 20 }"##,
    );
    assert_eq!(named(&play(&file), "after")[0], 30.0);

    // A skinned Path fits its bone where the Layout puts the two of them.
    let skinned = play(&load(
        r##"{ "type": "Artboard", "width": 200, "height": 60, "children": [
          { "type": "Layout", "x": 10, "y": 10, "gap": 10, "padding": 5, "children": [
            { "type": "Shape", "children": [ { "type": "Rectangle", "width": 20, "height": 10 } ] },
            { "type": "Bone", "name": "arm", "length": 20 },
            { "type": "Shape", "name": "sleeve", "children": [
              { "type": "Path", "d": "M0 0 L30 0 L30 8 L0 8 Z", "children": [ { "type": "Skin", "target": "arm" } ] } ] } ] } ] }"##,
    ));
    assert_eq!(named(&skinned, "sleeve"), [45., 15., 75., 23.]);
}

#[test]
fn an_event_names_the_sound_the_host_plays() {
    // `tapped` has a sound at half volume; `quiet` has none. The Event is in a nested
    // artboard: what it reports, and the sound with it, reach the host through the parent.
    let src = r##"{ "zuri": 1, "children": [
      { "type": "AudioAsset", "name": "click", "bytes": "UklGRg==" },
      { "type": "ImageAsset", "name": "picture", "width": 1, "height": 1 },
      { "type": "Artboard", "name": "Main", "width": 100, "height": 100, "children": [
        { "type": "NestedArtboard", "artboard": "Pad" } ] },
      { "type": "Artboard", "name": "Pad", "width": 100, "height": 100, "children": [
        { "type": "Event", "name": "tapped", "asset": "click", "volume": 0.5 },
        { "type": "Event", "name": "quiet" },
        { "type": "Shape", "name": "pad", "x": 50, "y": 50, "children": [ { "type": "Rectangle", "width": 100, "height": 100 } ] },
        { "type": "StateMachine", "children": [
          { "type": "Listener", "target": "pad", "event": "down", "emit": "tapped" },
          { "type": "Listener", "target": "pad", "event": "up", "emit": "quiet" } ] } ] } ] }"##;
    let file = File::load(&format::write(&json::compile(src, &|_| unreachable!()).unwrap(), 0).unwrap()).unwrap();
    let mut inst = play(&file);
    let view = inst.fit(Fit::None, 0.5, 0.5, 100.0, 100.0);
    inst.render(&mut Raster::default(), &mut [0; 100 * 100 * 4], 100, 100, view);
    inst.pointer(Pointer::Down, 50.0, 50.0);
    inst.pointer(Pointer::Up, 50.0, 50.0);
    inst.advance(0.0);
    let reports = inst.take_reports();
    let named: Vec<_> = reports.iter().map(|e| file.objects[*e as usize].str(p::NAME).unwrap()).collect();
    assert_eq!(named, ["tapped", "quiet"]);
    let click = object(&file, "click");
    assert_eq!((file.event_sound(reports[0]), file.event_sound(reports[1])), (Some((click, 0.5)), None));
    assert_eq!((file.objects[click as usize].bytes(p::BYTES), inst.take_reports().len()), (Some(&b"RIFF"[..]), 0));
    // A volume that is not a number (a file made by hand) is the default, and one out of range is brought in.
    let with_volume = |volume: f32| {
        let mut objects = json::compile(src, &|_| unreachable!()).unwrap();
        let tapped = objects.iter().position(|o| o.str(p::NAME) == Some("tapped")).unwrap();
        for (key, value) in &mut objects[tapped].props {
            if *key == p::VOLUME {
                *value = format::Value::Float(volume);
            }
        }
        File::from_objects(objects).ok().and_then(|file| file.event_sound(tapped as u32)).map(|sound| sound.1)
    };
    assert_eq!((with_volume(f32::NAN), with_volume(7.0), with_volume(-1.0)), (Some(1.0), Some(1.0), Some(0.0)));
    // Not an Event, and not a sound: nothing to play.
    assert_eq!((file.event_sound(click), file.event_sound(u32::MAX)), (None, None));
    let wrong = json::compile(&src.replace(r#""asset": "click""#, r#""asset": "picture""#), &|_| unreachable!()).unwrap_err().0;
    assert!(wrong.contains("AudioAsset"), "{wrong}");
}
