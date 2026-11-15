// The control for one property of one object, chosen by the property's schema kind.
// Shared by the inspector and the compact rows of the timeline and state machine panels.

import { setProp, setPropsMany } from '../actions.ts';
import { valueOf, valuePatch } from '../doc/autokey.ts';
import { type Node, parentOf } from '../doc/model.ts';
import { type PropDef, defaultOf, isAnimatable, propDef, refCandidates, resolveRef } from '../doc/schema.ts';
import { schema } from '../runtime.ts';
import { useStore } from '../store.ts';
import { Checkbox, ColorField, NumberField, SelectField, TextField } from './fields.tsx';

const MULTILINE = new Set(['d', 'text', 'wgsl', 'glsl', 'hlsl']);

/** Drag sensitivity per number property. Presentational only: unit-range properties need finer steps. */
export const stepOf = (def: PropDef): number => (/0\.\.1|scale|multipl|seconds|fraction|easing|trim/i.test(def.doc) ? 0.01 : 1);

/** The properties a Track or Bind may name: those of the object it drives. */
function propertyNames(doc: readonly Node[], node: Node): string[] {
  const target = node.type === 'Track' ? resolveRef(schema, doc, node.id, 'target', node.props.target) : parentOf(doc, node.id);
  const usable = (p: PropDef) => (node.type === 'Track' ? isAnimatable(p) || p.name === 'd' : !['name', 'bytes'].includes(p.name) && p.kind !== 'reference' && p.kind !== 'property-name');
  return (schema.types.get(target?.type ?? '')?.properties ?? []).filter(usable).map((p) => p.name);
}

/** What kind of value a Key or Property holds: decided by the track's property or the valueType. */
export function valueKind(doc: readonly Node[], node: Node): 'number' | 'color' | 'string' | 'boolean' | 'none' {
  if (node.type === 'Property') {
    const type = node.props.valueType ?? 'number';
    return type === 'bool' ? 'boolean' : type === 'trigger' ? 'none' : (type as 'number' | 'color' | 'string');
  }
  const kind = schema.props.get(String(parentOf(doc, node.id)?.props.property))?.kind;
  return kind === 'color' || kind === 'string' ? kind : 'number';
}

export function ValueControl({ node, kind }: { node: Node; kind: ReturnType<typeof valueKind> }) {
  const value = valueOf(node);
  const write = (v: unknown, coalesce = '') => setPropsMany([[node.id, valuePatch(v, kind === 'string')]], coalesce);
  if (kind === 'boolean') return <Checkbox label="value" value={Boolean(value)} onChange={write} />;
  if (kind === 'color') return <ColorField label="value" value={typeof value === 'string' ? value : '#000000'} onChange={(v) => write(v, `${node.id}:value`)} />;
  if (kind === 'string') return <TextField label="value" value={typeof value === 'string' ? value : ''} onChange={write} multiline />;
  return <NumberField label="value" value={typeof value === 'number' ? value : Number(value) || 0} step={0.1} onChange={write} />;
}

/**
 * Edits `prop` of `node`. `shown` overrides the displayed value (the animated value at
 * the playhead in Animate mode); writes always go through `setProp`, which handles auto-key.
 */
export function PropControl({ node, prop, shown }: { node: Node; prop: string; shown?: unknown }) {
  const doc = useStore((s) => s.doc);
  const def = propDef(schema, node.type, prop);
  if (!def) return null;
  const value = shown ?? node.props[prop] ?? defaultOf(def);
  const set = (v: unknown, coalesce = '') => setProp(node.id, def.name, v, coalesce);
  const text = typeof value === 'string' ? value : '';
  switch (def.kind) {
    case 'number':
    case 'integer':
      return (
        <NumberField label={def.name} value={typeof value === 'number' ? value : 0} step={stepOf(def)} integer={def.kind === 'integer'} min={def.kind === 'integer' ? 0 : undefined} onChange={set} />
      );
    case 'boolean':
      return <Checkbox label={def.name} value={Boolean(value)} onChange={set} />;
    case 'enum':
      return <SelectField label={def.name} value={text} options={def.values ?? []} onChange={set} />;
    case 'color':
      return <ColorField label={def.name} value={text || '#000000'} onChange={(v) => set(v, `${node.id}:${def.name}`)} />;
    case 'reference':
      return <SelectField label={def.name} value={text} options={refCandidates(schema, doc, node.id, def.name)} placeholder="None" onChange={(v) => set(v || undefined)} />;
    case 'property-name':
      return <SelectField label={def.name} value={text} options={propertyNames(doc, node)} placeholder="Choose…" onChange={(v) => set(v || undefined)} />;
    case 'bytes':
      return <span className="flex h-5.5 items-center truncate px-1.5 text-fg-dim">{text.startsWith('@') ? text : `${Math.round((text.length * 3) / 4 / 102.4) / 10} KB embedded`}</span>;
    case 'string':
      return (
        <TextField
          label={def.name}
          value={text}
          placeholder={def.name === 'name' ? node.type : ''}
          multiline={MULTILINE.has(def.name)}
          mono={MULTILINE.has(def.name) && def.name !== 'text'}
          onChange={(v) => set(v === '' ? undefined : v)}
        />
      );
  }
}
