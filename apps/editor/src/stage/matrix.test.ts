import assert from 'node:assert/strict';
import { test } from 'node:test';
import { type Mat, apply, applyLinear, invert } from './matrix.ts';

const close = (a: readonly number[], b: readonly number[]) => a.forEach((v, i) => assert.ok(Math.abs(v - b[i]) < 1e-9, `${a} != ${b}`));

test('apply and invert agree', () => {
  const m: Mat = [0, 2, -2, 0, 10, 20]; // rotate 90 degrees, scale 2, translate
  close(apply(m, 1, 0), [10, 22]);
  close(applyLinear(m, 1, 0), [0, 2]);
  const inv = invert(m)!;
  close(apply(inv, ...apply(m, 3, -4)), [3, -4]);
  assert.equal(invert([0, 0, 0, 0, 1, 1]), null);
});
