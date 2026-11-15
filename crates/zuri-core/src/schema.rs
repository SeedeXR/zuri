//! The schema: every object type and property Zuri knows, in one place.
//!
//! Everything else is driven by these two tables: the JSON twin, validation, `zuri schema`,
//! the editor inspector and the docs. To add a property: add one `props!` line, list it under
//! the types that carry it, and read it where it is used. Keys are permanent once shipped;
//! never reuse or renumber one (old files and old runtimes depend on them).

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Uint,
    Bool,
    Float,
    Color,
    Str,
    Bytes,
    /// File index of another object (a name in JSON).
    Ref,
    /// A property key (a property name in JSON).
    PropKey,
    Enum(&'static [&'static str]),
}

pub struct PropDef {
    pub key: u16,
    pub name: &'static str,
    pub kind: Kind,
    /// Value used when the property is absent (numeric kinds only).
    pub default: f32,
    pub doc: &'static str,
}

pub struct TypeDef {
    pub key: u16,
    pub name: &'static str,
    /// Property groups this type carries.
    pub props: &'static [&'static [u16]],
    /// Types this one may be a child of. Empty = file root.
    pub parents: &'static [u16],
    pub doc: &'static str,
}

impl TypeDef {
    pub fn has(&self, key: u16) -> bool {
        self.props.iter().any(|g| g.contains(&key))
    }
}

pub const DEFAULT_COLOR: u32 = 0xff00_0000;

macro_rules! props {
    ($($id:ident = $key:literal, $name:literal, $kind:expr, $def:expr, $doc:literal;)*) => {
        /// Property keys.
        pub mod p { $(pub const $id: u16 = $key;)* }
        pub static PROPS: &[PropDef] = &[
            // Only the authoring build (feature `json`) reads the docs: keep them out of the player.
            $(PropDef { key: $key, name: $name, kind: $kind, default: $def, doc: if cfg!(feature = "json") { $doc } else { "" } }),*
        ];
    };
}

macro_rules! types {
    ($($id:ident = $key:literal, $name:literal, [$($g:expr),*], [$($par:expr),*], $doc:literal;)*) => {
        /// Type keys.
        pub mod t { $(pub const $id: u16 = $key;)* }
        pub static TYPES: &[TypeDef] = &[
            $(TypeDef { key: $key, name: $name, props: &[$($g),*], parents: &[$($par),*], doc: if cfg!(feature = "json") { $doc } else { "" } }),*
        ];
    };
}

use Kind::*;

