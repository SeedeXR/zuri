# Web runtime

`@seedexr/zuri` plays `.zur` files in the browser. It is the Rust runtime compiled to
WebAssembly (from 84 KB brotli) with a small TypeScript wrapper and no dependencies.

> The packages are not published to npm yet. Build them from this repository:
> `pnpm install && pnpm --filter @seedexr/zuri build`.

## Player

```html
<canvas id="c" style="width: 320px; height: 160px"></canvas>
<script type="module">
  import { ZuriPlayer } from '@seedexr/zuri';

  const player = await ZuriPlayer.create(document.getElementById('c'), {
    src: '/button.zur',                       // URL or bytes
    onEvent: (name) => console.log(name),     // events the artboard emits
  });

  player.instance.setColor('accent', '#ff3366');   // drive the view model
  player.instance.getNumber('clicks');
</script>
```

Size the canvas with CSS. The player renders at device resolution (capped by
`maxPixelRatio`, default 2), follows resizes, forwards pointer events, pauses when
off-screen, and **skips rendering whenever nothing changed**.

| Option | Meaning |
|---|---|
| `src` | URL of a `.zur` file, or its bytes |
| `artboard` | Name or index (default: the first) |
| `play` | State machine or animation to play (default: first state machine, else first animation) |
| `autoplay` | Start immediately (default `true`) |
| `fit` | `contain` (default), `cover`, `fill`, `none` |
| `background` | Color behind the artboard (default transparent) |
| `maxPixelRatio` | Cap on `devicePixelRatio` (default 2) |
| `wasm` | A different runtime build or location |
| `onEvent` | Called with each emitted event name |

Methods: `play()`, `pause()`, `resize()`, `destroy()`; fields: `instance`, `file`, `playing`.

## Custom element

```html
<script type="module">import '@seedexr/zuri/element';</script>
<zuri-player src="/hello.zur" fit="contain" style="width: 400px; height: 300px"></zuri-player>
```

Attributes: `src`, `artboard`, `play`, `fit`, `background`. It dispatches `load`, `error`
and `zuri-event` (`detail` is the event name); `element.player` is the `ZuriPlayer`.

## React

```tsx
import { Zuri, useZuri } from '@seedexr/zuri-react';

<Zuri src="/button.zur" style={{ width: 320, height: 160 }} onEvent={(name) => ...} />

// or, to reach the player:
const { canvasRef, player } = useZuri({ src: '/button.zur' });
useEffect(() => player?.instance.setColor('accent', color), [player, color]);
return <canvas ref={canvasRef} style={{ width: 320, height: 160 }} />;
```

Client-only (a canvas cannot render on the server): in Next.js use a `"use client"` file.
Status: the React package type-checks and builds; it has not been exercised in a browser yet.

## Low-level API

```ts
import { loadRuntime } from '@seedexr/zuri';

const runtime = await loadRuntime();                 // loads zuri.wasm once
const file = runtime.load(bytes);                    // throws with the reason if invalid
await file.decodeImages();                           // only needed if the file has images
const instance = file.instantiate('Main');
instance.play();                                     // or play('name')

function frame(seconds) {
  if (instance.advance(seconds)) {                   // false: nothing changed
    const pixels = instance.render(width, height, { fit: 'contain' });
    ctx.putImageData(new ImageData(pixels, width, height), 0, 0);
  }
}
```

`Instance` also has: `animations`, `stateMachines`, `properties`, `setNumber` /
`setBoolean` / `setColor` / `setString` / `fire` and their getters, `pointer(kind, x, y)`,
`events()`, `hitTest`, `bounds`, `worldMatrix`, `free()`. The pixels returned by `render`
live in wasm memory: use them before calling into Zuri again.

Finer control:

| Call | What it does |
|---|---|
| `playStateMachine(name)`, `playAnimation(name)` | Like `play(name)`, but say which kind you mean (a state machine and an animation may share a name; `play` prefers the state machine). |
| `seek(seconds)` | Jump an animation started with `playAnimation` to a time on its timeline; `advance(0)` then shows it. Nested artboards restart and run to the same time. For scrubbers and frame export. |
| `declaredEvents` | The event names the artboard can report, including those of artboards nested in it. |
| `layerStates` | The object id of the state each state-machine layer is in. |
| `property(object, key)`, `propertyColor(object, key)` | The number (or 0xAARRGGBB color) an object currently draws with, after animation and data binding. `object` comes from `hitTest`; `key` is the property's number in `zuri schema`. For tools. |

## Choosing a build

| File | Contains | Size (brotli) |
|---|---|---|
| `zuri-lite.wasm` | shapes, animation, state machines, data binding, 3D models | 84 KB |
| `zuri.wasm` (default) | + text | 112 KB |
| `zuri-editor.wasm` | + JSON compile / decompile / schema | 146 KB |

```ts
const lite = await loadRuntime(new URL('@seedexr/zuri/zuri-lite.wasm', import.meta.url));
const editor = await loadRuntime(new URL('@seedexr/zuri/zuri-editor.wasm', import.meta.url));
const zur = editor.compile(jsonSource);              // Uint8Array; errors name the JSON path
```

## How it renders
The runtime rasterizes on the CPU inside WebAssembly and the player puts the pixels on a
2D canvas. Measured on an Apple M4 in Node: 3.8 ms per 800x600 frame of `hello`, 3 MB of
wasm memory. That is right for UI-sized content; a GPU renderer for large canvases is on
the [roadmap](../dev/rendering.md). Shader paints are drawn with WebGL2 (see [shaders](shaders.md)).

## Limits today
No WebGPU / WebGL vector backend yet; no worker / OffscreenCanvas mode.
