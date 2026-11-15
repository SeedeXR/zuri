// The inspector: every property of the selected object, driven by the runtime's schema
// (kinds pick the control, docs become tooltips). Children that have no place in the
// hierarchy's main flow (geometry, paints, gradients, stops, binds, conditions) are
// edited inline as cards, recursively.

import { ChevronDown, ChevronRight, Diamond, Languages, Plus, X } from 'lucide-react';
import { useState, useSyncExternalStore } from 'react';
import { activeAnimation, addChild, deleteNodes, keysProp, setProp, setPropsMany, shownValue, toggleKey } from '../actions.ts';
import { findTrack, isKeyed, keyAt } from '../doc/autokey.ts';
import { type Node, find } from '../doc/model.ts';
import { labelOf } from '../doc/ops.ts';
import { childTypes, defaultOf } from '../doc/schema.ts';
import { guarded } from '../io/files.ts';
import { isTauri, translateShader } from '../io/tauri.ts';
import { schema } from '../runtime.ts';
import { preview } from '../stage/preview.ts';
import { type State, useKeyingState, useStore } from '../store.ts';
import { scrub } from './fields.tsx';
import { PropControl, ValueControl, stepOf, valueKind } from './PropControl.tsx';
import { TypeIcon } from './icons.tsx';
import { Button, IconButton, cx } from './ui.tsx';

const NEVER = () => () => {};
/** Types with a home of their own (hierarchy rows, timeline, state machine panel): not edited inline. */
const ELSEWHERE = new Set(['Node', 'Shape', 'Image', 'Text', 'NestedArtboard', 'Model', 'Bone', 'Animation', 'StateMachine', 'Event', 'ViewModel', 'Track', 'Key', 'Layer', 'State', 'Listener', 'Transition']);
/** Diamond next to an animatable property: filled when a key sits on the playhead. */
function KeyToggle({ node, prop, state }: { node: Node; prop: string; state: State }) {
  const animation = activeAnimation(state);
  const keyed = keyAt(animation, node, prop, Math.round(state.frame)) !== undefined;
  const tracked = animation !== undefined && typeof node.props.name === 'string' && findTrack(animation, node.props.name, prop) !== undefined;
  return (
    <button
      type="button"
      className={cx('inline-flex h-5.5 w-4 items-center justify-center rounded-s', keyed || tracked ? 'text-key' : 'text-fg-faint hover:text-fg')}
      aria-label={keyed ? `Remove the ${prop} key at the playhead` : `Add a ${prop} key at the playhead`}
      aria-pressed={keyed}
      title={keyed ? 'Remove key at the playhead' : tracked ? 'Animated. Add a key at the playhead' : 'Add key at the playhead'}
      onClick={() => toggleKey(node.id, prop)}
    >
      <Diamond size={10} aria-hidden className={keyed ? 'fill-key' : ''} />
    </button>
  );
}

function PropertyRows({ node }: { node: Node }) {
  const state = useKeyingState();
  // Only the rows of a keyed node show what the runtime draws, so only they follow the stage's poses.
  const live = isKeyed(schema, state, node);
  useSyncExternalStore(live ? preview.subscribe : NEVER, () => (live ? preview.pose : 0));
  const { doc } = state;
  const sugar = node.type === 'Key' || node.type === 'Property';
  return (
    <div className="flex flex-col gap-[3px] py-0.5 pr-2.5 pl-3">
      {schema.types.get(node.type)?.properties.map((def) => {
        // Key / Property spell their value three ways in the schema; one row edits whichever applies.
        if (sugar && (def.name === 'colorValue' || def.name === 'textValue')) return null;
        const kind = sugar && def.name === 'value' ? valueKind(doc, node) : null;
        if (kind === 'none') return null;
        const keys = keysProp(state, node, def.name);
        const value = keys ? shownValue(state, node, def.name) : (node.props[def.name] ?? defaultOf(def));
        const draggable = kind === null && (def.kind === 'number' || def.kind === 'integer');
        return (
          <div className="grid min-h-5.5 grid-cols-[84px_minmax(0,1fr)_16px] items-start gap-1" key={def.name}>
            <span
              className={cx('h-5.5 truncate leading-[22px] text-fg-dim', draggable && 'cursor-ew-resize')}
              title={def.doc}
              {...(draggable ? scrub(typeof value === 'number' ? value : 0, (v) => setProp(node.id, def.name, v), stepOf(def)) : {})}
            >
              {def.name}
            </span>
            {kind ? <ValueControl node={node} kind={kind} /> : <PropControl node={node} prop={def.name} shown={keys ? value : undefined} />}
            {keys && <KeyToggle node={node} prop={def.name} state={state} />}
          </div>
        );
      })}
    </div>
  );
}

