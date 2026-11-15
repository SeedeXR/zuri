# The document format

A Zuri document is JSON. `zuri build` compiles it to a compact binary `.zur`; `zuri dump`
turns a `.zur` back into JSON. This page explains the model. The exhaustive list of types
and properties is the [schema reference](../reference/schema.md) (`zuri schema`).

## The one rule

```json
{ "type": "Shape", "name": "ball", "x": 200, "y": 150, "children": [ ... ] }
```

Every object is a `type`, its properties, and `children`. To refer to another object,
write its `name`. That is the whole syntax.

The document itself is `{ "zuri": 1, "children": [ ...Artboards and assets... ] }`.

Conventions: positions are in artboard units with **y down**; an artboard's origin is its
top-left corner; shapes are **centered on their position**; rotation is in **degrees,
clockwise**; colors are `#rrggbb` or `#rrggbbaa`; later children draw **on top**.

## Drawing

```json
{ "type": "Shape", "name": "badge", "x": 100, "y": 100, "rotation": 10, "opacity": 0.9, "blendMode": "screen", "children": [
  { "type": "Rectangle", "width": 120, "height": 60, "radius": 12 },
  { "type": "Fill", "children": [
    { "type": "LinearGradient", "x0": -60, "y0": 0, "x1": 60, "y1": 0, "children": [
      { "type": "GradientStop", "position": 0, "color": "#7af0c8" },
      { "type": "GradientStop", "position": 1, "color": "#5b8cff" } ] } ] },
  { "type": "Stroke", "color": "#ffffff", "thickness": 2, "cap": "round", "join": "round" } ] }
```

- A **Shape** combines geometry children (`Rectangle`, `Ellipse`, `Polygon`, `Star`,
  `Path`) and paints them with its `Fill` and `Stroke` children, in order.
