// Bottom panel, Timeline tab (Animate mode): the animation list and settings, a ruler
// with a draggable playhead, and one row per Track with its Keys as diamonds. Keys drag
// to retime; selecting one shows its interpolation and easing in the inspector.

import { Pause, Play, Plus, Trash2, X } from 'lucide-react';
import { type PointerEvent as ReactPointerEvent, useEffect, useRef, useState } from 'react';
import { addChild, deleteNodes, select, setFrame, setPropsMany, togglePlay } from '../actions.ts';
import { sortedKeys } from '../doc/autokey.ts';
import { type Node, find, num } from '../doc/model.ts';
import { labelOf } from '../doc/ops.ts';
import { resolveRef } from '../doc/schema.ts';
import { NumberField, SelectField } from '../panels/fields.tsx';
import { PropControl } from '../panels/PropControl.tsx';
import { Button, IconButton, cx, drag } from '../panels/ui.tsx';
import { schema } from '../runtime.ts';
import { store, useStore } from '../store.ts';

/** Space left and right of the frame range so the first and last keys are not clipped. */
const PAD = 14;
const TICK_STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1200];
/** The animation settings shown in the bar, with the width of each control. */
const BAR_FIELDS = [
  ['name', 'w-28'],
  ['fps', 'w-13'],
  ['duration', 'w-13'],
  ['loop', 'w-24'],
  ['speed', 'w-13'],
] as const;
/** A key's shape tells its interpolation: diamond = linear, square = hold, circle = cubic, a drop (one square corner) = elastic. */
const ELASTIC_SHAPE = 'rounded-full rounded-tr-none';
const KEY_SHAPES: Record<string, string> = { linear: 'rotate-45 rounded-[1.5px]', hold: '', cubic: 'rounded-full', elastic: ELASTIC_SHAPE, elasticIn: ELASTIC_SHAPE, elasticInOut: ELASTIC_SHAPE };

