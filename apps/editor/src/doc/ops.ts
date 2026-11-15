// Document-level operations the UI commands are made of: building new objects with
// sensible defaults, grouping, duplicating and merging an imported document.
// Framework-free; every function returns a new tree.

import { type Node, append, clone, find, insert, node, parentOf, remove, rootOf } from './model.ts';
import { uniqueName, uniquifyNames } from './refs.ts';
import { type Schema, isAsset, refCandidates } from './schema.ts';

export type Geometry = 'Rectangle' | 'Ellipse' | 'Polygon' | 'Star';

const round = (v: number): number => Math.round(v * 100) / 100;

/** A new document: one empty artboard with a background. */
export function defaultDocument(): Node[] {
  return [node('Artboard', { name: 'Artboard', width: 400, height: 300 }, [node('Fill', { color: '#1b1d27' })])];
}

/** `Shape > [geometry, Fill]` centered on (x, y) in its parent's space. */
export function makeShape(geometry: Geometry, name: string, x: number, y: number, width: number, height: number): Node {
  return node('Shape', { name, x: round(x), y: round(y) }, [node(geometry, { width: round(width), height: round(height) }), node('Fill', { color: '#d9d9d9' })]);
}

export interface PenPoint {
  x: number;
  y: number;
  /** Outgoing handle, relative to the point. The incoming handle mirrors it. (0, 0) = corner. */
  hx: number;
  hy: number;
}

/** SVG path data through pen points, relative to `origin`. */
export function penPath(points: readonly PenPoint[], closed: boolean, origin = { x: 0, y: 0 }): string {
  if (!points.length) return '';
  const at = (x: number, y: number) => `${round(x - origin.x)} ${round(y - origin.y)}`;
  const segment = (a: PenPoint, b: PenPoint) => (a.hx || a.hy || b.hx || b.hy ? `C${at(a.x + a.hx, a.y + a.hy)} ${at(b.x - b.hx, b.y - b.hy)} ${at(b.x, b.y)}` : `L${at(b.x, b.y)}`);
  const parts = [`M${at(points[0].x, points[0].y)}`];
  for (let i = 1; i < points.length; i++) parts.push(segment(points[i - 1], points[i]));
  if (closed) {
    const [last, first] = [points.at(-1)!, points[0]];
    if (last.hx || last.hy || first.hx || first.hy) parts.push(segment(last, first));
    parts.push('Z');
  }
  return parts.join(' ');
}

/** `Shape > [Path, (Fill), Stroke]` for a finished pen drawing, positioned at the center of its points. */
export function makePenShape(points: readonly PenPoint[], closed: boolean, name: string): Node {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const origin = { x: round((Math.min(...xs) + Math.max(...xs)) / 2), y: round((Math.min(...ys) + Math.max(...ys)) / 2) };
  const paints = [...(closed ? [node('Fill', { color: '#d9d9d9' })] : []), node('Stroke', { color: closed ? '#ffffff' : '#d9d9d9', thickness: 2, cap: 'round', join: 'round' })];
  return node('Shape', { name, ...origin }, [node('Path', { d: penPath(points, closed, origin) }), ...paints]);
}

const stops = (): Node[] => [node('GradientStop', { position: 0, color: '#ffffff' }), node('GradientStop', { position: 1, color: '#4f8cff' })];

/** A new child of `parent`, filled in so that it does something visible and compiles. */
export function makeChild(type: string, roots: readonly Node[], parent: Node, schema: Schema): Node {
  // A reference to the first object that fits, so the new child compiles straight away.
  const withRef = (props: Record<string, unknown>, prop: string): Node => {
    const probe = node(type, props);
    const name = refCandidates(schema, append(roots, parent.id, probe), probe.id, prop)[0];
    return name === undefined ? probe : { ...probe, props: { ...props, [prop]: name } };
  };
  const artboard = rootOf(roots, parent.id);
  const named = (base: string) => uniqueName(schema, roots, artboard, type, base);
  // A layer starts with an entry State, named among the artboard's States (not among `type`'s class).
  const entry = () => node('State', { name: uniqueName(schema, roots, artboard, 'State', 'entry'), kind: 'entry' });
  switch (type) {
    case 'Fill':
      return node(type, { color: '#d9d9d9' });
    case 'Stroke':
      return node(type, { color: '#ffffff', thickness: 2 });
    case 'Rectangle':
    case 'Ellipse':
    case 'Polygon':
    case 'Star':
      return node(type, { width: 100, height: 100 });
    case 'Path':
      return node(type, { d: 'M-50 0 L50 0' });
    case 'LinearGradient':
      return node(type, { x0: -50, y0: 0, x1: 50, y1: 0 }, stops());
    case 'RadialGradient':
      return node(type, { radius: 60 }, stops());
    case 'GradientStop':
      return node(type, { position: 1, color: '#ffffff' });
    case 'Property':
      return node(type, { name: named('property'), valueType: 'number', value: 0 });
    case 'Condition':
      return withRef({ op: 'eq', value: 1 }, 'input');
    case 'Bind':
      return withRef({ property: parent.type === 'Run' ? 'text' : parent.type === 'Constraint' ? 'strength' : parent.type === 'Fill' || parent.type === 'Stroke' || parent.type === 'GradientStop' ? 'color' : 'opacity' }, 'source');
    case 'Clip':
      return withRef({}, 'source');
    case 'BlendAnimation':
      return withRef({ value: 0 }, 'animation');
    case 'ShaderPaint':
      return withRef({}, 'asset');
    case 'Listener':
      return withRef({ event: 'click', action: 'set', value: 1 }, 'input');
    case 'Animation':
      return node(type, { name: named('animation'), fps: 60, duration: 60 });
    case 'StateMachine':
      return node(type, { name: named('machine') }, [node('Layer', { name: 'layer' }, [entry()])]);
    case 'Layer':
      return node(type, { name: named('layer') }, [entry()]);
    case 'Event':
      return node(type, { name: named('event') });
    case 'ViewModel':
      return node(type, { name: 'vm' });
    default:
      return node(type);
  }
}

