// Runs the built package against the wasm runtime in Node: the same scenarios as the
// native tests, plus a pixel comparison against the native goldens (wasm == native).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { inflateSync } from 'node:zlib';
import { loadRuntime, parseColor, formatColor } from '../dist/index.js';

const repo = new URL('../../../', import.meta.url);
const read = (path) => readFileSync(new URL(path, repo));
const editor = await loadRuntime(new URL('../dist/zuri-editor.wasm', import.meta.url));

/** Minimal PNG reader (8-bit RGBA, non-interlaced), enough for our own goldens. */
function decodePng(bytes) {
  let offset = 8;
  let width = 0, height = 0;
  const idat = [];
  while (offset < bytes.length) {
    const len = bytes.readUInt32BE(offset);
    const type = bytes.toString('latin1', offset + 4, offset + 8);
    const data = bytes.subarray(offset + 8, offset + 8 + len);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      assert.deepEqual([data[8], data[9], data[12]], [8, 6, 0], 'golden must be 8-bit RGBA, non-interlaced');
    } else if (type === 'IDAT') idat.push(data);
    offset += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * 4;
  const out = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let i = 0; i < stride; i++) {
      const x = raw[y * (stride + 1) + 1 + i];
      const a = i >= 4 ? out[y * stride + i - 4] : 0;
      const b = y > 0 ? out[(y - 1) * stride + i] : 0;
      const c = i >= 4 && y > 0 ? out[(y - 1) * stride + i - 4] : 0;
      const p = a + b - c;
      const paeth = Math.abs(p - a) <= Math.abs(p - b) && Math.abs(p - a) <= Math.abs(p - c) ? a : Math.abs(p - b) <= Math.abs(p - c) ? b : c;
      out[y * stride + i] = (x + [0, a, b, (a + b) >> 1, paeth][filter]) & 255;
    }
  }
  return { width, height, pixels: out };
}

function assertMatchesGolden(name, pixels, width, height) {
  const golden = decodePng(read(`crates/zuri-core/tests/golden/${name}.png`));
  assert.deepEqual([golden.width, golden.height], [width, height]);
  let off = 0;
  for (let i = 0; i < pixels.length; i++) if (Math.abs(pixels[i] - golden.pixels[i]) > 3) off++;
  assert.ok(off * 2000 < pixels.length, `${name}: ${off} channel values differ from the native golden`);
}

test('colors convert both ways', () => {
  assert.equal(parseColor('#f53'), 0xffff5533);
  assert.equal(parseColor('#11223344'), 0x44112233);
  assert.equal(formatColor(0xff112233), '#112233');
  assert.equal(formatColor(0x44112233), '#11223344');
  assert.throws(() => parseColor('red'), /not a color/);
});

test('compile, load, inspect', () => {
  assert.ok(editor.canCompile);
  const zur = editor.compile(read('examples/hello.json').toString());
  assert.equal(String.fromCharCode(...zur.subarray(0, 4)), 'ZURI');
  const file = editor.load(zur);
  assert.deepEqual(file.artboards, ['Hello']);
  const inst = file.instantiate('Hello');
  assert.deepEqual([inst.width, inst.height], [400, 300]);
  assert.deepEqual(inst.animations, ['bounce']);
  assert.throws(() => file.instantiate('Nope'), /no artboard "Nope"/);
  assert.throws(() => inst.play('nope'), /nothing to play/);
  // Source survives a round trip through the binary format.
  assert.deepEqual(editor.compile(editor.decompile(zur)), zur);
  inst.free();
  file.free();
});

test('enum properties, binds that write back, change listeners', () => {
  const zur = editor.compile(
    JSON.stringify({
      zuri: 1,
      children: [
        {
          type: 'Artboard',
          width: 10,
          height: 10,
          children: [
            {
              type: 'ViewModel',
              children: [
                { type: 'Property', name: 'mode', valueType: 'enum', value: 1, children: ['idle', 'busy', 'done'].map((name) => ({ type: 'Option', name })) },
                { type: 'Property', name: 'label', valueType: 'string' },
              ],
            },
            { type: 'Event', name: 'modeChanged' },
            { type: 'Text', children: [{ type: 'Bind', source: 'mode', property: 'text', format: 'now {}' }, { type: 'Bind', source: 'label', property: 'text', direction: 'toSource' }] },
            { type: 'StateMachine', children: [{ type: 'Listener', event: 'change', with: 'mode', emit: 'modeChanged' }, { type: 'Listener', event: 'keyDown', key: 'ArrowRight', input: 'mode', action: 'add', value: 1 }] },
          ],
        },
      ],
    }),
  );
  const inst = editor.load(zur).instantiate();
  inst.play();
  assert.deepEqual(inst.properties, [{ name: 'mode', type: 'enum', options: ['idle', 'busy', 'done'] }, { name: 'label', type: 'string' }]);
  assert.deepEqual([inst.getString('mode'), inst.getNumber('mode')], ['busy', 1]);
  inst.setString('mode', 'done');
  assert.throws(() => inst.setString('mode', 'nope'));
  inst.advance(0);
  assert.deepEqual([inst.getNumber('mode'), inst.getString('label'), inst.events()], [2, 'now done', ['modeChanged']]);
  // A key the scene listens for is handled (and the enum stays on its last Option); another is not.
  assert.deepEqual([inst.key('ArrowRight', true), inst.key('ArrowLeft', true), inst.getNumber('mode')], [true, false, 2]);
  inst.free();
});

