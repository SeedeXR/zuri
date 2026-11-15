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
- `feather` on a paint blurs its edge: use it for glows and soft shadows.
- A stroke can be trimmed (`trimStart`, `trimEnd`, `trimOffset`) to draw part of the path,
  and dashed (`dash`, `gap`, `dashOffset`).
- A **Node** groups: it transforms and fades everything inside it.
- A **Clip** child clips its parent to another shape: `{ "type": "Clip", "source": "mask" }`.
- An **Artboard**'s own `Fill` children paint its background; `clip` (default true) clips to its bounds.
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
- A **Listener** turns a pointer event on a `target` (`enter`, `exit`, `down`, `up`,
  `click`, `move`) into a change of a view-model property (`set`, `toggle`, `fire`, `add`).
  Omit `target` to listen on the whole artboard.
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

## Assets

```json
{ "type": "FontAsset",  "name": "Outfit", "bytes": "@assets/Outfit.ttf" },
{ "type": "MeshAsset",  "name": "rings",  "bytes": "@assets/rings.glb" },
{ "type": "ImageAsset", "name": "logo",   "bytes": "@assets/logo.png", "width": 256, "height": 256 },
{ "type": "ShaderAsset","name": "plasma", "wgsl": "fn shade(uv: vec2f) -> vec4f { ... }" }
```

`"@path"` embeds a file at build time, relative to the JSON file. A 3D file (`.obj`,
`.gltf`, `.glb`, `.fbx`) referenced this way is converted to Zuri's mesh format by the
build. Without `@`, `bytes` is base64. Assets live at the top level and are shared by
all artboards.

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
