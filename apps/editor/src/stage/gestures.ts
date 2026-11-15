// Pointer gestures on the stage: pan, move, resize, rotate, marquee-select and the
// drag-to-create tools. A gesture is created on pointer down and fed moves until
// pointer up; each one that edits the document is a single undo step.

import { activeArtboard, addArtboard, addShape, select, setPropsMany, shownNumber, topSelection } from '../actions.ts';
import { find, parentOf } from '../doc/model.ts';
import type { Geometry } from '../doc/ops.ts';
import { propDef } from '../doc/schema.ts';
import { schema } from '../runtime.ts';
import { store } from '../store.ts';
import { HANDLES, isMovable, resizeTarget } from './handles.ts';
import { type Point, IDENTITY, apply, applyLinear, invert } from './matrix.ts';
import { preview } from './preview.ts';

export interface Gesture {
  /** `art` is the pointer in artboard space, `stage` in stage CSS pixels. */
  move(e: PointerEvent, art: Point, stage: Point): void;
  up(e: PointerEvent, art: Point, stage: Point): void;
}

/** A rectangle being dragged out on the stage (stage pixels), drawn by the overlay. */
export interface Draft {
  kind: 'shape' | 'marquee';
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Pixels the pointer must travel before a press becomes a drag. */
const DRAG_THRESHOLD = 3;
const round = (v: number, step: number): number => Math.round(v / step) * step;
/** Positions snap to whole units until the stage is zoomed far in. */
const snap = (): number => (store.get().view.zoom >= 4 ? 0.1 : 1);

function rectOf(a: Point, b: Point) {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), width: Math.abs(b[0] - a[0]), height: Math.abs(b[1] - a[1]) };
}

export function panGesture(start: Point): Gesture {
  const origin = store.get().view;
  const move = (_e: PointerEvent, _art: Point, stage: Point) => store.set({ view: { ...origin, x: origin.x + stage[0] - start[0], y: origin.y + stage[1] - start[1] } });
  return { move, up: move };
}

/** Drag the selection. The pointer delta is taken into each object's parent space. */
export function moveGesture(start: Point): Gesture {
  const s = store.get();
  const items = topSelection(s).flatMap((id) => {
    const node = find(s.doc, id)!;
    const parent = parentOf(s.doc, id);
    const toParent = invert((parent && preview.world(parent.id)) ?? IDENTITY);
    return isMovable(node) && toParent ? [{ id, x: shownNumber(s, node, 'x'), y: shownNumber(s, node, 'y'), toParent }] : [];
  });
  let dragging = false;
  return {
    move(e, art) {
      let [dx, dy] = [art[0] - start[0], art[1] - start[1]];
      if (!dragging && Math.hypot(dx, dy) * s.view.zoom < DRAG_THRESHOLD) return;
      if (!dragging) store.beginGesture();
      dragging = true;
      if (e.shiftKey) [dx, dy] = Math.abs(dx) > Math.abs(dy) ? [dx, 0] : [0, dy];
      setPropsMany(
        items.map(({ id, x, y, toParent }) => {
          const [lx, ly] = applyLinear(toParent, dx, dy);
          return [id, { x: round(x + lx, snap()), y: round(y + ly, snap()) }];
        }),
      );
    },
    up() {
      if (dragging) store.endGesture();
    },
  };
}

/**
 * Drag a resize handle. The opposite edge (or the center, with Alt) stays where it is;
 * Shift keeps the proportions. Works in the sized object's own space, so rotated and
 * scaled objects resize along their own axes.
 */
