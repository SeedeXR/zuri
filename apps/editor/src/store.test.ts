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
