# Importing SVG, Lottie and 3D models

`zuri import <file>` converts other formats into Zuri JSON source that you can edit and
build. The editor's Assets tab calls the same code.

```bash
zuri import logo.svg  -o logo.json      # an Artboard of Shapes, in Nodes for the SVG's groups
zuri import wave.json                   # a Lottie animation -> wave.zuri.json: its layers, and an Animation of its keys
zuri import robot.glb -o robot.json     # a MeshAsset + an Artboard showing it, with a turntable animation
zuri build robot.json                   # -> robot.zur
```

## 3D models: OBJ, glTF / GLB, FBX

A model becomes a `MeshAsset` (Zuri's packed mesh) shown by a `Model` node:

```json
{ "type": "MeshAsset", "name": "rings", "bytes": "@assets/rings.glb" },
...
{ "type": "Model", "name": "model", "asset": "rings", "x": 200, "y": 200, "width": 300, "height": 300, "rotationX": 18 }
```

You do not have to run `import` for 3D: point a `MeshAsset`'s `bytes` at the original file
with `@path` and `zuri build` converts it, so the source keeps referring to the real asset.

A `Model` draws inside a `width` x `height` viewport centered on its position. Its
`rotationX` / `rotationY` / `rotationZ` (degrees) and `fov` are ordinary properties:
animate them, bind them to data, drive them from a state machine. The model composites in
order with the vector content around it and takes part in opacity and clipping.

What is imported: triangles (polygons are triangulated), node transforms (baked), normals
(generated when missing), per-material base color, vertex colors. Every model is centered
and scaled to a unit sphere so models frame the same way.

What is not imported yet: textures, skeletons and skinning, morph targets, the file's own
animations, cameras and lights. Lighting is one fixed key light plus ambient.

Verified with the same scene exported from Blender 5.3 as FBX, GLB and OBJ: the three
importers produce the same 1,646 triangles and the same picture
(`crates/zuri-import/tests/import.rs`).

| Format | Parser | Notes |
|---|---|---|
| `.obj` | built in | Colors from the `.mtl` next to it (`Kd`, `d`) |
| `.gltf`, `.glb` | `gltf` crate | Embedded, base64 and external buffers |
| `.fbx` | `ufbx` | Binary and ASCII; converted to right-handed, Y up, metres |

## SVG

Each SVG path becomes a `Shape` with a `Path`, `Fill` and `Stroke`; `id` attributes become
names; the artboard takes the SVG's size.

A group that says something (a `transform`, an `opacity`, an `id`) becomes a `Node` with
that transform as `x`, `y`, `rotation`, `scaleX` and `scaleY`, so that it can be keyed. An
element with a transform of its own, and what a `use` places, is one Shape that says it.
- A Node is at the point its transform leaves where it is, when that is near what it
  holds: a clock's hand drawn with `rotate(30 100 100)` arrives as a Node at (100, 100)
  turned 30 degrees, and turns about the clock's middle. Otherwise it is at the middle of
  what it holds; a Shape is at the middle of its points.
- A mirror is a `scaleX` below nothing (with half a turn, when it was drawn as a flip
  the other way).
- Under a large scale (a drawing one unit across) points and widths are written to more
  decimals, so that they are as fine on the screen.

What is imported: all basic shapes and paths (SVG's own rectangles, circles and so on
arrive as paths), transforms, fills and strokes with opacity, fill rules, caps and joins,
linear and radial gradients, group opacity, paint order.

What is not imported yet: clip paths, masks, filters, patterns (drawn grey), embedded
images, dashes, and text that has not been converted to outlines. A Node cannot skew: a
group with `skewX` / `skewY` (or a matrix that leans) keeps its name and opacity, and its
lean is put into the points of what it holds, groups included, which then have no
transform of their own to key.

## Lottie

A Lottie animation (the `.json` that Bodymovin and other tools write) becomes an Artboard
of its size with a `Node` for every layer, `Shape`s for what the layer draws, and one
`Animation`, named after the file, at the file's frame rate, that loops: a `Track` for
every property the file keys, each key with the file's easing (`cubic`, or `hold`).
A Lottie file is JSON itself, so without `-o` the source goes to `<name>.zuri.json`.

What is imported:

- Shape layers, solid layers, null layers (as parents) and precompositions with their
  start time. A layer is there from its in point to its out point, the first layer of
  the file on top. A parent hands on its transform and not its opacity.
- A transform is a Node at the position, turned, scaled and faded, with a Node inside
  that takes the anchor off: key either.
- Rectangles, ellipses, stars, polygons and paths, in groups with their transforms. A
  path that is keyed morphs (its `d` is keyed).
- Fills and strokes: color and opacity (as the color's alpha), fill rule, width, caps,
  joins, a dash and a gap, linear and round gradients with their opacity stops. A style
  paints the shapes before it in its group, and the first style is on top. A trim path
  cuts the shapes before it (those of the groups before it too): the strokes that
  paint them are trimmed.

What is not imported: the command says which of these a file has ("not imported: ...";
`notes` with `--json` and from the MCP tool).

- Image and text layers, masks, effects, expressions (the keys under them are kept),
  blend modes of layers, auto-orient, 3D layers (the turn about z is kept).
- Track mattes: the matte is left out, and what it cut is drawn whole, which can be a
  very different picture.
- Time stretch and time remapping of a precomposition; skew; a motion path's curve (the
  way from key to key is straight).
- Modifiers other than trim (rounded corners, repeaters, merges); a trim on a fill, or
  on some only of the shapes a stroke paints (all are trimmed); a star's roundness and
  keys on its radii; keys on a gradient's colors or on a round gradient's size; a dash
  pattern beyond one dash and gap.
- A precomposition that the file does not have; layers beyond 20,000 (a file that nests
  precompositions without end is cut short there).

And without a word from the command: a precomposition is not cut to its own size; a
style in a group around another group paints that group's shapes from inside it, under
what the group draws itself; `.lottie` (a zip) is not read: unzip it and import the
`.json` in it. The editor does not open Lottie files yet.

Checked against lottie-web 5.12.2 on five of its demo animations (2026-11-18, one or two
frames each, counting the pixels that differ visibly): 0.01 % for the one without masks
or mattes; 0.01 %, 0.11 % and 2.7 % for three that have masks (the last where its mask
is); 94 % for one that leans on mattes, which does not look like itself.

## Images and fonts

These need no import step: reference the file from an asset.

```json
{ "type": "ImageAsset", "name": "logo", "bytes": "@assets/logo.png", "width": 256, "height": 256 },
{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }
```

Images are stored encoded and decoded by the host (the browser, Unity, Unreal; the CLI
decodes PNG). Fonts are TTF or OTF; with a variable font, a `Text` node's `weight` is animatable.