props! {
    PARENT = 1, "parent", Uint, 0.0, "File index of the parent object. Written by the compiler; not used in JSON.";
    NAME = 2, "name", Str, 0.0, "Name. Other objects refer to this one by it, so referenced names must be unique within an artboard.";
    X = 3, "x", Float, 0.0, "Horizontal position in the parent's space, in artboard units.";
    Y = 4, "y", Float, 0.0, "Vertical position in the parent's space (y grows downward).";
    ROTATION = 5, "rotation", Float, 0.0, "Rotation in degrees, clockwise.";
    SCALE_X = 6, "scaleX", Float, 1.0, "Horizontal scale.";
    SCALE_Y = 7, "scaleY", Float, 1.0, "Vertical scale.";
    OPACITY = 8, "opacity", Float, 1.0, "Opacity 0..1. Multiplies down to descendants.";
    WIDTH = 9, "width", Float, 0.0, "Width in artboard units. Geometry is centered on its position; an artboard starts at 0,0.";
    HEIGHT = 10, "height", Float, 0.0, "Height in artboard units.";
    RADIUS = 11, "radius", Float, 0.0, "Corner radius (Rectangle) or gradient radius (RadialGradient).";
    POINTS = 12, "points", Uint, 5.0, "Number of corners (Polygon) or tips (Star).";
    INNER_RADIUS = 13, "innerRadius", Float, 0.5, "Star inner radius as a fraction of the outer radius, 0..1.";
    D = 14, "d", Str, 0.0, "SVG path data. Keyframe it to morph between paths with the same command structure.";
    COLOR = 15, "color", Color, 0.0, "Color as #rrggbb or #rrggbbaa.";
    FILL_RULE = 16, "fillRule", Enum(&["nonzero", "evenodd"]), 0.0, "Which regions of a self-overlapping path are inside.";
    FEATHER = 17, "feather", Float, 0.0, "Soft-edge blur radius in artboard units. Use it for glows and shadows.";
    THICKNESS = 18, "thickness", Float, 1.0, "Stroke width in artboard units.";
    CAP = 19, "cap", Enum(&["butt", "round", "square"]), 0.0, "Shape of open stroke ends.";
    JOIN = 20, "join", Enum(&["miter", "round", "bevel"]), 0.0, "Shape of stroke corners.";
    TRIM_START = 21, "trimStart", Float, 0.0, "Where the visible stroke starts along the path, 0..1.";
    TRIM_END = 22, "trimEnd", Float, 1.0, "Where the visible stroke ends along the path, 0..1.";
    TRIM_OFFSET = 23, "trimOffset", Float, 0.0, "Shifts the trimmed range along the path; wraps around.";
    X0 = 24, "x0", Float, 0.0, "Gradient start x (linear) or center x (radial), in the shape's space.";
    Y0 = 25, "y0", Float, 0.0, "Gradient start y (linear) or center y (radial).";
    X1 = 26, "x1", Float, 0.0, "Linear gradient end x.";
    Y1 = 27, "y1", Float, 0.0, "Linear gradient end y.";
    POSITION = 28, "position", Float, 0.0, "Gradient stop position, 0..1.";
    BLEND_MODE = 29, "blendMode", Enum(&["normal", "multiply", "screen", "overlay", "darken", "lighten", "colorDodge", "colorBurn", "hardLight", "softLight", "difference", "exclusion", "hue", "saturation", "color", "luminosity"]), 0.0, "How the shape's paints combine with what is beneath.";
    CLIP = 30, "clip", Bool, 1.0, "Clip content to the artboard bounds.";
    SOURCE = 31, "source", Ref, 0.0, "Clip: the Shape (or Text, Image, Model) whose outline clips. Bind: the view-model Property to read.";
    ASSET = 32, "asset", Ref, 0.0, "The ImageAsset, FontAsset, MeshAsset or ShaderAsset this object uses.";
    TEXT = 33, "text", Str, 0.0, "The text to display. Newlines start new lines.";
    SIZE = 34, "size", Float, 16.0, "Font size in artboard units.";
    ALIGN = 35, "align", Enum(&["left", "center", "right"]), 0.0, "Horizontal text alignment.";
    LINE_HEIGHT = 36, "lineHeight", Float, 1.2, "Line height as a multiple of the font size.";
    FPS = 37, "fps", Float, 60.0, "Frames per second the animation's key frames are expressed in.";
    DURATION = 38, "duration", Float, 60.0, "Animation length in frames.";
    LOOP = 39, "loop", Enum(&["oneshot", "loop", "pingpong"]), 0.0, "What happens when the animation reaches its end.";
    SPEED = 40, "speed", Float, 1.0, "Playback speed multiplier.";
    TARGET = 41, "target", Ref, 0.0, "Track: the object to animate. Listener: the shape or node that receives the pointer (omit for the whole artboard).";
    PROPERTY = 42, "property", PropKey, 0.0, "Name of the property to animate or bind.";
    FRAME = 43, "frame", Float, 0.0, "Frame this Key or Cue sits on.";
    VALUE = 44, "value", Float, 0.0, "Number value. In JSON, `value` may also be a #color or a string; the compiler files it under colorValue or textValue.";
    COLOR_VALUE = 45, "colorValue", Color, 0.0, "Color value of a Key or Property.";
    TEXT_VALUE = 46, "textValue", Str, 0.0, "String value of a Key or Property (also path data for `d` tracks).";
    INTERP = 47, "interp", Enum(&["linear", "hold", "cubic"]), 0.0, "How the value moves from this key to the next. On a Transition: how the cross-fade progresses (`hold` switches at the end).";
    EASE_X1 = 48, "easeX1", Float, 0.42, "Cubic easing control point 1 x (as in CSS cubic-bezier).";
    EASE_Y1 = 49, "easeY1", Float, 0.0, "Cubic easing control point 1 y.";
    EASE_X2 = 50, "easeX2", Float, 0.58, "Cubic easing control point 2 x.";
    EASE_Y2 = 51, "easeY2", Float, 1.0, "Cubic easing control point 2 y.";
    KIND = 52, "kind", Enum(&["animation", "entry", "any", "exit", "blend", "direct"]), 0.0, "What a state does. `blend` mixes its BlendAnimation children along `input`; `direct` lays each of them over the design values, at its own weight.";
    ANIMATION_REF = 53, "animation", Ref, 0.0, "The Animation a State or BlendAnimation plays.";
    TO = 54, "to", Ref, 0.0, "The State a Transition leads to.";
    BLEND = 55, "blend", Float, 0.0, "Seconds over which a Transition cross-fades from the old state to the new one.";
    EXIT_TIME = 56, "exitTime", Float, 0.0, "Fraction of the source animation (0..1) that must have played before the Transition may fire. 0 disables the wait.";
    INPUT = 57, "input", Ref, 0.0, "The view-model Property a Condition tests, a Listener or Action changes, or a blend State is driven by. On a BlendAnimation of a `direct` State: the number (0..1) that is its weight.";
    OP = 58, "op", Enum(&["eq", "ne", "lt", "le", "gt", "ge"]), 0.0, "Comparison a Condition applies between its input and `value`. Ignored for triggers.";
    EVENT = 59, "event", Enum(&["enter", "exit", "down", "up", "click", "move"]), 0.0, "Pointer event a Listener reacts to.";
    ACTION = 60, "action", Enum(&["set", "toggle", "fire", "add"]), 0.0, "What a Listener or Action does to its input: set it to `value`, toggle a bool, fire a trigger, or add `value`.";
    EMIT = 61, "emit", Ref, 0.0, "An Event to report to the host when this Transition or Listener fires, or this Cue is passed.";
    VALUE_TYPE = 62, "valueType", Enum(&["number", "bool", "trigger", "color", "string"]), 0.0, "Type of a view-model Property.";
    MUL = 63, "mul", Float, 1.0, "Bind: multiply the source number by this before writing it.";
    ADD = 64, "add", Float, 0.0, "Bind: add this after multiplying.";
    BYTES = 65, "bytes", Bytes, 0.0, "Asset payload. In JSON: base64, or \"@relative/path\" to embed a file at build time.";
    WGSL = 66, "wgsl", Str, 0.0, "Shader source in WGSL. `zuri build` validates it and fills in `glsl` and `hlsl`.";
    GLSL = 67, "glsl", Str, 0.0, "GLSL ES 3.00 translation of `wgsl`, generated at build time.";
    HLSL = 68, "hlsl", Str, 0.0, "HLSL translation of `wgsl`, generated at build time.";
    ROTATION_X = 69, "rotationX", Float, 0.0, "3D model pitch in degrees.";
    ROTATION_Y = 70, "rotationY", Float, 0.0, "3D model yaw in degrees.";
    ROTATION_Z = 71, "rotationZ", Float, 0.0, "3D model roll in degrees.";
    FOV = 72, "fov", Float, 35.0, "3D camera vertical field of view in degrees.";
    ARTBOARD_REF = 73, "artboard", Ref, 0.0, "The Artboard a NestedArtboard instantiates.";
    P0 = 74, "p0", Float, 0.0, "Shader parameter 0 (animatable, bindable).";
    P1 = 75, "p1", Float, 0.0, "Shader parameter 1.";
    P2 = 76, "p2", Float, 0.0, "Shader parameter 2.";
    P3 = 77, "p3", Float, 0.0, "Shader parameter 3.";
    WEIGHT = 78, "weight", Float, 400.0, "Font weight (100..900) for variable fonts; ignored by static fonts. Animatable.";
    DASH = 79, "dash", Float, 0.0, "Length of each dash along a stroke. 0 draws a solid line.";
    GAP = 80, "gap", Float, 0.0, "Length of the gap between dashes. 0 uses the dash length.";
    DASH_OFFSET = 81, "dashOffset", Float, 0.0, "Shifts the dash pattern along the path. Animate it for marching ants.";
    PLAY = 82, "play", Str, 0.0, "Name of the StateMachine or Animation of the nested artboard to play. Omit to play its first state machine, or failing that its first animation.";
    INTO = 83, "into", Str, 0.0, "Bind inside a NestedArtboard: name of the nested artboard's view-model Property (of the same type) to write the source into, every frame. Triggers fire through.";
    ORDER = 84, "order", Float, 0.0, "Draw order among siblings: higher is drawn later (on top); equal values keep document order. Animatable.";
    PAUSE = 86, "pause", Bool, 0.0, "Freeze the state being left for the length of the cross-fade. By default it keeps playing underneath.";
    CHANCE = 87, "chance", Float, 0.0, "Random choice: when the first Transition that could fire has a `chance`, one of those that could fire and have a `chance` is picked at random, in proportion. 0 = no randomness.";
    WITH = 88, "with", Ref, 0.0, "Compare `input` against this view-model Property instead of against `value`.";
    SOLO = 85, "solo", Uint, 0.0, "Draw only this child: 1 is the first drawable child, 2 the second, and so on. 0 draws all. Key it to switch between children.";
}

