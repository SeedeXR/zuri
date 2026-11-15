// Left panel, Hierarchy tab: the scene tree of the active artboard. Later children draw
// on top, so each level is listed in reverse: the top row is the front-most object.
// Rows select, rename (double-click or F2), expand, and drag to reorder or reparent
// (validated against the schema's `parents`).

import { ChevronDown, ChevronRight } from 'lucide-react';
import { type DragEvent, type KeyboardEvent, useEffect, useState } from 'react';
import { select, setProp } from '../actions.ts';
import { type Node, find, move, parentOf, pathTo } from '../doc/model.ts';
import { labelOf } from '../doc/ops.ts';
import { canParent } from '../doc/schema.ts';
import { schema } from '../runtime.ts';
import { store, useStore } from '../store.ts';
import { TypeIcon } from './icons.tsx';
import { cx } from './ui.tsx';

/** Artboard children with panels of their own (timeline, state machine tab). */
const ELSEWHERE = new Set(['Animation', 'StateMachine', 'ViewModel', 'Event']);

interface Row {
  node: Node;
  depth: number;
  kids: Node[];
}
type Zone = 'above' | 'inside' | 'below';

function flatten(node: Node, depth: number, expanded: ReadonlySet<string>, out: Row[]): Row[] {
  const kids = node.children.filter((c) => !(depth === 0 && ELSEWHERE.has(c.type))).reverse();
  out.push({ node, depth, kids });
  if (depth === 0 || expanded.has(node.id)) for (const k of kids) flatten(k, depth + 1, expanded, out);
  return out;
}

/** Where a drop on `target` puts `dragged`: the new parent and index, or null when the schema forbids it. */
function dropPlace(doc: readonly Node[], dragged: Node, target: Node, zone: Zone): { parentId: string; index: number } | null {
  const parent = zone === 'inside' ? target : parentOf(doc, target.id);
  if (!parent || !canParent(schema, dragged.type, parent.type) || pathTo([dragged], parent.id)) return null;
  // The list is reversed: "above" a row means after it in document order.
  const index = zone === 'inside' ? parent.children.length : parent.children.indexOf(target) + (zone === 'above' ? 1 : 0);
  return { parentId: parent.id, index };
}

