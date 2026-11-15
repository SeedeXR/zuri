// Type-level checks: compiled by `tsc -p test/tsconfig.json` (never run). They pin the
// promises the public types make, which the JavaScript tests cannot see.
import { loadRuntime, ZuriPlayer, type Instance } from '../dist/index.js';

const runtime = await loadRuntime(new Uint8Array(0));
// The output of compile() must be accepted by load(), and by the player.
const zur: Uint8Array = runtime.compile('{}');
const instance: Instance = runtime.load(zur).instantiate();
runtime.load(zur.buffer);
runtime.load(new DataView(zur.buffer));
const pixels: Uint8ClampedArray = instance.render(1, 1);
void pixels;
declare const canvas: HTMLCanvasElement;
void ZuriPlayer.create(canvas, { src: zur });
void ZuriPlayer.create(canvas, { src: '/a.zur', fit: 'cover', onEvent: (name: string) => void name });
// @ts-expect-error fit is a closed set
void ZuriPlayer.create(canvas, { src: '/a.zur', fit: 'stretch' });
