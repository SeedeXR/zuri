// What test/browser.mjs loads: the editor, started by its own entry point, then driven
// through its buttons and its stage as a person would, with what happened written down
// for the checks. The document is read from the store, the picture from the canvas.
import { find, walk } from '../src/doc/model.ts';
import { openBytes } from '../src/io/files.ts';
import { preview } from '../src/stage/preview.ts';
import { store } from '../src/store.ts';

const out: Record<string, unknown> = {};
// Anything the page complains about fails the run: errors, rejections, React's warnings.
const problems: string[] = [];
addEventListener('error', (e) => problems.push(e.message));
addEventListener('unhandledrejection', (e) => problems.push(String(e.reason)));
for (const level of ['error', 'warn'] as const) console[level] = (...args: unknown[]) => void problems.push(args.map(String).join(' '));
// ponytail: the pointer events made here are not a real pointer, which is all that can be
// captured, so capturing does nothing and is not tested; send real ones through Chrome's
// debugging protocol (Input.dispatchMouseEvent) to cover it.
Element.prototype.setPointerCapture = () => {};
// The sounds that start: how long each is, and how loud it is played.
const started: number[][] = [];
const loudness = new WeakMap<AudioNode, AudioParam>();
const connect = AudioNode.prototype.connect as (this: AudioNode, to: AudioNode, ...rest: number[]) => AudioNode;
AudioNode.prototype.connect = function (this: AudioNode, to: AudioNode, ...rest: number[]) {
  if (to instanceof GainNode) loudness.set(this, to.gain);
  return connect.call(this, to, ...rest);
} as typeof AudioNode.prototype.connect;
const start = AudioBufferSourceNode.prototype.start;
AudioBufferSourceNode.prototype.start = function (...args) {
  started.push([Math.round(this.buffer!.duration * 1000), Math.round(loudness.get(this)!.value * 100) / 100]);
  return start.apply(this, args);
};

// A headless browser may have no display to drive requestAnimationFrame: the frames the
// editor asks for are called from here, a 60th of a second apart.
const asked: FrameRequestCallback[] = [];
let clock = 0;
window.requestAnimationFrame = (callback) => asked.push(callback);
const frames = async (count: number) => {
  for (; count > 0; count--) {
    clock += 1000 / 60;
    for (const callback of asked.splice(0)) callback(clock);
    await new Promise((done) => setTimeout(done, 0));
  }
};
/**
 * Real time until `met`, for what the browser does in its own time (drawing the page,
 * sizing the stage, decoding and starting a sound). This page's clock is virtual and
 * runs far ahead of that. A fetch moves it by 10 ms however long the answer takes
 * (measured in the web package's test), and the server takes 100 ms to answer /wait:
 * up to 5 s of real time, then a failure. A sound has 3 s of them before the player's
 * own rule (300 ms by the page's clock) drops it as too late. `each` runs after every wait.
 */
const settle = async (what: string, met: () => boolean, each = async () => {}) => {
  for (let turn = 0; !met(); turn++) {
    if (turn === 50) throw new Error(`waited in vain for ${what}`);
    await fetch('/wait');
    await each();
  }
};
/** The same, with a frame of the editor after every wait: for what it has to draw. */
const until = (what: string, met: () => boolean) => settle(what, met, () => frames(1));
const named = (button: Element) => (button.getAttribute('aria-label') ?? button.textContent ?? '').trim();
const buttons = () => [...document.querySelectorAll('button')].map(named);
const press = async (name: string) => {
  const button = [...document.querySelectorAll('button')].find((b) => named(b) === name);
  if (!button) throw new Error(`there is no button called "${name}" among: ${buttons().join(', ')}`);
  button.click();
  await frames(3);
};
const types = () => [...walk(store.get().doc)].map((node) => node.type);
const stage = () => document.querySelector('canvas')!.parentElement!;
/** Where a point of the artboard is on the page. */
const onPage = (x: number, y: number) => {
  const [{ view }, box] = [store.get(), stage().getBoundingClientRect()];
  return { clientX: box.left + view.x + x * view.zoom, clientY: box.top + view.y + y * view.zoom };
};
/** The color the stage shows at a point of the artboard. */
const pixel = (x: number, y: number) => {
  const canvas = document.querySelector('canvas')!;
  const [at, box] = [onPage(x, y), canvas.getBoundingClientRect()];
  const scale = canvas.width / box.width;
  return [...canvas.getContext('2d')!.getImageData(Math.round((at.clientX - box.left) * scale), Math.round((at.clientY - box.top) * scale), 1, 1).data];
};
/** A tap on the stage, a wait for the sounds that should have started by then, and frames enough for the pad's pulse to end. */
const tap = async (x: number, y: number, sounds: number) => {
  for (const type of ['pointerdown', 'pointerup']) stage().dispatchEvent(new PointerEvent(type, { ...onPage(x, y), bubbles: true }));
  await frames(2);
  await settle(`sound ${sounds}`, () => started.length >= sounds);
  await frames(28);
};

