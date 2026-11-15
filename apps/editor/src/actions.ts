// Editor commands: everything the toolbar, panels, stage and shortcuts can do to the
// store. Property writes go through `writeProps`, which is where auto-key lives.

import { isKeyed, keyAt, setKey, valueOf } from './doc/autokey.ts';
import { type Node, append, find, insert, node, num, parentOf, pathTo, setProps } from './doc/model.ts';
import { type Geometry, type PenPoint, duplicate, group, makeChild, makePenShape, makeShape, makeState } from './doc/ops.ts';
import { ensureName, removeWithDependents, rename, uniqueName } from './doc/refs.ts';
import { canParent, defaultOf, isAnimatable, propDef } from './doc/schema.ts';
import { schema } from './runtime.ts';
import { preview } from './stage/preview.ts';
import { type Mode, type State, type Tool, store } from './store.ts';

let toastId = 0;
export function toast(text: string, kind: 'info' | 'error' = 'info'): void {
  store.set({ toast: { text, kind, id: ++toastId } });
}

// ---- reading -----------------------------------------------------------------------------

export const activeArtboard = (s: State = store.get()): Node | undefined => s.doc.find((n) => n.id === s.artboardId);
export const activeAnimation = (s: State = store.get()): Node | undefined => activeArtboard(s)?.children.find((n) => n.id === s.animationId);

/** Does a write to this property go to the timeline (auto-key) instead of the base value? */
export const keysProp = (s: State, n: Node, prop: string): boolean => isKeyed(schema, s, n) && isAnimatable(propDef(schema, n.type, prop));

/** A property as the stage shows it: what the runtime draws at the playhead when writes to it are keyed, else the base value. */
export function shownValue(s: State, n: Node, prop: string): unknown {
  const def = propDef(schema, n.type, prop);
  const base = n.props[prop] ?? (def && defaultOf(def));
  if (!def || !keysProp(s, n, prop)) return base;
  // A key on the resting playhead is the value there. It is read from the document, which is
  // current inside the frame an edit was made in (the drawn instance is one frame behind), so
  // edits that build on the shown value, like arrow-key nudges, never lose a step.
  const key = s.playing ? undefined : keyAt(activeAnimation(s), n, prop, s.frame);
  return (key && valueOf(key)) ?? preview.value(n.id, def) ?? base;
}
export const shownNumber = (s: State, n: Node, prop: string): number => num(shownValue(s, n, prop));

// ---- writing properties ------------------------------------------------------------------

function writeProps(doc: Node[], s: State, id: string, patch: Record<string, unknown>): Node[] {
  const n = find(doc, id);
  if (!n) return doc;
  let out = doc;
  const base: Record<string, unknown> = {};
  for (const [prop, value] of Object.entries(patch)) {
    if (prop === 'name') out = rename(out, id, typeof value === 'string' ? value.trim() : '', schema);
    else if (value !== undefined && keysProp(s, n, prop)) out = setKey(schema, out, s.animationId!, id, prop, Math.round(s.frame), value);
    else base[prop] = value;
  }
  return Object.keys(base).length ? setProps(out, id, base) : out;
}

/** Set properties on several nodes as one edit. In Animate mode animatable ones become keys. */
export function setPropsMany(writes: readonly [id: string, patch: Record<string, unknown>][], coalesce = ''): void {
  const s = store.get();
  const keyed = writes.some(([id, patch]) => Object.keys(patch).some((p) => keysProp(s, find(s.doc, id)!, p)));
  store.edit((doc) => writes.reduce((d, [id, patch]) => writeProps(d, s, id, patch), doc), keyed ? { playing: false, frame: Math.round(s.frame) } : {}, coalesce);
}

export const setProp = (id: string, prop: string, value: unknown, coalesce = ''): void => setPropsMany([[id, { [prop]: value }]], coalesce);

