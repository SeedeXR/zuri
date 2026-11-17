import assert from 'node:assert/strict';
import { test } from 'node:test';
import { node } from './doc/model.ts';
import { store } from './store.ts';

test('the playhead rests on a whole frame; only playback sits between frames', () => {
  store.open([node('Artboard', {}, [node('Animation', { name: 'a', duration: 24 })])], 'x.json', null);
  store.set({ mode: 'animate', playing: true, frame: 12.6 });
  assert.equal(store.get().frame, 12.6);
  store.set({ playing: false });
  assert.equal(store.get().frame, 13);
  store.set({ frame: 99.4 });
  assert.equal(store.get().frame, 24);
});

test('a work area is a part of the current animation', () => {
  const [a, b] = [node('Animation', { name: 'a', duration: 24 }), node('Animation', { name: 'b', duration: 24 })];
  store.open([node('Artboard', {}, [a, b])], 'x.json', null);
  const area = (from: number, to: number, animation = a.id) => {
    store.set({ workArea: { animation, from, to } });
    return store.get().workArea;
  };
  assert.deepEqual(area(6, 12), { animation: a.id, from: 6, to: 12 });
  // Nothing of it, outside it at either end, or another animation's: no work area.
  assert.deepEqual([area(12, 12), area(6, 30), area(-6, 6), area(6, 12, b.id)], [null, null, null, null]);
  assert.ok(area(6, 12));
  store.set({ animationId: b.id });
  assert.equal(store.get().workArea, null);
});
