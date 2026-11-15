// The live preview: compiles the document with the real runtime and draws the active
// artboard to the stage canvas ("what you see is what ships"). Also answers the stage's
// geometry questions (hit test, bounds, world matrices) from the same instance.

import type { Instance, PointerKind, ZuriFile } from '@seedexr/zuri';
import { liveValue, mapFrame, playable } from '../doc/autokey.ts';
import { type Node, atIndex, num, toSource, walk } from '../doc/model.ts';
import type { PropDef } from '../doc/schema.ts';
import { runtime } from '../runtime.ts';
import { type State, store } from '../store.ts';
import type { Mat } from './matrix.ts';

interface Pixels {
  width: number;
  height: number;
  rgba: Uint8ClampedArray;
}

/** ponytail: full-stage CPU render each frame; lower this cap if very large windows feel slow. */
const MAX_PIXEL_RATIO = 2;
const EVENT_LOG_LIMIT = 50;

async function decodeImage(base64: string): Promise<Pixels | null> {
  try {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes]), { premultiplyAlpha: 'none' });
    const ctx = new OffscreenCanvas(bitmap.width, bitmap.height).getContext('2d')!;
    ctx.drawImage(bitmap, 0, 0);
    const { width, height } = bitmap;
    bitmap.close();
    return { width, height, rgba: ctx.getImageData(0, 0, width, height).data };
  } catch {
    return null; // not an image the browser can decode; the Image simply does not draw
  }
}

class Preview {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private file: ZuriFile | null = null;
  private instance: Instance | null = null;
  /** The document `file` was compiled from: runtime object ids index into this tree. */
  private compiled: readonly Node[] = [];
  /** Runtime object id (pre-order index in `compiled`) by node id. */
  private objects = new Map<string, number>();
  /** The last document handed to the compiler, valid or not. */
  private tried: readonly Node[] | null = null;
  private instanceKey = '';
  /** The playhead frame the instance was last seeked to. */
  private seeked = NaN;
  private stale = true;
  private drawnView: State['view'] | null = null;
  private raf = 0;
  private lastTick = 0;
  /** Frames played since play was pressed (before loop mapping). */
  private elapsed = 0;
  private wasPlaying = false;
  /** Decoded image pixels by base64 payload; a Promise while decoding. */
  private images = new Map<string, Pixels | null | Promise<void>>();
  private listeners = new Set<() => void>();

  /** Bumped after every draw; the overlay re-reads geometry when it changes. */
  version = 0;
  /** Bumped when a draw shows a new pose (a frame, an edit), not a pan or zoom: live values are re-read on it. */
  pose = 0;
  /** Stage size in CSS pixels. */
  width = 0;
  height = 0;
  private ratio = 1;
  /** Events the state machine preview emitted, newest last. */
  events: { name: string; at: number }[] = [];

  constructor() {
    store.subscribe(() => this.schedule());
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  attach(canvas: HTMLCanvasElement | null): void {
    this.canvas = canvas;
    this.ctx = canvas?.getContext('2d') ?? null;
    this.invalidate();
  }

  /** Match the canvas backing store to the stage's CSS size. */
  resize(width: number, height: number): void {
    this.width = width;
    this.height = height;
    this.ratio = Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO);
    if (this.canvas) {
      this.canvas.width = Math.max(1, Math.round(width * this.ratio));
      this.canvas.height = Math.max(1, Math.round(height * this.ratio));
    }
    this.invalidate();
  }

  private invalidate(): void {
    this.stale = true;
    this.schedule();
  }

  private schedule(): void {
    this.raf ||= requestAnimationFrame(this.tick);
  }

  // ---- the frame ---------------------------------------------------------------------

  private tick = (now: number): void => {
    this.raf = 0;
    const dt = this.lastTick ? Math.min((now - this.lastTick) / 1000, 0.1) : 0;
    let s = store.get();
    const artboards = s.doc.filter((n) => n.type === 'Artboard');
    const artboard = artboards.find((n) => n.id === s.artboardId);
    const animation = artboard?.children.find((n) => n.id === s.animationId);
    const [fps, duration, speed] = [num(animation?.props.fps, 60), num(animation?.props.duration, 60), num(animation?.props.speed, 1)];

    // Timeline playback moves the store's playhead; drawing a frame is the same as scrubbing to it.
    if (s.playing) {
      if (!this.wasPlaying) this.elapsed = s.frame >= duration ? 0 : s.frame;
      this.elapsed += dt * fps * speed;
      const { frame, finished } = mapFrame(duration, animation?.props.loop, this.elapsed);
      store.set({ frame, playing: !finished });
      s = store.get();
    }
    this.wasPlaying = s.playing;

    if (s.doc !== this.tried) this.compile(s.doc);

    const machine = playable(this.compiled, s.previewing ? artboard?.children.find((n) => n.id === s.machineId) : undefined);
    // The animation the timeline shows: the instance plays it and is seeked to the playhead.
    const timeline = s.mode === 'animate' && !machine ? playable(this.compiled, animation) : undefined;
    const key = [artboards.indexOf(artboard!), machine?.id ?? '', timeline?.id ?? ''].join('|');
    if (this.file && artboard && (!this.instance || key !== this.instanceKey)) {
      this.instance?.free();
      const instance = this.file.instantiate(artboards.indexOf(artboard));
      if (machine) {
        instance.playStateMachine(machine.props.name as string);
        this.events = [];
      } else if (timeline) instance.playAnimation(timeline.props.name as string);
      instance.advance(0);
      this.instance = instance;
      this.instanceKey = key;
      this.seeked = NaN;
      this.stale = true;
    } else if (!artboard || !this.file) {
      this.instance?.free();
      this.instance = null;
      this.stale = true;
    } else if (machine && this.instance) {
      if (this.instance.advance(dt)) this.stale = true;
    }
    if (timeline && this.instance && s.frame !== this.seeked) {
      // A looping animation wraps exactly at its end, so stay a hair inside it.
      this.instance.seek((timeline.props.loop === 'loop' ? Math.min(s.frame, duration - 1e-3) : s.frame) / fps);
      this.instance.advance(0);
      this.seeked = s.frame;
      this.stale = true;
    }

    if (machine && this.instance) {
      const emitted = this.instance.events();
      if (emitted.length) {
        this.events = [...this.events, ...emitted.map((name) => ({ name, at: now }))].slice(-EVENT_LOG_LIMIT);
        this.stale = true;
      }
    }

    if (this.stale || s.view !== this.drawnView) this.draw(s);
    const running = s.playing || s.previewing;
    this.lastTick = running ? now : 0;
    if (running) this.schedule();
  };

