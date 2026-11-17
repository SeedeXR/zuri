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
- Letters are kerned with the font's own pairs, also across Runs in the same font; a
  subset font keeps the kerning of the glyphs it keeps.
- **Text on a path**: `"follow": "ring"` sets a Text along the outline of the Shape named
  `ring`, its baseline on the line and each letter turned to it. `position` (0..1) is
  where along the outline the text starts, or with `"align": "center"` / `"right"` where
  it is centered or ends; animate it to send the text along the path. A closed outline
  wraps; letters past the end of an open one are not drawn. `width` does not wrap it,
  and a second line runs beside the first at the same places along the path (so on a
  tight curve its letters crowd or spread). Mirrored, the text stays on its side of the
  line. See [`examples/badge.json`](../../examples/badge.json).
- A **Text** in one style has `text`, `asset`, `size` and `weight`. For several styles,
  give it **Run** children, each with its own `text`, `size` and `weight`:

  ```json
  { "type": "Text", "asset": "Outfit", "width": 240, "children": [
    { "type": "Fill", "color": "#c9d1e6" },
    { "type": "Run", "size": 17, "text": "Laid out " },
    { "type": "Run", "size": 17, "weight": 700, "text": "at run time", "children": [
      { "type": "Fill", "color": "#7af0c8" } ] } ] }
  ```

  The Runs are set one after another as one text: lines wrap between and inside them,
  and a line is as tall as the largest size on it. A Run with a Fill or Stroke of its own
  is painted by it, on top; the others by the Text's. A Run is set in the Text's font
  unless it names an `asset` of its own. Its `size` and `weight` are its own (16 and 400
  unless it says otherwise: it does not take the Text's), and the Text's `text` is not
  shown once it has Runs. A Run's `text` can be bound and its `size` and `weight`
  animated, like any property.

## Layout

A **Layout** arranges what is inside it in a row or a column, like a CSS flex container,
and can paint its own box:

```json
{ "type": "Layout", "name": "button", "padding": 9, "gap": 6, "radius": 8, "alignItems": "center", "children": [
  { "type": "Fill", "color": "#4f7cff" },
  { "type": "Shape", "children": [ ... an icon ... ] },
  { "type": "Text", "asset": "Outfit", "text": "Open", "size": 13, "children": [ ... ] } ] }
```

- Its children that are drawn (Shapes, Texts, Images, Models, NestedArtboards, Nodes and
  other Layouts) stand one after the other along `flow` (`row`, the default, or
  `column`), `gap` apart, `padding` in from the edge (`paddingLeft`, `paddingTop`,
  `paddingRight` and `paddingBottom` add to it on one side: `"padding": 6, "paddingLeft": 8`
  is 14 on the left and 6 elsewhere). Each takes the room of its own
  box: what it draws, turned and scaled as it is. A Text takes the room of its lines
  (so a label keeps its height whatever its letters are), and one set along a path
  takes none. A Layout starts at its top-left corner.
- With `width` (or `height`) 0 a Layout is as large as what it holds, so a button fits
  its label and grows when the label does. With a size of its own there may be room to
  spare along the flow: `justify` puts the children at the `start`, the `center` or the
  `end` of it, or spreads them out (`spaceBetween`). Across the flow, `alignItems` puts
  each at the `start`, `center` or `end`.
- `minWidth`, `maxWidth`, `minHeight` and `maxHeight` hold a Layout's size, whatever
  it holds and whatever `grow` or `stretch` would give it: with `"minWidth": 100` a
  Layout that fits 80 of content is 100 wide and has 20 to spare for `justify`; with
  `"maxWidth": 50` one that grows stops at 50 (the room it leaves is not handed to the
  others that grow). Where a `max` is under its `min`, the `max` wins, and a `max` of 0
  is 0: key `maxHeight` down to it to fold a Layout away. They hold the Layout's own
  box only: a Text inside keeps the width it has.
- With `"wrap": true`, a child that does not fit along the flow starts a further line:
  a row below (or, in a column, a column to the right), `gap` away. Each line is as
  tall as its tallest child, the lines start at the top (the left, in a column), and
  `justify`, `alignItems` and `grow` work line by line. The room is the Layout's own
  size along the flow, or the one its parent gives it with `grow` or `stretch` (a row
  of tags in a column that stretches it, say); a Layout that fits its content has one
  line, unless a `maxWidth` (`maxHeight`, in a column) stops it short.
- With `"scroll": true` a Layout shows its children inside its box only, and what does
  not fit along the flow can be scrolled into it: a list is a column with a `height`
  (or a `maxHeight`) and more rows than fit. A press inside the box takes hold of the
  content and a drag moves it, as far as it goes; `scrolled` is how far that is, in
  the Layout's units, and can be keyed or bound
  (`{ "type": "Bind", "source": "pos", "property": "scrolled", "direction": "twoWay" }`
  lets the host read where a drag left it, and set it:
  `scrolled` is always kept between 0 and what does not fit). The wheel scrolls it
  too, where the host passes it on (the web player, the Unity views and the Unreal
  widget do): the innermost Layout under the pointer that has further to go that way
  moves, and one that scrolls the other way around it has what is across. A Layout that wraps
  scrolls its lines instead (a row of chips scrolls down). A strip that scrolls
  sideways inside a list that scrolls down: a press on the strip holds both, and each
  follows the pointer its own way. What is scrolled out of the box cannot be pressed,
  rows that are NestedArtboards included.
- A tap is not a scroll, and a scroll is not a tap. The content stays where it is
  until the pointer has gone further than the drag threshold along the Layout (8 pixels
  of the surface until the host sets it; see the host guides), and from there
  on goes with it (nothing jumps). A press that has moved a Layout, in this artboard
  or in one nested at any level,
  is a scroll: from then until it is let go the pointer is on nothing, for every
  Listener of every artboard in the instance (an `up` Listener of what was pressed
  fires, what was hovered is left, nothing is clicked). The Layouts held go on
  following meanwhile, wherever the pointer goes. A press against the end of a list,
  which moves nothing, stays a press. A Layout whose `scrolled` is bound from a
  Property one way is where the Property says, and a press does not take hold of it.
  The scene has one pointer and one button, and the runtime keeps to them whatever the
  host has: a press begins when a primary button goes down and ends when it goes up or
  its pointer leaves, and meanwhile no other pointer (a second finger, a mouse beside a
  finger) is the scene's, nor is another button. The host reports, for every press, its
  release or the pointer leaving.
  Let go, the pointer is on what is under it again. Against an end of the list the
  threshold is not spent: a press there that comes a little way back is still a press.
- Not there: momentum and a spring at the ends; and a list inside a nested artboard
  moves together with one around it that scrolls the same way, where the inner one
  should go first. A `scrolled` that is keyed is put back by the key every frame, and
  a press that goes along such a Layout is lost to what is in it. See
  [`examples/list.json`](../../examples/list.json).
- A child with `"absolute": true` takes no room and is not placed: it stays where its
  own `x` and `y` put it, from the Layout's corner, and does not go with what scrolls
  (a badge on a card, a header over a list). A Layout that scrolls still cuts it to
  its box, and a press on it there can still drag the Layout. It is for a direct
  child of a Layout: anywhere else it is a compile error.
- A Layout inside a Layout can take the spare room: `grow` is its share of it (an empty
  Layout with `"grow": 1` is a spacer), and `"alignItems": "stretch"` on the parent
  makes it as large as the room across, whatever its own size. Only Layouts change
  size, and only upright ones (a turned Layout keeps its size); a Shape is as large as
  its geometry says, and nothing shrinks to fit.
- Fill and Stroke children paint the box (`radius` rounds its corners). It can be
  clicked (a Listener's `target`), clipped to (a Clip's `source`) and followed like a
  Shape. To cut off what sticks out of the box, give the Layout a Clip that names it:
  `{ "type": "Clip", "source": "card" }` inside `card`.
- Inside a Layout, the `x` and `y` of a child that is not `absolute` move it from the
  place the Layout gives it,
  without moving its neighbours: animate them for a nudge on hover.
- See [`examples/card.json`](../../examples/card.json) and, for wrapping,
  [`examples/chips.json`](../../examples/chips.json).

## Animation

```json
{ "type": "Animation", "name": "bounce", "fps": 60, "duration": 60, "loop": "pingpong", "children": [
  { "type": "Track", "target": "ball", "property": "y", "children": [
    { "type": "Key", "frame": 0, "value": 90, "interp": "cubic", "easeX1": 0.55, "easeY1": 0, "easeX2": 1, "easeY2": 0.45 },
    { "type": "Key", "frame": 60, "value": 210 } ] } ] }
```

- A **Track** animates one property of one object. Any number or color property can be
  animated, including gradient stops, trim values, 3D rotation, shader parameters and font weight.
  The property has to be one the `target` has, and one that holds a value: a Stroke
  has no `opacity` (its Shape has), and an `asset` names another object. The compiler
  says so and lists what the object does have.
- `duration` is in frames; `loop` is `oneshot`, `loop` or `pingpong`.
- A **Key**'s `interp` governs the segment that starts at it: `linear`, `hold`,
  `cubic` (CSS cubic-bezier control points; `easeY1` / `easeY2` outside 0..1 make it
  overshoot) or one of the elastic ones, which swing past the next key's value and
  back like a spring, each swing smaller, and are exactly there at the end:
  `{ "type": "Key", "frame": 0, "value": 0, "interp": "elasticIn", "amplitude": 2, "period": 0.3 }`.
  `elastic` swings on arriving, `elasticIn` on leaving, `elasticInOut` both (half the
  distance each way). `period` is how long one swing takes, as a part of the way to
  the next key (0.5 by default, at least 0.05); `amplitude` (1 or more) is how far it
  goes. A color does not go past the next key's: it waits there while the curve is beyond.
- `value` may be a number, a `#color`, or a string. Keying a Path's `d` morphs the path
  when both keys have the same commands.
- A **Cue** reports an Event when playback passes its frame:
  `{ "type": "Cue", "frame": 18, "emit": "landed" }`. It fires on every lap of a loop and
  in both directions of a pingpong, at most once per `advance`, and not when time runs
  backwards (a negative `speed`) or when the host seeks. In a state machine the current
  state reports its cues (in a blend state, the animations that have weight); a state
  that is being faded out is silent.
- A **Joystick** poses the scene with a handle:
  `{ "type": "Joystick", "name": "look", "xAnimation": "lookAcross", "yAnimation": "lookDown" }`.
  Its `handleX` and `handleY` go from -1 to 1, and each says how far into its animation
  the scene is: -1 is the animation's first frame, 0 its middle, 1 its last (beyond
  them the first or last key holds). One axis is enough. `"invertX": true` or
  `"invertY": true` runs that axis the other way. Move the handle by keying
  `handleX` and `handleY` in the animation that plays, or with a Bind inside the
  Joystick (`{ "type": "Bind", "source": "gaze", "property": "handleX" }`) so the host
  steers it through the view model.
  - The animations a joystick scrubs are poses: nothing plays them unless asked to by
    name, and one that is being played by name is left to play (so it can be watched
    and edited). They cannot key a joystick's handle.
  - What a joystick poses is applied after the state machine or animation that is
    playing and after the binds, in document order, and the Cues of its animations are
    not reported. Keep the properties it poses to itself: one that the playing
    animation also keys is written twice a frame (the joystick wins), and such a scene
    never comes to rest.
  - See [`examples/joystick.json`](../../examples/joystick.json).

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
  Omit `target` to listen on the whole artboard. An `up` Listener also fires when the
  pointer leaves the surface in the middle of a press that began on its `target`, so
  that what `down` set does not stay set; that was the release, and it is not told
  again if the pointer comes back and is let go.
