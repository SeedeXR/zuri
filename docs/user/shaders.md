# Shader paints

A `ShaderPaint` fills or strokes a shape with a fragment shader you write in WGSL.

```json
{ "type": "ShaderAsset", "name": "plasma",
  "wgsl": "fn shade(uv: vec2f) -> vec4f {\n  let t = zuri.time * zuri.params.x;\n  let p = (uv - 0.5) * 6.0;\n  let v = sin(p.x + t) + sin(p.y * 1.3 - t * 0.7);\n  return vec4f(0.5 + 0.5 * cos(vec3f(0.0, 2.1, 4.2) + v), 1.0);\n}" },
...
{ "type": "Fill", "color": "#5b3df5", "children": [
  { "type": "ShaderPaint", "asset": "plasma", "p0": 1 } ] }
```

The full example is [`examples/shader.json`](../../examples/shader.json).

## What you write

One function:

```wgsl
fn shade(uv: vec2f) -> vec4f      // uv is 0..1 across the shape's bounding box, y down
```

It returns a straight-alpha color. In scope:

| Name | Type | Meaning |
|---|---|---|
| `zuri.time` | `f32` | Seconds since the instance started |
| `zuri.resolution` | `vec2f` | Size of the painted area in pixels |
| `zuri.params` | `vec4f` | The paint's `p0`..`p3` |

`p0`..`p3` are ordinary properties: key them in an animation or `Bind` them to view-model
properties to control a shader from your app.

## What the build does

`zuri build` wraps `shade` in a fragment shader, validates it with naga, and stores GLSL
ES 3.00 and HLSL translations in the file next to the WGSL. A shader that does not compile
fails the build with the compiler's message and the offending line.

## How it runs

The reference renderer runs on the CPU and cannot execute shaders. Instead the **host**
draws the shader into a texture and the runtime composites it as the paint, so a shader
fill keeps its place in the draw order, is clipped to its shape, and blends and fades like
any other paint.

| Host | Status |
|---|---|
| Web (`ZuriPlayer`, `ShaderHost`) | Done: drawn with WebGL2 from the embedded GLSL |
| CLI (`zuri render`) | Falls back to the paint's `color` |
| Unity, Unreal | Fall back to the paint's `color` (HLSL is in the file; hosts are planned) |

Always give the `Fill` / `Stroke` a `color`: it is what shows where no GPU is available,
and for the first frame before the first shader texture arrives.

Hosts written against the C API see each shader paint as a request
(`zuri_instance_shader_request`: which shader, what size, time, parameters) and answer
with pixels (`zuri_instance_set_shader_texture`).

## Limits today
One frame of latency between a parameter change and the picture; one readback per shader
paint per frame (fine for cards and backgrounds, not for hundreds of shader fills);
no textures or other inputs to the shader yet. A shader paint inside a nested artboard
works, but every instance of that component shows the same texture (the first instance's
size and parameters). `time` reaches the shader as a 32-bit float: after hours of
continuous running, wrap it yourself in the shader (`fract`, `sin`) for smooth motion.
