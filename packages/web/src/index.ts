/**
 * Zuri web runtime.
 *
 * Two levels:
 *  - `ZuriPlayer.create(canvas, { src })` plays a `.zur` file on a canvas and handles
 *    sizing, the frame loop, pointer input and shader paints for you.
 *  - `loadRuntime()` -> `Runtime` -> `ZuriFile` -> `Instance` is the low-level API the
 *    player is built on; use it for custom render loops, workers or tooling.
 *
 * The runtime is the same Rust core that ships in the Unity and Unreal plugins, compiled
 * to WebAssembly and called through its C ABI. No JavaScript objects are allocated per
 * frame: pixels are read straight out of wasm memory.
 */

type Ptr = number;
// The wasm module's C ABI (see crates/zuri-ffi/include/zuri.h).
type Exports = { memory: WebAssembly.Memory } & Record<string, (...args: number[]) => number>;

/** Raw bytes in any of the forms browsers and Node hand them out. */
export type Bytes = ArrayBufferLike | ArrayBufferView;
export type WasmSource = string | URL | Response | Promise<Response> | Bytes | WebAssembly.Module;
/**
 * How the artboard is scaled into the surface. `fitWidth` / `fitHeight` match that side and
 * keep the aspect ratio; `scaleDown` is `contain` when the artboard is larger than the
 * surface, and its own size otherwise.
 */
export type Fit = 'contain' | 'cover' | 'fill' | 'none' | 'fitWidth' | 'fitHeight' | 'scaleDown';
/** Where the artboard sits in the room left over, or which part shows when it overflows. */
export type Alignment = 'center' | 'topLeft' | 'topCenter' | 'topRight' | 'centerLeft' | 'centerRight' | 'bottomLeft' | 'bottomCenter' | 'bottomRight';
export type PointerKind = 'move' | 'down' | 'up' | 'exit';
export type PropertyType = 'number' | 'boolean' | 'trigger' | 'color' | 'string' | 'enum';
/** Artboard-to-pixel transform `[a, b, c, d, tx, ty]`: x' = a*x + c*y + tx, y' = b*x + d*y + ty. */
export type Matrix = [number, number, number, number, number, number];

export interface RenderOptions {
  fit?: Fit;
  /** Default `center`. */
  align?: Alignment;
  /** `#rrggbb` / `#rrggbbaa` or 0xAARRGGBB. Default transparent. */
  background?: string | number;
  /** Explicit transform (zoom / pan). Overrides `fit`. */
  view?: Matrix;
}

export interface ShaderRequest {
  paint: number;
  asset: number;
  width: number;
  height: number;
  params: [number, number, number, number];
  time: number;
}

// Both in the order of ZURI_FIT_* / ZURI_ALIGN_* in zuri.h.
const FITS: Fit[] = ['contain', 'cover', 'fill', 'none', 'fitWidth', 'fitHeight', 'scaleDown'];
const ALIGNS: Alignment[] = ['center', 'topLeft', 'topCenter', 'topRight', 'centerLeft', 'centerRight', 'bottomLeft', 'bottomCenter', 'bottomRight'];
const POINTERS: PointerKind[] = ['move', 'down', 'up', 'exit'];
const TYPES: PropertyType[] = ['number', 'boolean', 'trigger', 'color', 'string', 'enum'];
const NONE = 0xffffffff;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** `#rgb`, `#rrggbb`, `#rrggbbaa` or a 0xAARRGGBB number -> 0xAARRGGBB. */
export function parseColor(color: string | number): number {
  if (typeof color === 'number') return color >>> 0;
  let hex = color.replace('#', '');
  if (hex.length === 3 || hex.length === 4) hex = [...hex].map((c) => c + c).join('');
  if (!/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(hex)) throw new Error(`Zuri: "${color}" is not a color like #rrggbb`);
  const rgb = parseInt(hex.slice(0, 6), 16);
  const alpha = hex.length === 8 ? parseInt(hex.slice(6), 16) : 0xff;
  return ((alpha << 24) | rgb) >>> 0;
}

/** 0xAARRGGBB -> `#rrggbb` (or `#rrggbbaa` when not opaque). */
export function formatColor(argb: number): string {
  const rgb = (argb & 0xffffff).toString(16).padStart(6, '0');
  const alpha = argb >>> 24;
  return alpha === 0xff ? `#${rgb}` : `#${rgb}${alpha.toString(16).padStart(2, '0')}`;
}

function isBytes(value: unknown): value is Bytes {
  return ArrayBuffer.isView(value) || value instanceof ArrayBuffer || (typeof SharedArrayBuffer !== 'undefined' && value instanceof SharedArrayBuffer);
}
function view(bytes: Bytes): Uint8Array {
  return ArrayBuffer.isView(bytes) ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength) : new Uint8Array(bytes);
}

