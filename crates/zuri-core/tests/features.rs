//! One focused scene per feature, checked through the public API. If a feature is listed
//! as done in docs/parity.md, a test here (or in runtime.rs) drives it.

use std::sync::Arc;
use zuri_core::schema::p;
use zuri_core::{format, json, raster, File, Fit, Instance, Raster};

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
    let view = inst.fit(Fit::None, w as f32, h as f32);
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