/** Fills in a ShaderAsset's GLSL / HLSL from its WGSL (desktop app only; export does this too). */
function TranslateShader({ node }: { node: Node }) {
  const run = () =>
    guarded('Could not translate the shader', async () => {
      const [glsl, hlsl] = await translateShader(String(node.props.wgsl ?? ''));
      setPropsMany([[node.id, { glsl, hlsl }]]);
    });
  return (
    <div className="mt-2 ml-3">
      <Button disabled={!isTauri} title={isTauri ? 'Translate the WGSL into GLSL and HLSL' : 'Shader translation needs the desktop app'} onClick={run}>
        <Languages size={13} aria-hidden /> Translate WGSL
      </Button>
    </div>
  );
}

function ChildCard({ node, depth }: { node: Node; depth: number }) {
  const [open, setOpen] = useState(depth < 2);
  const color = typeof node.props.color === 'string' && !node.children.some((c) => c.type.endsWith('Gradient')) ? node.props.color : null;
  return (
    <div className={cx('pt-1 pb-1.5', depth > 1 ? 'mt-1.5 ml-3 border-l border-line' : 'mt-2 border-t border-line')}>
      <div className="flex h-6.5 items-center gap-1 pr-2 pl-1.5">
        <button type="button" className="flex h-5.5 min-w-0 flex-1 items-center gap-1.5 rounded-s text-left" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown size={12} aria-hidden className="shrink-0 text-fg-faint" /> : <ChevronRight size={12} aria-hidden className="shrink-0 text-fg-faint" />}
          <TypeIcon node={node} size={13} />
          <span className="font-semibold">{node.type}</span>
          {typeof node.props.name === 'string' && <span className="truncate text-fg-dim">{node.props.name}</span>}
        </button>
        {color && <span className="size-3 shrink-0 rounded-[2px] ring-1 ring-line ring-inset" style={{ background: color }} aria-hidden />}
        <IconButton icon={X} size={13} label={`Remove ${labelOf(node)}`} onClick={() => deleteNodes([node.id])} />
      </div>
      {open && <NodeEditor node={node} depth={depth} />}
    </div>
  );
}

function NodeEditor({ node, depth }: { node: Node; depth: number }) {
  const inline = childTypes(schema, node.type).filter((t) => !ELSEWHERE.has(t));
  return (
    <>
      <PropertyRows node={node} />
      {node.type === 'ShaderAsset' && <TranslateShader node={node} />}
      {node.children
        .filter((c) => inline.includes(c.type))
        .map((c) => (
          <ChildCard key={c.id} node={c} depth={depth + 1} />
        ))}
      {inline.length > 0 && (
        <label className="relative mt-2 ml-3 inline-flex h-5.5 items-center gap-1 rounded-s bg-input pr-2 pl-1.5 text-fg-dim focus-within:outline-2 focus-within:outline-accent hover:bg-hover hover:text-fg">
          <Plus size={12} aria-hidden />
          <select className="appearance-none bg-transparent outline-none" aria-label={`Add to ${labelOf(node)}`} value="" onChange={(e) => e.target.value && addChild(node.id, e.target.value)}>
            <option value="">{depth === 0 ? 'Add…' : `Add to ${node.type}…`}</option>
            {inline.map((t) => (
              <option key={t} value={t} title={schema.types.get(t)?.doc}>
                {t}
              </option>
            ))}
          </select>
        </label>
      )}
    </>
  );
}

export function Inspector() {
  const doc = useStore((s) => s.doc);
  const selection = useStore((s) => s.selection);
  const artboardId = useStore((s) => s.artboardId);
  const node = find(doc, selection.length === 1 ? selection[0] : artboardId);
  return (
    <aside className="flex h-full min-h-0 flex-col bg-panel" aria-label="Inspector">
      {selection.length > 1 ? (
        <p className="empty px-3 py-2.5">{selection.length} objects selected. Select one to edit its properties.</p>
      ) : node ? (
        <>
          <header className="flex h-8 shrink-0 items-center gap-2 border-b border-line px-3 font-semibold" title={schema.types.get(node.type)?.doc}>
            <TypeIcon node={node} size={15} />
            <span>{node.type}</span>
            <span className="truncate font-normal text-fg-dim">{typeof node.props.name === 'string' ? node.props.name : ''}</span>
          </header>
          <div className="min-h-0 flex-1 overflow-auto pt-2 pb-6" key={node.id}>
            <NodeEditor node={node} depth={0} />
          </div>
        </>
      ) : (
        <p className="empty px-3 py-2.5">Nothing to inspect: the document has no artboard.</p>
      )}
    </aside>
  );
}