async function instantiate(source: WasmSource): Promise<Exports> {
  const exportsOf = (r: WebAssembly.Instance | WebAssembly.WebAssemblyInstantiatedSource) => ('instance' in r ? r.instance : r).exports as unknown as Exports;
  if (source instanceof WebAssembly.Module) return exportsOf(await WebAssembly.instantiate(source, {}));
  if (isBytes(source)) return exportsOf(await WebAssembly.instantiate(view(source) as BufferSource, {}));
  let pending: Response | Promise<Response>;
  if (typeof source === 'string' || source instanceof URL) {
    const url = new URL(source, typeof location === 'undefined' ? import.meta.url : location.href);
    if (url.protocol === 'file:') {
      // Node (tests, SSR tooling): fetch() cannot read file: URLs. The specifier is a
      // variable so bundlers targeting the browser do not try to resolve it.
      const fs = 'node:fs/promises';
      const { readFile } = await import(/* @vite-ignore */ fs);
      return exportsOf(await WebAssembly.instantiate(await readFile(url), {}));
    }
    pending = fetch(url);
  } else {
    pending = source;
  }
  const response = await pending;
  if (!response.ok) throw new Error(`Zuri: could not fetch the wasm runtime (${response.status} ${response.url})`);
  try {
    return exportsOf(await WebAssembly.instantiateStreaming(response.clone(), {}));
  } catch {
    // Servers that do not send application/wasm cannot stream-compile.
    return exportsOf(await WebAssembly.instantiate(await response.arrayBuffer(), {}));
  }
}

let shared: Promise<Runtime> | undefined;

/**
 * Load the wasm runtime. With no argument the default player build next to this module is
 * loaded once and shared. Pass a URL, bytes, a `Response` or a compiled module to choose
 * another build (`zuri-lite.wasm`, `zuri-editor.wasm`) or to control hosting.
 */
export function loadRuntime(wasm?: WasmSource): Promise<Runtime> {
  if (wasm !== undefined) return instantiate(wasm).then((x) => new Runtime(x));
  if (!shared) {
    const loading = (shared = instantiate(new URL('./zuri.wasm', import.meta.url)).then((x) => new Runtime(x)));
    // A failed load (network blip) must not be what every later call gets.
    loading.catch(() => {
      if (shared === loading) shared = undefined;
    });
  }
  return shared;
}

export class Runtime {
  /** @internal */
  constructor(readonly x: Exports) {
    if (x.zuri_abi_version() !== 1) throw new Error('Zuri: the wasm runtime does not match this version of the package');
  }

  /** @internal */
  bytes(ptr: Ptr, len: number): Uint8Array {
    return new Uint8Array(this.x.memory.buffer, ptr, len);
  }
  /** @internal Copy data into wasm memory. The caller frees it with `zuri_free`. */
  put(data: Uint8Array): Ptr {
    const ptr = this.x.zuri_alloc(data.length);
    this.bytes(ptr, data.length).set(data);
    return ptr;
  }
  /** @internal Run `f` with data copied into wasm memory, then release it. */
  with<T>(data: Uint8Array | string, f: (ptr: Ptr, len: number) => T): T {
    const raw = typeof data === 'string' ? encoder.encode(data) : data;
    const ptr = this.put(raw);
    try {
      return f(ptr, raw.length);
    } finally {
      this.x.zuri_free(ptr, raw.length);
    }
  }
  /** @internal Read a "string out" C function: measure, then fill. */
  string(call: (buf: Ptr, cap: number) => number): string {
    const len = call(0, 0);
    if (!len) return '';
    const ptr = this.x.zuri_alloc(len);
    try {
      call(ptr, len);
      return decoder.decode(this.bytes(ptr, len));
    } finally {
      this.x.zuri_free(ptr, len);
    }
  }
  /** @internal */
  error(what: string): Error {
    return new Error(`Zuri: ${what}: ${this.string((b, c) => this.x.zuri_last_error(b, c)) || 'unknown error'}`);
  }
  /** @internal Take ownership of a buffer returned through an `out_len` pointer. */
  private take(call: (outLen: Ptr) => Ptr, what: string): Uint8Array {
    const outLen = this.x.zuri_alloc(4);
    try {
      const ptr = call(outLen);
      if (!ptr) throw this.error(what);
      const len = new Uint32Array(this.x.memory.buffer, outLen, 1)[0];
      const copy = this.bytes(ptr, len).slice();
      this.x.zuri_free(ptr, len);
      return copy;
    } finally {
      this.x.zuri_free(outLen, 4);
    }
  }

  /** Parse a `.zur` file. Throws with the reason if it is not valid. */
  load(bytes: Bytes): ZuriFile {
    const handle = this.with(view(bytes), (ptr, len) => this.x.zuri_file_load(ptr, len));
    if (!handle) throw this.error('could not load the file');
    return new ZuriFile(this, handle);
  }

