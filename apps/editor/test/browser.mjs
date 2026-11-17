// Runs the editor in headless Chrome (served by Vite) and checks what test/drive.ts saw
// while driving it: that it starts, edits, undoes, opens a file and previews it.
// Needs: built packages (pnpm --filter "@seedexr/*" build) and examples (scripts/build-examples.sh).
//   node apps/editor/test/browser.mjs
import { join } from 'node:path';
import { createServer } from 'vite';
import { checker, measure, same } from '../../../packages/web/test/chrome.mjs';

// The example scenes are the site's public files (/sound.zur); any free port, not the one
// Tauri expects; and nothing is watched (the folder holds the desktop shell's build).
// For the page to let real time pass: see `settle` in drive.ts.
const wait = { name: 'wait', configureServer: (server) => void server.middlewares.use('/wait', (_request, response) => setTimeout(() => response.writeHead(204).end(), 100)) };
const server = await createServer({ plugins: [wait], root: join(import.meta.dirname, '..'), publicDir: '../../examples/dist', appType: 'mpa', logLevel: 'error', server: { host: '127.0.0.1', port: 0, watch: null, hmr: false } });
await server.listen();
const r = await measure(`${server.resolvedUrls.local[0]}test/index.html`).finally(() => server.close());
const check = checker(r);
check(!r.failed, 'the page ran to its end');
check(r.booted.title === 'Untitled.json · Zuri' && same(r.booted.types, ['Artboard', 'Fill']), 'the editor starts with the default document');
check(same(r.booted.fill, [0x1b, 0x1d, 0x27, 255]) && r.booted.beside === 0, 'the stage draws the artboard, and nothing beside it');
check(same(r.layout.types, ['Artboard', 'Fill', 'Layout']) && same(r.layout.listed, ['gap', 'paddingLeft', 'wrap']), 'the Layout button adds a Layout, and the inspector lists its properties from the schema');
check(same(r.undone, ['Artboard', 'Fill']), 'Undo takes it out again');
check(r.opened.title === 'sound.json · Zuri' && same(r.opened.pad, [0x4f, 0x7c, 0xff, 255]), 'a .zur file opens and is drawn');
check(same(r.previewed.events, ['tapped', 'tapped']), 'the preview plays the state machine: a tap on the pad reports its event, a tap beside it none');
check(same(r.previewed.started, [[60, 0.8], [60, 0.8]]), 'and starts the event\'s sound at its volume, once per tap');
check(r.muted.events === 3 && r.muted.started === 2, 'muted, a tap reports the event and starts no sound');
check(r.stopped.started === 3 && same(r.stopped.selected, ['pad']) && r.stopped.preview, 'unmuted it sounds again; stopped, a tap selects the pad and starts no sound');
check(r.workArea.from === 6 && r.workArea.to === 12 && r.workArea.kept && r.workArea.within && r.workArea.laps >= 3, 'a drag with Shift on the timeline\'s ruler sets a work area (a click with Shift on a lane leaves it), and Play goes round it');
check(r.workArea.start >= 6 && r.workArea.start < 9, `with the playhead outside it, Play starts at its first frame (frame ${r.workArea.start} after the first frames)`);
check(r.workArea.cleared && r.workArea.after.playing && r.workArea.after.frame < 13 && r.workArea.end === 18, `a click with Shift clears it, and the animation plays on from the playhead to its end (${JSON.stringify(r.workArea.after)}, then ${r.workArea.end})`);
check(same(r.problems, []), 'the page reported no error or warning');
console.log('editor browser checks passed');
