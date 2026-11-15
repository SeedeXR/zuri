// Animation documents: finding and writing keys, and reading what the runtime draws.
// Nothing here samples a track: the value at the playhead is asked of the running
// instance (`liveValue`), so the inspector cannot disagree with the stage. Framework-free.

import { type Instance, formatColor } from '@seedexr/zuri';
import { type Node, append, find, node, rootOf, setProps, update } from './model.ts';
import { ensureName } from './refs.ts';
import type { PropDef, Schema } from './schema.ts';

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

/**
 * What the runtime draws object `index` (its pre-order index in the compiled document)
 * with for a number or color property, after animation and binds, in source form.
 * Undefined when the instance does not carry it.
 */
export function liveValue(instance: Instance, index: number, def: PropDef): number | string | undefined {
  const value = index < 0 ? null : def.kind === 'color' ? instance.propertyColor(index, def.key) : instance.property(index, def.key);
  // The runtime holds f32: seven significant digits give a number back the way it was written.
  return value === null ? undefined : def.kind === 'color' ? formatColor(value) : Number(value.toPrecision(7));
}

/** The part of the editor state that decides whether an edit is keyed. */
export interface Keying {
  mode: string;
  animationId: string | null;
  previewing: boolean;
  doc: readonly Node[];
  artboardId: string;
}

/**
 * Does a write to `n` go to the timeline (auto-key) instead of the base value? Only while
 * the stage shows the timeline's animation at the playhead: Animate mode, an animation, a
 * scene object of the active artboard, and no state machine preview (the instance then
 * plays the machine, and its pose is not the playhead's).
 */
export function isKeyed(schema: Schema, s: Keying, n: Node): boolean {
  return s.mode === 'animate' && s.animationId !== null && !s.previewing && schema.types.get(n.type)?.scene === true && n.type !== 'Artboard' && rootOf(s.doc, n.id)?.id === s.artboardId;
}

/**
 * `n` (a state machine or animation) when the file compiled from `compiled` can play it.
 * The runtime plays by name, so one without a name, or renamed or added since that
 * compile, is not there to play: asking for it would throw.
 */
export const playable = (compiled: readonly Node[], n: Node | undefined): Node | undefined =>
  typeof n?.props.name === 'string' && n.props.name !== '' && find(compiled, n.id)?.props.name === n.props.name ? n : undefined;

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

/**
 * The editor's transport: where the playhead is after `frames` of playback, and whether
 * it has ended. The runtime is then seeked to that frame; it reports no position to read back.
 */
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
