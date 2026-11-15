<p><img src="brand/zuri-logo.svg" alt="Zuri" width="96" height="96"></p>

# Zuri

An open alternative to [Rive](https://rive.app): interactive vector graphics with
animation, state machines and data binding, in one small Rust runtime that plays the same
`.zur` file on the **web**, in **Unity** and in **Unreal**.

- **Small.** The web player is 92 KB of WebAssembly (brotli), 120 KB with text. No dependencies.
- **Fast where it counts.** UI-sized scenes render in 0.1 - 0.5 ms on one core; an idle
  scene is not rendered at all.
- **One runtime, one C ABI.** The browser, Unity, Unreal and the CLI run the same code and
  produce the same pixels (the wasm build is tested against the native golden images).
- **Agent-native.** Scenes are plain JSON with a schema, a compiler whose errors say how
  to fix them, and a CLI that renders to PNG. People, the editor and AI agents edit the
  same document.
- **More than 2D.** 3D models (OBJ, glTF/GLB, FBX) inside vector scenes, WGSL shader
  paints, animatable variable-font weight.
- **Open.** MIT OR Apache-2.0, including the editor. Export is free.

> **Status: early.** The foundation is built and tested end to end; many Rive features are
> not implemented yet. [`docs/parity.md`](docs/parity.md) lists exactly what is done,
> partial and planned. Nothing is published to npm or a package registry yet.

## Platforms

Zuri is meant to work on macOS, Linux and Windows. What has actually run where
(CI runs on all three for every push):

| | macOS | Linux | Windows |
|---|---|---|---|
| Runtime, C ABI, CLI, importers | tested | tested | tested |
| Web package: wasm builds, tests, headless Chrome | tested | tested | tested |
| Editor | web app and shell tested; the app has been run | web app tested, shell builds; app not run | web app tested, shell builds; app not run |
| Unity package | tested in Unity 6000.4 on the GPU | library builds, C# bindings tested; not run in Unity | library builds, C# bindings tested; not run in Unity |
| Unreal plugin | tested in UE 5.8 on the GPU | static library builds; plugin not compiled | static library builds; plugin not compiled |

## Try it

```bash
cargo build -p zuri-cli --release
alias zuri="$PWD/target/release/zuri"

zuri new button.json                         # a starter scene: an interactive button
zuri build button.json                       # -> button.zur (1,141 bytes)
zuri render button.zur -o button.png --set hover=true --time 0.5 --scale 2
zuri import model.glb -o model.json          # also .fbx, .obj, .svg
```

In a browser:

```bash
pnpm install && pnpm --filter @seedexr/zuri build && bash scripts/build-examples.sh
python3 -m http.server 8000                  # then open http://localhost:8000/packages/web/demo/
```

```js
import { ZuriPlayer } from '@seedexr/zuri';

const player = await ZuriPlayer.create(canvas, { src: '/button.zur', onEvent: console.log });
player.instance.setColor('accent', '#ff3366');
```

## A scene is JSON

```json
{ "zuri": 1, "children": [
  { "type": "Artboard", "name": "Main", "width": 400, "height": 300, "children": [
    { "type": "Shape", "name": "ball", "x": 200, "y": 150, "children": [
      { "type": "Ellipse", "width": 80, "height": 80 },
      { "type": "Fill", "color": "#ff5533" } ] },
    { "type": "Animation", "name": "bounce", "duration": 60, "loop": "pingpong", "children": [
      { "type": "Track", "target": "ball", "property": "y", "children": [
        { "type": "Key", "frame": 0, "value": 150, "interp": "cubic" },
        { "type": "Key", "frame": 60, "value": 60 } ] } ] } ] } ] }
```

One rule: an object is a `type`, its properties and its `children`; a reference is the
target's `name`. `zuri schema` lists every type and property.

## What is in the box

| | |
|---|---|
| [`crates/zuri-core`](crates/zuri-core) | The runtime: format, scene, animation, state machines, data binding, reference renderer |
| [`crates/zuri-ffi`](crates/zuri-ffi) | The C ABI ([`zuri.h`](crates/zuri-ffi/include/zuri.h)) behind every host |
| [`crates/zuri-cli`](crates/zuri-cli) | `zuri`: build, dump, import, validate, info, render, bench, schema |
| [`crates/zuri-import`](crates/zuri-import) | SVG / OBJ / glTF / FBX importers, WGSL shader translation |
| [`packages/web`](packages/web), [`packages/react`](packages/react) | `@seedexr/zuri`, `@seedexr/zuri-react` |
| [`apps/editor`](apps/editor) | The editor (Tauri + React) |
| [`integrations/unity`](integrations/unity), [`integrations/unreal`](integrations/unreal) | Engine plugins |
| [`examples`](examples) | Example scenes |
| [`brand`](brand) | Logo, colour and type tokens, Tailwind theme |

## Documentation
[`docs/`](docs/README.md): getting started, the format, the CLI, the web runtime,
importing, shaders, engine guides, the schema reference and the parity table.

## Working on Zuri
Read [`AGENTS.md`](AGENTS.md) (it applies to people too), then [`memory/`](memory).
`bash scripts/verify.sh` is the gate: format, lint, tests, golden images, the C ABI, the
wasm build, browser checks and generated docs.

## License
MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
The example font (Outfit) is under the SIL Open Font License (`examples/assets/Outfit-OFL.txt`).
