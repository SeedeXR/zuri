// The application shell: toolbar, left panel (Hierarchy / Assets), stage, bottom panel
// (Timeline / State Machine / Source), inspector, and the resizers between them.

import { ChevronDown, ChevronUp, Plus } from 'lucide-react';
import { type KeyboardEvent, type PointerEvent as ReactPointerEvent, useEffect, useState } from 'react';
import { addArtboard } from './actions.ts';
import { labelOf } from './doc/ops.ts';
import { Machine } from './machine/Machine.tsx';
import { Assets } from './panels/Assets.tsx';
import { Hierarchy } from './panels/Hierarchy.tsx';
import { Inspector } from './panels/Inspector.tsx';
import { Source } from './panels/Source.tsx';
import { Toolbar } from './panels/Toolbar.tsx';
import { SelectField } from './panels/fields.tsx';
import { IconButton, cx, drag } from './panels/ui.tsx';
import { Stage } from './stage/Stage.tsx';
import { type State, store, useStore } from './store.ts';
import { Timeline } from './timeline/Timeline.tsx';

const TOAST_MS = 5000;
const BOTTOM_TABS: [tab: State['bottomTab'], label: string][] = [
  ['timeline', 'Timeline'],
  ['machine', 'State Machine'],
  ['source', 'Source'],
];

/** Where each resizer sits on its panel: a 7px strip centered on the edge. */
const RESIZER_PLACE = {
  left: 'inset-y-0 -left-1 w-[7px] cursor-col-resize',
  right: 'inset-y-0 -right-1 w-[7px] cursor-col-resize',
  top: 'inset-x-0 -top-1 h-[7px] cursor-row-resize',
};

interface ResizerProps {
  /** Which edge of its panel the handle sits on; dragging away from the panel grows it. */
  edge: 'left' | 'right' | 'top';
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}

function Resizer({ edge, label, value, min, max, onChange }: ResizerProps) {
  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  const grows = edge === 'right' ? 1 : -1;
  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    const start = edge === 'top' ? e.clientY : e.clientX;
    drag(e, (ev) => onChange(clamp(value + ((edge === 'top' ? ev.clientY : ev.clientX) - start) * grows)));
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const bigger = edge === 'top' ? 'ArrowUp' : edge === 'right' ? 'ArrowRight' : 'ArrowLeft';
    const smaller = edge === 'top' ? 'ArrowDown' : edge === 'right' ? 'ArrowLeft' : 'ArrowRight';
    if (e.key === bigger) onChange(clamp(value + 16));
    else if (e.key === smaller) onChange(clamp(value - 16));
  };
  return (
    <div
      className={cx('absolute z-5 hover:bg-accent/30 focus-visible:bg-accent/30', RESIZER_PLACE[edge])}
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-orientation={edge === 'top' ? 'horizontal' : 'vertical'}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}

function Tab({ current, onSelect, children }: { current: boolean; onSelect: () => void; children: string }) {
  return (
    <button type="button" role="tab" aria-selected={current} className={cx('h-5.5 rounded-s px-2 font-medium', current ? 'bg-hover text-fg' : 'text-fg-dim hover:text-fg')} onClick={onSelect}>
      {children}
    </button>
  );
}

function Toast() {
  const toast = useStore((s) => s.toast);
  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => store.get().toast?.id === toast.id && store.set({ toast: null }), TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);
  return (
    <div className="pointer-events-none fixed bottom-6 left-1/2 z-50 -translate-x-1/2" role="status" aria-live="polite">
      {toast && (
        <div className={cx('max-w-140 rounded-m border bg-raised px-3.5 py-2 leading-normal shadow-xl shadow-black/60', toast.kind === 'error' ? 'border-danger' : 'border-line-strong')}>
          {toast.text}
        </div>
      )}
    </div>
  );
}

const TAB_BAR = 'flex h-8 shrink-0 items-center gap-0.5 border-b border-line px-1.5';

