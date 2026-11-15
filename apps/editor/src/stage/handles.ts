// Selection geometry: which object a click selects, and the box and handles drawn
// around a selected object. Reads the live preview for bounds and world matrices.

import { shownNumber } from '../actions.ts';
import { type Node, find, parentOf, pathTo } from '../doc/model.ts';
import { GEOMETRY_TYPES, propDef } from '../doc/schema.ts';
import { schema } from '../runtime.ts';
import type { State, View } from '../store.ts';
import { type Mat, type Point, apply } from './matrix.ts';
import { preview } from './preview.ts';

export const viewMatrix = (v: View): Mat => [v.zoom, 0, 0, v.zoom, v.x, v.y];
export const toArtboard = (v: View, [x, y]: Point): Point => [(x - v.x) / v.zoom, (y - v.y) / v.zoom];

const has = (n: Node, prop: string): boolean => propDef(schema, n.type, prop) !== undefined;
/** Has a position the stage can drag. */
export const isMovable = (n: Node): boolean => has(n, 'x') && has(n, 'y');

export interface ResizeTarget {
  /** The object whose width / height change. */
  sized: Node;
  /** The object whose position follows, so the opposite edge stays put. */
  mover: Node;
}

/**
 * What the resize handles of a selected node act on: the node itself when it is sized and
 * centered on its position (Rectangle, Ellipse, Polygon, Star, Image, Model), or the one
 * sized geometry of a Shape.
 */
export function resizeTarget(node: Node): ResizeTarget | null {
  const sized = (n: Node) => has(n, 'width') && has(n, 'height') && isMovable(n);
  if (sized(node)) return { sized: node, mover: node };
  const geometry = node.type === 'Shape' ? node.children.filter((c) => GEOMETRY_TYPES.has(c.type)) : [];
  return geometry.length === 1 && sized(geometry[0]) ? { sized: geometry[0], mover: node } : null;
}

/**
 * The node a click on `hitId` selects. Like Figma: the outermost group first; an object
 * that is already selected (or a sibling of the selection) at its own depth; `deep`
 * (Cmd/Ctrl) goes straight to the object under the pointer.
 */
export function pickTarget(s: State, hitId: string, deep: boolean): string {
  const chain = pathTo(s.doc, hitId)?.slice(1) ?? [];
  if (deep || !chain.length) return hitId;
  const selected = chain.find((n) => s.selection.includes(n.id));
  if (selected) return selected.id;
  const scope = s.selection.length ? parentOf(s.doc, s.selection[0]) : undefined;
  const depth = scope ? chain.findIndex((n) => n.id === scope.id) : -1;
  return (chain[depth + 1] ?? chain[0]).id;
}

/** Double-click: one level deeper than the current selection, toward the object under the pointer. */
export function pickDeeper(s: State, hitId: string): string {
  const chain = pathTo(s.doc, hitId)?.slice(1) ?? [];
  const current = chain.findIndex((n) => s.selection.includes(n.id));
  return (chain[current + 1] ?? chain.at(-1))?.id ?? hitId;
}

export interface SelectionBox {
  id: string;
  /** Stage-space corners: top-left, top-right, bottom-right, bottom-left of the object's own box. */
  corners: [Point, Point, Point, Point];
  resizable: boolean;
  rotatable: boolean;
}

/** The outline of a selected node, or null when it has nothing to outline (a Key, an empty group). */
export function selectionBox(s: State, id: string): SelectionBox | null {
  const node = find(s.doc, id);
  if (!node) return null;
  const view = viewMatrix(s.view);
  const target = resizeTarget(node);
  const world = target && preview.world(target.sized.id);
  const [w, h] = target ? [shownNumber(s, target.sized, 'width'), shownNumber(s, target.sized, 'height')] : [0, 0];
  const rotatable = has(node, 'rotation');
  if (world && w > 0 && h > 0) {
    const corner = (x: number, y: number) => apply(view, ...apply(world, x, y));
    return { id, corners: [corner(-w / 2, -h / 2), corner(w / 2, -h / 2), corner(w / 2, h / 2), corner(-w / 2, h / 2)], resizable: true, rotatable };
  }
  const b = preview.bounds(id);
  if (!b) return null;
  const corner = (x: number, y: number) => apply(view, x, y);
  return { id, corners: [corner(b[0], b[1]), corner(b[2], b[1]), corner(b[2], b[3]), corner(b[0], b[3])], resizable: false, rotatable };
}

/** The eight resize handles as (sx, sy) signs: -1 = left / top, 0 = middle, 1 = right / bottom. */
export const HANDLES: readonly (readonly [name: string, sx: number, sy: number])[] = [
  ['nw', -1, -1],
  ['n', 0, -1],
  ['ne', 1, -1],
  ['e', 1, 0],
  ['se', 1, 1],
  ['s', 0, 1],
  ['sw', -1, 1],
  ['w', -1, 0],
];

/** Stage position of the point at (sx, sy) on a selection box. */
export function boxPoint([nw, ne, se, sw]: SelectionBox['corners'], sx: number, sy: number): Point {
  const [u, v] = [(sx + 1) / 2, (sy + 1) / 2];
  const lerp = (a: Point, b: Point, t: number): Point => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  return lerp(lerp(nw, ne, u), lerp(sw, se, u), v);
}

/** Where the rotate handle sits: `distance` px outside the middle of the top edge. */
export function rotateHandle(corners: SelectionBox['corners'], distance: number): Point {
  const [top, center] = [boxPoint(corners, 0, -1), boxPoint(corners, 0, 0)];
  const [dx, dy] = [top[0] - center[0], top[1] - center[1]];
  const length = Math.hypot(dx, dy) || 1;
  return [top[0] + (dx / length) * distance, top[1] + (dy / length) * distance];
}
