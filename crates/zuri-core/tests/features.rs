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
    assert!(broken(r#"{ "type": "Condition", "input": "mix", "with": "go" }"#).contains("'with' must be a number or bool"));
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
fn constraints_make_an_object_follow_another() {
    // `leader` and `track` are fixed; `follower` carries one constraint and a red square.
    let scene = |follower: &str, constraint: &str| {
        load(&format!(
            r##"{{ "type": "Artboard", "width": 120, "height": 120, "children": [
              {{ "type": "ViewModel", "children": [ {{ "type": "Property", "name": "mix", "valueType": "number", "value": 1 }} ] }},
              {{ "type": "Node", "name": "follower", {follower} "children": [
                {{ "type": "Constraint", {constraint}, "children": [ {{ "type": "Bind", "source": "mix", "property": "strength" }} ] }},
                {{ "type": "Shape", "name": "mark", "children": [ {{ "type": "Rectangle", "width": 6, "height": 6 }}, {{ "type": "Fill", "color": "#ff0000" }} ] }} ] }},
              {{ "type": "Node", "name": "leader", "x": 60, "y": 20, "rotation": 350, "scaleX": 2 }},
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