/** Add or remove the key of (node, prop) at the playhead. */
export function toggleKey(id: string, prop: string): void {
  const s = store.get();
  const n = find(s.doc, id);
  const animation = activeAnimation(s);
  if (!n || !animation || !keysProp(s, n, prop)) return;
  const frame = Math.round(s.frame);
  const key = keyAt(animation, n, prop, frame);
  if (key) store.edit((doc) => removeWithDependents(doc, new Set([key.id]), schema));
  else store.edit((doc) => setKey(schema, doc, animation.id, id, prop, frame, shownValue(s, n, prop)), { frame, playing: false });
}

// ---- structure ---------------------------------------------------------------------------

export function select(ids: string[], additive = false): void {
  const s = store.get();
  if (!additive) return store.set({ selection: ids });
  const toggled = ids.reduce((sel, id) => (sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id]), s.selection);
  store.set({ selection: toggled });
}

export function deleteNodes(ids: readonly string[]): void {
  const s = store.get();
  // The last artboard stays: a document without one has nothing to show or edit.
  const artboards = s.doc.filter((n) => n.type === 'Artboard');
  const doomed = ids.filter((id) => !(artboards.length === 1 && artboards[0].id === id));
  if (doomed.length) store.edit((doc) => removeWithDependents(doc, new Set(doomed), schema));
}
export const deleteSelection = (): void => deleteNodes(store.get().selection);

/** Selected nodes that are not inside another selected node. */
export function topSelection(s: State = store.get()): string[] {
  return s.selection.filter(
    (id) =>
      !pathTo(s.doc, id)
        ?.slice(0, -1)
        .some((a) => s.selection.includes(a.id)),
  );
}

export function duplicateSelection(): void {
  const { doc } = store.get();
  const made = duplicate(
    doc,
    topSelection().filter((id) => parentOf(doc, id)),
    schema,
  );
  if (made.ids.length) store.edit(() => made.roots, { selection: made.ids });
}

export function groupSelection(): void {
  const s = store.get();
  const result = group(s.doc, topSelection(s), schema);
  if (!result) return toast('Select objects that share a parent to group them.');
  store.edit(() => result.roots, { selection: [result.id] });
}

export function nudge(dx: number, dy: number): void {
  const s = store.get();
  const writes = topSelection(s).flatMap((id): [string, Record<string, unknown>][] => {
    const n = find(s.doc, id)!;
    return propDef(schema, n.type, 'x') && propDef(schema, n.type, 'y') ? [[id, { x: shownNumber(s, n, 'x') + dx, y: shownNumber(s, n, 'y') + dy }]] : [];
  });
  if (writes.length) setPropsMany(writes, 'nudge');
}

/** Add a schema-default child of `type` under `parentId`. Returns its id. */
export function addChild(parentId: string, type: string, selectIt = false): string | undefined {
  const s = store.get();
  const parent = find(s.doc, parentId);
  if (!parent || !canParent(schema, type, parent.type)) return undefined;
  const child = makeChild(type, s.doc, parent, schema);
  store.edit((doc) => append(doc, parentId, child), selectIt ? { selection: [child.id] } : {});
  return child.id;
}

/** Add a node to the active artboard, in front of everything, and select it. */
export function addToArtboard(child: Node, patch: Partial<State> = {}): void {
  const s = store.get();
  if (!s.artboardId) return;
  store.edit((doc) => append(doc, s.artboardId, child), { selection: [child.id], ...patch });
}

/** A name for a new drawable that nothing else in the active artboard uses. */
export const freshName = (base: string): string => uniqueName(schema, store.get().doc, activeArtboard(), 'Node', base);

export function addShape(geometry: Geometry, x: number, y: number, width: number, height: number): void {
  addToArtboard(makeShape(geometry, freshName(geometry.toLowerCase()), x, y, width, height), { tool: 'select' });
}

