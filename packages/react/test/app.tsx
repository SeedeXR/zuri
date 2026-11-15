// What test/browser.mjs loads: <Zuri> and useZuri (the built package) in a small app, in
// StrictMode, which mounts every effect twice in development. Players are counted as
// they are made and destroyed, so one that is left behind, or made too often, shows.
import { StrictMode, useEffect, useState } from 'react';
import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';
import { ZuriPlayer, type Fit } from '@seedexr/zuri';
import { Zuri, useZuri } from '../dist/index.js';

const out: Record<string, unknown> = {};
const made: ZuriPlayer[] = [];
const dead = new WeakSet<ZuriPlayer>();
const create = ZuriPlayer.create;
ZuriPlayer.create = async (...args) => {
  const player = await create(...args);
  made.push(player);
  return player;
};
const destroy = ZuriPlayer.prototype.destroy;
ZuriPlayer.prototype.destroy = function () {
  dead.add(this);
  destroy.call(this);
};
/** [players made so far, players alive]. */
const players = () => [made.length, made.filter((player) => !dead.has(player)).length];

const loads: ZuriPlayer[] = [];
const heard: string[] = [];
const errors: string[] = [];
let handedDead = 0;
let set!: { src: (v: string) => void; label: (v: string) => void; fit: (v: Fit) => void; shown: (v: boolean) => void; late: (v: boolean) => void; hookSrc: (v: string) => void };
let hook!: ReturnType<typeof useZuri>;

function App() {
  const [src, setSrc] = useState('/button.zur');
  const [label, setLabel] = useState('first');
  const [fit, setFit] = useState<Fit>('contain');
  const [shown, setShown] = useState(true);
  const [late, setLate] = useState(false);
  const [hookSrc, setHookSrc] = useState('/hello.zur');
  set = { src: setSrc, label: setLabel, fit: setFit, shown: setShown, late: setLate, hookSrc: setHookSrc };
  // A URL made anew on every render, and a canvas that is not there at first.
  // (Headless browsers may have no display to drive requestAnimationFrame: the checks tick the players.)
  hook = useZuri({ src: new URL(hookSrc, location.href), maxPixelRatio: 1, autoplay: false });
  const { player } = hook;
  // What a component does with the player: it must never be handed one that is gone.
  useEffect(() => {
    if (player && dead.has(player)) handedDead++;
  }, [player, hookSrc]);
  return (
    <>
      {shown && (
        <Zuri
          src={src}
          fit={fit}
          maxPixelRatio={1}
          autoplay={false}
          style={{ display: 'block', width: 320, height: 160 }}
          onLoad={(player) => loads.push(player)}
          onEvent={(name) => heard.push(`${label}:${name}`)}
          onError={(error) => errors.push(error.message)}
        />
      )}
      {late && <canvas id="late" ref={hook.canvasRef} style={{ display: 'block', width: 400, height: 300 }} />}
    </>
  );
}

const step = () => new Promise((done) => setTimeout(done, 0));
const until = async (condition: () => boolean, what: string) => {
  for (let i = 0; i < 2000 && !condition(); i++) await step();
  if (!condition()) throw new Error(`never happened: ${what}`);
};
const frames = async (player: ZuriPlayer, n: number) => {
  for (let i = 0; i < n; i++) {
    player.tick(1 / 60);
    await step();
  }
};
const pixel = (canvas: HTMLCanvasElement, x: number, y: number) => [...canvas.getContext('2d')!.getImageData(x, y, 1, 1).data];
const click = (canvas: HTMLCanvasElement) => {
  const rect = canvas.getBoundingClientRect();
  for (const type of ['pointermove', 'pointerdown', 'pointerup']) canvas.dispatchEvent(new PointerEvent(type, { clientX: rect.left + 160, clientY: rect.top + 80, bubbles: true }));
};

try {
  const root = createRoot(document.getElementById('root')!);
  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
  // <Zuri>: one player, though StrictMode mounted it twice.
  await until(() => loads.length > 0, 'onLoad');
  await frames(loads[0], 3);
  for (let i = 0; i < 20; i++) await step();
  const canvas = loads[0].canvas;
  out.mounted = { loads: loads.length, players: players(), size: [canvas.width, canvas.height], idle: pixel(canvas, 100, 80) };
  click(canvas);
  await frames(loads[0], 4);
  // A re-render with a new onEvent keeps the player and uses the new callback. (The
  // button is a switch that reports when it goes on: every other click.)
  set.label('second');
  await step();
  for (let i = 0; i < 2; i++) {
    click(canvas);
    await frames(loads[0], 4);
  }
  out.events = { heard: [...heard], loads: loads.length };

  // useZuri: no canvas yet, so no player; when the canvas appears, one player draws on it.
  out.early = [hook.player, hook.error];
  set.late(true);
  await until(() => hook.player !== null, 'the hook player');
  await frames(hook.player!, 3);
  for (let i = 0; i < 20; i++) await step();
  const late = document.querySelector<HTMLCanvasElement>('#late')!;
  out.hook = { drawn: pixel(late, 200, 150)[3], error: hook.error, players: players() };
  // Another file for the hook, and another before the first has loaded: the old player
  // goes, the one that was overtaken is destroyed when it arrives, and no effect is
  // handed a player that is gone.
  // (Committed one after the other, with no time between them for a file to arrive.)
  flushSync(() => set.hookSrc('/button.zur'));
  flushSync(() => set.hookSrc('/text.zur'));
  await until(() => made.length >= 4 && hook.player !== null, 'the hook reloading');
  for (let i = 0; i < 20; i++) await step();
  out.hookReloaded = { players: players(), current: hook.player === made.filter((player) => !dead.has(player) && player.canvas === late).at(-1), handedDead };

  // An option the player is made from: a new player on the same canvas, drawing; the old one destroyed.
  set.fit('cover');
  await until(() => loads.length > 1, 'the second onLoad');
  await frames(loads[1], 3);
  out.refit = { sameCanvas: loads[1].canvas === canvas, drawn: pixel(canvas, 100, 80)[3], oldGone: dead.has(loads[0]), players: players() };

  // Unmounting <Zuri> destroys its player and leaves the rest running.
  set.shown(false);
  await step();
  out.unmounted = { gone: dead.has(loads[1]), canvases: document.querySelectorAll('canvas').length, players: players() };

  // A file that is not there is an error, once; and the page is left with nothing alive.
  set.src('/missing.zur');
  set.shown(true);
  await until(() => errors.length > 0, 'onError');
  for (let i = 0; i < 20; i++) await step();
  root.unmount();
  await step();
  out.end = { errors: [...errors], alive: players()[1], handedDead };
  out.ok = true;
} catch (error) {
  out.error = String(error);
}
document.getElementById('result')!.textContent = JSON.stringify(out);
