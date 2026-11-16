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
    WIDTH = 9, "width", Float, 0.0, "Width in artboard units. Geometry is centered on its position; an Artboard and a Layout start at 0,0. A Layout with width (or height) 0 is as large as what it holds.";
    HEIGHT = 10, "height", Float, 0.0, "Height in artboard units.";
    RADIUS = 11, "radius", Float, 0.0, "Corner radius (Rectangle, Layout) or gradient radius (RadialGradient).";
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
    POSITION = 28, "position", Float, 0.0, "Gradient stop position, 0..1. `followPath` Constraint: where along the target's outline, 0..1 (it wraps, so it can be animated past 1). Text with `follow`: where along the outline the text sits.";
    BLEND_MODE = 29, "blendMode", Enum(&["normal", "multiply", "screen", "overlay", "darken", "lighten", "colorDodge", "colorBurn", "hardLight", "softLight", "difference", "exclusion", "hue", "saturation", "color", "luminosity"]), 0.0, "How the shape's paints combine with what is beneath.";
    CLIP = 30, "clip", Bool, 1.0, "Clip content to the artboard bounds.";
    SOURCE = 31, "source", Ref, 0.0, "Clip: the Shape (or Text, Image, Model) whose outline clips. Bind: the view-model Property to read.";
    ASSET = 32, "asset", Ref, 0.0, "The ImageAsset, FontAsset, MeshAsset or ShaderAsset this object uses, or the AudioAsset an Event plays.";
    TEXT = 33, "text", Str, 0.0, "The text to display. Newlines start new lines.";
    SIZE = 34, "size", Float, 16.0, "Font size in artboard units.";
    ALIGN = 35, "align", Enum(&["left", "center", "right"]), 0.0, "Horizontal text alignment.";
    LINE_HEIGHT = 36, "lineHeight", Float, 1.2, "Line height as a multiple of the font size.";
    FPS = 37, "fps", Float, 60.0, "Frames per second the animation's key frames are expressed in.";
    DURATION = 38, "duration", Float, 60.0, "Animation length in frames.";
    LOOP = 39, "loop", Enum(&["oneshot", "loop", "pingpong"]), 0.0, "What happens when the animation reaches its end.";
    SPEED = 40, "speed", Float, 1.0, "Playback speed multiplier.";
    TARGET = 41, "target", Ref, 0.0, "Track: the object to animate. Listener: the shape or node that receives the pointer (omit for the whole artboard). Constraint: the object to follow. Skin: the Bone that holds the path.";
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
    EVENT = 59, "event", Enum(&["enter", "exit", "down", "up", "click", "move", "change", "keyDown", "keyUp"]), 0.0, "What a Listener reacts to: a pointer event on its `target`; `change`: the Property named by `with` got a new value (or, for a trigger, fired); `keyDown` / `keyUp`: the host reported a key (see `key`).";
    ACTION = 60, "action", Enum(&["set", "toggle", "fire", "add"]), 0.0, "What a Listener or Action does to its input: set it to `value`, toggle a bool, fire a trigger, or add `value`.";
    EMIT = 61, "emit", Ref, 0.0, "An Event to report to the host when this Transition or Listener fires, or this Cue is passed.";
    VALUE_TYPE = 62, "valueType", Enum(&["number", "bool", "trigger", "color", "string", "enum"]), 0.0, "Type of a view-model Property. An `enum` holds one of its Option children: `value` is the Option's position, counting from 0.";
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
    GAP = 80, "gap", Float, 0.0, "Stroke: length of the gap between dashes (0 uses the dash length). Layout: the space between one child and the next.";
    DASH_OFFSET = 81, "dashOffset", Float, 0.0, "Shifts the dash pattern along the path. Animate it for marching ants.";
    PLAY = 82, "play", Str, 0.0, "Name of the StateMachine or Animation of the nested artboard to play. Omit to play its first state machine, or failing that its first animation.";
    INTO = 83, "into", Str, 0.0, "Bind inside a NestedArtboard: name of the nested artboard's view-model Property (of the same type) to write the source into, every frame. Triggers fire through.";
    ORDER = 84, "order", Float, 0.0, "Draw order among siblings: higher is drawn later (on top); equal values keep document order. Animatable.";
    PAUSE = 86, "pause", Bool, 0.0, "Freeze the state being left for the length of the cross-fade. By default it keeps playing underneath.";
    CHANCE = 87, "chance", Float, 0.0, "Random choice: when the first Transition that could fire has a `chance`, one of those that could fire and have a `chance` is picked at random, in proportion. 0 = no randomness.";
    WITH = 88, "with", Ref, 0.0, "Condition: compare `input` against this view-model Property instead of against `value`. Listener with event `change`: the Property to watch.";
    FEATHER_INNER = 89, "featherInner", Bool, 0.0, "Feather inward from the edge instead of outward: the paint hugs the inside of the outline and fades toward the middle, like an inner shadow or a vignette.";
    FEATHER_X = 90, "featherX", Float, 0.0, "Shifts the feathered paint horizontally, in the shape's units: a drop shadow's offset, or which edge an inner shadow leans on.";
    FEATHER_Y = 91, "featherY", Float, 0.0, "Shifts the feathered paint vertically.";
    TRIM_MODE = 92, "trimMode", Enum(&["sequential", "synced"]), 0.0, "With several paths in a shape: `sequential` trims them as one long path, one after another; `synced` trims each by the same fraction.";
    CONSTRAIN = 93, "constrain", Enum(&["translation", "rotation", "scale", "transform", "distance", "followPath", "ik"]), 0.0, "What a Constraint takes from its target: its position, rotation, scale or all three; a `distance` to keep from it; a place along its outline (`followPath`); or, on a Bone, `ik`: turn this bone and its parents so that its far end reaches the target.";
    STRENGTH = 94, "strength", Float, 1.0, "How much of the constraint applies, 0..1. Animate it to blend a constraint in and out.";
    DISTANCE = 95, "distance", Float, 0.0, "The distance a `distance` Constraint keeps from its target, in artboard units.";
    LIMIT = 96, "limit", Enum(&["closer", "further", "exact"]), 0.0, "`distance` Constraint: stay no further than `distance` (`closer`), no nearer (`further`), or exactly at it.";
    ORIENT = 97, "orient", Bool, 1.0, "`followPath` Constraint: turn to face along the path.";
    DIRECTION = 98, "direction", Enum(&["toTarget", "toSource", "twoWay"]), 0.0, "Which way a Bind copies: the view-model Property into the object's property (`toTarget`), the object's property into the Property (`toSource`, to read or test an animated value), or whichever of the two changed (`twoWay`; the Property wins a tie). Only `toTarget` applies `mul`, `add` and `format`.";
    FORMAT = 99, "format", Str, 0.0, "Bind of a Property onto a string property: the text to write, with `{}` standing for the value. A number is written with `decimals` digits after the point.";
    DECIMALS = 100, "decimals", Uint, 0.0, "Bind of a number onto a string property: digits after the decimal point.";
    GLYPHS = 101, "glyphs", Str, 0.0, "Subset the font when `zuri build` or the editor's export writes the file: keep the glyphs for the characters the document's texts in this font can show, plus the characters listed here (\"\" keeps only the document's). Without `glyphs` the whole font is embedded. Text set at run time can only show characters that were kept.";
    KEY_NAME = 102, "key", Str, 0.0, "Listener with event `keyDown` or `keyUp`: the key to react to, named as the web's KeyboardEvent.key names it (\"Enter\", \"ArrowLeft\", \" \", \"a\"). Omit to react to any key.";
    SLICE_LEFT = 103, "sliceLeft", Float, 0.0, "Nine-slice: how many pixels of the picture (of its ImageAsset's `width` x `height`) at the left keep their size when the Image is drawn wider or narrower than the picture. With any slice set, corners keep their size, edges stretch along their length and the middle stretches both ways (a panel or button background).";
    SLICE_TOP = 104, "sliceTop", Float, 0.0, "Nine-slice: image pixels at the top that keep their size.";
    SLICE_RIGHT = 105, "sliceRight", Float, 0.0, "Nine-slice: image pixels at the right that keep their size.";
    SLICE_BOTTOM = 106, "sliceBottom", Float, 0.0, "Nine-slice: image pixels at the bottom that keep their size.";
    LENGTH = 107, "length", Float, 0.0, "Length of a Bone, along its x axis. A Bone inside a Bone starts at its parent's far end.";
    BONES = 108, "bones", Uint, 2.0, "`ik` Constraint: how many bones turn to reach the target, counting the bone the Constraint is on and then its parents.";
    WEIGHTS = 109, "weights", Str, 0.0, "Skin: how firmly the bone holds each point of the Path, from 0 to 1, as numbers separated by spaces: one per point, in the order of the path data. A line (`M`, `L`, `H`, `V`) has one point; every curve has three, its two handles and then its end (`C`; `S`, `Q` and `T` each become one such curve, an arc `A` one per quarter turn). A point goes with its bones in proportion to their weights. Without `weights` the bone holds every point alike.";
    AXIS = 110, "axis", Enum(&["both", "x", "y"]), 0.0, "`translation` and `scale` Constraints: the axis to take from the target. The other keeps its own value.";
    MIN = 111, "min", Float, -1e9, "`translation`, `rotation` and `scale` Constraints: the least the value taken from the target may be: a position in artboard units, a rotation in degrees (read between -180 and 180), or a size (a mirrored target stays mirrored). The default is far below anything in a scene, which is no limit.";
    MAX = 112, "max", Float, 1e9, "`translation`, `rotation` and `scale` Constraints: the most the value taken from the target may be. The default is no limit; if `min` is above `max`, `max` wins.";
    FOLLOW = 113, "follow", Ref, 0.0, "Text: a Shape whose outline the text is set along, its baseline on the line, each letter turned to it. `position` says where along the outline (0..1) the text starts (or is centered, or ends: `align`), and can be animated. A closed outline wraps; letters past the end of an open one are not drawn.";
    FLOW = 114, "flow", Enum(&["row", "column"]), 0.0, "Layout: whether its children stand side by side (`row`) or one under the other (`column`).";
    PADDING = 115, "padding", Float, 0.0, "Layout: the space between its edge and its children, on every side.";
    JUSTIFY = 116, "justify", Enum(&["start", "center", "end", "spaceBetween"]), 0.0, "Layout: where the children go along the flow when there is room to spare: at the start, in the middle, at the end, or spread out with the room between them.";
    ALIGN_ITEMS = 117, "alignItems", Enum(&["start", "center", "end", "stretch"]), 0.0, "Layout: where each child goes across the flow: at the start (the top of a row), in the middle, at the end; `stretch` also makes child Layouts as large as the room.";
    GROW = 118, "grow", Float, 0.0, "A Layout inside a Layout: its share of the room left over along the parent's flow (0 keeps its own size). Two children with `grow` 1 and 2 take a third and two thirds of it.";
    VOLUME = 119, "volume", Float, 1.0, "Event with a sound: how loud the host plays it, 0..1.";
    PADDING_LEFT = 120, "paddingLeft", Float, 0.0, "Layout: more padding on its left side, added to `padding` (less, with a negative number).";
    PADDING_TOP = 121, "paddingTop", Float, 0.0, "Layout: more padding on its top side, added to `padding` (less, with a negative number).";
    PADDING_RIGHT = 122, "paddingRight", Float, 0.0, "Layout: more padding on its right side, added to `padding` (less, with a negative number).";
    PADDING_BOTTOM = 123, "paddingBottom", Float, 0.0, "Layout: more padding on its bottom side, added to `padding` (less, with a negative number).";
    WRAP = 124, "wrap", Bool, 0.0, "Layout: a child that does not fit along the flow starts a further line (a row below, or a column to the right), `gap` away; each line is as large across as its largest child. `justify`, `alignItems` and `grow` then work line by line. It takes a size along the flow: its own, or one given by `grow` or `stretch`.";
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
    NODE = 2, "Node", [NAMED, XF, OPAQUE, &[SOLO]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "A group: transforms and fades everything inside it. With `solo` it shows one child at a time.";
    SHAPE = 3, "Shape", [NAMED, XF, OPAQUE, &[BLEND_MODE]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "Geometry children (Rectangle, Ellipse, Polygon, Star, Path) painted by Fill and Stroke children, in order.";
    RECTANGLE = 4, "Rectangle", [NAMED, XF, SIZED, &[RADIUS]], [SHAPE], "Rectangle centered on its position.";
    ELLIPSE = 5, "Ellipse", [NAMED, XF, SIZED], [SHAPE], "Ellipse centered on its position.";
    POLYGON = 6, "Polygon", [NAMED, XF, SIZED, &[POINTS]], [SHAPE], "Regular polygon inscribed in width x height.";
    STAR = 7, "Star", [NAMED, XF, SIZED, &[POINTS, INNER_RADIUS]], [SHAPE], "Star inscribed in width x height.";
    PATH = 8, "Path", [NAMED, XF, &[D]], [SHAPE], "Free-form path from SVG path data.";
    FILL = 9, "Fill", [NAMED, &[COLOR, FILL_RULE, FEATHER, FEATHER_INNER, FEATHER_X, FEATHER_Y]], [SHAPE, ARTBOARD, TEXT_NODE, RUN, LAYOUT], "Fills the parent's geometry with a color, or with a gradient / shader child.";
    STROKE = 10, "Stroke", [NAMED, &[COLOR, THICKNESS, CAP, JOIN, TRIM_START, TRIM_END, TRIM_OFFSET, TRIM_MODE, DASH, GAP, DASH_OFFSET, FEATHER, FEATHER_X, FEATHER_Y]], [SHAPE, ARTBOARD, TEXT_NODE, RUN, LAYOUT], "Outlines the parent's geometry.";
    LINEAR_GRADIENT = 11, "LinearGradient", [NAMED, &[X0, Y0, X1, Y1]], [FILL, STROKE], "Gradient along a line. Needs GradientStop children.";
    RADIAL_GRADIENT = 12, "RadialGradient", [NAMED, &[X0, Y0, RADIUS]], [FILL, STROKE], "Gradient outward from a center. Needs GradientStop children.";
    GRADIENT_STOP = 13, "GradientStop", [NAMED, &[COLOR, POSITION]], [LINEAR_GRADIENT, RADIAL_GRADIENT], "One color along a gradient.";
    CLIP_NODE = 14, "Clip", [&[SOURCE]], [NODE, SHAPE, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL, BONE, LAYOUT], "Clips its parent (and everything inside it) to the outline of `source`: a Shape, a Text, or the box of an Image or Model.";
    IMAGE = 15, "Image", [NAMED, XF, OPAQUE, SIZED, &[ASSET, SLICE_LEFT, SLICE_TOP, SLICE_RIGHT, SLICE_BOTTOM]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "A bitmap from an ImageAsset, centered on its position. Width/height 0 = the image's own size.";
    TEXT_NODE = 16, "Text", [NAMED, XF, OPAQUE, &[TEXT, ASSET, SIZE, WIDTH, ALIGN, LINE_HEIGHT, WEIGHT, FOLLOW, POSITION]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "Text set in a FontAsset, painted by its Fill/Stroke children. `width` > 0 wraps lines.";
    NESTED_ARTBOARD = 17, "NestedArtboard", [NAMED, XF, OPAQUE, &[ARTBOARD_REF, PLAY]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "A live instance of another artboard (a reusable component). Bind children with `into` pass it data.";
    MODEL = 18, "Model", [NAMED, XF, OPAQUE, SIZED, &[ASSET, ROTATION_X, ROTATION_Y, ROTATION_Z, FOV]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "A 3D mesh (imported from OBJ, glTF/GLB or FBX) rendered inside a width x height viewport.";
    SHADER_PAINT = 19, "ShaderPaint", [NAMED, &[ASSET, P0, P1, P2, P3]], [FILL, STROKE], "Paints with a ShaderAsset. Where no GPU is available the parent paint's color is used.";
    ANIMATION = 20, "Animation", [NAMED, &[FPS, DURATION, LOOP, SPEED]], [ARTBOARD], "A timeline of Tracks.";
    TRACK = 21, "Track", [&[TARGET, PROPERTY]], [ANIMATION], "Animates one property of one object through its Key children.";
    CONSTRAINT = 24, "Constraint", [NAMED, &[CONSTRAIN, TARGET, STRENGTH, AXIS, MIN, MAX, DISTANCE, LIMIT, POSITION, ORIENT, BONES]], [NODE, SHAPE, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL, BONE, LAYOUT], "Makes its parent follow another object: its position, rotation or scale, a distance from it, or a place along its outline. Applied after animation, in artboard space; several apply in order.";
    BONE = 25, "Bone", [NAMED, XF, OPAQUE, &[LENGTH]], [ARTBOARD, NODE, SHAPE, BONE, LAYOUT], "A joint of a skeleton: it carries what is inside it, as a Node does, and a Bone inside it starts at its far end (`length` along its x axis). Chains of bones can be posed by an `ik` Constraint.";
    SKIN = 26, "Skin", [&[TARGET, WEIGHTS]], [PATH], "Binds its parent Path to a Bone, so that the outline bends when the bone moves: one Skin per bone, each with `weights`. The path fits its bones in the pose the document's own values describe (before Constraints, animation and Binds).";
    RUN = 27, "Run", [NAMED, &[TEXT, ASSET, SIZE, WEIGHT]], [TEXT_NODE], "A stretch of a Text in its own style. A Text with Runs shows them one after another as one text (its own `text` is not shown). A Run has its own `size` and `weight`, and its Text's font unless it names an `asset`; with Fill or Stroke children it is painted by those, on top of what the Text's paints paint.";
    LAYOUT = 28, "Layout", [NAMED, XF, OPAQUE, SIZED, &[FLOW, GAP, PADDING, PADDING_LEFT, PADDING_TOP, PADDING_RIGHT, PADDING_BOTTOM, WRAP, JUSTIFY, ALIGN_ITEMS, GROW, RADIUS]], [ARTBOARD, NODE, LAYOUT, BONE], "A box that arranges what is inside it in a row or a column, like a CSS flex container: Shapes, Texts, Images, Nodes and other Layouts, each taking the room of its own box. Fill and Stroke children paint the box. Inside a Layout, a child's `x` and `y` move it from the place the Layout gives it.";
    CUE = 23, "Cue", [&[FRAME, EMIT]], [ANIMATION], "Reports the `emit` Event to the host each time playback passes `frame`.";
    KEY = 22, "Key", [&[FRAME, VALUE, COLOR_VALUE, TEXT_VALUE, INTERP, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2]], [TRACK], "A key frame.";
    STATE_MACHINE = 30, "StateMachine", [NAMED], [ARTBOARD], "Interactive logic: Layers of States joined by Transitions, plus pointer Listeners.";
    LAYER = 31, "Layer", [NAMED], [STATE_MACHINE], "An independent state graph. Layers run together; later layers override earlier ones. Needs one `entry` State.";
    STATE = 32, "State", [NAMED, &[KIND, ANIMATION_REF, SPEED, INPUT]], [LAYER], "A node of the state graph.";
    BLEND_ANIMATION = 33, "BlendAnimation", [&[ANIMATION_REF, VALUE, INPUT]], [STATE], "One animation of a `blend` State, placed at `value` on the input's axis; or of a `direct` State, where `value` (or `input`) is its weight.";
    TRANSITION = 34, "Transition", [&[TO, BLEND, EXIT_TIME, EMIT, INTERP, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2, PAUSE, CHANCE]], [STATE], "Leaves the parent State for `to` when all its Condition children hold. Action children run when it fires.";
    CONDITION = 35, "Condition", [&[INPUT, OP, VALUE, WITH]], [TRANSITION], "A test on a view-model Property.";
    ACTION_NODE = 38, "Action", [&[INPUT, ACTION, VALUE]], [TRANSITION, LISTENER], "Changes a view-model Property when the Transition or Listener it is in fires.";
    LISTENER = 36, "Listener", [NAMED, &[TARGET, EVENT, INPUT, ACTION, VALUE, EMIT, WITH, KEY_NAME]], [STATE_MACHINE], "Turns a pointer event on `target`, a key, or a change of a view-model Property, into changes of view-model Properties and an Event for the host.";
    EVENT_NODE = 37, "Event", [NAMED, &[ASSET, VOLUME]], [ARTBOARD], "A named signal reported to the host application. With an `asset` (an AudioAsset) the host also plays that sound each time the event is reported.";
    VIEW_MODEL = 40, "ViewModel", [NAMED], [ARTBOARD], "The artboard's data contract with the host: a set of typed Properties.";
    PROPERTY_NODE = 41, "Property", [NAMED, &[VALUE_TYPE, VALUE, COLOR_VALUE, TEXT_VALUE]], [VIEW_MODEL], "A typed value the host, listeners and state machines read and write.";
    OPTION = 43, "Option", [NAMED], [PROPERTY_NODE], "One of the values an `enum` Property can take, by name.";
    BIND = 42, "Bind", [&[SOURCE, PROPERTY, MUL, ADD, INTO, DIRECTION, FORMAT, DECIMALS]], [NODE, SHAPE, RECTANGLE, ELLIPSE, POLYGON, STAR, PATH, FILL, STROKE, LINEAR_GRADIENT, RADIAL_GRADIENT, GRADIENT_STOP, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL, SHADER_PAINT, CONSTRAINT, BONE, RUN, LAYOUT], "Writes a view-model Property into `property` of the parent object every frame (or `into` a nested artboard's Property).";
    IMAGE_ASSET = 50, "ImageAsset", [NAMED, SIZED, &[BYTES]], [], "An encoded image (PNG, JPEG, WebP). The host decodes it.";
    FONT_ASSET = 51, "FontAsset", [NAMED, &[BYTES, GLYPHS]], [], "A TTF or OTF font. `glyphs` keeps the file small.";
    MESH_ASSET = 52, "MeshAsset", [NAMED, &[BYTES]], [], "A 3D mesh in Zuri's packed format. Produced by `zuri import`.";
    AUDIO_ASSET = 54, "AudioAsset", [NAMED, &[BYTES]], [], "An encoded sound (WAV, MP3, OGG...). The host decodes and plays it when an Event that names it is reported; the runtime itself makes no sound.";
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
    (ARTBOARD..=SHADER_PAINT).contains(&ty) || matches!(ty, CONSTRAINT | BONE | RUN | LAYOUT)
}
/// Objects that are drawn in their own right, in sibling order (the rest paint or shape their parent).
pub fn is_drawable(ty: u16) -> bool {
    matches!(ty, NODE | SHAPE | IMAGE | TEXT_NODE | NESTED_ARTBOARD | MODEL | BONE | LAYOUT)
}
/// Objects with x / y / rotation / scale.
pub fn has_transform(ty: u16) -> bool {
    matches!(ty, NODE | SHAPE | RECTANGLE | ELLIPSE | POLYGON | STAR | PATH | IMAGE | TEXT_NODE | NESTED_ARTBOARD | MODEL | BONE | LAYOUT)
}
pub fn is_geometry(ty: u16) -> bool {
    matches!(ty, RECTANGLE | ELLIPSE | POLYGON | STAR | PATH)
}

/// Names are resolved per class, so a State and an Animation may share a name.
/// 0 = drawable object, 1 = State, 2 = Animation, 3 = Property, 4 = Event, 5 = asset, 6 = Artboard,
/// 7 = Option (nothing refers to one, so two enums may use the same names).
pub fn ref_class(ty: u16) -> u8 {
    match ty {
        STATE => 1,
        ANIMATION => 2,
        PROPERTY_NODE => 3,
        EVENT_NODE => 4,
        IMAGE_ASSET | FONT_ASSET | MESH_ASSET | SHADER_ASSET | AUDIO_ASSET => 5,
        ARTBOARD => 6,
        OPTION => 7,
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
        (TEXT_NODE | RUN, ASSET) => &[FONT_ASSET],
        (MODEL, ASSET) => &[MESH_ASSET],
        (SHADER_PAINT, ASSET) => &[SHADER_ASSET],
        (EVENT_NODE, ASSET) => &[AUDIO_ASSET],
        (LISTENER, TARGET) => &[NODE, SHAPE, TEXT_NODE, IMAGE, MODEL, BONE, LAYOUT],
        (CLIP_NODE, SOURCE) => &[SHAPE, TEXT_NODE, IMAGE, MODEL, LAYOUT],
        (CONSTRAINT, TARGET) => &[NODE, SHAPE, IMAGE, TEXT_NODE, NESTED_ARTBOARD, MODEL, BONE, LAYOUT],
        (TEXT_NODE, FOLLOW) => &[SHAPE],
        (SKIN, TARGET) => &[BONE],
        _ => &[],
    }
}
/// Hosts play and set these by name, so a name may be used once per artboard (Artboards: once per file).
pub fn host_named(ty: u16) -> bool {
    matches!(ty, ARTBOARD | ANIMATION | STATE_MACHINE | PROPERTY_NODE | EVENT_NODE)
}
pub const CLASS_NAMES: [&str; 8] = ["drawable object", "State", "Animation", "view-model Property", "Event", "asset", "Artboard", "Option"];

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