  /** True when this build can compile JSON source (the `zuri-editor.wasm` build). */
  get canCompile(): boolean {
    return 'zuri_json_to_zur' in this.x;
  }
  private authoring(): void {
    if (!this.canCompile) throw new Error('Zuri: this needs the editor build; load it with loadRuntime(new URL("@seedexr/zuri/zuri-editor.wasm", import.meta.url))');
  }
  /** Compile JSON source to `.zur` bytes. Errors name the JSON path and the fix. */
  compile(json: string): Uint8Array {
    this.authoring();
    return this.with(json, (ptr, len) => this.take((out) => this.x.zuri_json_to_zur(ptr, len, out), 'could not compile'));
  }
  /** Turn `.zur` bytes back into JSON source. */
  decompile(zur: Uint8Array): string {
    this.authoring();
    return decoder.decode(this.with(zur, (ptr, len) => this.take((out) => this.x.zuri_zur_to_json(ptr, len, out), 'could not decompile')));
  }
  /** Every object type and property, as parsed JSON. */
  schema(): unknown {
    this.authoring();
    return JSON.parse(decoder.decode(this.take((out) => this.x.zuri_schema_json(out), 'could not read the schema')));
  }
}

export class ZuriFile {
  private handle: Ptr;
  /** @internal */
  constructor(readonly runtime: Runtime, handle: Ptr) {
    this.handle = handle;
  }

  /** Artboard names, in file order. */
  get artboards(): string[] {
    const { x } = this.runtime;
    return Array.from({ length: x.zuri_file_artboard_count(this.handle) }, (_, i) => this.runtime.string((b, c) => x.zuri_file_artboard_name(this.handle, i, b, c)));
  }

  /** Encoded image assets (PNG / JPEG / WebP) that need decoding before they can draw. */
  get images(): { id: number; bytes: Uint8Array }[] {
    return this.assets('image');
  }

  /** Encoded audio assets: the sounds that events of this file play (see `Instance.reports`). */
  get sounds(): { id: number; bytes: Uint8Array }[] {
    return this.assets('sound');
  }

  private assets(kind: 'image' | 'sound'): { id: number; bytes: Uint8Array }[] {
    const { x } = this.runtime;
    const outLen = x.zuri_alloc(4);
    const list = Array.from({ length: x[`zuri_file_${kind}_count`](this.handle) }, (_, i) => {
      const id = x[`zuri_file_${kind}_id`](this.handle, i);
      const ptr = x[`zuri_file_${kind}_bytes`](this.handle, id, outLen);
      return { id, bytes: this.runtime.bytes(ptr, new Uint32Array(x.memory.buffer, outLen, 1)[0]).slice() };
    });
    x.zuri_free(outLen, 4);
    return list;
  }

  /** Supply decoded pixels (straight-alpha RGBA) for an image asset. */
  setImage(id: number, width: number, height: number, rgba: Uint8Array | Uint8ClampedArray): void {
    const ok = this.runtime.with(new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength), (ptr) => this.runtime.x.zuri_file_set_image(this.handle, id, width, height, ptr));
    if (!ok) throw this.runtime.error('could not set the image');
  }

  /** Decode every image asset with the browser's decoders. Resolves when all are ready. */
  async decodeImages(): Promise<void> {
    await Promise.all(
      this.images.map(async ({ id, bytes }) => {
        const bitmap = await createImageBitmap(new Blob([bytes as BlobPart]), { premultiplyAlpha: 'none' });
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = canvas.getContext('2d')!;
        ctx.drawImage(bitmap, 0, 0);
        this.setImage(id, bitmap.width, bitmap.height, ctx.getImageData(0, 0, bitmap.width, bitmap.height).data);
        bitmap.close();
      }),
    );
  }

  /** Source of a shader asset in the given language. Empty if the file does not carry it. */
  shaderSource(asset: number, language: 'wgsl' | 'glsl' | 'hlsl'): string {
    const lang = ['wgsl', 'glsl', 'hlsl'].indexOf(language);
    return this.runtime.string((b, c) => this.runtime.x.zuri_file_shader_source(this.handle, asset, lang, b, c));
  }

  /** Create a playable instance of an artboard (by name or index; default the first). */
  instantiate(artboard: number | string = 0): Instance {
    const index = typeof artboard === 'number' ? artboard : this.artboards.indexOf(artboard);
    if (index < 0) throw new Error(`Zuri: no artboard "${artboard}". Artboards: ${this.artboards.join(', ')}`);
    const handle = this.runtime.x.zuri_instance_new(this.handle, index);
    if (!handle) throw this.runtime.error('could not create the instance');
    return new Instance(this.runtime, handle);
  }

  /** Release the file. Instances created from it stay valid. */
  free(): void {
    this.runtime.x.zuri_file_free(this.handle);
    this.handle = 0;
  }
}

export class Instance {
  private handle: Ptr;
  private names = new Map<string, [Ptr, number]>();
  private pixels: Ptr = 0;
  private pixelsLen = 0;
  private scratch: Ptr;

  /** @internal */
  constructor(private runtime: Runtime, handle: Ptr) {
    this.handle = handle;
    this.scratch = runtime.x.zuri_alloc(40);
  }

