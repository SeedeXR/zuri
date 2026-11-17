# @seedexr/zuri-react

React bindings for [`@seedexr/zuri`](https://www.npmjs.com/package/@seedexr/zuri), the web runtime of
[Zuri](https://github.com/SeedeXR/zuri).

```bash
npm install @seedexr/zuri @seedexr/zuri-react
```

```tsx
import { Zuri, useZuri } from '@seedexr/zuri-react';

<Zuri src="/button.zur" style={{ width: 320, height: 160 }} onEvent={(name) => console.log(name)} />
```

For access to the player (view-model properties, play / pause):

```tsx
const { canvasRef, player, error } = useZuri({ src: '/button.zur' });
useEffect(() => player?.instance.setColor('accent', color), [player, color]);
return <canvas ref={canvasRef} style={{ width: 320, height: 160 }} />;
```

The player is created once per `src` / `artboard` / `play` / `fit` / `align` / `background`,
and `player` is null while another one is loading.
Client-only: in Next.js put it in a `"use client"` file.

Status: builds against React 19; run in headless Chrome under StrictMode (`node test/browser.mjs`).