use p::*;
const NAMED: &[u16] = &[NAME];
const XF: &[u16] = &[X, Y, ROTATION, SCALE_X, SCALE_Y];
const OPAQUE: &[u16] = &[OPACITY, ORDER];
const SIZED: &[u16] = &[WIDTH, HEIGHT];

use t::*;

types! {
    ARTBOARD = 1, "Artboard", [NAMED, SIZED, &[CLIP]], [], "A canvas with its own size, content, animations, state machines and view model. Fill children paint its background.";
    NODE = 2, "Node", [NAMED, XF, OPAQUE, &[SOLO]], [ARTBOARD, NODE, SHAPE], "A group: transforms and fades everything inside it. With `solo` it shows one child at a time.";
    SHAPE = 3, "Shape", [NAMED, XF, OPAQUE, &[BLEND_MODE]], [ARTBOARD, NODE, SHAPE], "Geometry children (Rectangle, Ellipse, Polygon, Star, Path) painted by Fill and Stroke children, in order.";
    RECTANGLE = 4, "Rectangle", [NAMED, XF, SIZED, &[RADIUS]], [SHAPE], "Rectangle centered on its position.";
    ELLIPSE = 5, "Ellipse", [NAMED, XF, SIZED], [SHAPE], "Ellipse centered on its position.";
    POLYGON = 6, "Polygon", [NAMED, XF, SIZED, &[POINTS]], [SHAPE], "Regular polygon inscribed in width x height.";
    STAR = 7, "Star", [NAMED, XF, SIZED, &[POINTS, INNER_RADIUS]], [SHAPE], "Star inscribed in width x height.";
    PATH = 8, "Path", [NAMED, XF, &[D]], [SHAPE], "Free-form path from SVG path data.";
    FILL = 9, "Fill", [NAMED, &[COLOR, FILL_RULE, FEATHER]], [SHAPE, ARTBOARD, TEXT_NODE], "Fills the parent's geometry with a color, or with a gradient / shader child.";
    STROKE = 10, "Stroke", [NAMED, &[COLOR, THICKNESS, CAP, JOIN, TRIM_START, TRIM_END, TRIM_OFFSET, DASH, GAP, DASH_OFFSET, FEATHER]], [SHAPE, ARTBOARD, TEXT_NODE], "Outlines the parent's geometry.";
    LINEAR_GRADIENT = 11, "LinearGradient", [NAMED, &[X0, Y0, X1, Y1]], [FILL, STROKE], "Gradient along a line. Needs GradientStop children.";
    RADIAL_GRADIENT = 12, "RadialGradient", [NAMED, &[X0, Y0, RADIUS]], [FILL, STROKE], "Gradient outward from a center. Needs GradientStop children.";
    GRADIENT_STOP = 13, "GradientStop", [NAMED, &[COLOR, POSITION]], [LINEAR_GRADIENT, RADIAL_GRADIENT], "One color along a gradient.";
    CLIP_NODE = 14, "Clip", [&[SOURCE]], [NODE, SHAPE, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL], "Clips its parent (and everything inside it) to the outline of `source`: a Shape, a Text, or the box of an Image or Model.";
    IMAGE = 15, "Image", [NAMED, XF, OPAQUE, SIZED, &[ASSET]], [ARTBOARD, NODE, SHAPE], "A bitmap from an ImageAsset, centered on its position. Width/height 0 = the image's own size.";
    TEXT_NODE = 16, "Text", [NAMED, XF, OPAQUE, &[TEXT, ASSET, SIZE, WIDTH, ALIGN, LINE_HEIGHT, WEIGHT]], [ARTBOARD, NODE, SHAPE], "Text set in a FontAsset, painted by its Fill/Stroke children. `width` > 0 wraps lines.";
    NESTED_ARTBOARD = 17, "NestedArtboard", [NAMED, XF, OPAQUE, &[ARTBOARD_REF, PLAY]], [ARTBOARD, NODE, SHAPE], "A live instance of another artboard (a reusable component). Bind children with `into` pass it data.";
    MODEL = 18, "Model", [NAMED, XF, OPAQUE, SIZED, &[ASSET, ROTATION_X, ROTATION_Y, ROTATION_Z, FOV]], [ARTBOARD, NODE, SHAPE], "A 3D mesh (imported from OBJ, glTF/GLB or FBX) rendered inside a width x height viewport.";
    SHADER_PAINT = 19, "ShaderPaint", [NAMED, &[ASSET, P0, P1, P2, P3]], [FILL, STROKE], "Paints with a ShaderAsset. Where no GPU is available the parent paint's color is used.";
    ANIMATION = 20, "Animation", [NAMED, &[FPS, DURATION, LOOP, SPEED]], [ARTBOARD], "A timeline of Tracks.";
    TRACK = 21, "Track", [&[TARGET, PROPERTY]], [ANIMATION], "Animates one property of one object through its Key children.";
    CUE = 23, "Cue", [&[FRAME, EMIT]], [ANIMATION], "Reports the `emit` Event to the host each time playback passes `frame`.";
    KEY = 22, "Key", [&[FRAME, VALUE, COLOR_VALUE, TEXT_VALUE, INTERP, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2]], [TRACK], "A key frame.";
    STATE_MACHINE = 30, "StateMachine", [NAMED], [ARTBOARD], "Interactive logic: Layers of States joined by Transitions, plus pointer Listeners.";
    LAYER = 31, "Layer", [NAMED], [STATE_MACHINE], "An independent state graph. Layers run together; later layers override earlier ones. Needs one `entry` State.";
    STATE = 32, "State", [NAMED, &[KIND, ANIMATION_REF, SPEED, INPUT]], [LAYER], "A node of the state graph.";
    BLEND_ANIMATION = 33, "BlendAnimation", [&[ANIMATION_REF, VALUE, INPUT]], [STATE], "One animation of a `blend` State, placed at `value` on the input's axis; or of a `direct` State, where `value` (or `input`) is its weight.";
    TRANSITION = 34, "Transition", [&[TO, BLEND, EXIT_TIME, EMIT, INTERP, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2, PAUSE, CHANCE]], [STATE], "Leaves the parent State for `to` when all its Condition children hold. Action children run when it fires.";
    CONDITION = 35, "Condition", [&[INPUT, OP, VALUE, WITH]], [TRANSITION], "A test on a view-model Property.";
    ACTION_NODE = 38, "Action", [&[INPUT, ACTION, VALUE]], [TRANSITION, LISTENER], "Changes a view-model Property when the Transition or Listener it is in fires.";
    LISTENER = 36, "Listener", [NAMED, &[TARGET, EVENT, INPUT, ACTION, VALUE, EMIT]], [STATE_MACHINE], "Turns a pointer event on `target` into a change of a view-model Property.";
    EVENT_NODE = 37, "Event", [NAMED], [ARTBOARD], "A named signal reported to the host application.";
    VIEW_MODEL = 40, "ViewModel", [NAMED], [ARTBOARD], "The artboard's data contract with the host: a set of typed Properties.";
    PROPERTY_NODE = 41, "Property", [NAMED, &[VALUE_TYPE, VALUE, COLOR_VALUE, TEXT_VALUE]], [VIEW_MODEL], "A typed value the host, listeners and state machines read and write.";
    BIND = 42, "Bind", [&[SOURCE, PROPERTY, MUL, ADD, INTO]], [NODE, SHAPE, RECTANGLE, ELLIPSE, POLYGON, STAR, PATH, FILL, STROKE, LINEAR_GRADIENT, RADIAL_GRADIENT, GRADIENT_STOP, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL, SHADER_PAINT], "Writes a view-model Property into `property` of the parent object every frame (or `into` a nested artboard's Property).";
    IMAGE_ASSET = 50, "ImageAsset", [NAMED, SIZED, &[BYTES]], [], "An encoded image (PNG, JPEG, WebP). The host decodes it.";
    FONT_ASSET = 51, "FontAsset", [NAMED, &[BYTES]], [], "A TTF or OTF font.";
    MESH_ASSET = 52, "MeshAsset", [NAMED, &[BYTES]], [], "A 3D mesh in Zuri's packed format. Produced by `zuri import`.";
    SHADER_ASSET = 53, "ShaderAsset", [NAMED, &[WGSL, GLSL, HLSL]], [], "A fragment shader.";
}