- A pointer event is for everything under the pointer: every Listener whose `target`
  is there hears it, and so does every nested artboard there. `"opaque": true` on
  something drawn (Rive's "Opaque Target") keeps the pointer from what is drawn under
  it: where the pointer is on it, the Listeners of what is under hear the pointer leave
  (a press of theirs that is open stays open), a nested artboard under it has none of
  the pointer, and a Layout under it is not scrolled, by a drag or by the wheel: not
  even the Layout it is in (a header over a list). On a group it is that of everything
  in the group; on a NestedArtboard, of its box; inside a nested artboard it shields
  what the artboard around has under that. What is not drawn shields nothing (an
  `opacity` of 0, what an artboard cuts off at its box). A Listener hears a press on
  what is opaque in its own `target`, and one without a `target` hears regardless.
- What an artboard cuts off at its box (it clips unless it says `"clip": false`) is
  not there to be touched: outside the box a Listener's `target` is not hit, nor is
  anything in an artboard nested there, and a Layout is not scrolled.
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
  drive, which has to have that property (the compiler refuses one it does not have, or
  one that names another object and holds no value).
  Numbers can be scaled: `"mul": 3.6, "add": 0`, kept in a range after
  that (`"mul": 2, "add": 10, "min": 20, "max": 100` turns 0..60 into 10..130 and holds
  it within 20..100), and made whole last of all: `"round"` is `nearest` (a half goes
  away from 0), `down` or `up`. `"mul": -1, "add": 1` turns a Bool round. A Bind that
  writes to its source, or goes both ways, hands the value on as it is.
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
{ "type": "ShaderAsset","name": "plasma", "wgsl": "fn shade(uv: vec2f) -> vec4f { ... }" },
{ "type": "AudioAsset", "name": "click",  "bytes": "@assets/click.wav" }
```

**Sound.** An Event with an `asset` plays that AudioAsset each time it is reported:
`{ "type": "Event", "name": "tapped", "asset": "click", "volume": 0.8 }` (`volume` is
0..1, 1 by default). The runtime makes no sound itself: it tells the host which sound and
how loud, and the host decodes and plays it. The web player does (any format the browser
decodes: WAV, MP3, OGG, AAC; a browser plays nothing until the person has touched the
page or pressed a key). In Unity the player plays it through an `AudioSource`, and the sound
must be an uncompressed WAV file. In Unreal, importing the file makes a Sound Wave of each
sound and the instance plays it in a game world (WAV is tested; other formats are left to
Unreal's importer). Each host can be muted. WAV is the one format all three play.
An AudioAsset is embedded whole, so keep sounds short. See
[`examples/sound.json`](../../examples/sound.json).

A font is embedded whole unless its FontAsset has `glyphs`. With it, the build keeps only
the glyphs for the characters the document's own texts use, plus the characters listed:
`{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf", "glyphs": "0123456789" }`
(`"glyphs": ""` keeps only the document's). `examples/text.json` goes from 111 KB to 30 KB
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
  rotation is copied as the number the target's own `rotation` makes it (350 is 350,
  not -10, and two turns are 720), so limits and a `factor` see it turn steadily; a
  target whose parent spins, or is mirrored, still reads between -180 and 180 and
  makes a limited or slowed follower jump when it passes 180.
  The other kinds of Constraint do not read `axis`, `min` or `max`.
- `factor` multiplies what a `translation`, `rotation` or `scale` Constraint copies:
  `"factor": 0.5` follows at half the rate (a far layer in a parallax), `-1` goes the
  other way (a gear turning against its neighbour). With `"offset": true` the object's
  own `x` / `y` (or `rotation`) is added to what is copied, and its own scale
  multiplied in (a flipped object stays flipped), so it sits that far from the target
  instead of on it. The limits apply to what comes out:
  `{ "type": "Constraint", "constrain": "translation", "target": "leader", "factor": 2, "max": 100 }`
  goes twice as far as `leader` and stops at 100. A `transform` Constraint copies
  everything as it is and reads neither.
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
  the first state machine plays, or failing that the first animation (that no Joystick scrubs).
- A **Bind** with `into` writes a view-model Property of this artboard into the nested
  artboard's Property of that name, every frame. The two must have the same `valueType`.
  A trigger fired here fires there in the same frame.
- The host sets the outer artboard's properties and the data flows down. It can also
  reach a nested artboard's own view model, by the NestedArtboard's `name` and a `/`:
  with the example's Meter nested as `cpuMeter`, `cpuMeter/level` is that Meter's
  `level` to every call that takes a property name (set, get, fire), and
  `card/badge/count` goes two levels down. The host can list what is there (see each
  wrapper's docs). A Property that a Bind writes `into` is
  written again every frame, so set those through the outer artboard. Because of the
  `/`, a Property's or a NestedArtboard's name cannot have one in it, and two
  NestedArtboards of one artboard cannot share a name.

### Lists

A NestedArtboard with a `count` is a list: that many copies of the artboard, one after
the other along its `flow` (`row`, the default, or `column`) with its `gap` between them.

```json
{ "type": "NestedArtboard", "name": "rows", "artboard": "Row", "flow": "column", "gap": 8, "children": [
  { "type": "Bind", "source": "items", "property": "count" } ] }
```

- `count` is a number like any other: write it, key it, or bind it to a number Property
  as above, and the host sets how long the list is. A part of one is none (2.9 is two
  copies), and less than none is none.
- An instance holds 4096 nested instances in all, at every level together. A list
  stops growing there and grows again when another gives up room; the lists take what
  there is in the order they are written, and a copy made as the room runs out may
  lack what is nested in it. When a file is instantiated, a list that does not fit is
  an error.
- A Bind of `count` from a Property is what counts, whatever `count` is written beside
  it. (One that writes back, `toSource` or `twoWay`, is followed at an advance like
  any other.)
- Each copy is a live instance with a view model of its own. The host reaches it by the
  copy's position, from 0: `rows/2/title` is `title` in the third Row, and `rows/title`
  is the first one's. That is where the items' data goes; a Bind with `into` writes the
  same value into every copy.
- A copy can know which one it is: `{ "type": "Bind", "into": "number", "index": true }`
  in the NestedArtboard writes each copy's position (from 0) into its number Property
  `number`, in place of a `source`. What the row binds to that Property (a label with
  `"add": 1`, a colour picked by a state machine, a delay) then differs from row to
  row with no host involved. Like anything bound `into` a copy it is written every
  frame. A `change` Listener in the row hears it at the first frame where it differs
  from the Property's own value in the document: give that a value no row has (`-1`)
  for every row to hear its position.
- One Bind writes a value into a Property of a copy: two Binds `into` the same one are
  refused (two triggers into one trigger are not: either fires it). And `mul`, `add`,
  `min`, `max`, `round`, `direction`, `format` and `decimals` shape what a Bind gives a
  `property`; set on a Bind without one they are refused (shape the value in the copy,
  where it is bound to something).
- When the list gets longer the copies that are there keep what they hold and the new
  ones start as the document has them; when it gets shorter the last ones go.
- A new instance has the list at the length the document gives it, also through a Bind
  (`items` is 3 in the example, so `rows/2/title` is there at once), and a list whose
  `count` is bound follows the Property as soon as the host sets it: after setting
  `items` to 6 it can write `rows/5/title` straight away. That holds further down too,
  where the Property is bound `into` a component whose list is as long as it says. A
  `count` that changes while the scene runs (a key, an Action, a Listener) is followed
  at that advance.
- The copies stand from the NestedArtboard's own place, each as wide (or as tall) as
  that copy's artboard is. Inside a `Layout` the whole list takes room as one child, so
  a Layout that scrolls around it is a scrolling list.
- An Event that a copy reports does not say which copy: let the row set a Property of
  its own (`chosen` in the example) and read `rows/2/chosen`.
- Because a number in a path is a position, a NestedArtboard's name cannot be one.

[`examples/rows.json`](../../examples/rows.json) is a scrolling list of rows that are
chosen with a click, each with its own number: `zuri render examples/rows.json --set
items=6 --set rows/1/title=Second --set rows/2/chosen=true -o rows.png`.

The full example of components is [`examples/components.json`](../../examples/components.json).

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
See [feature parity](../parity.md) for what is not implemented yet (meshes, scrolling,
audio, scripting, and more).
