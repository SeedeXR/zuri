// Bottom panel, State Machine tab: machines and layers, the state graph of the current
// layer (auto-laid-out; drag from a state's port to another state to connect them), the
// view model, listeners and events, and live controls while the machine is previewed.

import { Play, Plus, Square, X, Zap } from 'lucide-react';
import { type PointerEvent as ReactPointerEvent, type ReactNode, useRef, useState, useSyncExternalStore } from 'react';
import { addChild, addState, addTransition, deleteNodes, select, togglePlay } from '../actions.ts';
import { type Node, append } from '../doc/model.ts';
import { labelOf, makeChild } from '../doc/ops.ts';
import { propDef } from '../doc/schema.ts';
import { Checkbox, ColorField, NumberField, TextField } from '../panels/fields.tsx';
import { PropControl, ValueControl, valueKind } from '../panels/PropControl.tsx';
import { Button, IconButton, Menu, cx, drag } from '../panels/ui.tsx';
import { schema } from '../runtime.ts';
import { preview } from '../stage/preview.ts';
import { store, useStore } from '../store.ts';
import { type Placed, STATE_H, STATE_W, layoutLayer } from './graph.ts';

/** What a state does, by color: entry is where it starts, exit where it ends. */
const KIND_COLORS: Record<string, string> = { entry: 'fill-success', any: 'fill-warning', exit: 'fill-danger', blend: 'fill-accent' };
const OPS: Record<string, string> = { eq: '=', ne: '≠', lt: '<', le: '≤', gt: '>', ge: '≥' };

/** "hover = 1 & pulse": what has to hold for a transition to fire. Triggers have no comparison. */
function conditionText(transition: Node, triggers: ReadonlySet<unknown>): string {
  return transition.children
    .filter((c) => c.type === 'Condition')
    .map((c) => (triggers.has(c.props.input) ? String(c.props.input) : `${String(c.props.input ?? '?')} ${OPS[String(c.props.op ?? 'eq')]} ${c.props.value ?? 0}`))
    .join(' & ');
}

