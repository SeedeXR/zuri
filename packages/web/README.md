# @seedexr/zuri

Play [Zuri](https://github.com/SeedeXR/zuri) `.zur` files in the browser: interactive
vector animation with state machines and data binding, an open alternative to Rive. The
runtime is Rust compiled to WebAssembly (from 81 KB brotli); the package has no
dependencies. React bindings are in
[`@seedexr/zuri-react`](https://www.npmjs.com/package/@seedexr/zuri-react).

Early: what of Rive is done, partial and planned is listed in
[`docs/parity.md`](https://github.com/SeedeXR/zuri/blob/main/docs/parity.md).

```bash
npm install @seedexr/zuri
```

```js
import { ZuriPlayer } from '@seedexr/zuri';

const player = await ZuriPlayer.create(document.querySelector('canvas'), {
  src: '/button.zur',
  onEvent: (name) => console.log('event', name),
});
player.instance.setColor('accent', '#ff3366');
```

Or as a custom element (with a bundler or an import map that resolves the package's name):

```html
<script type="module">import '@seedexr/zuri/element';</script>
<zuri-player src="/hello.zur" style="width: 400px; height: 300px"></zuri-player>
```

## Making a scene

A scene is a JSON file (shapes, animations, a state machine, the properties your app
sets), built into a `.zur` by the `zuri` command or drawn in the editor: see
[Getting started](https://github.com/SeedeXR/zuri/blob/main/docs/user/getting-started.md)
and the [format](https://github.com/SeedeXR/zuri/blob/main/docs/user/format.md). The
package can build one too, with its editor build (in Node, as an ES module):

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { loadRuntime } from '@seedexr/zuri';

const zuri = await loadRuntime(import.meta.resolve('@seedexr/zuri/zuri-editor.wasm'));
writeFileSync('button.zur', zuri.compile(readFileSync('button.json', 'utf8')));
```

A scene that names files (`"bytes": "@assets/font.ttf"`) is for `zuri build`, which reads
them; here, give the bytes as base64.

Full guide: [`docs/user/web.md`](https://github.com/SeedeXR/zuri/blob/main/docs/user/web.md).

## Builds

| File | Contains | Brotli |
|---|---|---|
| `zuri-lite.wasm` | everything except text | 81 KB |
| `zuri.wasm` (default) | + text | 134 KB |
| `zuri-editor.wasm` | + JSON compile / decompile / schema | 185 KB |

## Develop

```bash
pnpm --filter @seedexr/zuri build     # cargo (wasm32-unknown-unknown) + tsc -> dist/
pnpm --filter @seedexr/zuri test      # Node tests, including wasm vs native golden images
node packages/web/test/browser.mjs    # headless Chrome: player, input, WebGL2 shader paints
```

The wrapper in `src/index.ts` calls the C ABI in
[`crates/zuri-ffi/include/zuri.h`](https://github.com/SeedeXR/zuri/blob/main/crates/zuri-ffi/include/zuri.h) directly; there is no
generated glue.