pub fn prop(key: u16) -> Option<&'static PropDef> {
    PROPS.iter().find(|p| p.key == key)
}
pub fn prop_by_name(name: &str) -> Option<&'static PropDef> {
    PROPS.iter().find(|p| p.name == name)
}
pub fn type_def(key: u16) -> Option<&'static TypeDef> {
    TYPES.iter().find(|t| t.key == key)
}
pub fn type_by_name(name: &str) -> Option<&'static TypeDef> {
    TYPES.iter().find(|t| t.name == name)
}

const MAX_KEY: usize = 128;

/// Default of a numeric property. O(1); used on the render path.
pub fn default(key: u16) -> f32 {
    static TABLE: OnceLock<[f32; MAX_KEY]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0.0; MAX_KEY];
        for p in PROPS {
            t[p.key as usize] = p.default;
        }
        t
    });
    table.get(key as usize).copied().unwrap_or(0.0)
}

/// Objects that live in the scene tree of an artboard instance.
pub fn is_scene(ty: u16) -> bool {
    (ARTBOARD..=SHADER_PAINT).contains(&ty)
}
/// Objects that are drawn in their own right, in sibling order (the rest paint or shape their parent).
pub fn is_drawable(ty: u16) -> bool {
    matches!(ty, NODE | SHAPE | IMAGE | TEXT_NODE | NESTED_ARTBOARD | MODEL)
}
/// Objects with x / y / rotation / scale.
pub fn has_transform(ty: u16) -> bool {
    matches!(ty, NODE | SHAPE | RECTANGLE | ELLIPSE | POLYGON | STAR | PATH | IMAGE | TEXT_NODE | NESTED_ARTBOARD | MODEL)
}
pub fn is_geometry(ty: u16) -> bool {
    matches!(ty, RECTANGLE | ELLIPSE | POLYGON | STAR | PATH)
}