export function resizeGesture(handle: string, nodeId: string): Gesture | null {
  const s = store.get();
  const node = find(s.doc, nodeId);
  const target = node && resizeTarget(node);
  const world = target && preview.world(target.sized.id);
  const toLocal = world && invert(world);
  const moverParent = target && parentOf(s.doc, target.mover.id);
  const toParent = invert((moverParent && preview.world(moverParent.id)) ?? IDENTITY);
  const signs = HANDLES.find(([name]) => name === handle);
  if (!target || !world || !toLocal || !toParent || !signs) return null;
  const [, sx, sy] = signs;
  const [w0, h0] = [shownNumber(s, target.sized, 'width'), shownNumber(s, target.sized, 'height')];
  const [x0, y0] = [shownNumber(s, target.mover, 'x'), shownNumber(s, target.mover, 'y')];
  // The fixed point, in the sized object's space (which is centered on its position).
  const anchor: Point = [(-sx * w0) / 2, (-sy * h0) / 2];
  store.beginGesture(); // armed: the undo step is taken by the first edit, so a press without a move adds none
  return {
    move(e, art) {
      const p = apply(toLocal, art[0], art[1]);
      const centered = e.altKey;
      let w = sx === 0 ? w0 : centered ? Math.abs(p[0]) * 2 : Math.abs(p[0] - anchor[0]);
      let h = sy === 0 ? h0 : centered ? Math.abs(p[1]) * 2 : Math.abs(p[1] - anchor[1]);
      if (e.shiftKey && w0 > 0 && h0 > 0) {
        const k = sx === 0 ? h / h0 : sy === 0 ? w / w0 : Math.max(w / w0, h / h0);
        [w, h] = [w0 * k, h0 * k];
      }
      [w, h] = [Math.max(round(w, snap()), snap()), Math.max(round(h, snap()), snap())];
      // New center in the sized object's space: halfway from the anchor toward the pointer's side.
      const side = (pointer: number, fixed: number, sign: number) => (pointer === fixed ? sign : Math.sign(pointer - fixed));
      const cx = sx === 0 || centered ? 0 : anchor[0] + (side(p[0], anchor[0], sx) * w) / 2;
      const cy = sy === 0 || centered ? 0 : anchor[1] + (side(p[1], anchor[1], sy) * h) / 2;
      const [dx, dy] = applyLinear(toParent, ...applyLinear(world, cx, cy));
      const size = { width: w, height: h };
      const position = { x: Math.round((x0 + dx) * 100) / 100, y: Math.round((y0 + dy) * 100) / 100 };
      setPropsMany(
        target.sized.id === target.mover.id
          ? [[target.sized.id, { ...size, ...position }]]
          : [
              [target.sized.id, size],
              [target.mover.id, position],
            ],
      );
    },
    up: store.endGesture,
  };
}

/** Drag the rotate handle: turns the object about its own origin. Shift snaps to 15 degrees. */
export function rotateGesture(start: Point, nodeId: string): Gesture | null {
  const s = store.get();
  const node = find(s.doc, nodeId);
  const world = node && preview.world(nodeId);
  if (!node || !world || !propDef(schema, node.type, 'rotation')) return null;
  const pivot: Point = [world[4], world[5]];
  const angle = (p: Point) => (Math.atan2(p[1] - pivot[1], p[0] - pivot[0]) * 180) / Math.PI;
  const [rotation0, angle0] = [shownNumber(s, node, 'rotation'), angle(start)];
  store.beginGesture();
  return {
    move(e, art) {
      // Screen y grows downward, so atan2 already increases clockwise like `rotation`.
      const turned = rotation0 + angle(art) - angle0;
      setPropsMany([[nodeId, { rotation: e.shiftKey ? round(turned, 15) : Math.round(turned * 10) / 10 }]]);
    },
    up: store.endGesture,
  };
}

/** Drag on empty stage: select the artboard's direct children whose bounds the rectangle touches. */
export function marqueeGesture(start: Point, startArt: Point, additive: boolean, setDraft: (d: Draft | null) => void): Gesture {
  return {
    move(_e, _art, stage) {
      setDraft({ kind: 'marquee', ...rectOf(start, stage) });
    },
    up(_e, art, stage) {
      setDraft(null);
      if (Math.hypot(stage[0] - start[0], stage[1] - start[1]) < DRAG_THRESHOLD) return;
      const r = rectOf(startArt, art);
      const touched = (activeArtboard()?.children ?? []).filter((c) => {
        const b = isMovable(c) ? preview.bounds(c.id) : null;
        return b && b[0] <= r.x + r.width && b[2] >= r.x && b[1] <= r.y + r.height && b[3] >= r.y;
      });
      select(
        touched.map((c) => c.id),
        additive,
      );
    },
  };
}

/** Drag out a new shape or artboard. A click without a drag makes one of a default size. */
export function createGesture(tool: Geometry | 'artboard', start: Point, startArt: Point, setDraft: (d: Draft | null) => void): Gesture {
  /** With Shift the dragged corner is pulled onto a square. */
  const corner = (e: PointerEvent, from: Point, to: Point): Point => {
    if (!e.shiftKey) return to;
    const side = Math.max(Math.abs(to[0] - from[0]), Math.abs(to[1] - from[1]));
    return [from[0] + Math.sign(to[0] - from[0] || 1) * side, from[1] + Math.sign(to[1] - from[1] || 1) * side];
  };
  const snapped = ([x, y]: Point): Point => [round(x, snap()), round(y, snap())];
  return {
    move(e, _art, stage) {
      setDraft({ kind: 'shape', ...rectOf(start, corner(e, start, stage)) });
    },
    up(e, art, stage) {
      setDraft(null);
      const dragged = Math.hypot(stage[0] - start[0], stage[1] - start[1]) >= DRAG_THRESHOLD;
      const r = rectOf(snapped(startArt), snapped(corner(e, startArt, art)));
      const [w, h] = [Math.max(1, r.width), Math.max(1, r.height)];
      if (tool === 'artboard') addArtboard(dragged ? w : undefined, dragged ? h : undefined);
      else if (dragged) addShape(tool, r.x + w / 2, r.y + h / 2, w, h);
      else addShape(tool, r.x, r.y, 100, 100);
    },
  };
}
