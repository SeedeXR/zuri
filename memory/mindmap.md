# Mind map

A map of the whole system. Update it when a module, dependency or flow is added or removed.

```
Zuri
|
+-- Document (JSON source)                      examples/*.json
|   +-- Artboard
|   |   +-- scene: Node, Shape{geometry + Fill/Stroke}, Text, Image, Model, NestedArtboard, Clip
|   |   +-- paint sources: color | LinearGradient | RadialGradient | ShaderPaint
|   |   +-- Animation > Track(target, property) > Key(frame, value, interp)
|   |   +-- StateMachine > Layer > State > Transition > Condition ; Listener
|   |   +-- ViewModel > Property (number | bool | trigger | color | string)
|   |   +-- Bind (view-model property -> scene property), Event
|   +-- assets: ImageAsset, FontAsset, MeshAsset, ShaderAsset
|
+-- Toolchain (authoring)
|   +-- zuri-cli           new build dump import validate info render bench schema
|   +-- zuri-import        svg (usvg) | obj | gltf (gltf) | fbx (ufbx) | wgsl -> glsl/hlsl (naga)
|   +-- editor             Tauri v2 + React + lucide ; document = JSON ; preview = runtime
|
+-- .zur (binary)          header + ToC + flat objects ; forward compatible ; deterministic
|
+-- zuri-core (runtime)    no required deps ; wasm32 ok
|   format -> schema -> geom -> raster -> model -> file -> instance -> draw
|   features: json (serde_json), text (ttf-parser)
|   loop: advance(dt) -> changed? -> render(pixels)
|
+-- zuri-ffi (C ABI)       include/zuri.h ; cdylib + staticlib + wasm
|   +-- web                packages/web  (@seedexr/zuri: Runtime, ZuriFile, Instance, ZuriPlayer, ShaderHost, <zuri-player>)
|   |                      packages/react (@seedexr/zuri-react: <Zuri>, useZuri)
|   +-- Unity              integrations/unity  (P/Invoke -> Texture2D)
|   +-- Unreal             integrations/unreal (static lib -> UTexture2D, UMG widget, Blueprint)
|
+-- Quality
|   scripts/verify.sh      fmt, clippy, tests, examples, docs, web, editor, dotnet, graphify
|   goldens                crates/zuri-core/tests/golden (native == wasm)
|   memory/                context, architecture, instruction, test, docs, design, todo, handover
```

## Data flow at run time
```
host input (pointer, property set, trigger)
      -> view model  --conditions-->  state machine --plays--> animations
      -> listeners                                             |
view model --binds--> scene properties <-----------------------+
scene properties -> world transforms -> draw -> raster -> RGBA8 -> host texture / canvas
state machine / listeners --emit--> events -> host
```

## Data flow at authoring time
```
SVG / OBJ / GLB / FBX --zuri import--> JSON source <--> editor
JSON source --zuri build (compile, convert 3D, translate shaders, validate)--> .zur
.zur --zuri dump--> JSON source            (round trip is byte-exact)
```

## External dependencies (and why)
| Dependency | Used by | Why |
|---|---|---|
| serde_json | core (feature json), import, cli | JSON twin |
| ttf-parser | core (feature text) | glyph outlines, variable fonts |
| png | cli, tests | PNG in and out |
| naga | import | validate WGSL, translate to GLSL / HLSL |
| usvg | import | SVG simplification |
| gltf | import | glTF / GLB parsing |
| ufbx | import | FBX parsing (C library, authoring only) |
| typescript | packages | compile the wrappers |
| react, vite, lucide-react, tauri | editor | UI shell |