  private get x(): Exports {
    return this.runtime.x;
  }
  /** Names are encoded into wasm memory once, so per-frame property writes allocate nothing. */
  private name(name: string): [Ptr, number] {
    let entry = this.names.get(name);
    if (!entry) {
      const raw = encoder.encode(name);
      entry = [this.runtime.put(raw), raw.length];
      this.names.set(name, entry);
    }
    return entry;
  }
  private list(count: string, name: string): string[] {
    return Array.from({ length: this.x[count](this.handle) }, (_, i) => this.runtime.string((b, c) => this.x[name](this.handle, i, b, c)));
  }
  private f32(): Float32Array {
    return new Float32Array(this.x.memory.buffer, this.scratch, 10);
  }

  /** Artboard size in its own units. */
  get width(): number {
    return this.x.zuri_instance_width(this.handle);
  }
  get height(): number {
    return this.x.zuri_instance_height(this.handle);
  }
  get animations(): string[] {
    return this.list('zuri_instance_animation_count', 'zuri_instance_animation_name');
  }
  get stateMachines(): string[] {
    return this.list('zuri_instance_state_machine_count', 'zuri_instance_state_machine_name');
  }
  /**
   * The view-model properties: the data contract between this artboard and your app. An
   * `enum` lists its `options`: set it with `setString(name, option)` or, by position, with
   * `setNumber`.
   */
  get properties(): { name: string; type: PropertyType; options?: string[] }[] {
    return this.list('zuri_instance_property_count', 'zuri_instance_property_name').map((name, i) => {
      const type = TYPES[this.x.zuri_instance_property_type(this.handle, i)];
      if (type !== 'enum') return { name, type };
      const count = this.x.zuri_instance_property_option_count(this.handle, i);
      const options = Array.from({ length: count }, (_, o) => this.runtime.string((buf, cap) => this.x.zuri_instance_property_option_name(this.handle, i, o, buf, cap)));
      return { name, type, options };
    });
  }

  /**
   * Start playback. With a name, plays that state machine or animation. With no name,
   * plays the first state machine, or the first animation if there is none.
   */
  play(name?: string): void {
    if (name === undefined) {
      this.x.zuri_instance_autoplay(this.handle);
      return;
    }
    if (this.stateMachines.includes(name)) this.playStateMachine(name);
    else if (this.animations.includes(name)) this.playAnimation(name);
    else throw new Error(`Zuri: nothing to play named "${name}". State machines: ${this.stateMachines.join(', ') || '(none)'}. Animations: ${this.animations.join(', ') || '(none)'}`);
  }

  /** Play a state machine by name (use this when an animation shares the name). */
  playStateMachine(name: string): void {
    const index = this.stateMachines.indexOf(name);
    if (index < 0) throw new Error(`Zuri: no state machine named "${name}". State machines: ${this.stateMachines.join(', ') || '(none)'}`);
    this.x.zuri_instance_play_state_machine(this.handle, index);
  }

  /** Play one animation directly, without a state machine. */
  playAnimation(name: string): void {
    const index = this.animations.indexOf(name);
    if (index < 0) throw new Error(`Zuri: no animation named "${name}". Animations: ${this.animations.join(', ') || '(none)'}`);
    this.x.zuri_instance_play_animation(this.handle, index);
  }

  /**
   * Jump the animation started with `playAnimation` to `seconds` on its timeline. The next
   * `advance` (0 is fine) shows it. Throws when no animation is playing directly.
   */
  seek(seconds: number): void {
    if (!this.x.zuri_instance_seek(this.handle, seconds)) throw new Error('Zuri: seek needs an animation started with playAnimation()');
  }

  /** Names of the events this artboard declares (what `events()` can report). */
  get declaredEvents(): string[] {
    return this.list('zuri_instance_declared_event_count', 'zuri_instance_declared_event_name');
  }

  /** Object ids of the State each layer of the playing state machine is in (null: none). */
  get layerStates(): (number | null)[] {
    return Array.from({ length: this.x.zuri_instance_layer_count(this.handle) }, (_, i) => {
      const id = this.x.zuri_instance_layer_state(this.handle, i) >>> 0;
      return id === NONE ? null : id;
    });
  }

  /** Advance by `seconds`. Returns false when nothing changed, so the draw can be skipped. */
  advance(seconds: number): boolean {
    return this.x.zuri_instance_advance(this.handle, seconds) !== 0;
  }