function Graph({ layer, artboard }: { layer: Node; artboard: Node }) {
  const selection = useStore((s) => s.selection);
  const svg = useRef<SVGSVGElement>(null);
  const [link, setLink] = useState<{ from: Placed; x: number; y: number } | null>(null);
  const graph = layoutLayer(layer);
  const kinds = propDef(schema, 'State', 'kind')?.values ?? [];
  const properties = artboard.children.filter((n) => n.type === 'ViewModel').flatMap((vm) => vm.children);
  const triggers = new Set(properties.filter((p) => p.props.valueType === 'trigger').map((p) => p.props.name));

  /** Drag from a state's port: releasing over another state creates the Transition. */
  const connect = (e: ReactPointerEvent<SVGCircleElement>, from: Placed) => {
    e.stopPropagation();
    const at = (ev: { clientX: number; clientY: number }) => {
      const box = svg.current!.getBoundingClientRect();
      return { x: ev.clientX - box.left, y: ev.clientY - box.top };
    };
    setLink({ from, ...at(e) });
    const up = (ev: PointerEvent) => {
      setLink(null);
      const target = ev.type === 'pointerup' && document.elementFromPoint(ev.clientX, ev.clientY)?.closest('[data-state]')?.getAttribute('data-state');
      if (target) addTransition(from.state.id, target);
    };
    drag(e, (ev) => setLink({ from, ...at(ev) }), up);
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-col bg-stage">
      <div className="flex h-8.5 shrink-0 items-center gap-2 border-b border-line bg-panel px-2">
        <Menu
          label="Add a state"
          className="h-5.5 bg-input px-2 text-fg hover:bg-hover"
          trigger={
            <>
              <Plus size={13} aria-hidden /> State
            </>
          }
          items={kinds.map((kind) => ({
            label: kind,
            run: () => addState(kind),
            title: kind === 'animation' ? 'Plays an animation' : kind === 'blend' ? 'Mixes animations by an input' : `The layer's ${kind} state`,
          }))}
        />
        <span className="truncate text-[11px] text-fg-faint">Drag from the dot on a state to another state to add a transition. Select a state or an arrow to edit it in the inspector.</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto" onPointerDown={(e) => e.target === e.currentTarget && select([])}>
        <svg ref={svg} className="block" width={Math.max(graph.width, 200)} height={Math.max(graph.height, 120)} role="group" aria-label={`States of layer ${labelOf(layer)}`}>
          <defs>
            {(['arrow', 'arrow-selected'] as const).map((id) => (
              <marker key={id} id={id} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                <path d="M0 0 L8 4 L0 8 Z" className={id === 'arrow' ? 'fill-fg-faint' : 'fill-accent'} />
              </marker>
            ))}
          </defs>
          {graph.edges.map((edge) => {
            const selected = selection.includes(edge.transition.id);
            const label = conditionText(edge.transition, triggers);
            return (
              <g key={edge.transition.id} className="group" onPointerDown={() => select([edge.transition.id])}>
                <path className="fill-none stroke-transparent stroke-[12]" d={edge.path} />
                <path
                  className={cx('fill-none', selected ? 'stroke-accent stroke-[1.75]' : 'stroke-fg-faint stroke-[1.25] group-hover:stroke-fg')}
                  d={edge.path}
                  markerEnd={`url(#${selected ? 'arrow-selected' : 'arrow'})`}
                />
                {label && (
                  <text className={cx('stroke-stage stroke-[3px] text-[10px] [paint-order:stroke]', selected ? 'fill-accent' : 'fill-fg-dim')} x={edge.mid[0]} y={edge.mid[1]} textAnchor="middle">
                    {label}
                  </text>
                )}
              </g>
            );
          })}
          {graph.states.map((placed) => {
            const { state, x, y } = placed;
            const kind = String(state.props.kind ?? 'animation');
            const detail = kind === 'animation' ? String(state.props.animation ?? 'no animation') : kind;
            return (
              <g
                key={state.id}
                data-state={state.id}
                className="group"
                transform={`translate(${x} ${y})`}
                tabIndex={0}
                role="button"
                aria-label={`State ${labelOf(state)} (${detail})`}
                onPointerDown={() => select([state.id])}
                onKeyDown={(e) => e.key === 'Enter' && select([state.id])}
              >
                <rect
                  className={cx('fill-raised group-hover:fill-hover', selection.includes(state.id) ? 'stroke-accent stroke-[1.75]' : 'stroke-line-strong stroke-1')}
                  width={STATE_W}
                  height={STATE_H}
                  rx={7}
                />
                <text className="fill-fg font-semibold" x={12} y={17}>
                  {labelOf(state)}
                </text>
                <text className={cx('text-[11px]', KIND_COLORS[kind] ?? 'fill-fg-faint')} x={12} y={31}>
                  {detail}
                </text>
                {kind !== 'exit' && (
                  <circle
                    className="cursor-crosshair fill-stage stroke-fg-dim stroke-[1.25] hover:fill-accent hover:stroke-accent-fg"
                    cx={STATE_W}
                    cy={STATE_H / 2}
                    r={5}
                    onPointerDown={(e) => connect(e, placed)}
                  />
                )}
              </g>
            );
          })}
          {link && <line className="pointer-events-none stroke-accent stroke-[1.5] [stroke-dasharray:4_3]" x1={link.from.x + STATE_W} y1={link.from.y + STATE_H / 2} x2={link.x} y2={link.y} />}
        </svg>
      </div>
    </div>
  );
}

/** A titled block of the side columns, with its add / action buttons on the right. */
function Section({ title, doc, actions, children }: { title: ReactNode; doc?: string; actions: ReactNode; children: ReactNode }) {
  return (
    <section className="border-b border-line px-2 pt-1 pb-2.5">
      <header className="flex h-7 items-center">
        <h3 className="flex-1 truncate font-semibold" title={doc}>
          {title}
        </h3>
        {actions}
      </header>
      {children}
    </section>
  );
}

const NOTE = 'empty py-0.5';
const LIST_ROW = 'block h-6 w-full truncate rounded-s px-2 text-left';

/** A view-model property's value in the running preview, wired straight to the instance. */
function LiveControl({ property }: { property: Node }) {
  const live = preview.live;
  const name = String(property.props.name ?? '');
  const type = String(property.props.valueType ?? 'number');
  if (!live) return null;
  try {
    if (type === 'trigger') {
      return (
        <Button onClick={() => live.fire(name)}>
          <Zap size={12} aria-hidden /> Fire
        </Button>
      );
    }
    if (type === 'bool') return <Checkbox label={`${name} (live)`} value={live.getBoolean(name)} onChange={(v) => live.setBoolean(name, v)} />;
    if (type === 'color') return <ColorField label={`${name} (live)`} value={live.getColor(name)} onChange={(v) => live.setColor(name, v)} />;
    if (type === 'string') return <TextField label={`${name} (live)`} value={live.getString(name)} onChange={(v) => live.setString(name, v)} />;
    return <NumberField label={`${name} (live)`} value={live.getNumber(name)} step={0.1} onChange={(v) => live.setNumber(name, v)} />;
  } catch {
    return <span className="text-[11px] text-fg-faint">not in the running preview</span>; // added or renamed since the preview started
  }
}

