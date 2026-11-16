// Runs test/browser.html in headless Chrome and checks what it measured: the canvas player,
// pointer input, data binding, events, WebGL2 shader paints, text, 3D, the custom element.
// Needs: built package (pnpm --filter @seedexr/zuri build) and examples (scripts/build-examples.sh).
//   node packages/web/test/browser.mjs
import { readFile } from 'node:fs';
import { createServer } from 'node:http';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checker, measure, same } from './chrome.mjs';

const repo = fileURLToPath(new URL('../../..', import.meta.url));

// The repository over HTTP: modules and wasm need real MIME types, which file:// does not give.
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = createServer((request, response) => {
  // For the page to let real time pass: see `settle` in browser.html.
  if (request.url === '/wait') return void setTimeout(() => response.writeHead(204).end(), 100);
  const file = resolve(repo, '.' + decodeURIComponent(new URL(request.url, 'http://x').pathname));
  if (!file.startsWith(resolve(repo) + sep)) return response.writeHead(403).end();
  readFile(file, (error, bytes) => {
    if (error) return response.writeHead(404).end();
    response.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(bytes);
  });
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const r = await measure(`http://127.0.0.1:${server.address().port}/packages/web/test/browser.html`).finally(() => server.close());
const check = checker(r);
check(r.ok, 'the page ran to its end');
check(same(r.buttonSize, [320, 160]), 'canvas backing store follows CSS size');
check(same(r.buttonIdle, [0x4f, 0x7c, 0xff, 255]), 'button renders its blue');
check(r.clicks === 1 && r.hover === true, 'pointer events reach listeners (hover, click)');
check(same(r.events, ['clicked']), 'the click event is reported once');
check(same(r.keys, [true, false, true, false, 2]), 'Enter on the focused canvas presses the button once; other keys and shortcuts pass through');
check(same(r.elementKeys, [true, 1]), '<zuri-player> takes keys on itself (its canvas is in a shadow root)');
check(same(r.fingers, [4, true]), `a second finger and another mouse button are not the scene's (${r.fingers}: clicks, hover)`);
check(!same(r.buttonRecolored.slice(0, 3), r.buttonIdle.slice(0, 3)) && r.buttonRecolored[0] > 200, 'setColor recolors through the binding');
check(r.shaderRequests === 1, 'the scene asks the host for one shader texture');
check(!same(r.shaderLater, [0x5b, 0x3d, 0xf5, 255]), 'WebGL2 drew the shader (not the fallback color)');
check(!same(r.shaderLater, r.shaderEarly), 'the shader animates with time');
check(same(r.shaderOutsideCard, [0x0e, 0x10, 0x16, 255]), 'the shader is clipped to its shape');
check(same(r.shaderDotOnTop.slice(0, 3), [255, 255, 255]), 'vector content still draws over the shader');
check(r.textInk > 1000 && r.textInkLonger > r.textInk + 500, 'text renders and follows its bound string');
check(r.modelCentre[3] === 255, 'the 3D model renders');
check(same(r.soundAssets, [2690]), 'the file hands over its sound');
check(same(r.soundBeforeTouch, [1, 0]), 'before anyone has touched the page an event is reported and its sound is not played');
check(same(r.sounds.heard, ['tapped', 'tapped', 'tapped', 'tapped']), 'each tap reports the event');
check(same(r.ownSound, [[5000, 0.5]]) && same(r.ownStopped, [1, 1, 1]), '`Sounds` on its own plays a sound asked for while it is decoding (and none for an unknown id), and `stop` cuts it off once');
check(same(r.sounds.started, [[60, 0.8], [60, 0.8]]), 'the player plays the event\'s sound at its volume, once per tap (nothing held over), and not while muted');
check(r.elementBackground[3] === 255, '<zuri-player> loads and renders');
check(r.destroyed, 'pause() stops the loop and destroy() releases without error');
console.log('browser checks passed');