  private missing(name: string): Error {
    return new Error(`Zuri: no view-model property "${name}". Properties: ${this.properties.map((p) => p.name).join(', ') || '(none)'}`);
  }
  setNumber(name: string, value: number): void {
    const [ptr, len] = this.name(name);
    if (!this.x.zuri_instance_set_number(this.handle, ptr, len, value)) throw this.missing(name);
  }
  getNumber(name: string): number {
    const [ptr, len] = this.name(name);
    if (!this.x.zuri_instance_get_number(this.handle, ptr, len, this.scratch)) throw this.missing(name);
    return this.f32()[0];
  }
  setBoolean(name: string, value: boolean): void {
    this.setNumber(name, value ? 1 : 0);
  }
  getBoolean(name: string): boolean {
    return this.getNumber(name) !== 0;
  }
  setColor(name: string, color: string | number): void {
    const [ptr, len] = this.name(name);
    if (!this.x.zuri_instance_set_color(this.handle, ptr, len, parseColor(color))) throw this.missing(name);
  }
  getColor(name: string): string {
    const [ptr, len] = this.name(name);
    if (!this.x.zuri_instance_get_color(this.handle, ptr, len, this.scratch)) throw this.missing(name);
    return formatColor(new Uint32Array(this.x.memory.buffer, this.scratch, 1)[0]);
  }
  setString(name: string, value: string): void {
    const [ptr, len] = this.name(name);
    if (!this.runtime.with(value, (v, vlen) => this.x.zuri_instance_set_string(this.handle, ptr, len, v, vlen))) throw this.missing(name);
  }
  getString(name: string): string {
    const [ptr, len] = this.name(name);
    return this.runtime.string((b, c) => this.x.zuri_instance_get_string(this.handle, ptr, len, b, c));
  }
  /** Fire a trigger property. */
  fire(name: string): void {
    const [ptr, len] = this.name(name);
    if (!this.x.zuri_instance_fire(this.handle, ptr, len)) throw this.missing(name);
  }

  /**
   * Report a key going down or up to the scene's `keyDown` / `keyUp` Listeners. `key` is
   * `KeyboardEvent.key`. True when a Listener reacted.
   */
  key(key: string, down: boolean): boolean {
    const [ptr, len] = this.name(key);
    return this.x.zuri_instance_key(this.handle, ptr, len, down ? 1 : 0) !== 0;
  }

  /** Pointer input in the pixel space of the last `render`. */
  pointer(kind: PointerKind, x: number, y: number): void {
    this.x.zuri_instance_pointer(this.handle, POINTERS.indexOf(kind), x, y);
  }

  /** Names of the events emitted since the last call. */
  events(): string[] {
    return this.reports().map((event) => event.name);
  }

  /**
   * The events emitted since the last call (this or `events`), each with the sound it
   * plays: the id of an audio asset (see `ZuriFile.sounds`), or null, and how loud, 0..1.
   */
  reports(): { name: string; sound: number | null; volume: number }[] {
    const { x } = this;
    const count = x.zuri_instance_poll_events(this.handle);
    const out = x.zuri_alloc(4);
    const list = Array.from({ length: count }, (_, i) => {
      const sound = x.zuri_instance_event_sound(this.handle, i, out) >>> 0;
      return {
        name: this.runtime.string((b, c) => x.zuri_instance_event_name(this.handle, i, b, c)),
        sound: sound === 0xffffffff ? null : sound,
        volume: sound === 0xffffffff ? 1 : new Float32Array(x.memory.buffer, out, 1)[0],
      };
    });
    x.zuri_free(out, 4);
    return list;
  }

  /**
   * Render a frame. Returns straight-alpha RGBA pixels that live in wasm memory: use them
   * (for example `new ImageData(pixels, width, height)`) before the next call into Zuri.
   */
  render(width: number, height: number, options: RenderOptions = {}): Uint8ClampedArray {
    const len = width * height * 4;
    if (len !== this.pixelsLen) {
      if (this.pixels) this.x.zuri_free(this.pixels, this.pixelsLen);
      this.pixels = this.x.zuri_alloc(len);
      this.pixelsLen = len;
    }
    const background = options.background === undefined ? 0 : parseColor(options.background);
    let ok: number;
    if (options.view) {
      this.f32().set(options.view);
      ok = this.x.zuri_instance_render_view(this.handle, this.pixels, width, height, this.scratch, background, 0);
    } else {
      ok = this.x.zuri_instance_render(this.handle, this.pixels, width, height, // A name the runtime does not know becomes a byte it does not know: contain, center.
        (FITS.indexOf(options.fit ?? 'contain') & 0xff) | (ALIGNS.indexOf(options.align ?? 'center') << 8), background, 0);
    }
    if (!ok) throw this.runtime.error('could not render');
    return new Uint8ClampedArray(this.x.memory.buffer, this.pixels, len);
  }

  /** Shader paints the last render needs pixels for (see `ShaderHost`). */
  shaderRequests(): ShaderRequest[] {
    return Array.from({ length: this.x.zuri_instance_shader_request_count(this.handle) }, (_, i) => {
      this.x.zuri_instance_shader_request(this.handle, i, this.scratch);
      const u = new Uint32Array(this.x.memory.buffer, this.scratch, 4);
      const f = new Float32Array(this.x.memory.buffer, this.scratch + 16, 5);
      return { paint: u[0], asset: u[1], width: u[2], height: u[3], params: [f[0], f[1], f[2], f[3]], time: f[4] };
    });
  }
  /** Supply the pixels of a shader paint (straight-alpha RGBA, rows top to bottom). */
  setShaderTexture(paint: number, width: number, height: number, rgba: Uint8Array): void {
    this.runtime.with(rgba, (ptr) => this.x.zuri_instance_set_shader_texture(this.handle, paint, width, height, ptr));
  }