try {
  await import('../src/main.tsx');
  // React draws the editor a moment later. A start that failed is reported in the page instead: pass it on.
  const root = document.getElementById('root')!;
  await until('the editor to appear', () => root.childNodes.length > 0);
  if (!document.querySelector('canvas')) throw new Error(root.textContent ?? '');
  await until('the stage to be sized', () => preview.width > 0);
  await frames(3);
  // The default document: one artboard, 400 x 300, filled #1b1d27.
  out.booted = { title: document.title, types: types(), fill: pixel(200, 150), beside: pixel(-20, 150)[3] };

  // The Layout button puts a Layout in the artboard and selects it; the inspector lists
  // its properties from the runtime's schema, the newest among them; Undo takes it out.
  await press('Layout');
  const inspector = document.querySelector('[aria-label="Inspector"]')!.textContent ?? '';
  out.layout = { types: types(), listed: ['gap', 'paddingLeft', 'wrap'].filter((property) => inspector.includes(property)) };
  await press('Undo');
  out.undone = types();

  // A file with a sound, opened as File > Open does once the dialog has given it the
  // bytes. The preview plays its state machine: a tap on the pad (at the middle of the
  // artboard, 110 across) reports the event and starts its sound; a tap beside it does neither.
  const response = await fetch('/sound.zur');
  if (!response.ok) throw new Error(`sound.zur: ${response.status}; build the examples first (scripts/build-examples.sh)`);
  openBytes('sound.zur', new Uint8Array(await response.arrayBuffer()), null);
  await frames(5);
  out.opened = { title: document.title, pad: pixel(100, 100) };
  await press('Preview the state machine');
  await tap(100, 100, 1);
  await tap(20, 20, 1);
  await tap(100, 100, 2);
  out.previewed = { events: preview.events.map((event) => event.name), started: [...started] };
  // Muted, a tap on the pad still reports its event and starts no sound; unmuted, it does again.
  await press('Mute the preview');
  await tap(100, 100, 2);
  out.muted = { events: preview.events.length, started: started.length };
  await press('Mute the preview');
  await tap(100, 100, 3);
  // Stopped, the stage is for editing again: a tap selects the pad and nothing sounds.
  await press('Stop');
  await tap(100, 100, 3);
  const selected = store.get().selection.map((id) => find(store.get().doc, id)?.props.name);
  out.stopped = { started: started.length, selected, preview: buttons().includes('Preview the state machine') };

  // The timeline's work area: a drag with Shift along the ruler, from a third of the way
  // to two thirds, takes frames 6 to 12 of `pulse` (18 frames, which ends when it has
  // played), and Play goes round those. A click with Shift clears it.
  store.set({ mode: 'animate', bottomTab: 'timeline', bottomOpen: true });
  await frames(3);
  const ruler = document.querySelector('[aria-label="Playhead"]')!;
  // The timeline measures itself when the browser tells it its size: its last tick is then well along.
  await until('the timeline to be sized', () => parseFloat((ruler.lastElementChild as HTMLElement).style.left) > 100);
  const lanes = ruler.parentElement!.getBoundingClientRect();
  // (The lanes have 14 pixels of padding at each end.)
  const on = (type: string, part: number, where: Element = ruler) => where.dispatchEvent(new PointerEvent(type, { clientX: lanes.left + 14 + (lanes.width - 28) * part, clientY: lanes.top + 4, shiftKey: true, bubbles: true }));
  on('pointerdown', 1 / 3);
  on('pointermove', 2 / 3);
  on('pointerup', 2 / 3);
  const area = store.get().workArea;
  // A click with Shift on a track's lane, where it is for selecting keys, leaves it alone.
  const lane = ruler.nextElementSibling!;
  on('pointerdown', 1 / 2, lane);
  on('pointerup', 1 / 2, lane);
  const kept = store.get().workArea !== null;
  // With the playhead outside it (frame 15), Play starts at its first frame.
  store.set({ frame: 15 });
  await press('Play');
  const played = [store.get().frame];
  for (let i = 0; i < 30; i++) {
    await frames(1);
    played.push(store.get().frame);
  }
  // Cleared in the middle of playing (a click with Shift), the animation plays on from
  // where the playhead is, to its end; it does not jump there.
  on('pointerdown', 1 / 2);
  on('pointerup', 1 / 2);
  await frames(1);
  const after = { playing: store.get().playing, frame: store.get().frame };
  await until('the animation to end', () => !store.get().playing);
  out.workArea = { from: area?.from, to: area?.to, kept, start: played[0], within: played.every((f) => f >= 6 && f < 12), laps: played.filter((f, i) => f < played[i - 1]!).length, cleared: store.get().workArea === null, after, end: store.get().frame };
} catch (e) {
  out.failed = e instanceof Error ? `${e.message}\n${e.stack}` : String(e);
}
out.problems = problems;
document.getElementById('result')!.textContent = JSON.stringify(out);
