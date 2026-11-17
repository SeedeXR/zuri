# Importing SVG and 3D models

`zuri import <file>` converts other formats into Zuri JSON source that you can edit and
build. The editor's Assets tab calls the same code.

```bash
zuri import logo.svg  -o logo.json      # an Artboard of Shapes, in Nodes for the SVG's groups
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

## Images and fonts

These need no import step: reference the file from an asset.

```json
{ "type": "ImageAsset", "name": "logo", "bytes": "@assets/logo.png", "width": 256, "height": 256 },
{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }
```

Images are stored encoded and decoded by the host (the browser, Unity, Unreal; the CLI
decodes PNG). Fonts are TTF or OTF; with a variable font, a `Text` node's `weight` is animatable.