  /** Id of the top-most object under a pixel of the last render, or null. For editors. */
  hitTest(x: number, y: number): number | null {
    const id = this.x.zuri_instance_hit_test(this.handle, x, y) >>> 0;
    return id === NONE ? null : id;
  }
  /**
   * The value an object currently draws with, after animation and data binding, for a
   * schema property key (see `zuri schema`). Numbers, booleans and enums come back as a
   * number; null if the object has no such number property (colors: `propertyColor`).
   */
  property(object: number, key: number): number | null {
    return this.x.zuri_instance_get_property(this.handle, object, key, this.scratch) ? this.f32()[0] : null;
  }
  /** Like `property`, for color properties: 0xAARRGGBB, or null. */
  propertyColor(object: number, key: number): number | null {
    return this.x.zuri_instance_get_property_color(this.handle, object, key, this.scratch) ? new Uint32Array(this.x.memory.buffer, this.scratch, 1)[0] : null;
  }

  /** Artboard-space bounding box `[x0, y0, x1, y1]` of an object and its contents. */
  bounds(object: number): [number, number, number, number] | null {
    if (!this.x.zuri_instance_bounds(this.handle, object, this.scratch)) return null;
    const f = this.f32();
    return [f[0], f[1], f[2], f[3]];
  }
  /** Object-to-artboard transform. */
  worldMatrix(object: number): Matrix | null {
    if (!this.x.zuri_instance_world_matrix(this.handle, object, this.scratch)) return null;
    const f = this.f32();
    return [f[0], f[1], f[2], f[3], f[4], f[5]];
  }

  free(): void {
    if (!this.handle) return;
    for (const [ptr, len] of this.names.values()) this.x.zuri_free(ptr, len);
    if (this.pixels) this.x.zuri_free(this.pixels, this.pixelsLen);
    this.x.zuri_free(this.scratch, 40);
    this.x.zuri_instance_free(this.handle);
    this.handle = 0;
  }
}

/**
 * Draws shader paints with WebGL2 and hands the pixels back to an instance.
 * The GLSL comes from the file (`zuri build` translates each shader's WGSL).
 */
export class ShaderHost {
  private canvas: OffscreenCanvas | HTMLCanvasElement;
  private gl: WebGL2RenderingContext | null;
  private programs = new Map<number, WebGLProgram | null>();
  private uniforms: WebGLBuffer | null = null;

  constructor(private file: ZuriFile) {
    this.canvas = typeof OffscreenCanvas === 'undefined' ? document.createElement('canvas') : new OffscreenCanvas(1, 1);
    this.gl = this.canvas.getContext('webgl2', { premultipliedAlpha: false, alpha: true, antialias: false }) as WebGL2RenderingContext | null;
  }

  private program(asset: number): WebGLProgram | null {
    const gl = this.gl!;
    if (this.programs.has(asset)) return this.programs.get(asset)!;
    const compile = (type: number, source: string) => {
      const shader = gl.createShader(type)!;
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) console.warn('Zuri: shader failed to compile:', gl.getShaderInfoLog(shader));
      return shader;
    };
    // One triangle that covers the viewport.
    const vertex = '#version 300 es\nvoid main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.0-1.0,0.0,1.0);}';
    const program = gl.createProgram()!;
    gl.attachShader(program, compile(gl.VERTEX_SHADER, vertex));
    gl.attachShader(program, compile(gl.FRAGMENT_SHADER, this.file.shaderSource(asset, 'glsl')));
    gl.linkProgram(program);
    const ok = gl.getProgramParameter(program, gl.LINK_STATUS) as boolean;
    if (ok && gl.getProgramParameter(program, gl.ACTIVE_UNIFORM_BLOCKS) > 0) gl.uniformBlockBinding(program, 0, 0);
    this.programs.set(asset, ok ? program : null);
    return ok ? program : null;
  }

  /** Fulfil every pending shader request of `instance`. */
  update(instance: Instance): void {
    const gl = this.gl;
    if (!gl) return;
    for (const request of instance.shaderRequests()) {
      const program = this.program(request.asset);
      if (!program) continue;
      const { width, height } = request;
      if (this.canvas.width !== width || this.canvas.height !== height) {
        this.canvas.width = width;
        this.canvas.height = height;
      }
      gl.viewport(0, 0, width, height);
      gl.useProgram(program);
      // struct Zuri { resolution: vec2f, time: f32, _pad: f32, params: vec4f } in std140.
      this.uniforms ??= gl.createBuffer();
      gl.bindBuffer(gl.UNIFORM_BUFFER, this.uniforms);
      gl.bufferData(gl.UNIFORM_BUFFER, new Float32Array([width, height, request.time, 0, ...request.params]), gl.DYNAMIC_DRAW);
      gl.bindBufferBase(gl.UNIFORM_BUFFER, 0, this.uniforms);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      // WGSL's position has y = 0 at the top; readPixels returns the row with
      // gl_FragCoord.y = 0.5 first. Treating that row as the top row matches WGSL exactly.
      const pixels = new Uint8Array(width * height * 4);
      gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      instance.setShaderTexture(request.paint, width, height, pixels);
    }
  }
}