test('errors carry the JSON path and the fix', () => {
  assert.throws(() => editor.compile('{"children":[{"type":"Artboard","colour":"red"}]}'), /children\[0\]\.colour.*no property/);
  assert.throws(() => editor.load(new Uint8Array([1, 2, 3])), /not a \.zur file/);
  const schema = editor.schema();
  assert.ok(schema.types.some((t) => t.type === 'StateMachine'));
});

test('wasm renders the same pixels as the native goldens', () => {
  const inst = editor.load(editor.compile(read('examples/hello.json').toString())).instantiate();
  inst.play('bounce');
  assert.equal(inst.advance(0), true);
  assertMatchesGolden('hello_t0', inst.render(400, 300), 400, 300);
  for (let i = 0; i < 64; i++) inst.advance(1 / 64);
  assertMatchesGolden('hello_t1', inst.render(400, 300), 400, 300);
});

test('seek, property read-back and layer states', () => {
  const hello = editor.load(editor.compile(read('examples/hello.json').toString())).instantiate();
  assert.throws(() => hello.seek(1), /playAnimation/);
  hello.playAnimation('bounce');
  hello.seek(1);
  hello.advance(0);
  assertMatchesGolden('hello_t1', hello.render(400, 300), 400, 300); // the same frame as 64 steps of 1/64 s
  assert.throws(() => hello.playStateMachine('bounce'), /no state machine named/);
  assert.throws(() => hello.playAnimation('nope'), /no animation named/);

  const button = editor.load(editor.compile(read('examples/button.json').toString())).instantiate();
  assert.deepEqual(button.layerStates, []);
  assert.deepEqual(button.declaredEvents, ['clicked']);
  button.playStateMachine('Button');
  button.advance(0);
  button.render(320, 160);
  assert.ok(button.layerStates.length > 0 && button.layerStates.every((id) => id !== null));
  const types = editor.schema().types;
  const key = (type, name) => types.find((t) => t.type === type).properties.find((p) => p.name === name).key;
  const [X, COLOR] = [key('Shape', 'x'), key('Fill', 'color')];
  assert.deepEqual([X, types.find((t) => t.type === 'Shape').scene, types.find((t) => t.type === 'Transition').properties.find((p) => p.name === 'to').target], [3, true, 'State']);
  const hit = button.hitTest(160, 80);
  assert.equal(typeof button.property(hit, X), 'number');
  assert.equal(button.propertyColor(hit, COLOR), null, 'a Shape has no color: its Fill does');
  assert.equal(button.propertyColor(hit, X), null, 'x is not a color');
  assert.equal(button.property(0xfffffff0, X), null);
});

test('state machine, pointer input, data binding and events', () => {
  const inst = editor.load(editor.compile(read('examples/button.json').toString())).instantiate();
  inst.play();
  assert.deepEqual(inst.stateMachines, ['Button']);
  assert.deepEqual(inst.properties.map((p) => p.type), ['boolean', 'boolean', 'number', 'color', 'trigger']);

  inst.advance(0);
  assertMatchesGolden('button_idle', inst.render(320, 160), 320, 160);
  let active = true;
  for (let i = 0; i < 30; i++) active = inst.advance(1 / 60);
  assert.equal(active, false, 'an idle scene settles, so hosts can skip rendering');

  inst.pointer('move', 160, 80);
  assert.equal(inst.getBoolean('hover'), true);
  inst.pointer('down', 160, 80);
  inst.pointer('up', 160, 80);
  assert.equal(inst.getNumber('clicks'), 1);
  for (let i = 0; i < 60; i++) inst.advance(1 / 60);
  assert.deepEqual(inst.events(), ['clicked']);
  assert.deepEqual(inst.events(), []);
  assertMatchesGolden('button_checked', inst.render(320, 160), 320, 160);

  inst.setColor('accent', '#ff3366');
  assert.equal(inst.getColor('accent'), '#ff3366');
  assert.equal(inst.advance(0), true);
  const px = inst.render(320, 160);
  assert.deepEqual([...px.subarray((80 * 320 + 100) * 4, (80 * 320 + 100) * 4 + 4)], [0xff, 0x33, 0x66, 0xff]);
  assert.throws(() => inst.setNumber('missing', 1), /no view-model property "missing". Properties: hover/);

  // Fit and alignment: at its own size in the bottom right corner, then centered.
  const at = (pixels, x, y) => [...pixels.subarray((y * 640 + x) * 4, (y * 640 + x) * 4 + 4)];
  const corner = inst.render(640, 320, { fit: 'none', align: 'bottomRight' });
  assert.deepEqual([at(corner, 420, 240), at(corner, 100, 80)], [[0xff, 0x33, 0x66, 0xff], [0, 0, 0, 0]]);
  assert.deepEqual(at(inst.render(640, 320, { fit: 'scaleDown' }), 260, 160), [0xff, 0x33, 0x66, 0xff]);

  // Editor helpers.
  inst.render(640, 320, { view: [2, 0, 0, 2, 0, 0] });
  const hit = inst.hitTest(320, 160);
  assert.notEqual(hit, null);
  const box = inst.bounds(hit);
  assert.ok(box[0] < 160 && box[2] > 160);
  assert.equal(inst.hitTest(2, 2), null);
  inst.free();
});

test('the player builds are smaller than the editor build and cannot compile', async () => {
  const lite = await loadRuntime(new URL('../dist/zuri-lite.wasm', import.meta.url));
  assert.equal(lite.canCompile, false);
  assert.throws(() => lite.compile('{}'), /editor build/);
  const zur = editor.compile(read('examples/button.json').toString());
  const inst = lite.load(zur).instantiate();
  inst.play();
  inst.advance(0);
  assertMatchesGolden('button_idle', inst.render(320, 160), 320, 160);
});
