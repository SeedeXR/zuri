// Runs test/browser.html in headless Chrome and checks what it measured: the canvas player,
// pointer input, data binding, events, WebGL2 shader paints, text, 3D, the custom element.
// Needs: built package (pnpm --filter @seedexr/zuri build) and examples (scripts/build-examples.sh).
//   node packages/web/test/browser.mjs
// CHROME=/path/to/chrome overrides where Chrome is looked for.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { existsSync, mkdtempSync, readFile, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { delimiter, extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../..', import.meta.url));
const onPath = (name) => (process.env.PATH ?? '').split(delimiter).map((dir) => join(dir, name));
const chrome = [
  process.env.CHROME,
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ...['google-chrome', 'chromium', 'chromium-browser'].flatMap(onPath),
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
].find((path) => path && existsSync(path));
if (!chrome) {
  // A gate that skips silently is not a gate: skipping has to be asked for.
  if (process.env.ZURI_SKIP_BROWSER) {
    console.log('browser checks skipped (ZURI_SKIP_BROWSER is set)');
    process.exit(0);
  }
  console.error('Chrome not found: set CHROME=/path/to/chrome, or ZURI_SKIP_BROWSER=1 to skip the browser checks on purpose');
  process.exit(1);
}

// The repository over HTTP: modules and wasm need real MIME types, which file:// does not give.
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = createServer((request, response) => {
  const file = resolve(repo, '.' + decodeURIComponent(new URL(request.url, 'http://x').pathname));
  if (!file.startsWith(resolve(repo) + sep)) return response.writeHead(403).end();
  readFile(file, (error, bytes) => {
    if (error) return response.writeHead(404).end();
    response.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(bytes);
  });
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const profile = mkdtempSync(join(tmpdir(), 'zuri-chrome-'));

const { stdout, stderr } = await new Promise((done) =>
  execFile(
    chrome,
    // --enable-unsafe-swiftshader lets WebGL fall back to software where there is no GPU (CI).
    ['--headless=new', '--no-first-run', '--window-size=900,1400', '--virtual-time-budget=20000', '--enable-unsafe-swiftshader', `--user-data-dir=${profile}`, '--dump-dom', `http://127.0.0.1:${server.address().port}/packages/web/test/browser.html`],
    { maxBuffer: 64 << 20 },
    (_error, stdout, stderr) => done({ stdout, stderr }),
  ),
);
server.close();
rmSync(profile, { recursive: true, force: true, maxRetries: 5 });

try {
  const result = /<pre id="result">(.*?)<\/pre>/s.exec(stdout)?.[1];
  assert.ok(result, 'the page produced no result');
  const r = JSON.parse(result.replaceAll('&quot;', '"').replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&'));
  assert.ok(r.ok, r.error);
  const check = (condition, what) => {
    assert.ok(condition, `FAILED: ${what}\n${JSON.stringify(r, null, 1)}`);
    console.log('ok  ', what);
  };
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  check(same(r.buttonSize, [320, 160]), 'canvas backing store follows CSS size');
  check(same(r.buttonIdle, [0x4f, 0x7c, 0xff, 255]), 'button renders its blue');
  check(r.clicks === 1 && r.hover === true, 'pointer events reach listeners (hover, click)');
  check(same(r.events, ['clicked']), 'the click event is reported once');
  check(!same(r.buttonRecolored.slice(0, 3), r.buttonIdle.slice(0, 3)) && r.buttonRecolored[0] > 200, 'setColor recolors through the binding');
  check(r.shaderRequests === 1, 'the scene asks the host for one shader texture');
  check(!same(r.shaderLater, [0x5b, 0x3d, 0xf5, 255]), 'WebGL2 drew the shader (not the fallback color)');
  check(!same(r.shaderLater, r.shaderEarly), 'the shader animates with time');
  check(same(r.shaderOutsideCard, [0x0e, 0x10, 0x16, 255]), 'the shader is clipped to its shape');
  check(same(r.shaderDotOnTop.slice(0, 3), [255, 255, 255]), 'vector content still draws over the shader');
  check(r.textInk > 1000 && r.textInkLonger > r.textInk + 500, 'text renders and follows its bound string');
  check(r.modelCentre[3] === 255, 'the 3D model renders');
  check(r.elementBackground[3] === 255, '<zuri-player> loads and renders');
  check(r.destroyed, 'pause() stops the loop and destroy() releases without error');
  console.log('browser checks passed');
} catch (error) {
  // What Chrome said is usually the reason (no GPU, sandbox, a page error).
  console.error(`${error.message}\n--- Chrome stderr (last lines) ---\n${stderr.split('\n').slice(-25).join('\n')}`);
  process.exit(1);
}