export interface PlayerOptions {
  /** URL of a `.zur` file, or its bytes. */
  src: string | URL | Bytes;
  /** Artboard name or index. Default: the first. */
  artboard?: number | string;
  /** Name of the state machine or animation to play. Default: first state machine, else first animation. */
  play?: string;
  /** Start playing immediately. Default true. */
  autoplay?: boolean;
  fit?: Fit;
  /** Default `center`. */
  align?: Alignment;
  /** Color behind the artboard. Default transparent. */
  background?: string;
  /** Cap on devicePixelRatio, to bound the pixel count on very dense screens. Default 2. */
  maxPixelRatio?: number;
  /** Custom wasm build or location. */
  wasm?: WasmSource;
  /** Called for each event the artboard emits. */
  onEvent?: (name: string) => void;
}

// Sound goes out through one AudioContext for the page, shared by every player and
// never closed (so a sound outlives the player that started it). Browsers hold audio
// back until the person has touched the page: the output is opened at the first touch
// or key anywhere on it, and woken again at later ones (a browser may put it to sleep).
let speaker: AudioContext | undefined;
let listening = false;
function listenForTouch(): void {
  if (listening || typeof AudioContext === 'undefined') return;
  listening = true;
  const wake = () => {
    try {
      speaker ??= new AudioContext();
      // A touch counts when the finger lifts, a mouse press when it goes down.
      if (speaker.state !== 'running') speaker.resume().catch(() => {});
    } catch {
      // No audio output to be had: the page stays silent.
    }
  };
  for (const type of ['pointerdown', 'pointerup', 'keydown']) addEventListener(type, wake, { capture: true, passive: true });
}

/** Plays a `.zur` file on a canvas: sizing, frame loop, pointer input, shader paints, sounds. */
export class ZuriPlayer {
  private frame = 0;
  private last = 0;
  private stale = true;
  private visible = true;
  private observers: { disconnect(): void }[] = [];
  private abort = new AbortController();
  private shaders?: ShaderHost;
  private ctx: CanvasRenderingContext2D;
  private running = false;
  /** The file's sounds, decoded, by audio asset id. */
  private sounds = new Map<number, AudioBuffer>();
  /** Keep the sounds of the file's events quiet. */
  muted = false;

  private constructor(readonly canvas: HTMLCanvasElement, readonly file: ZuriFile, readonly instance: Instance, private options: PlayerOptions) {
    this.ctx = canvas.getContext('2d')!;
  }

  static async create(canvas: HTMLCanvasElement, options: PlayerOptions): Promise<ZuriPlayer> {
    const runtime = loadRuntime(options.wasm);
    const { src } = options;
    const bytes = typeof src === 'string' || src instanceof URL
      ? fetch(src).then((r) => {
          if (!r.ok) throw new Error(`Zuri: could not fetch ${r.url} (${r.status})`);
          return r.arrayBuffer();
        })
      : src;
    const file = (await runtime).load(await bytes);
    await file.decodeImages();
    const instance = file.instantiate(options.artboard);
    instance.play(options.play);
    const player = new ZuriPlayer(canvas, file, instance, options);
    player.decodeSounds();
    player.attach();
    if (options.autoplay !== false) player.play();
    else player.tick(0);
    return player;
  }

  /**
   * Decode the file's sounds with the browser's decoders, in the background: nothing
   * waits for them, and one that is not ready, or cannot be decoded, stays silent.
   */
  private decodeSounds(): void {
    const sounds = this.file.sounds;
    if (!sounds.length || typeof OfflineAudioContext === 'undefined') return;
    listenForTouch();
    // Decoding needs no audio output, so none is opened for it.
    const decoder = new OfflineAudioContext(1, 1, 44100);
    for (const { id, bytes } of sounds) {
      decoder.decodeAudioData(bytes.buffer as ArrayBuffer).then(
        (buffer) => this.sounds.set(id, buffer),
        () => console.warn(`Zuri: the browser cannot decode sound ${id} of this file; it will not play`),
      );
    }
  }

  /** Play a decoded sound, if the page may make sound now (or within a moment). */
  private playSound(id: number, volume: number): void {
    const [buffer, out, asked] = [this.sounds.get(id), speaker, performance.now()];
    if (!buffer || !out || this.muted) return;
    // The output may still be waking from the touch that caused this sound. A sound
    // that would only start much later (nobody has touched the page yet) is dropped.
    const start = () => {
      if (performance.now() - asked > 300) return;
      const source = out.createBufferSource();
      const gain = out.createGain();
      source.buffer = buffer;
      gain.gain.value = volume;
      source.connect(gain).connect(out.destination);
      source.start();
    };
    out.resume().then(start, () => {});
  }

