# @seedexr/zuri

Play [Zuri](https://github.com/SeedeXR/zuri) `.zur` files in the browser: interactive
vector animation with state machines and data binding. The runtime is Rust compiled to
WebAssembly (from 81 KB brotli); the package has no dependencies.

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

Full guide: [`docs/user/web.md`](https://github.com/SeedeXR/zuri/blob/main/docs/user/web.md).

## Builds

| File | Contains | Brotli |
|---|---|---|
| `zuri-lite.wasm` | everything except text | 81 KB |
| `zuri.wasm` (default) | + text | 134 KB |
| `zuri-editor.wasm` | + JSON compile / decompile / schema | 186 KB |

## Develop

```bash
pnpm --filter @seedexr/zuri build     # cargo (wasm32-unknown-unknown) + tsc -> dist/
pnpm --filter @seedexr/zuri test      # Node tests, including wasm vs native golden images
node packages/web/test/browser.mjs    # headless Chrome: player, input, WebGL2 shader paints
```

The wrapper in `src/index.ts` calls the C ABI in
[`crates/zuri-ffi/include/zuri.h`](https://github.com/SeedeXR/zuri/blob/main/crates/zuri-ffi/include/zuri.h) directly; there is no
generated glue.