export function Timeline() {
  const doc = useStore((s) => s.doc);
  const artboardId = useStore((s) => s.artboardId);
  const animationId = useStore((s) => s.animationId);
  const frame = useStore((s) => s.frame);
  const playing = useStore((s) => s.playing);
  const area = useStore((s) => s.workArea);
  const selection = useStore((s) => s.selection);
  const lanes = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  const artboard = doc.find((n) => n.id === artboardId);
  const animations = artboard?.children.filter((n) => n.type === 'Animation') ?? [];
  const animation = animations.find((n) => n.id === animationId);

  useEffect(() => {
    const el = lanes.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setWidth(el.clientWidth));
    observer.observe(el);
    return () => observer.disconnect();
  }, [animation === undefined]);

  const addAnimation = () => {
    const id = artboard && addChild(artboard.id, 'Animation');
    if (id) store.set({ animationId: id, frame: 0 });
  };

  if (!artboard || !animation) {
    return (
      <div className="flex flex-col items-start gap-0.5 px-3 py-1.5">
        <p className="empty py-2.5">This artboard has no animation yet.</p>
        <Button tone="accent" onClick={addAnimation} disabled={!artboard}>
          <Plus size={13} aria-hidden /> New animation
        </Button>
      </div>
    );
  }

  const duration = Math.max(1, num(animation.props.duration, 60));
  const perFrame = Math.max(0.01, (width - PAD * 2) / duration);
  const x = (f: number) => PAD + f * perFrame;
  const frameAt = (clientX: number) => Math.min(duration, Math.max(0, Math.round((clientX - lanes.current!.getBoundingClientRect().left - PAD) / perFrame)));
  const step = TICK_STEPS.find((s) => s * perFrame >= 56) ?? TICK_STEPS.at(-1)!;
  const ticks = Array.from({ length: Math.floor(duration / step) + 1 }, (_, i) => i * step);
  const tracks = animation.children.filter((n) => n.type === 'Track');

  const scrub = (e: ReactPointerEvent<HTMLElement>) => {
    setFrame(frameAt(e.clientX));
    drag(e, (ev) => setFrame(frameAt(ev.clientX)));
  };
  /** On the ruler with Shift, the work area: from the frame pressed to the frame let go. No frames (a click) is none, which the store sees to. */
  const stretchArea = (e: ReactPointerEvent<HTMLElement>) => {
    const pressed = frameAt(e.clientX);
    const stretch = (ev: { clientX: number }) => {
      const [at, now] = [frameAt(ev.clientX), store.get().workArea];
      const [from, to] = [Math.min(pressed, at), Math.max(pressed, at)];
      if (now?.from !== from || now.to !== to) store.set({ workArea: { animation: animation.id, from, to } });
    };
    stretch(e);
    drag(e, stretch);
  };

  /** Drag a key (and the other selected keys with it) to another frame. */
  const dragKey = (e: ReactPointerEvent<HTMLElement>, key: Node) => {
    e.stopPropagation();
    const additive = e.shiftKey || e.metaKey || e.ctrlKey;
    const current = store.get().selection;
    if (additive || !current.includes(key.id)) select([key.id], additive);
    const moving = store.get().selection.flatMap((id) => {
      const k = find(store.get().doc, id);
      return k?.type === 'Key' ? [{ id, frame: num(k.props.frame, 0) }] : [];
    });
    const startX = e.clientX;
    let dragging = false;
    drag(
      e,
      (ev) => {
        const delta = Math.round((ev.clientX - startX) / perFrame);
        if (!dragging && delta === 0) return;
        if (!dragging) store.beginGesture();
        dragging = true;
        setPropsMany(moving.map((k) => [k.id, { frame: Math.min(duration, Math.max(0, k.frame + delta)) }]));
      },
      () => dragging && store.endGesture(),
    );
  };

  const rowClass = 'h-6 border-b border-line';
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8.5 shrink-0 items-center gap-1.5 border-b border-line px-2">
        <SelectField
          className="w-33"
          label="Animation"
          value={animation.id}
          options={animations.map((a) => ({ value: a.id, label: labelOf(a) }))}
          onChange={(id) => store.set({ animationId: id, frame: 0, playing: false })}
        />
        <IconButton icon={Plus} size={14} label="New animation" onClick={addAnimation} />
        <IconButton icon={Trash2} size={14} label={`Delete animation ${labelOf(animation)}`} onClick={() => deleteNodes([animation.id])} />
        <span className="mx-1 h-4 w-px bg-line" />
        {BAR_FIELDS.map(([prop, width]) => (
          <label key={prop} className="flex items-center gap-1.5 text-[11px] text-fg-faint" title={schema.props.get(prop)?.doc}>
            {prop}
            <span className={cx('text-[12px] text-fg', width)}>
              <PropControl node={animation} prop={prop} />
            </span>
          </label>
        ))}
        <span className="mx-1 h-4 w-px bg-line" />
        <IconButton icon={playing ? Pause : Play} size={14} label={playing ? 'Pause' : 'Play'} shortcut="⇧Space" active={playing} onClick={togglePlay} />
        <label className="flex items-center gap-1.5 text-[11px] text-fg-faint">
          frame
          <NumberField className="w-14 text-[12px] text-fg" label="Current frame" value={Math.round(frame)} min={0} max={duration} integer onChange={setFrame} />
        </label>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-[220px_minmax(0,1fr)] overflow-x-hidden overflow-y-auto">
        <div className="border-r border-line">
          <div className="sticky top-0 z-3 flex h-6 items-center border-b border-line bg-panel pl-2.5 text-[11px] text-fg-faint">
            {tracks.length} {tracks.length === 1 ? 'track' : 'tracks'}
          </div>
          {tracks.map((track) => {
            const target = resolveRef(schema, doc, track.id, 'target', track.props.target);
            return (
              <div key={track.id} className={cx('group flex h-6 items-center border-b border-line pr-1 pl-2.5', target && selection.includes(target.id) && 'bg-accent/20')}>
                <button
                  type="button"
                  className="flex h-full min-w-0 flex-1 items-center gap-1.5 truncate rounded-s text-left"
                  title="Select the animated object"
                  onClick={() => target && select([target.id])}
                >
                  <span>{String(track.props.target ?? '?')}</span>
                  <span className="text-fg-dim">{String(track.props.property ?? '?')}</span>
                </button>
                <IconButton
                  className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  icon={X}
                  size={12}
                  label={`Delete the ${String(track.props.property)} track of ${String(track.props.target)}`}
                  onClick={() => deleteNodes([track.id])}
                />
              </div>
            );
          })}
        </div>
        <div className="relative min-h-full overflow-x-clip bg-stage" ref={lanes}>
          <div
            className={cx(rowClass, 'sticky top-0 z-3 cursor-ew-resize bg-panel -outline-offset-2')}
            onPointerDown={(e) => (e.shiftKey ? stretchArea(e) : scrub(e))}
            role="slider"
            aria-label="Playhead"
            title="Drag to move the playhead. Shift-drag to set a work area, which Play goes round; Shift-click to clear it."
            aria-valuemin={0}
            aria-valuemax={duration}
            aria-valuenow={Math.round(frame)}
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
                e.stopPropagation();
                setFrame(Math.min(duration, Math.max(0, Math.round(frame) + (e.key === 'ArrowRight' ? 1 : -1) * (e.shiftKey ? 10 : 1))));
              }
            }}
          >
            {ticks.map((t) => (
              <span key={t} className="pointer-events-none absolute inset-y-0 border-l border-line-strong pt-1 pl-1 text-[10px] text-fg-faint tabular-nums" style={{ left: x(t) }}>
                {t}
              </span>
            ))}
          </div>
          {tracks.map((track) => (
            <div key={track.id} className={cx(rowClass, 'relative')} onPointerDown={scrub}>
              {sortedKeys(track).map((key) => (
                <button
                  key={key.id}
                  type="button"
                  className={cx(
                    'absolute top-1/2 -mt-[4.5px] -ml-[4.5px] size-[9px] hover:brightness-150',
                    selection.includes(key.id) ? 'bg-accent-fg ring-2 ring-accent' : 'bg-key ring-1 ring-stage',
                    KEY_SHAPES[String(key.props.interp ?? 'linear')],
                  )}
                  style={{ left: x(num(key.props.frame, 0)) }}
                  aria-label={`Key at frame ${num(key.props.frame, 0)} of ${String(track.props.target)} ${String(track.props.property)}`}
                  title={`Frame ${num(key.props.frame, 0)} (${String(key.props.interp ?? 'linear')})`}
                  onPointerDown={(e) => dragKey(e, key)}
                />
              ))}
            </div>
          ))}
          {tracks.length === 0 && <p className="empty max-w-130 px-3 py-2.5">Move the playhead, then change a property on the stage or in the inspector: a key is written here automatically.</p>}
          {area && <div className="pointer-events-none absolute inset-y-0 z-3 border-x border-accent bg-accent/15" style={{ left: x(area.from), width: (area.to - area.from) * perFrame }} aria-hidden />}
          <div className="pointer-events-none absolute inset-y-0 z-4 w-px bg-accent" style={{ left: x(frame) }} aria-hidden>
            <span className="absolute top-0 -left-[5px] h-[11px] w-[11px] rounded-t-[2px] rounded-b-[6px] bg-accent" />
          </div>
        </div>
      </div>
    </div>
  );
}