/** A new State of `kind` in a layer. Animation states start on the first animation. */
export function makeState(roots: readonly Node[], layer: Node, kind: string, schema: Schema): Node {
  const artboard = rootOf(roots, layer.id);
  const animation = kind === 'animation' ? artboard?.children.find((c) => c.type === 'Animation' && c.props.name)?.props.name : undefined;
  const name = uniqueName(schema, roots, artboard, 'State', typeof animation === 'string' ? animation : kind === 'animation' ? 'state' : kind);
  return node('State', { name, ...(kind === 'animation' ? {} : { kind }), ...(animation ? { animation } : {}) });
}

/** Wrap sibling nodes in a new `Node` group, in place of the first of them. */
export function group(roots: readonly Node[], ids: readonly string[], schema: Schema): { roots: Node[]; id: string } | undefined {
  const parent = ids.length ? parentOf(roots, ids[0]) : undefined;
  if (!parent || !schema.types.get('Node')?.parents.includes(parent.type)) return undefined;
  const members = parent.children.filter((c) => ids.includes(c.id));
  const ok = members.length === ids.length && members.every((m) => schema.types.get(m.type)?.parents.includes('Node'));
  if (!ok) return undefined;
  const wrapper = node('Node', { name: uniqueName(schema, roots, rootOf(roots, parent.id), 'Node', 'group') }, members);
  const index = parent.children.indexOf(members[0]);
  return { roots: insert(remove(roots, new Set(ids)), parent.id, index, wrapper), id: wrapper.id };
}

/** Copy nodes next to themselves (in front), with unique names. Returns the copies' ids. */
export function duplicate(roots: readonly Node[], ids: readonly string[], schema: Schema): { roots: Node[]; ids: string[] } {
  let out = roots as Node[];
  const made: string[] = [];
  for (const id of ids) {
    const original = find(out, id);
    if (!original) continue;
    const parent = parentOf(out, id);
    const copy = clone(original);
    out = uniquifyNames(insert(out, parent?.id ?? null, (parent?.children ?? out).indexOf(original) + 1, copy), copy.id, schema);
    made.push(copy.id);
  }
  return { roots: out, ids: made };
}

/**
 * Merge a document produced by an importer: its asset roots are appended to the document
 * roots, and the children of its artboard are wrapped in a `Node` inside `artboardId`.
 */
export function mergeImport(roots: readonly Node[], imported: readonly Node[], artboardId: string, name: string, schema: Schema): { roots: Node[]; id: string | undefined } {
  let out = roots as Node[];
  // Assets share one namespace across the document: rename the incoming ones that clash.
  const renamed = new Map<unknown, string>();
  for (const asset of imported.filter((r) => isAsset(schema, r.type))) {
    const old = asset.props.name;
    const fresh = uniqueName(schema, out, undefined, asset.type, typeof old === 'string' && old ? old : asset.type.toLowerCase());
    renamed.set(old, fresh);
    out = append(out, null, { ...asset, props: { ...asset.props, name: fresh } });
  }
  const retarget = (n: Node): Node => ({ ...n, props: renamed.has(n.props.asset) ? { ...n.props, asset: renamed.get(n.props.asset) } : n.props, children: n.children.map(retarget) });
  const content = imported.find((r) => r.type === 'Artboard')?.children.filter((c) => schema.types.get(c.type)?.parents.includes('Node')) ?? [];
  if (!content.length) return { roots: out, id: undefined };
  const wrapper = node('Node', { name }, content.map(retarget));
  return { roots: uniquifyNames(append(out, artboardId, wrapper), wrapper.id, schema), id: wrapper.id };
}

/** The scene-relevant label of a node: its name, or its type when unnamed. */
export const labelOf = (n: Node): string => (typeof n.props.name === 'string' && n.props.name ? n.props.name : n.type);
