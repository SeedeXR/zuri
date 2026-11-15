// Animation documents: reading a property's value at a frame and writing keys.
// The sampling mirrors crates/zuri-core/src/instance.rs (`sample`, `cubic_ease`,
// `map_frame`) so the inspector shows the same value the runtime draws. Framework-free.

import { lerpHex } from './color.ts';
import { type Node, append, find, node, setProps, update } from './model.ts';
import { ensureName } from './refs.ts';
import { type Schema, defaultOf, propDef } from './schema.ts';

/** A Key's or Property's value. Decompiled files spell colors and strings as colorValue / textValue. */
export const valueOf = (n: Node): unknown => n.props.value ?? n.props.colorValue ?? n.props.textValue;

/**
 * The property patch that stores `value` on a Key or Property. `"value"` is sugar for
 * numbers, colors and strings alike, except that the compiler reads a string starting
 * with `#` as a color: text like that has to be spelled `textValue`.
 */
export function valuePatch(value: unknown, isText = false): Record<string, unknown> {
  const literal = isText && typeof value === 'string' && value.startsWith('#');
  return { value: literal ? undefined : value, colorValue: undefined, textValue: literal ? value : undefined };
}

const frameOf = (key: Node): number => (typeof key.props.frame === 'number' ? key.props.frame : 0);

/** Keys of a track in time order (the runtime sorts them the same way). */
export const sortedKeys = (track: Node): Node[] => track.children.filter((k) => k.type === 'Key').sort((a, b) => frameOf(a) - frameOf(b));

export const findTrack = (animation: Node, target: string, prop: string): Node | undefined =>
  animation.children.find((t) => t.type === 'Track' && t.props.target === target && t.props.property === prop);

/** CSS-style cubic-bezier easing: the y of the curve where x == t. */
export function cubicEase([x1, y1, x2, y2]: number[], t: number): number {
  const bez = (a: number, b: number, s: number) => 3 * (1 - s) * (1 - s) * s * a + 3 * (1 - s) * s * s * b + s * s * s;
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  let [lo, hi] = [0, 1];
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (bez(clamp(x1), clamp(x2), mid) < t) lo = mid;
    else hi = mid;
  }
  return bez(y1, y2, (lo + hi) / 2);
}

/** Value of a track at `frame`, or undefined when it has no keys. */
export function sample(track: Node, frame: number, schema: Schema): unknown {
  const keys = sortedKeys(track);
  if (!keys.length) return undefined;
  if (frame <= frameOf(keys[0])) return valueOf(keys[0]);
  if (frame >= frameOf(keys.at(-1)!)) return valueOf(keys.at(-1)!);
  const hi = keys.findIndex((k) => frameOf(k) > frame);
  const [k0, k1] = [keys[hi - 1], keys[hi]];
  const [a, b] = [valueOf(k0), valueOf(k1)];
  const prop = (name: string) => (typeof k0.props[name] === 'number' ? (k0.props[name] as number) : (propDef(schema, 'Key', name)?.default as number));
  let t = (frame - frameOf(k0)) / (frameOf(k1) - frameOf(k0));
  if (k0.props.interp === 'hold') return a;
  if (k0.props.interp === 'cubic') t = cubicEase([prop('easeX1'), prop('easeY1'), prop('easeX2'), prop('easeY2')], t);
  if (typeof a === 'number' && typeof b === 'number') return a + (b - a) * t;
  if (typeof a === 'string' && typeof b === 'string' && a.startsWith('#') && b.startsWith('#')) return lerpHex(a, b, Math.min(1, Math.max(0, t)));
  return a;
}

/**
 * What a property of `target` is at `frame`: the animation's value when a track drives
 * it, otherwise the base value from the document (or the schema default).
 */
export function valueAt(schema: Schema, animation: Node | undefined, target: Node, prop: string, frame: number): unknown {
  const def = propDef(schema, target.type, prop);
  const base = target.props[prop] ?? (def ? defaultOf(def) : undefined);
  const name = target.props.name;
  if (!animation || typeof name !== 'string') return base;
  const track = findTrack(animation, name, prop);
  return (track && sample(track, frame, schema)) ?? base;
}

/** The key of (target, prop) that sits exactly on `frame`, if any. */
export function keyAt(animation: Node | undefined, target: Node, prop: string, frame: number): Node | undefined {
  const name = target.props.name;
  if (!animation || typeof name !== 'string') return undefined;
  return findTrack(animation, name, prop)?.children.find((k) => k.type === 'Key' && frameOf(k) === frame);
}

/**
 * Auto-key: write `value` as the key at `frame` in the track for (target, prop) of an
 * animation, creating the track (and a name for the target) when needed. The base
 * value in the document is left alone.
 */
export function setKey(schema: Schema, roots: readonly Node[], animationId: string, targetId: string, prop: string, frame: number, value: unknown): Node[] {
  const named = ensureName(roots, targetId, schema);
  let out = named.roots;
  const animation = find(out, animationId);
  if (!animation || !named.name) return roots as Node[];
  const track = findTrack(animation, named.name, prop);
  if (!track) return append(out, animationId, node('Track', { target: named.name, property: prop }, [node('Key', { frame, value })]));
  const existing = track.children.find((k) => k.type === 'Key' && frameOf(k) === frame);
  if (existing) return setProps(out, existing.id, valuePatch(value));
  out = update(out, track.id, (t) => ({ ...t, children: sortedKeys({ ...t, children: [...t.children, node('Key', { frame, value })] }) }));
  return out;
}

/** Timeline position for an elapsed frame count, and whether playback has ended. */
export function mapFrame(duration: number, loop: unknown, frames: number): { frame: number; finished: boolean } {
  if (!(duration > 0)) return { frame: 0, finished: true };
  const f = Math.max(0, frames);
  if (loop === 'loop') return { frame: f % duration, finished: false };
  if (loop === 'pingpong') {
    const m = f % (2 * duration);
    return { frame: m > duration ? 2 * duration - m : m, finished: false };
  }
  return { frame: Math.min(f, duration), finished: f >= duration };
}
