# @seedexr/zuri

Play [Zuri](https://github.com/SeedeXR/zuri) `.zur` files in the browser: interactive
vector animation with state machines and data binding. The runtime is Rust compiled to
WebAssembly (from 78 KB brotli); the package has no dependencies.

```js
import { ZuriPlayer } from '@seedexr/zuri';

const player = await ZuriPlayer.create(document.querySelector('canvas'), {
  src: '/button.zur',
  onEvent: (name) => console.log('event', name),
});
player.instance.setColor('accent', '#ff3366');
```

Or with no JavaScript of your own:

```html
<script type="module">import '@seedexr/zuri/element';</script>
<zuri-player src="/hello.zur" style="width: 400px; height: 300px"></zuri-player>
```

Full guide: [`docs/user/web.md`](../../docs/user/web.md).

## Builds

| File | Contains | Brotli |
|---|---|---|
| `zuri-lite.wasm` | everything except text | 78 KB |
| `zuri.wasm` (default) | + text | 131 KB |
| `zuri-editor.wasm` | + JSON compile / decompile / schema | 181 KB |

## Develop

```bash
pnpm --filter @seedexr/zuri build     # cargo (wasm32-unknown-unknown) + tsc -> dist/
pnpm --filter @seedexr/zuri test      # Node tests, including wasm vs native golden images
node packages/web/test/browser.mjs    # headless Chrome: player, input, WebGL2 shader paints
```

The wrapper in `src/index.ts` calls the C ABI in
[`crates/zuri-ffi/include/zuri.h`](../../crates/zuri-ffi/include/zuri.h) directly; there is no
generated glue.
