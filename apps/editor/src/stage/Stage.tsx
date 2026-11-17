// The stage: the runtime-rendered canvas, the overlay drawn on top of it (selection,
// handles, tool previews) and all pointer and wheel input. Overlays use the view matrix;
// nothing is ever baked into the runtime render.

import { TriangleAlert } from 'lucide-react';
import { type PointerEvent as ReactPointerEvent, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { activeArtboard, addText, finishPen, select, zoomAt } from '../actions.ts';
import { num } from '../doc/model.ts';
import { labelOf, penPath } from '../doc/ops.ts';
import { cx } from '../panels/ui.tsx';
import { type View, store, useStore } from '../store.ts';
import { type Draft, type Gesture, createGesture, marqueeGesture, moveGesture, panGesture, resizeGesture, rotateGesture } from './gestures.ts';
import { HANDLES, boxPoint, pickDeeper, pickTarget, rotateHandle, selectionBox, toArtboard } from './handles.ts';
import type { Point } from './matrix.ts';
import { preview } from './preview.ts';

const HANDLE_SIZE = 8;
const ROTATE_DISTANCE = 22;
/** Stage pixels within which a click on the first pen point closes the path. */
const CLOSE_DISTANCE = 9;
const FIT_MARGIN = 56;
const CURSORS: Record<string, string> = { nw: 'nwse-resize', se: 'nwse-resize', ne: 'nesw-resize', sw: 'nesw-resize', n: 'ns-resize', s: 'ns-resize', e: 'ew-resize', w: 'ew-resize' };

/** A stable ref callback: an inline one would re-attach (and redraw) on every render. */
const attachCanvas = (el: HTMLCanvasElement | null): void => preview.attach(el);
const points = (list: readonly Point[]): string => list.map((p) => `${p[0]},${p[1]}`).join(' ');

export function Stage() {
  const state = useStore((s) => s);
  useSyncExternalStore(preview.subscribe, () => preview.version);
  const container = useRef<HTMLDivElement>(null);
  const gesture = useRef<Gesture | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [cursor, setCursor] = useState<Point | null>(null);
  const { view, tool, selection, pen, previewing } = state;
  const artboard = activeArtboard(state);
  const panning = state.spacePan || tool === 'hand';

  // Zoom to fit when asked, when a document opens and when the artboard changes.
  const fitted = useRef<View | null>(null);
  const fit = () => {
    const board = activeArtboard();
    const [w, h] = [num(board?.props.width), num(board?.props.height)];
    if (w <= 0 || h <= 0 || preview.width <= 0) return;
    const zoom = Math.max(0.05, Math.min((preview.width - FIT_MARGIN * 2) / w, (preview.height - FIT_MARGIN * 2) / h, 8));
    fitted.current = { zoom, x: Math.round((preview.width - w * zoom) / 2), y: Math.round((preview.height - h * zoom) / 2) };
    store.set({ view: fitted.current });
  };
  useEffect(fit, [state.fitRequest, state.artboardId]);

  // Keep the canvas the size of the stage. While the view is still the fitted one
  // (the user has not panned or zoomed), resizing the stage keeps the artboard fitted.
  useEffect(() => {
    const el = container.current!;
    const observer = new ResizeObserver(() => {
      const first = preview.width === 0;
      preview.resize(el.clientWidth, el.clientHeight);
      if (first || store.get().view === fitted.current) fit();
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Wheel pans; Ctrl/Cmd+wheel (and trackpad pinch, which browsers report that way) zooms about the cursor.
  useEffect(() => {
    const el = container.current!;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const box = el.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) zoomAt(e.clientX - box.left, e.clientY - box.top, Math.exp(-e.deltaY * 0.01));
      else store.set({ view: { ...store.get().view, x: store.get().view.x - e.deltaX, y: store.get().view.y - e.deltaY } });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  const stagePoint = (e: { clientX: number; clientY: number }): Point => {
    const box = container.current!.getBoundingClientRect();
    return [e.clientX - box.left, e.clientY - box.top];
  };

  /** Pen: a press adds a point (dragging pulls out its curve handle); pressing the first point closes the path. */
  const penGesture = (stage: Point, art: Point): Gesture | null => {
    const s = store.get();
    const first = s.pen[0];
    if (first && s.pen.length >= 2 && Math.hypot(first.x * s.view.zoom + s.view.x - stage[0], first.y * s.view.zoom + s.view.y - stage[1]) <= CLOSE_DISTANCE) {
      finishPen(true);
      return null;
    }
    const index = s.pen.length;
    store.set({ pen: [...s.pen, { x: art[0], y: art[1], hx: 0, hy: 0 }] });
    const move = (_e: PointerEvent, at: Point) => store.set({ pen: store.get().pen.map((p, i) => (i === index ? { ...p, hx: at[0] - p.x, hy: at[1] - p.y } : p)) });
    return { move, up: () => {} };
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button === 2) return;
    const s = store.get();
    const stage = stagePoint(e);
    const art = toArtboard(s.view, stage);
    const handle = (e.target as Element).getAttribute('data-handle');
    e.currentTarget.setPointerCapture(e.pointerId);
    let next: Gesture | null = null;
    if (e.button === 1 || s.spacePan || s.tool === 'hand') next = panGesture(stage);
    else if (s.previewing) {
      // (Kept from the press: a browser's release carries the last button let go.)
      const from = { pointerId: e.pointerId, button: e.button, pointerType: e.pointerType };
      preview.pointer('down', stage[0], stage[1], from);
      next = { move: (_e, _a, at) => preview.pointer('move', at[0], at[1], from), up: (_e, _a, at) => preview.pointer('up', at[0], at[1], from) };
    } else if (s.tool === 'pen') next = penGesture(stage, art);
    else if (s.tool === 'text') addText(art[0], art[1]);
    else if (s.tool !== 'select') next = createGesture(s.tool, stage, art, setDraft);
    else if (handle && s.selection.length === 1) next = handle === 'rotate' ? rotateGesture(art, s.selection[0]) : resizeGesture(handle, s.selection[0]);
    else {
      const additive = e.shiftKey;
      const hit = preview.hit(stage[0], stage[1]);
      if (!hit) {
        if (!additive) select([]);
        next = marqueeGesture(stage, art, additive, setDraft);
      } else {
        const target = pickTarget(s, hit, e.metaKey || e.ctrlKey);
        if (additive) select([target], true);
        else if (!s.selection.includes(target)) select([target]);
        if (!additive) next = moveGesture(art);
      }
    }
    gesture.current = next;
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const s = store.get();
    const stage = stagePoint(e);
    const art = toArtboard(s.view, stage);
    if (gesture.current) gesture.current.move(e.nativeEvent, art, stage);
    else if (s.previewing) preview.pointer('move', stage[0], stage[1], e);
    if (s.tool === 'pen') setCursor(art);
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const stage = stagePoint(e);
    gesture.current?.up(e.nativeEvent, toArtboard(store.get().view, stage), stage);
    gesture.current = null;
  };

  const onDoubleClick = (e: React.MouseEvent) => {
    const s = store.get();
    // The second press of the double-click already added a point on top of the last one.
    if (s.tool === 'pen') {
      store.set({ pen: s.pen.slice(0, -1) });
      return finishPen(false);
    }
    const stage = stagePoint(e);
    const hit = s.tool === 'select' && !s.previewing ? preview.hit(stage[0], stage[1]) : null;
    if (hit) select([pickDeeper(s, hit)]);
  };

  const boxes = previewing ? [] : selection.flatMap((id) => selectionBox(state, id) ?? []);
  const single = boxes.length === 1 && selection.length === 1 && tool === 'select' ? boxes[0] : null;
  const toStage = (x: number, y: number): Point => [x * view.zoom + view.x, y * view.zoom + view.y];
  const penStage = pen.map((p) => ({ x: p.x * view.zoom + view.x, y: p.y * view.zoom + view.y, hx: p.hx * view.zoom, hy: p.hy * view.zoom }));
  const [boardW, boardH] = [num(artboard?.props.width) * view.zoom, num(artboard?.props.height) * view.zoom];

  const cursorClass = panning ? 'cursor-grab active:cursor-grabbing' : previewing || tool === 'select' ? '' : tool === 'text' ? 'cursor-text' : 'cursor-crosshair';
  const outline = 'fill-none stroke-accent stroke-[1.25]';
  const handleClass = 'pointer-events-auto fill-accent-fg stroke-accent stroke-[1.25]';

  return (
    <div
      ref={container}
      className={cx('absolute inset-0 touch-none overflow-hidden bg-stage', cursorClass)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onPointerLeave={(e) => {
        if (previewing && !gesture.current) preview.pointer('exit', 0, 0, e);
        setCursor(null);
      }}
      onDoubleClick={onDoubleClick}
      onContextMenu={(e) => e.preventDefault()}
    >
      {artboard && <div className="checker absolute shadow-2xl ring-1 shadow-black/60 ring-line-strong" style={{ left: view.x, top: view.y, width: boardW, height: boardH }} />}
      <canvas ref={attachCanvas} className="absolute inset-0 size-full" aria-label="Artboard preview" />
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden>
        {boxes.map((box) => (
          <polygon key={box.id} className={outline} points={points(box.corners)} />
        ))}
        {single?.rotatable && (
          <>
            <line
              className={outline}
              x1={boxPoint(single.corners, 0, -1)[0]}
              y1={boxPoint(single.corners, 0, -1)[1]}
              x2={rotateHandle(single.corners, ROTATE_DISTANCE)[0]}
              y2={rotateHandle(single.corners, ROTATE_DISTANCE)[1]}
            />
            <circle
              className={cx(handleClass, 'cursor-grab')}
              data-handle="rotate"
              r={HANDLE_SIZE / 2 + 0.5}
              cx={rotateHandle(single.corners, ROTATE_DISTANCE)[0]}
              cy={rotateHandle(single.corners, ROTATE_DISTANCE)[1]}
            />
          </>
        )}
        {single?.resizable &&
          HANDLES.map(([name, sx, sy]) => {
            const [x, y] = boxPoint(single.corners, sx, sy);
            return (
              <rect key={name} className={handleClass} data-handle={name} style={{ cursor: CURSORS[name] }} x={x - HANDLE_SIZE / 2} y={y - HANDLE_SIZE / 2} width={HANDLE_SIZE} height={HANDLE_SIZE} />
            );
          })}
        {draft && (
          <rect className={cx('fill-accent/20 stroke-accent stroke-1', draft.kind === 'marquee' && '[stroke-dasharray:3_3]')} x={draft.x} y={draft.y} width={draft.width} height={draft.height} />
        )}
        {penStage.length > 0 && (
          <>
            <path className="fill-none stroke-accent stroke-[1.5]" d={penPath(penStage, false)} />
            {cursor && (
              <line
                className="stroke-accent stroke-1 [stroke-dasharray:3_3]"
                x1={penStage.at(-1)!.x}
                y1={penStage.at(-1)!.y}
                x2={toStage(cursor[0], cursor[1])[0]}
                y2={toStage(cursor[0], cursor[1])[1]}
              />
            )}
            {penStage.map((p, i) => (
              <g key={i}>
                {(p.hx !== 0 || p.hy !== 0) && <line className="stroke-accent stroke-1 opacity-70" x1={p.x - p.hx} y1={p.y - p.hy} x2={p.x + p.hx} y2={p.y + p.hy} />}
                <circle className={cx('stroke-[1.5]', i === 0 ? 'fill-accent stroke-accent-fg' : 'fill-accent-fg stroke-accent')} cx={p.x} cy={p.y} r={i === 0 ? 4.5 : 3.5} />
              </g>
            ))}
          </>
        )}
      </svg>
      {artboard && (
        <button
          type="button"
          className={cx('absolute flex h-4.5 gap-2 rounded-s px-0.5 text-[11px] whitespace-nowrap hover:text-accent', selection.includes(artboard.id) ? 'text-accent' : 'text-fg-dim')}
          style={{ left: view.x, top: view.y - 22 }}
          title="Select the artboard"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={() => select([artboard.id])}
        >
          {labelOf(artboard)}
          <span className="opacity-60">
            {num(artboard.props.width)} × {num(artboard.props.height)}
          </span>
        </button>
      )}
      {previewing && (
        <div className="absolute top-3 left-1/2 -translate-x-1/2 rounded-m border border-accent bg-raised px-3 py-1.5 text-[11px] whitespace-nowrap text-fg-dim shadow-xl shadow-black/60">
          Previewing the state machine. The stage receives your pointer. Press Esc to stop.
        </div>
      )}
      {state.error && (
        <div
          className="absolute bottom-3 left-1/2 flex max-w-[min(720px,calc(100%-24px))] -translate-x-1/2 cursor-text gap-2 rounded-m border border-danger bg-raised px-3 py-1.5 text-[11px] leading-normal shadow-xl shadow-black/60 select-text"
          role="alert"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <TriangleAlert size={14} aria-hidden className="mt-0.5 shrink-0 text-danger" />
          <span>
            <strong className="font-semibold">The document does not compile, so the stage shows the last version that did.</strong> {state.error}
          </span>
        </div>
      )}
    </div>
  );
}
