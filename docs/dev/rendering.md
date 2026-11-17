# Rendering

How Zuri turns a scene into pixels today, what it costs (measured), and where it goes next.

## The reference renderer

`crates/zuri-core/src/raster.rs` + `draw.rs`. CPU, deterministic, no dependencies, runs
natively and in WebAssembly. Every other backend will be compared against its output.

Pipeline for one paint of one shape:

1. **Outline.** Geometry children give path elements (`geom::rect`, `ellipse`, `star`, or
   parsed SVG path data); text gives glyph outlines.
2. **Flatten** curves to polylines in the shape's own space. Segment count per cubic comes
   from Wang's formula for a tolerance of 0.2 device pixels (scaled by the transform).
3. **Stroke** (for a `Stroke`): each segment becomes a quad, each join a triangle, quad or
   disc, each cap a quad or disc, all wound the same way so overlaps add up under the
   non-zero rule. Trim cuts the polyline by arc length first. Stroking happens before the
   transform, so non-uniform scale stretches the pen as it should.
4. **Transform** to device pixels.
5. **Coverage.** Each edge deposits its signed area into an accumulation buffer; a running
   sum along each row gives exact fractional coverage per pixel (the font-rs /
   stb_truetype technique). No supersampling, no tessellation. Edges outside the surface
   are clipped so they still contribute winding.
6. **Feather** (optional): three box blurs of the coverage mask approximate a gaussian.
   This blurs pixels; Rive softens the path itself, which stays exact at any size.
   An offset moves the outline (and its paint) before it is rasterized. An **inner**
   feather takes a second coverage pass: the crisp outline, times one minus the blurred
   outline (moved by the offset), so the paint is strongest just inside the edge and
   nothing is drawn outside it.
7. **Composite** into premultiplied RGBA8: solid, gradient (256-entry premultiplied ramp),
   or image / shader / 3D texture (bilinear); source-over or one of 11 other separable
   blend modes; multiplied by the clip mask and bounded by the scissor.

3D `Model` nodes are rasterized by `model.rs` (perspective projection, z-buffer,
per-vertex lighting, 2x2 supersampling) into a temporary image that is composited at step 7.

## Measured cost

Apple M4, one core, release build, 2026-11-15. Reproduce with
`target/release/zuri bench <file> [--width W --height H]`.

| Scene | Size | Render | Notes |
|---|---|---|---|
| `button` | 320x160 | 0.14 ms | idle: not rendered at all |
| `text` | 400x200 | 0.21 ms | |
| `shader` | 400x240 | 0.33 ms | CPU fallback color |
| `hello` | 400x300 | 0.48 ms | gradients, feather, blend, trim |
| `model` | 400x400 | 2.7 ms | 1,646 triangles, 2x2 supersampled (4.1 ms until 2026-11-17) |
| `button` / `text` / `hello` / `shader` | 1920x1080 | 4.6 / 4.0 / 5.7 / 6.6 ms | |
| `model` | 1920x1080 | 15.0 ms | 5.3 of it is the rest of the scene; 22.7 ms until 2026-11-17 |
| `hello` in WebAssembly (Node 26) | 800x600 | 3.8 ms | |

`advance` costs 0.2 - 0.5 microseconds. Peak memory of the CLI: 4.8 MB at 400x300, 28 MB at
1920x1080. The WebAssembly player holds 3 MB after a 640x320 render.

**The cost model.** Rendering time is proportional to the pixels inside each paint's
bounding box, plus the edges. So UI-sized content is far inside a frame budget, a
full-screen 1080p layer fits in 60 Hz on one core, and 4K at 120 Hz does not. That last
case is what the GPU backend is for.

**Gradients are dithered.** The color ramp keeps 8 fractional bits and each pixel rounds
up or down by its position (a threshold that steps evenly along both axes), so a slow
ramp has no bands. It is a pure function of the pixel position: the same frame every time.

**Idle is free.** `advance` returns `false` when nothing changed; hosts skip the render
and the upload. Measured: 0 of 600 idle frames rendered.

## Known ceilings (each has a `ponytail:` comment in the code)
- Coverage is computed over the whole bounding box of a paint, not just near its edges.
- Clip masks are full-surface byte masks.
- Flattened contours are rebuilt every frame (geometry outlines are cached, flattening is not).
- Feather blurs the bounding box plus its reach.
- 3D is software rasterized.
- Non-zero winding is approximated per pixel by `min(|sum|, 1)`, which is exact for
  non-overlapping coverage and slightly over-covers where two edges of the same shape
  share a pixel.

## Roadmap: GPU

The research in `research/` agrees on the destination: the core emits an engine-neutral
stream and thin backends draw it with each engine's own device, so consoles and render
pipelines come for free. The plan, in order:

1. **Lift the seam.** `draw.rs` calls exactly three things on `Raster` (`push_clip`,
   `pop_clip`, `fill`). Make that a trait; the CPU renderer is the first implementation.
2. **Sparse coverage.** Change the coverage stage to emit strips (coverage only near
   edges, solid spans elsewhere) instead of a full mask. This speeds up the CPU path and
   is the input a GPU compositor wants.
3. **WebGL2 / WebGPU compositor** in `packages/web`: upload strips, shade and blend on the
   GPU. Shader paints then run in place (no readback).
4. **Unity and Unreal backends** using the engine's command buffer / RDG with the same
   strip stream and HLSL versions of the same shaders.
5. Evaluate `vello_hybrid` (0.2.0 on crates.io as of 2026-11-15) as an alternative to
   steps 2-3 once it is API-stable. It stays behind the trait either way.

Until then, engines display a CPU-rendered texture: universal and verified, at the cost
of one upload per changed frame.