/// Names are resolved per class, so a State and an Animation may share a name.
/// 0 = drawable object, 1 = State, 2 = Animation, 3 = Property, 4 = Event, 5 = asset, 6 = Artboard.
pub fn ref_class(ty: u16) -> u8 {
    match ty {
        STATE => 1,
        ANIMATION => 2,
        PROPERTY_NODE => 3,
        EVENT_NODE => 4,
        IMAGE_ASSET | FONT_ASSET | MESH_ASSET | SHADER_ASSET => 5,
        ARTBOARD => 6,
        _ => 0,
    }
}
/// The class a reference property of an `owner` object must point at.
pub fn expected_class(owner: u16, prop: u16) -> u8 {
    match prop {
        TO => 1,
        ANIMATION_REF => 2,
        INPUT | WITH => 3,
        SOURCE if owner == BIND => 3,
        EMIT => 4,
        ASSET => 5,
        ARTBOARD_REF => 6,
        _ => 0,
    }
}
/// The types a reference must point at when its class alone is too wide: each `asset`
/// wants one kind of asset, a Listener hears only what the pointer can hit, and a Clip
/// needs something with an outline. Empty = anything of the class.
pub fn ref_types(owner: u16, prop: u16) -> &'static [u16] {
    match (owner, prop) {
        (IMAGE, ASSET) => &[IMAGE_ASSET],
        (TEXT_NODE, ASSET) => &[FONT_ASSET],
        (MODEL, ASSET) => &[MESH_ASSET],
        (SHADER_PAINT, ASSET) => &[SHADER_ASSET],
        (LISTENER, TARGET) => &[NODE, SHAPE, TEXT_NODE, IMAGE, MODEL],
        (CLIP_NODE, SOURCE) => &[SHAPE, TEXT_NODE, IMAGE, MODEL],
        _ => &[],
    }
}
/// Hosts play and set these by name, so a name may be used once per artboard (Artboards: once per file).
pub fn host_named(ty: u16) -> bool {
    matches!(ty, ARTBOARD | ANIMATION | STATE_MACHINE | PROPERTY_NODE | EVENT_NODE)
}
pub const CLASS_NAMES: [&str; 7] = ["drawable object", "State", "Animation", "view-model Property", "Event", "asset", "Artboard"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_names_are_unique_and_in_range() {
        for (i, a) in PROPS.iter().enumerate() {
            assert!((a.key as usize) < MAX_KEY, "{} exceeds MAX_KEY", a.name);
            assert!(a.key != 0);
            for b in &PROPS[i + 1..] {
                assert!(a.key != b.key && a.name != b.name, "{} / {}", a.name, b.name);
            }
        }
        for (i, a) in TYPES.iter().enumerate() {
            assert!(a.key != 0);
            for b in &TYPES[i + 1..] {
                assert!(a.key != b.key && a.name != b.name, "{} / {}", a.name, b.name);
            }
            for g in a.props {
                for k in *g {
                    assert!(prop(*k).is_some(), "{} lists unknown property {k}", a.name);
                }
            }
            for par in a.parents {
                assert!(type_def(*par).is_some());
            }
        }
        assert_eq!(p::PARENT, crate::format::PARENT);
        assert_eq!(default(p::SCALE_X), 1.0);
    }
}
