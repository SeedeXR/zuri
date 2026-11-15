# @seedexr/zuri-react

React bindings for [`@seedexr/zuri`](../web).

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

The player is created once per `src` / `artboard` / `play` / `fit` / `background`.
Client-only: in Next.js put it in a `"use client"` file.

Status: builds and type-checks against React 19; not yet exercised in a browser.