  private compile(doc: readonly Node[]): void {
    this.tried = doc;
    try {
      const file = runtime.load(runtime.compile(toSource(doc)));
      this.file?.free();
      this.file = file;
      this.compiled = doc;
      this.objects = new Map([...walk(doc)].map((n, i) => [n.id, i]));
      this.instance?.free();
      this.instance = null;
      this.supplyImages(file, doc);
      if (store.get().error !== null) store.set({ error: null });
    } catch (e) {
      // Keep showing the last good frame; the stage shows why the new document is not drawn.
      const message = (e as Error).message.replace(/^Zuri: could not (compile|load the file): /, '');
      if (store.get().error !== message) store.set({ error: message });
    }
  }

  /** Hand decoded pixels to the file. Images still decoding arrive with a later recompile. */
  private supplyImages(file: ZuriFile, doc: readonly Node[]): void {
    let index = -1;
    for (const n of walk(doc)) {
      index++;
      const bytes = n.props.bytes;
      if (n.type !== 'ImageAsset' || typeof bytes !== 'string' || bytes.startsWith('@')) continue;
      const known = this.images.get(bytes);
      if (known === undefined) {
        const pending = decodeImage(bytes).then((pixels) => {
          this.images.set(bytes, pixels);
          this.tried = null; // recompile so the pixels reach the current file
          this.schedule();
        });
        this.images.set(bytes, pending);
      } else if (known && !(known instanceof Promise)) {
        file.setImage(index, known.width, known.height, known.rgba);
      }
    }
  }

  private draw(s: State): void {
    if (this.stale) this.pose++;
    this.stale = false;
    this.drawnView = s.view;
    const { canvas, ctx, instance } = this;
    if (!canvas || !ctx) return;
    if (!instance) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
    } else {
      const k = s.view.zoom * this.ratio;
      const pixels = instance.render(canvas.width, canvas.height, { view: [k, 0, 0, k, s.view.x * this.ratio, s.view.y * this.ratio] });
      ctx.putImageData(new ImageData(pixels as Uint8ClampedArray<ArrayBuffer>, canvas.width, canvas.height), 0, 0);
    }
    this.version++;
    for (const l of this.listeners) l();
  }

  // ---- geometry queries (the last drawn frame) -----------------------------------------

  /** Artboard-space bounding box `[x0, y0, x1, y1]` of a node and its contents. */
  bounds(nodeId: string): [number, number, number, number] | null {
    const id = this.objects.get(nodeId);
    return id === undefined ? null : (this.instance?.bounds(id) ?? null);
  }

  /** Node-to-artboard transform. */
  world(nodeId: string): Mat | null {
    const id = this.objects.get(nodeId);
    return id === undefined ? null : (this.instance?.worldMatrix(id) ?? null);
  }

  /** The top-most drawable under a stage point (CSS pixels), as a node id. */
  hit(x: number, y: number): string | null {
    const id = this.instance?.hitTest(x * this.ratio, y * this.ratio);
    return id == null ? null : (atIndex(this.compiled, id)?.id ?? null);
  }

  /**
   * What a node draws with for a number or color property (after the animation at the playhead).
   * ponytail: read from the last drawn frame, so it is one animation frame behind an edit.
   * `shownValue` covers what that would break (a key on the playhead is read from the
   * document); bring the instance up to date on read if anything else ever needs it.
   */
  value(nodeId: string, def: PropDef): number | string | undefined {
    return this.instance ? liveValue(this.instance, this.objects.get(nodeId) ?? -1, def) : undefined;
  }

  // ---- state machine preview -----------------------------------------------------------

  /** The running instance while a state machine is being previewed. */
  get live(): Instance | null {
    return store.get().previewing ? this.instance : null;
  }

  /** Node ids of the State each layer of the previewed state machine is in. */
  get activeStates(): (string | undefined)[] {
    return this.live?.layerStates.map((id) => (id === null ? undefined : atIndex(this.compiled, id)?.id)) ?? [];
  }

  pointer(kind: PointerKind, x: number, y: number): void {
    this.live?.pointer(kind, x * this.ratio, y * this.ratio);
  }
}

export const preview = new Preview();