function ViewModel({ artboard }: { artboard: Node }) {
  const doc = useStore((s) => s.doc);
  const previewing = useStore((s) => s.previewing);
  useSyncExternalStore(preview.subscribe, () => preview.version);
  const vm = artboard.children.find((n) => n.type === 'ViewModel');
  const properties = vm?.children.filter((n) => n.type === 'Property') ?? [];
  const add = () =>
    store.edit((d) => {
      const model = vm ?? makeChild('ViewModel', d, artboard);
      const withModel = vm ? d : append(d, artboard.id, model);
      return append(withModel, model.id, makeChild('Property', withModel, model));
    });
  return (
    <Section
      doc={schema.types.get('ViewModel')?.doc}
      title={<>View model {previewing && <span className="ml-1.5 rounded-full bg-success px-1.5 py-px text-[10px] font-bold text-app uppercase">live</span>}</>}
      actions={<IconButton icon={Plus} size={13} label="Add a view-model property" onClick={add} />}
    >
      {properties.length === 0 && <p className={NOTE}>No properties. They are the inputs conditions and listeners use.</p>}
      {properties.map((p) => (
        <div key={p.id} className="grid min-h-6.5 grid-cols-[minmax(0,1fr)_72px_minmax(0,1.2fr)_22px] items-center gap-[3px] py-px">
          <PropControl node={p} prop="name" />
          <PropControl node={p} prop="valueType" />
          <span className="flex min-w-0 items-center *:min-w-0 *:flex-1 [&>button]:flex-none [&>input]:flex-none [&>textarea]:h-5.5 [&>textarea]:min-h-5.5 [&>textarea]:resize-none">
            {previewing ? <LiveControl property={p} /> : valueKind(doc, p) !== 'none' && <ValueControl node={p} kind={valueKind(doc, p)} />}
          </span>
          <IconButton icon={X} size={12} label={`Remove property ${labelOf(p)}`} onClick={() => deleteNodes([p.id])} />
        </div>
      ))}
    </Section>
  );
}

const LISTENER_COLUMNS = ['target', 'event', 'input', 'action', 'value'];
const LISTENER_GRID = 'grid grid-cols-[minmax(0,1fr)_62px_minmax(0,1fr)_62px_40px_22px] items-center gap-[3px]';

function Listeners({ machine }: { machine: Node }) {
  const selection = useStore((s) => s.selection);
  const listeners = machine.children.filter((n) => n.type === 'Listener');
  return (
    <Section doc={schema.types.get('Listener')?.doc} title="Listeners" actions={<IconButton icon={Plus} size={13} label="Add a listener" onClick={() => addChild(machine.id, 'Listener', true)} />}>
      {listeners.length === 0 && <p className={NOTE}>No listeners. They turn pointer events into property changes.</p>}
      {listeners.length > 0 && (
        <div className={cx(LISTENER_GRID, 'h-4.5 text-[11px] text-fg-faint')} aria-hidden>
          {LISTENER_COLUMNS.map((h) => (
            <span key={h} className="pl-0.5" title={schema.props.get(h)?.doc}>
              {h}
            </span>
          ))}
        </div>
      )}
      {listeners.map((l) => (
        <div key={l.id} className={cx(LISTENER_GRID, 'min-h-6.5 rounded-s py-px', selection.includes(l.id) && 'bg-accent/20 ring-3 ring-accent/20')} onPointerDown={() => select([l.id])}>
          {LISTENER_COLUMNS.map((prop) => (
            <PropControl key={prop} node={l} prop={prop} />
          ))}
          <IconButton icon={X} size={12} label="Remove listener" onClick={() => deleteNodes([l.id])} />
        </div>
      ))}
    </Section>
  );
}

