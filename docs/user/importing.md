# Importing SVG and 3D models

`zuri import <file>` converts other formats into Zuri JSON source that you can edit and
build. The editor's Assets tab calls the same code.

```bash
zuri import logo.svg  -o logo.json      # an Artboard of Shapes
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

What is imported: all basic shapes and paths (SVG's own rectangles, circles and so on
arrive as paths), transforms, fills and strokes with opacity, fill rules, caps and joins,
linear and radial gradients, group opacity, paint order.

What is not imported yet: clip paths, masks, filters, patterns (drawn grey), embedded
images, dashes, and text that has not been converted to outlines. Transforms are baked
into the path points, so imported shapes sit at 0,0 with absolute coordinates.

## Images and fonts

These need no import step: reference the file from an asset.

```json
{ "type": "ImageAsset", "name": "logo", "bytes": "@assets/logo.png", "width": 256, "height": 256 },
{ "type": "FontAsset", "name": "Outfit", "bytes": "@assets/Outfit.ttf" }
```

Images are stored encoded and decoded by the host (the browser, Unity, Unreal; the CLI
decodes PNG). Fonts are TTF or OTF; with a variable font, a `Text` node's `weight` is animatable.