export function addArtboard(width = 400, height = 300): void {
  const s = store.get();
  const artboard = node('Artboard', { name: uniqueName(schema, s.doc, undefined, 'Artboard', 'Artboard'), width, height }, [node('Fill', { color: '#1b1d27' })]);
  // After the last artboard, before the assets.
  const index = s.doc.findLastIndex((n) => n.type === 'Artboard') + 1;
  store.edit((doc) => insert(doc, null, index, artboard), { artboardId: artboard.id, selection: [artboard.id], tool: 'select', fitRequest: s.fitRequest + 1 });
}

export function addText(x: number, y: number): void {
  const font = store.get().doc.find((n) => n.type === 'FontAsset' && n.props.name)?.props.name;
  if (!font) return toast('Text needs a font: add one with Font in the Assets tab first.', 'error');
  addToArtboard(node('Text', { name: freshName('text'), text: 'Text', asset: font, size: 32, x: Math.round(x), y: Math.round(y) }, [node('Fill', { color: '#ffffff' })]), { tool: 'select' });
}

/** Finish the pen drawing in progress as a Shape. Fewer than two points cancels it. */
export function finishPen(closed: boolean): void {
  const points: PenPoint[] = store.get().pen;
  store.set({ pen: [] });
  if (points.length >= 2) addToArtboard(makePenShape(points, closed, freshName('path')), { tool: 'select' });
}

export function addState(kind: string): void {
  const s = store.get();
  const layer = s.layerId ? find(s.doc, s.layerId) : undefined;
  if (!layer) return;
  const state = makeState(s.doc, layer, kind, schema);
  store.edit((doc) => append(doc, layer.id, state), { selection: [state.id] });
}

/** Connect two states of the current layer with a new Transition. */
export function addTransition(fromId: string, toId: string): void {
  const named = ensureName(store.get().doc, toId, schema);
  const transition = node('Transition', { to: named.name });
  store.edit(() => append(named.roots, fromId, transition), { selection: [transition.id] });
}

// ---- modes, tools, playback, view --------------------------------------------------------

export function setTool(tool: Tool): void {
  if (store.get().pen.length) finishPen(false);
  store.set({ tool, previewing: false });
}

export function setMode(mode: Mode): void {
  const s = store.get();
  store.set({
    mode,
    playing: false,
    previewing: false,
    ...(mode === 'animate' ? { bottomOpen: true, bottomTab: 'timeline' } : s.bottomTab === 'timeline' ? { bottomTab: 'machine', bottomOpen: false } : {}),
  });
}

export function setFrame(frame: number): void {
  store.set({ frame, playing: false });
}

/** Play / pause: the timeline in Animate mode, the state machine preview otherwise. */
export function togglePlay(): void {
  const s = store.get();
  // The runtime plays by name: what is about to play gets a name if it has none, as auto-key names its target.
  const named = (id: string) => (doc: Node[]) => (find(doc, id)?.props.name ? doc : ensureName(doc, id, schema).roots);
  if (s.mode === 'animate' && s.bottomTab !== 'machine') {
    if (!s.animationId) return toast('Add an animation to play it.');
    store.edit(named(s.animationId), { playing: !s.playing, previewing: false });
  } else {
    if (!s.machineId) return toast('This artboard has no state machine to preview yet.');
    store.edit(named(s.machineId), { previewing: !s.previewing, playing: false, selection: [] });
  }
}

const ZOOM_MIN = 0.05;
const ZOOM_MAX = 64;

/** Zoom by `factor` keeping the stage point (cx, cy) fixed. */
export function zoomAt(cx: number, cy: number, factor: number): void {
  const { view } = store.get();
  const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, view.zoom * factor));
  const k = zoom / view.zoom;
  store.set({ view: { zoom, x: cx - (cx - view.x) * k, y: cy - (cy - view.y) * k } });
}

/** Zoom about the middle of the stage. */
export const zoomBy = (factor: number): void => zoomAt(preview.width / 2, preview.height / 2, factor);

export const zoomToFit = (): void => store.set({ fitRequest: store.get().fitRequest + 1 });