- A **Path** takes SVG path data: `{ "type": "Path", "d": "M-18 0 L-6 12 L18 -12" }`.
- A paint is a `color`, or has a `LinearGradient`, `RadialGradient` or `ShaderPaint` child.
- `feather` on a paint blurs its edge: use it for glows and soft shadows. `featherX` /
  `featherY` move the feathered paint (a drop shadow's offset). On a Fill,
  `"featherInner": true` feathers inward instead: the paint hugs the inside of the
  outline and fades toward the middle (an inner shadow; the offset says which edge it
  leans on).
- A stroke can be trimmed (`trimStart`, `trimEnd`, `trimOffset`) to draw part of the path,
  and dashed (`dash`, `gap`, `dashOffset`). With several paths in one shape, `trimMode`
  `sequential` (the default) trims them as one long path and `synced` trims each by the
  same fraction.
- A **Node** groups: it transforms and fades everything inside it. With `solo` it draws
  only one child (1 = the first, 2 = the second; 0 = all), and the hidden ones take no
  pointer input. Key `solo` to switch between children: whole numbers step, they do not
  glide.
- Siblings draw in document order, later ones on top. `order` rearranges them (higher is
  on top) and can be animated; the pointer hits what is drawn on top. `order` is a
  number like any other: in a cross-fade it moves gradually, and two siblings swap when
  their values cross.
- `blendMode` on a Shape: `multiply`, `screen`, `overlay`, `darken`, `lighten`,
  `colorDodge`, `colorBurn`, `hardLight`, `softLight`, `difference`, `exclusion`, `hue`,
  `saturation`, `color`, `luminosity`.
- A **Clip** child clips its parent to another shape: `{ "type": "Clip", "source": "mask" }`.
- An **Artboard**'s own `Fill` children paint its background; `clip` (default true) clips to its bounds.
- An **Image** can be nine-sliced: `"sliceLeft"`, `"sliceTop"`, `"sliceRight"` and
  `"sliceBottom"` say how many image pixels at each side keep their size. Corners then
  stay as they are, edges stretch along their length and the middle fills the rest, so
  one small picture makes a panel or a button of any size.
- Also drawable: `Text` (needs a `FontAsset`), `Image` (`ImageAsset`), `Model` (a 3D
  `MeshAsset`), `NestedArtboard` (an instance of another artboard).

## Animation

```json
{ "type": "Animation", "name": "bounce", "fps": 60, "duration": 60, "loop": "pingpong", "children": [
  { "type": "Track", "target": "ball", "property": "y", "children": [
    { "type": "Key", "frame": 0, "value": 90, "interp": "cubic", "easeX1": 0.55, "easeY1": 0, "easeX2": 1, "easeY2": 0.45 },
    { "type": "Key", "frame": 60, "value": 210 } ] } ] }
```

- A **Track** animates one property of one object. Any number or color property can be
  animated, including gradient stops, trim values, 3D rotation, shader parameters and font weight.
- `duration` is in frames; `loop` is `oneshot`, `loop` or `pingpong`.
- A **Key**'s `interp` (`linear`, `hold`, `cubic`) governs the segment that starts at it.
  `cubic` uses CSS cubic-bezier control points.
- `value` may be a number, a `#color`, or a string. Keying a Path's `d` morphs the path
  when both keys have the same commands.
- A **Cue** reports an Event when playback passes its frame:
  `{ "type": "Cue", "frame": 18, "emit": "landed" }`. It fires on every lap of a loop and
  in both directions of a pingpong, at most once per `advance`, and not when time runs
  backwards (a negative `speed`) or when the host seeks. In a state machine the current
  state reports its cues (in a blend state, the animations that have weight); a state
  that is being faded out is silent.

## State machines

A state machine decides which animations play, in response to data.

```json
{ "type": "StateMachine", "name": "Button", "children": [
  { "type": "Layer", "name": "interaction", "children": [
    { "type": "State", "name": "entry", "kind": "entry", "children": [ { "type": "Transition", "to": "idle" } ] },
    { "type": "State", "name": "idle", "animation": "idle", "children": [
      { "type": "Transition", "to": "hovered", "blend": 0.15, "children": [
        { "type": "Condition", "input": "hover", "op": "eq", "value": 1 } ] } ] },
    { "type": "State", "name": "hovered", "animation": "hover", "children": [ ... ] } ] },
  { "type": "Listener", "target": "body", "event": "enter", "input": "hover", "action": "set", "value": 1 },
  { "type": "Listener", "target": "body", "event": "exit",  "input": "hover", "action": "set", "value": 0 } ] }
```

- **Layers** run at the same time; later layers override earlier ones on the same property.
- **States**: `animation` (plays one), `entry` (where a layer starts), `any` (its
  transitions apply from every state), `exit`, `blend` (mixes its `BlendAnimation`
  children along a number `input`).
- A **Transition** fires when all its **Conditions** hold. `blend` is the cross-fade in
  seconds. `exitTime` (0..1) waits for that fraction of the source animation first.
  A trigger condition is true in the frame the trigger fired, and each trigger drives at
  most one transition per layer per frame.
  During a cross-fade the state being left keeps playing underneath. A transition that
  fires while a cross-fade is on screen fades from the picture as it is at that moment:
  the two states that were mixing stop there, and what the new state does not animate
  stays where it stopped.
- A cross-fade is linear unless the Transition says otherwise: `"interp": "cubic"` with
  `easeX1` .. `easeY2` eases it, `"interp": "hold"` switches at the end. `"pause": true`
  freezes the state being left for the length of the fade.
- **Action** children of a Transition (or a Listener) change view-model properties when
  it fires: `{ "type": "Action", "input": "score", "action": "add", "value": 1 }`. A
  trigger fired by a Transition's Action fires on the next frame, for every layer.
- A **Condition** compares its `input` with `value`, or with another Property:
  `{ "type": "Condition", "input": "health", "op": "lt", "with": "threshold" }`.
- `chance` makes a choice random: when the first Transition that could fire has a
  `chance`, one is drawn from those that could fire and have one, in proportion (1 and 3
  = one time in four, three in four). The draw is repeatable: the same file and the same
  calls give the same sequence.
- A `direct` State mixes its **BlendAnimation** children independently: it starts from
  the design values (what the document says, before any animation) and lays each
  animation over them in order, at its own weight: `value`, or a number Property named
  by `input` (0..1). Like any state it replaces what earlier layers set on the
  properties it animates, so put a base animation first, at weight 1, if there is one.
- A **Listener** turns a pointer event on a `target` (`enter`, `exit`, `down`, `up`,
  `click`, `move`) into a change of a view-model property (`set`, `toggle`, `fire`, `add`).
  Omit `target` to listen on the whole artboard.
- A Listener with `"event": "keyDown"` (or `keyUp`) reacts to a key the host reports:
  `{ "type": "Listener", "event": "keyDown", "key": "Enter", "input": "submit", "action": "fire" }`.
  `key` is the key's name as the web's `KeyboardEvent.key` gives it (`"Enter"`,
  `"Escape"`, `"ArrowLeft"`, `" "` for space, or the character typed); leave it out to
  react to any key. Nested artboards hear keys too.
- `emit` on a Transition or Listener reports a named **Event** to the host.

## Data binding

The **view model** is the contract between a scene and the app that hosts it.

```json
{ "type": "ViewModel", "name": "vm", "children": [
  { "type": "Property", "name": "hover",  "valueType": "bool",    "value": false },
  { "type": "Property", "name": "clicks", "valueType": "number",  "value": 0 },
  { "type": "Property", "name": "accent", "valueType": "color",   "value": "#4f7cff" },
  { "type": "Property", "name": "title",  "valueType": "string",  "value": "Zuri" },
  { "type": "Property", "name": "pulse",  "valueType": "trigger" } ] }
```

- The host reads and writes properties by name (`setColor('accent', ...)`).
- Conditions test them; listeners change them.
- A **Bind** copies a property into a scene property every frame:
  `{ "type": "Bind", "source": "accent", "property": "color" }` as a child of the object to
  drive. Numbers can be scaled: `"mul": 3.6, "add": 0`.
- A number bound to a text is written with `decimals` digits, inside `format` if there
  is one: `{ "type": "Bind", "source": "score", "property": "text", "format": "Score: {}" }`.
- `direction` turns a Bind around. `toSource` copies the scene property into the
  Property, which is how the host (or a Condition) reads an animated value. `twoWay`
  copies whichever side changed since the last frame; if both did, the Property wins.
  Animation runs before binds, so a value an animation keys comes back on the next frame.
  A Bind that writes back needs a Property that can hold the value: a number or bool
  for a number, a color for a color, a string for a text (not an enum or a trigger).
- An **enum** Property holds one of its Options:
  `{ "type": "Property", "name": "mode", "valueType": "enum", "value": 0, "children": [
  { "type": "Option", "name": "idle" }, { "type": "Option", "name": "busy" } ] }`. It is a
  number (the Option's position, from 0) wherever numbers are used: Conditions, Actions,
  binds to numbers. Bound to a text it is the Option's name, and the host can set and
  read it by name. Its `value` may be a position or a name (`"value": "busy"`), and it
  never leaves its Options: a host's out-of-range value is refused, an Action's is
  clamped. Passed `into` a nested artboard, both enums must list the same Options.
- A Listener with `"event": "change"` and `"with": <a Property>` runs when that Property
  gets a new value (or, for a trigger, fires): it can change other Properties with
  Actions and `emit` an Event. That is how a scene tells the host that its data changed.
  It runs once a frame, before the layers; a trigger it fires is heard on the next frame.

## Assets

```json
{ "type": "FontAsset",  "name": "Outfit", "bytes": "@assets/Outfit.ttf" },
{ "type": "MeshAsset",  "name": "rings",  "bytes": "@assets/rings.glb" },
{ "type": "ImageAsset", "name": "logo",   "bytes": "@assets/logo.png", "width": 256, "height": 256 },
{ "type": "ShaderAsset","name": "plasma", "wgsl": "fn shade(uv: vec2f) -> vec4f { ... }" }
```

A font is embedded whole unless its FontAsset has `glyphs`. With it, the build keeps only
the glyphs for the characters the document's own texts use, plus the characters listed:
`{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf", "glyphs": "0123456789" }`
(`"glyphs": ""` keeps only the document's). `examples/text.json` goes from 111 KB to 24 KB
this way. Text the host sets at run time can only show characters that were kept, so
list what it may need. Variable fonts stay variable. Fonts with CFF outlines (most
`.otf`) are embedded whole for now. The subsetting is done by
`zuri build` and the editor's export; compiling in the browser (`Runtime.compile`)
embeds the whole font. The built file no longer says `glyphs`: a `.zur` that is dumped
and rebuilt keeps the font it has.

`"@path"` embeds a file at build time, relative to the JSON file. A 3D file (`.obj`,
`.gltf`, `.glb`, `.fbx`) referenced this way is converted to Zuri's mesh format by the
build. Without `@`, `bytes` is base64. Assets live at the top level and are shared by
all artboards.

## Constraints

A **Constraint** child makes its parent follow another object, after animation has run:

```json
{ "type": "Node", "name": "eye", "children": [
  { "type": "Constraint", "constrain": "distance", "target": "socket", "distance": 6 },
  { "type": "Shape", "children": [ ... ] } ] }
```

- `constrain`: `translation`, `rotation`, `scale` or `transform` (all three) copy that
  part of the target's place in the artboard; `distance` keeps the object within
  `distance` of the target (`"limit": "closer"`, the default), outside it (`further`) or
  exactly on it (`exact`); `followPath` puts it at `position` (0..1) along the outline
  of a target Shape, facing along the path unless `"orient": false`.
- A `translation` or `scale` Constraint takes both axes, or one with `"axis": "x"` /
  `"y"`: a shadow that follows an object's x and stays on the ground. `min` and `max`
  limit what a `translation`, `rotation` or `scale` Constraint takes from its target: a
  position in artboard units, a rotation in degrees, or a size (a mirrored target stays
  mirrored). With two Constraints, one per axis, each axis gets limits of its own. A
  rotation is read between -180 and 180, so keep rotation limits inside that range; a
  target that turns through 180 carries a limited follower to the other limit at once.
  The other kinds of Constraint do not read `axis`, `min` or `max`.
- `strength` (0..1) blends the constraint in. It and `position` can be animated or bound,
  like any number: key `position` from 0 to 1 to send something along a path.
- A **Bone** is a joint of a skeleton. It carries what is inside it, as a Node does, and a
  Bone inside a Bone starts at its parent's far end (`length` along its x axis), so a
  chain is written as bones inside bones. Hang Shapes or Images on bones for a cut-out
  character.
- `"constrain": "ik"` on a Bone turns that bone and its parents (`bones` of them in all,
  2 by default) so that its far end reaches the `target`: put it on the forearm, point it
  at a hand target, and the elbow bends. Out of reach, the chain points straight at the
  target. The elbow bends to the side it bends to in the animated pose, and the answer is
  exact, so the same pose and target always give the same picture. A Bone can be faded
  (`opacity`), reordered (`order`), clipped and listened to like a Node.
  See [`examples/arm.json`](../../examples/arm.json).
- A **Skin** inside a Path binds the path to a Bone, so that the outline bends when the
  bone moves: `{ "type": "Skin", "target": "forearm", "weights": "1 1 0.5 0 0 0.5" }`.
  Write one Skin per bone. `weights` has one number from 0 to 1 per point of the path, in
  the order of the path data: a line has one point, a curve three (its two handles, then
  its end). Write a skinned path with `M`, `L`, `C` and `Z`, where every pair of numbers
  is a point; the other commands are turned into those (`S`, `Q` and `T` into one curve
  each, an arc `A` into one curve per quarter turn), so their points are not the ones you
  wrote. A point held by several bones goes where each would take it, in proportion to
  their weights; a point no bone holds stays put, and so does a point that a keyed `d`
  adds beyond the weights. Without `weights` the bone holds every point alike. The path fits its bones in the pose the document's own values describe
  (`x`, `y`, `rotation`, `length`, before Constraints, animation and Binds), so draw it
  over the bones as they are written. See [`examples/tail.json`](../../examples/tail.json).
- Several constraints on one object apply in order. A constraint reads its target as it
  is once the objects before it in the document have been constrained; a target later in
  the document is read before its own constraints.

## Components (nested artboards)

A `NestedArtboard` is a live instance of another artboard. It runs its own state machine,
takes pointer input, and its events reach the host.

```json
{ "type": "NestedArtboard", "artboard": "Meter", "x": 20, "y": 20, "play": "logic", "children": [
  { "type": "Bind", "source": "cpu", "into": "level" },
  { "type": "Bind", "source": "ping", "into": "bump" } ] }
```

- `play` names the StateMachine or Animation of the nested artboard to run. Without it
  the first state machine plays, or failing that the first animation.
- A **Bind** with `into` writes a view-model Property of this artboard into the nested
  artboard's Property of that name, every frame. The two must have the same `valueType`.
  A trigger fired here fires there in the same frame.
- The host talks to the outer artboard only: set its properties and the data flows down.

The full example is [`examples/components.json`](../../examples/components.json).

## Names and references
A referenced name must be unique among objects of its kind within the artboard (a State
and an Animation may share a name). Assets and artboards are visible from everywhere.

Two more rules, both checked by the compiler:
- What a host finds by name is unique whether or not anything refers to it: Artboards in
  the file, and Animations, StateMachines, view-model Properties (across all of its
  ViewModels) and Events in an artboard.
- A reference must reach the right kind of object: an Image's `asset` is an ImageAsset
  (Text: FontAsset, Model: MeshAsset, ShaderPaint: ShaderAsset), a Listener's `target` is
  something the pointer can hit (Node, Shape, Text, Image, Model), and a Clip's `source`
  has an outline (Shape, Text, or the box of an Image or Model).

## The binary format, briefly
`.zur` is: the bytes `ZURI`, a major and minor version, a table of contents listing every
property key used with its storage type, then a flat list of objects. A runtime skips
properties and objects it does not know, so newer files still load in older runtimes
(within a major version). Compiling is deterministic: the same source always gives the
same bytes. Details: [`crates/zuri-core/src/format.rs`](../../crates/zuri-core/src/format.rs).

## Limits today
See [feature parity](../parity.md) for what is not implemented yet (bones and meshes,
constraints, layout, dashes, rich text, audio, scripting, and more).