export function Hierarchy() {
  const doc = useStore((s) => s.doc);
  const artboardId = useStore((s) => s.artboardId);
  const selection = useStore((s) => s.selection);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [renaming, setRenaming] = useState<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [drop, setDrop] = useState<{ id: string; zone: Zone } | null>(null);
  const artboard = doc.find((n) => n.id === artboardId);

  // Reveal whatever gets selected (on the stage, say) by opening its ancestors.
  useEffect(() => {
    const ancestors = selection.flatMap(
      (id) =>
        pathTo(doc, id)
          ?.slice(0, -1)
          .map((n) => n.id) ?? [],
    );
    if (ancestors.some((id) => !expanded.has(id))) setExpanded(new Set([...expanded, ...ancestors]));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only a new selection reveals; collapsing by hand must stick
  }, [selection]);

  if (!artboard) return <p className="empty px-3 py-2.5">Add an artboard to start.</p>;
  const rows = flatten(artboard, 0, expanded, []);
  const toggle = (id: string, open = !expanded.has(id)) => {
    const next = new Set(expanded);
    if (open) next.add(id);
    else next.delete(id);
    setExpanded(next);
  };

  const zoneOf = (e: DragEvent<HTMLElement>, target: Node): Zone | null => {
    const dragged = dragging ? find(doc, dragging) : undefined;
    if (!dragged || dragged.id === target.id) return null;
    const box = e.currentTarget.getBoundingClientRect();
    const t = (e.clientY - box.top) / box.height;
    const inside = dropPlace(doc, dragged, target, 'inside') !== null;
    const zone: Zone = inside && t > 0.25 && t < 0.75 ? 'inside' : t < 0.5 ? 'above' : 'below';
    return dropPlace(doc, dragged, target, zone) ? zone : inside ? 'inside' : null;
  };

  const onKey = (e: KeyboardEvent<HTMLElement>, row: Row) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === 'Enter' || e.key === ' ') select([row.node.id], e.shiftKey || e.metaKey || e.ctrlKey);
    else if (e.key === 'F2') setRenaming(row.node.id);
    else if (e.key === 'ArrowRight') toggle(row.node.id, true);
    else if (e.key === 'ArrowLeft') toggle(row.node.id, false);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') (e.currentTarget[e.key === 'ArrowDown' ? 'nextElementSibling' : 'previousElementSibling'] as HTMLElement | null)?.focus();
    else return;
    // Handled here: the global shortcuts must not also nudge the selection with the same arrow key.
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className="py-1" role="tree" aria-label="Hierarchy">
      {rows.map((row) => {
        const { node, depth, kids } = row;
        const open = depth === 0 || expanded.has(node.id);
        const zone = drop?.id === node.id ? drop.zone : null;
        const selected = selection.includes(node.id);
        const named = typeof node.props.name === 'string' && node.props.name !== '';
        return (
          <div
            key={node.id}
            role="treeitem"
            tabIndex={0}
            aria-level={depth + 1}
            aria-selected={selected}
            aria-expanded={kids.length ? open : undefined}
            className={cx('relative flex h-6 items-center gap-1.5 pr-2 -outline-offset-2', selected ? 'bg-accent/20' : 'hover:bg-raised', zone === 'inside' && 'ring-[1.5px] ring-accent ring-inset')}
            style={{ paddingLeft: 6 + depth * 14 }}
            draggable={depth > 0 && renaming !== node.id}
            onClick={(e) => select([node.id], e.shiftKey || e.metaKey || e.ctrlKey)}
            onDoubleClick={() => setRenaming(node.id)}
            onKeyDown={(e) => onKey(e, row)}
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = 'move';
              e.dataTransfer.setData('text/plain', labelOf(node));
              setDragging(node.id);
            }}
            onDragEnd={() => {
              setDragging(null);
              setDrop(null);
            }}
            onDragOver={(e) => {
              const z = zoneOf(e, node);
              if (z) e.preventDefault();
              if (z !== zone) setDrop(z ? { id: node.id, zone: z } : null);
            }}
            onDrop={(e) => {
              e.preventDefault();
              const dragged = dragging ? find(doc, dragging) : undefined;
              const place = dragged && zone ? dropPlace(doc, dragged, node, zone) : null;
              if (dragged && place) {
                store.edit((d) => move(d, dragged.id, place.parentId, place.index));
                if (zone === 'inside') toggle(node.id, true);
              }
              setDrop(null);
            }}
          >
            {(zone === 'above' || zone === 'below') && <span className={cx('pointer-events-none absolute right-1 left-2 h-0.5 bg-accent', zone === 'above' ? '-top-px' : '-bottom-px')} />}
            {kids.length > 0 && depth > 0 ? (
              <button
                type="button"
                className="inline-flex h-4.5 w-3.5 shrink-0 items-center justify-center text-fg-faint hover:text-fg"
                tabIndex={-1}
                aria-label={open ? 'Collapse' : 'Expand'}
                onClick={(e) => {
                  e.stopPropagation();
                  toggle(node.id);
                }}
              >
                {open ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}
              </button>
            ) : (
              <span className="w-3.5 shrink-0" />
            )}
            <TypeIcon node={node} className={selected ? 'text-accent' : 'text-fg-dim'} />
            {renaming === node.id ? (
              <input
                className="h-5 min-w-0 flex-1 rounded-s border border-accent bg-stage px-1 outline-none select-text"
                aria-label="Name"
                autoFocus
                defaultValue={typeof node.props.name === 'string' ? node.props.name : ''}
                onFocus={(e) => e.target.select()}
                onClick={(e) => e.stopPropagation()}
                onBlur={(e) => {
                  if (e.target.value !== (node.props.name ?? '')) setProp(node.id, 'name', e.target.value);
                  setRenaming(null);
                }}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter') e.currentTarget.blur();
                  if (e.key === 'Escape') setRenaming(null);
                }}
              />
            ) : (
              <span className={cx('truncate', !named && 'text-fg-dim')}>{labelOf(node)}</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
