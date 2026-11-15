// Runs test/app.tsx in headless Chrome (served by Vite) and checks what it measured:
// <Zuri> and useZuri mounting, drawing, reporting events, reloading and cleaning up.
// Needs: built packages (pnpm --filter "@seedexr/*" build) and examples (scripts/build-examples.sh).
//   node packages/react/test/browser.mjs
import { createServer } from 'vite';
import { checker, measure, same } from '../../web/test/chrome.mjs';

// The example scenes are the site's public files (/button.zur, /hello.zur); `mpa` so
// that a file that is not there is a 404, not the page again.
const server = await createServer({ root: import.meta.dirname, publicDir: '../../../examples/dist', appType: 'mpa', logLevel: 'error', server: { host: '127.0.0.1', port: 0 } });
await server.listen();
const r = await measure(server.resolvedUrls.local[0]).finally(() => server.close());
const check = checker(r);
check(r.ok, 'the page ran to its end');
// [players made so far, players alive]
check(r.mounted.loads === 1 && same(r.mounted.players, [1, 1]), '<Zuri> makes one player, though StrictMode mounts it twice');
check(same(r.mounted.size, [320, 160]) && same(r.mounted.idle, [0x4f, 0x7c, 0xff, 255]), '<Zuri> sizes its canvas from CSS and draws the button');
check(same(r.events.heard, ['first:clicked', 'second:clicked']) && r.events.loads === 1, 'events reach the latest onEvent without the player being made again');
check(same(r.early, [null, null]), 'useZuri waits for its canvas');
check(r.hook.drawn === 255 && r.hook.error === null && same(r.hook.players, [2, 2]), 'when the canvas appears, one player draws on it (a src URL made anew each render does not reload it)');
check(same(r.hookReloaded.players, [4, 2]) && r.hookReloaded.current && r.hookReloaded.handedDead === 0, 'a new src replaces the player; one overtaken while loading is destroyed; no effect is handed a destroyed one');
check(r.refit.sameCanvas && r.refit.drawn === 255 && r.refit.oldGone && same(r.refit.players, [5, 2]), 'a new fit makes a new player on the same canvas and destroys the old one');
check(r.unmounted.gone && r.unmounted.canvases === 1 && same(r.unmounted.players, [5, 1]), 'unmounting <Zuri> destroys its player and leaves the rest running');
check(r.end.errors.length === 1 && r.end.errors[0].includes('(404)'), 'a file that is not there is reported through onError, once');
check(r.end.alive === 0 && r.end.handedDead === 0, 'unmounting the app leaves no player alive');
console.log('react browser checks passed');