export function App() {
  const doc = useStore((s) => s.doc);
  const artboardId = useStore((s) => s.artboardId);
  const mode = useStore((s) => s.mode);
  const leftTab = useStore((s) => s.leftTab);
  const bottomTab = useStore((s) => s.bottomTab);
  const bottomOpen = useStore((s) => s.bottomOpen);
  const title = useStore((s) => `${s.doc !== s.saved ? '• ' : ''}${s.fileName} · Zuri`);
  const [left, setLeft] = useState(248);
  const [right, setRight] = useState(296);
  const [bottom, setBottom] = useState(264);
  const artboards = doc.filter((n) => n.type === 'Artboard');
  const tabs = BOTTOM_TABS.filter(([tab]) => tab !== 'timeline' || mode === 'animate');

  return (
    // Rows: toolbar, work area, bottom panel. Columns: left panel, stage, inspector.
    <div className="grid h-full" style={{ gridTemplate: `44px minmax(0, 1fr) ${bottomOpen ? bottom : 32}px / ${left}px minmax(0, 1fr) ${right}px` }}>
      <title>{title}</title>
      <Toolbar />
      <aside className="relative col-start-1 row-start-2 flex min-h-0 min-w-0 flex-col border-r border-line bg-panel" aria-label="Hierarchy and assets">
        <div className="flex shrink-0 items-center gap-1 border-b border-line py-1.5 pr-1.5 pl-2">
          <SelectField
            className="flex-1"
            label="Active artboard"
            value={artboardId}
            options={artboards.map((a) => ({ value: a.id, label: labelOf(a) }))}
            onChange={(id) => store.set({ artboardId: id, selection: [], playing: false, previewing: false })}
          />
          <IconButton icon={Plus} size={14} label="Add an artboard" shortcut="A" onClick={() => addArtboard()} />
        </div>
        <div className={TAB_BAR} role="tablist" aria-label="Left panel">
          <Tab current={leftTab === 'hierarchy'} onSelect={() => store.set({ leftTab: 'hierarchy' })}>
            Hierarchy
          </Tab>
          <Tab current={leftTab === 'assets'} onSelect={() => store.set({ leftTab: 'assets' })}>
            Assets
          </Tab>
        </div>
        <div className="min-h-0 flex-1 overflow-auto" role="tabpanel">
          {leftTab === 'hierarchy' ? <Hierarchy /> : <Assets />}
        </div>
        <Resizer edge="right" label="Resize the left panel" value={left} min={180} max={480} onChange={setLeft} />
      </aside>
      <main className="relative col-start-2 row-start-2 min-h-0 min-w-0">
        <Stage />
      </main>
      <section className="relative col-span-2 col-start-1 row-start-3 flex min-h-0 min-w-0 flex-col border-t border-line bg-panel" aria-label="Timeline, state machine and source">
        {bottomOpen && <Resizer edge="top" label="Resize the bottom panel" value={bottom} min={140} max={640} onChange={setBottom} />}
        <div className={TAB_BAR} role="tablist" aria-label="Bottom panel">
          {tabs.map(([tab, label]) => (
            <Tab key={tab} current={bottomOpen && bottomTab === tab} onSelect={() => store.set({ bottomTab: tab, bottomOpen: true })}>
              {label}
            </Tab>
          ))}
          <span className="flex-1" />
          <IconButton
            icon={bottomOpen ? ChevronDown : ChevronUp}
            size={14}
            label={bottomOpen ? 'Collapse the bottom panel' : 'Expand the bottom panel'}
            onClick={() => store.set({ bottomOpen: !bottomOpen })}
          />
        </div>
        {bottomOpen && (
          <div className="min-h-0 flex-1 overflow-hidden" role="tabpanel">
            {bottomTab === 'timeline' && mode === 'animate' ? <Timeline /> : bottomTab === 'source' ? <Source /> : <Machine />}
          </div>
        )}
      </section>
      <div className="relative col-start-3 row-span-2 row-start-2 min-h-0 border-l border-line">
        <Inspector />
        <Resizer edge="left" label="Resize the inspector" value={right} min={240} max={520} onChange={setRight} />
      </div>
      <Toast />
    </div>
  );
}