  private attach(): void {
    const { canvas, instance } = this;
    const signal = this.abort.signal;
    const at = (e: PointerEvent): [number, number] => {
      const r = canvas.getBoundingClientRect();
      return [(e.clientX - r.left) * (canvas.width / r.width), (e.clientY - r.top) * (canvas.height / r.height)];
    };
    const send = (kind: PointerKind) => (e: PointerEvent) => {
      instance.pointer(kind, ...at(e));
      this.stale = true;
    };
    canvas.addEventListener('pointermove', send('move'), { signal });
    canvas.addEventListener('pointerdown', send('down'), { signal });
    canvas.addEventListener('pointerup', send('up'), { signal });
    canvas.addEventListener('pointerleave', send('exit'), { signal });
    // Keys reach an element that has focus (give it `tabindex="0"`); inside <zuri-player>
    // that is the element itself, not the canvas in its shadow root.
    const root = canvas.getRootNode();
    const focused = root instanceof ShadowRoot && root.host.localName === 'zuri-player' ? root.host : canvas;
    // The keys the scene reacted to and that are still down. Those are the scene's: they
    // do not also scroll the page. Tab always moves the focus on, and keys held with
    // Ctrl / Cmd / Alt stay the browser's shortcuts.
    const held = new Set<string>();
    const keys = (e: Event) => {
      const { key, type, repeat, ctrlKey, metaKey, altKey } = e as KeyboardEvent;
      if (ctrlKey || metaKey || altKey) return;
      if (type === 'keyup') held.delete(key);
      // A key that is held down repeats; the scene hears it once. (No redraw to ask for:
      // a key the scene reacts to makes its next advance report a change.)
      if (!repeat && instance.key(key, type === 'keydown') && type === 'keydown') held.add(key);
      if (held.has(key) && key !== 'Tab') e.preventDefault();
    };
    focused.addEventListener('keydown', keys, { signal });
    focused.addEventListener('keyup', keys, { signal });
    // A key still down when the focus leaves would never be released.
    const release = () => {
      for (const key of held) instance.key(key, false);
      held.clear();
    };
    focused.addEventListener('blur', release, { signal });

    const resize = new ResizeObserver(() => this.resize());
    resize.observe(canvas);
    // Off-screen players stop ticking entirely.
    const seen = new IntersectionObserver(([entry]) => (this.visible = entry.isIntersecting));
    seen.observe(canvas);
    this.observers.push(resize, seen);
    this.resize();
  }

  /** Match the canvas backing store to its CSS size and the device pixel ratio. */
  resize(): void {
    const ratio = Math.min(globalThis.devicePixelRatio || 1, this.options.maxPixelRatio ?? 2);
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width * ratio));
    const height = Math.max(1, Math.round(rect.height * ratio));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    this.stale = true;
  }

  /**
   * Advance by `seconds` and draw if anything changed. The player calls this on every
   * animation frame while playing; call it yourself to drive time from your own clock
   * (a game loop, a video export, a test).
   */
  tick(seconds = 0): void {
    const { instance, canvas, options } = this;
    // advance() returning false means the picture is unchanged: skip the render and upload.
    if (!instance.advance(seconds) && !this.stale) return;
    this.stale = false;
    const pixels = instance.render(canvas.width, canvas.height, { fit: options.fit, align: options.align, background: options.background });
    this.ctx.putImageData(new ImageData(pixels as Uint8ClampedArray<ArrayBuffer>, canvas.width, canvas.height), 0, 0);
    if (instance.shaderRequests().length) {
      this.shaders ??= new ShaderHost(this.file);
      this.shaders.update(instance);
    }
    // Always drain, so events do not pile up in the runtime when nobody listens.
    for (const { name, sound, volume } of instance.reports()) {
      if (sound !== null) this.playSound(sound, volume);
      options.onEvent?.(name);
    }
  }

  get playing(): boolean {
    return this.running;
  }

  play(): void {
    if (this.running) return;
    this.running = true;
    this.last = 0;
    const tick = (now: number) => {
      // Clamp so a background tab does not resume with one enormous step.
      const seconds = this.last ? Math.min((now - this.last) / 1000, 0.1) : 0;
      this.last = now;
      if (this.visible) this.tick(seconds);
      this.frame = requestAnimationFrame(tick);
    };
    this.frame = requestAnimationFrame(tick);
  }

  pause(): void {
    this.running = false;
    cancelAnimationFrame(this.frame);
  }

  /** Stop, detach every listener and release the wasm memory. */
  destroy(): void {
    this.pause();
    this.abort.abort();
    for (const o of this.observers) o.disconnect();
    this.instance.free();
    this.file.free();
  }
}
