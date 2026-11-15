// Names and the references that depend on them: unique naming, renaming with reference
// updates, and removing what a deletion left dangling. Keeps the document compiling
// while the user edits freely. Framework-free.

import { type Node, find, remove, rootOf, setProps, walk } from './model.ts';
import { type Schema, classOf, isGlobal, propDef, resolveRef } from './schema.ts';

/** Nodes whose names share a namespace with a node of `type` inside `root`: those of its class. */
function namespace(schema: Schema, roots: readonly Node[], root: Node | undefined, type: string): Node[] {
  const cls = classOf(schema, type);
  return (isGlobal(schema, cls) || !root ? roots : [...walk([root])]).filter((n) => classOf(schema, n.type) === cls);
}

/** `base`, or `base2`, `base3`... whichever no other object of the class of `type` in scope uses. */
export function uniqueName(schema: Schema, roots: readonly Node[], root: Node | undefined, type: string, base: string, ignoreId?: string): string {
  const taken = new Set(
    namespace(schema, roots, root, type)
      .filter((n) => n.id !== ignoreId)
      .map((n) => n.props.name),
  );
  if (!taken.has(base)) return base;
  const stem = base.replace(/\d+$/, '');
  for (let i = 2; ; i++) if (!taken.has(`${stem}${i}`)) return `${stem}${i}`;
}

/** The reference properties of a node, with their current values. */
function* refsOf(n: Node, schema: Schema): Generator<[prop: string, value: unknown]> {
  for (const def of schema.types.get(n.type)?.properties ?? []) {
    if (def.kind === 'reference' && n.props[def.name] !== undefined) yield [def.name, n.props[def.name]];
  }
}

/** Rename a node and every reference that resolved to it. An empty name removes the property. */
export function rename(roots: readonly Node[], id: string, name: string, schema: Schema): Node[] {
  const old = find(roots, id)?.props.name;
  let out = setProps(roots, id, { name: name || undefined });
  if (typeof old !== 'string' || !old || !name) return out;
  for (const n of walk(roots)) {
    for (const [prop, value] of refsOf(n, schema)) {
      if (value === old && resolveRef(schema, roots, n.id, prop, value)?.id === id) out = setProps(out, n.id, { [prop]: name });
    }
  }
  return out;
}

/** Give a node a name other objects can refer to (unique in its scope) if it lacks one. */
export function ensureName(roots: readonly Node[], id: string, schema: Schema): { roots: Node[]; name: string } {
  const n = find(roots, id);
  if (!n) return { roots: roots as Node[], name: '' };
  const root = rootOf(roots, id);
  const current = typeof n.props.name === 'string' ? n.props.name : '';
  const clash = namespace(schema, roots, root, n.type).some((o) => o.id !== id && o.props.name === current);
  if (current && !clash) return { roots: roots as Node[], name: current };
  const name = uniqueName(schema, roots, root, n.type, current || n.type.toLowerCase(), id);
  return { roots: rename(roots, id, name, schema), name };
}

/**
 * Make the names inside a freshly cloned subtree unique, and point the clone's internal
 * references at the cloned objects (references to things outside it are left alone).
 */
export function uniquifyNames(roots: readonly Node[], subtreeId: string, schema: Schema): Node[] {
  let out = roots as Node[];
  const renamed = new Map<string, string>();
  for (const n of walk([find(roots, subtreeId)].filter((n) => n !== undefined))) {
    const old = n.props.name;
    if (typeof old !== 'string' || !old) continue;
    const name = uniqueName(schema, out, rootOf(out, n.id), n.type, old, n.id);
    if (name === old) continue;
    renamed.set(`${classOf(schema, n.type)}:${old}`, name);
    out = setProps(out, n.id, { name });
  }
  for (const n of walk([find(out, subtreeId)].filter((n) => n !== undefined))) {
    for (const [prop, value] of refsOf(n, schema)) {
      const name = renamed.get(`${propDef(schema, n.type, prop)?.target}:${value}`);
      if (name) out = setProps(out, n.id, { [prop]: name });
    }
  }
  return out;
}

/** References an object cannot exist without; when one dangles the object goes too. */
const REQUIRED: Record<string, string> = { Track: 'target', Transition: 'to', Condition: 'input', Bind: 'source', Clip: 'source', BlendAnimation: 'animation' };

function dangling(roots: readonly Node[], schema: Schema): Set<string> {
  const out = new Set<string>();
  for (const n of walk(roots)) {
    for (const [prop, value] of refsOf(n, schema)) if (!resolveRef(schema, roots, n.id, prop, value)) out.add(`${n.id}:${prop}`);
  }
  return out;
}

/**
 * Delete nodes and what depended on them: objects whose required reference stopped
 * resolving are removed, optional references are cleared, and Tracks left without keys
 * go too. References that were already broken before the deletion are not touched.
 */
export function removeWithDependents(roots: readonly Node[], ids: ReadonlySet<string>, schema: Schema): Node[] {
  const before = dangling(roots, schema);
  let out = remove(roots, ids);
  // Removing one object can orphan another (a State, then the Transitions to it): repeat until stable.
  for (let pass = 0; pass < 8; pass++) {
    const dead = new Set<string>();
    let next = out;
    for (const key of dangling(out, schema)) {
      if (before.has(key)) continue;
      const [id, prop] = key.split(':');
      const n = find(out, id)!;
      if (REQUIRED[n.type] === prop) dead.add(id);
      else next = setProps(next, id, { [prop]: undefined });
    }
    for (const n of walk(out)) if (n.type === 'Track' && n.children.length === 0) dead.add(n.id);
    next = remove(next, dead);
    if (next === out) break;
    out = next;
  }
  return out;
}