function Events({ artboard }: { artboard: Node }) {
  const previewing = useStore((s) => s.previewing);
  useSyncExternalStore(preview.subscribe, () => preview.version);
  const events = artboard.children.filter((n) => n.type === 'Event');
  return (
    <Section doc={schema.types.get('Event')?.doc} title="Events" actions={<IconButton icon={Plus} size={13} label="Add an event" onClick={() => addChild(artboard.id, 'Event')} />}>
      {events.length === 0 && <p className={NOTE}>No events. Transitions and listeners can emit them to the host app.</p>}
      {events.map((ev) => (
        <div key={ev.id} className="grid min-h-6.5 grid-cols-[minmax(0,1fr)_22px] items-center gap-[3px] py-px">
          <PropControl node={ev} prop="name" />
          <IconButton icon={X} size={12} label={`Remove event ${labelOf(ev)}`} onClick={() => deleteNodes([ev.id])} />
        </div>
      ))}
      {previewing && (
        <>
          <h4 className="mt-2.5 mb-1 text-[11px] font-medium text-fg-dim">Emitted in this preview</h4>
          {preview.events.length === 0 && <p className={NOTE}>Nothing yet. Interact with the stage.</p>}
          <ol className="max-h-24 overflow-y-auto" aria-live="polite">
            {preview.events.toReversed().map((ev, i) => (
              <li key={`${ev.at}-${i}`} className="flex h-5 items-center gap-1.5 text-fg-dim">
                <Zap size={11} aria-hidden className="text-success" /> {ev.name}
              </li>
            ))}
          </ol>
        </>
      )}
    </Section>
  );
}

export function Machine() {
  const doc = useStore((s) => s.doc);
  const artboardId = useStore((s) => s.artboardId);
  const machineId = useStore((s) => s.machineId);
  const layerId = useStore((s) => s.layerId);
  const previewing = useStore((s) => s.previewing);
  const artboard = doc.find((n) => n.id === artboardId);
  if (!artboard) return <p className="empty px-3 py-2.5">Add an artboard to start.</p>;
  const machines = artboard.children.filter((n) => n.type === 'StateMachine');
  const machine = machines.find((n) => n.id === machineId);
  const layers = machine?.children.filter((n) => n.type === 'Layer') ?? [];
  const layer = layers.find((n) => n.id === layerId);

  const addMachine = () => {
    const id = addChild(artboard.id, 'StateMachine', true);
    if (id) store.set({ machineId: id });
  };
  const addLayer = () => {
    const id = machine && addChild(machine.id, 'Layer', true);
    if (id) store.set({ layerId: id });
  };
  return (
    <div className="grid h-full grid-cols-[152px_minmax(0,1fr)_364px]">
      <div className="overflow-y-auto border-r border-line">
        <Section
          doc={schema.types.get('StateMachine')?.doc}
          title="Machines"
          actions={
            <>
              <IconButton icon={previewing ? Square : Play} size={13} label={previewing ? 'Stop the preview' : 'Preview this state machine on the stage'} active={previewing} onClick={togglePlay} />
              <IconButton icon={Plus} size={13} label="Add a state machine" onClick={addMachine} />
            </>
          }
        >
          {machines.length === 0 && <p className={NOTE}>No state machine yet.</p>}
          {machines.map((m) => (
            <button
              key={m.id}
              type="button"
              className={cx(LIST_ROW, m.id === machineId ? 'bg-accent/20' : 'hover:bg-raised')}
              onClick={() => store.set({ machineId: m.id, selection: [m.id], previewing: false })}
            >
              {labelOf(m)}
            </button>
          ))}
        </Section>
        {machine && (
          <Section doc={schema.types.get('Layer')?.doc} title="Layers" actions={<IconButton icon={Plus} size={13} label="Add a layer" onClick={addLayer} />}>
            {layers.map((l) => (
              <button key={l.id} type="button" className={cx(LIST_ROW, l.id === layerId ? 'bg-accent/20' : 'hover:bg-raised')} onClick={() => store.set({ layerId: l.id, selection: [l.id] })}>
                {labelOf(l)}
              </button>
            ))}
          </Section>
        )}
      </div>
      {layer ? (
        <Graph layer={layer} artboard={artboard} />
      ) : (
        <p className="empty bg-stage px-3 py-2.5">{machine ? 'Add a layer to build its state graph.' : 'Add a state machine to make the artboard interactive.'}</p>
      )}
      <div className="overflow-y-auto border-l border-line">
        <ViewModel artboard={artboard} />
        {machine && <Listeners machine={machine} />}
        <Events artboard={artboard} />
      </div>
    </div>
  );
}
