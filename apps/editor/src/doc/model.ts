// The editor's document model: the JSON source tree with editor-only ids.
// Owns conversion to / from JSON source, traversal, pre-order runtime ids and the
// immutable tree edits everything else is built from. Framework-free.

export interface Node {
  /** Editor-only identity; never written to a file. */
  readonly id: string;
  readonly type: string;
  readonly props: Readonly<Record<string, unknown>>;
  readonly children: readonly Node[];
}

let counter = 0;
const newId = (): string => `n${++counter}`;

export function node(type: string, props: Record<string, unknown> = {}, children: readonly Node[] = []): Node {
  return { id: newId(), type, props, children };
}

// ---- JSON source ----------------------------------------------------------------------

function fromObject(value: unknown, path: string): Node {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error(`${path}: expected an object`);
  const { type, children, ...props } = value as Record<string, unknown>;
  if (typeof type !== 'string') throw new Error(`${path}: missing "type"`);
  if (children !== undefined && !Array.isArray(children)) throw new Error(`${path}.children: expected an array`);
  return node(
    type,
    props,
    (children ?? []).map((c, i) => fromObject(c, `${path}.children[${i}]`)),
  );
}

/** Parse JSON source into document roots. Throws with the JSON path of the problem. */
export function fromSource(text: string): Node[] {
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    throw new Error(`invalid JSON: ${(e as Error).message}`);
  }
  const roots = (doc as { children?: unknown } | null)?.children;
  if (!Array.isArray(roots)) throw new Error('the document needs a top-level "children" array of Artboards and assets');
  return roots.map((c, i) => fromObject(c, `children[${i}]`));
}

function toObject(n: Node): Record<string, unknown> {
  return { type: n.type, ...n.props, ...(n.children.length ? { children: n.children.map(toObject) } : {}) };
}

/** The document as JSON source text. */
export function toSource(roots: readonly Node[]): string {
  return JSON.stringify({ zuri: 1, children: roots.map(toObject) }, null, 2);
}

// ---- traversal ------------------------------------------------------------------------

/** Every node in depth-first pre-order: the order the compiler emits objects in. */
export function* walk(nodes: readonly Node[]): Generator<Node> {
  for (const n of nodes) {
    yield n;
    yield* walk(n.children);
  }
}

/** Ancestors of a node from its root down to the node itself. */
export function pathTo(nodes: readonly Node[], id: string): Node[] | undefined {
  for (const n of nodes) {
    if (n.id === id) return [n];
    const below = pathTo(n.children, id);
    if (below) return [n, ...below];
  }
  return undefined;
}

export const find = (roots: readonly Node[], id: string): Node | undefined => pathTo(roots, id)?.at(-1);
export const parentOf = (roots: readonly Node[], id: string): Node | undefined => pathTo(roots, id)?.at(-2);
/** The top-level object (Artboard or asset) that contains a node. */
export const rootOf = (roots: readonly Node[], id: string): Node | undefined => pathTo(roots, id)?.[0];

/** Runtime object id of a node: its pre-order index in the whole document, or -1. */
export const indexOf = (roots: readonly Node[], id: string): number => [...walk(roots)].findIndex((n) => n.id === id);
/** The node with a runtime object id. */
export const atIndex = (roots: readonly Node[], index: number): Node | undefined => [...walk(roots)][index];
/** A property value as a number, or `fallback` when it is absent or not one. */
export const num = (v: unknown, fallback = 0): number => (typeof v === 'number' ? v : fallback);

// ---- immutable edits ------------------------------------------------------------------

/** Replace one node; everything not on the path to it is shared with the old tree. */
export function update(nodes: readonly Node[], id: string, f: (n: Node) => Node): Node[] {
  let changed = false;
  const out = nodes.map((n) => {
    if (n.id === id) {
      changed = true;
      return f(n);
    }
    const children = update(n.children, id, f);
    if (children === n.children) return n;
    changed = true;
    return { ...n, children };
  });
  return changed ? out : (nodes as Node[]);
}

/** Merge a patch into a node's properties. `undefined` removes a property. */
export function setProps(roots: readonly Node[], id: string, patch: Record<string, unknown>): Node[] {
  return update(roots, id, (n) => {
    const props: Record<string, unknown> = { ...n.props };
    for (const [k, v] of Object.entries(patch)) {
      if (v === undefined) delete props[k];
      else props[k] = v;
    }
    return { ...n, props };
  });
}

export function remove(nodes: readonly Node[], ids: ReadonlySet<string>): Node[] {
  let changed = false;
  const out: Node[] = [];
  for (const n of nodes) {
    if (ids.has(n.id)) {
      changed = true;
      continue;
    }
    const children = remove(n.children, ids);
    if (children !== n.children) changed = true;
    out.push(children === n.children ? n : { ...n, children });
  }
  return changed ? out : (nodes as Node[]);
}

/** Insert `child` under `parentId` (null = document root) at `index` (clamped). */
export function insert(roots: readonly Node[], parentId: string | null, index: number, child: Node): Node[] {
  const place = (list: readonly Node[]) => [...list.slice(0, Math.max(0, index)), child, ...list.slice(Math.max(0, index))];
  if (parentId === null) return place(roots);
  return update(roots, parentId, (p) => ({ ...p, children: place(p.children) }));
}

/** Append `child` as the last (front-most) child of `parentId`. */
export const append = (roots: readonly Node[], parentId: string | null, child: Node): Node[] => insert(roots, parentId, Infinity, child);

/**
 * Move a node under `parentId` at `index`, where `index` counts the new parent's children
 * as they are before the move. Moving a node into itself or its descendants is refused.
 */
export function move(roots: readonly Node[], id: string, parentId: string | null, index: number): Node[] {
  const path = pathTo(roots, id);
  if (!path) return roots as Node[];
  const moving = path.at(-1)!;
  if (parentId !== null && pathTo([moving], parentId)) return roots as Node[];
  const oldParent = path.at(-2);
  const siblings = oldParent ? oldParent.children : roots;
  const sameParent = (oldParent?.id ?? null) === parentId;
  const at = sameParent && siblings.indexOf(moving) < index ? index - 1 : index;
  return insert(remove(roots, new Set([id])), parentId, at, moving);
}

/** Deep copy with fresh ids. */
export function clone(n: Node): Node {
  return node(n.type, n.props, n.children.map(clone));
}
