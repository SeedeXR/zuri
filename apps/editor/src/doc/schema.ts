// Typed view of the runtime's schema (`Runtime.schema()` / `zuri schema`): the types, their
// properties, where they may live, the class their names are unique in and the class each
// reference points at (and the types, where only some of the class will do). The one fact
// the schema does not carry, which types are geometry, is kept here by hand.

import { type Node, pathTo, walk } from './model.ts';

export type PropKind = 'number' | 'integer' | 'boolean' | 'color' | 'string' | 'bytes' | 'reference' | 'property-name' | 'enum';

export interface PropDef {
  name: string;
  /** The runtime's key for the property (`Instance.property`). */
  key: number;
  kind: PropKind;
  doc: string;
  default?: number | boolean | string;
  values?: string[];
  /** Reference properties: the class of object they must point at. */
  target?: string;
  /** Reference properties: the types they accept, when not every type of the class. */
  types?: string[];
}

export interface TypeDef {
  type: string;
  doc: string;
  /** Types this one may be a child of. Empty = document root. */
  parents: string[];
  properties: PropDef[];
  /** Names are unique, and references resolve, among the objects of one class. */
  class: string;
  /** Lives in an artboard instance's scene tree: tracks can target it. */
  scene: boolean;
}

export interface Schema {
  types: Map<string, TypeDef>;
  /** Property definitions by name (a name means the same thing on every type). */
  props: Map<string, PropDef>;
}

export function makeSchema(raw: unknown): Schema {
  const list = (raw as { types: TypeDef[] }).types;
  const types = new Map(list.map((t) => [t.type, t]));
  const props = new Map(list.flatMap((t) => t.properties.map((p) => [p.name, p] as const)));
  return { types, props };
}

export const propDef = (schema: Schema, type: string, name: string): PropDef | undefined => schema.types.get(type)?.properties.find((p) => p.name === name);

/** May `child` live under `parent` (null = document root)? */
export function canParent(schema: Schema, child: string, parent: string | null): boolean {
  const def = schema.types.get(child);
  if (!def) return false;
  return parent === null ? def.parents.length === 0 : def.parents.includes(parent);
}

/** Types that may be added under `parent`. */
export const childTypes = (schema: Schema, parent: string): string[] => [...schema.types.values()].filter((t) => t.parents.includes(parent)).map((t) => t.type);

/** The value a property has when it is absent from the source. */
export function defaultOf(def: PropDef): unknown {
  if (def.default !== undefined) return def.default;
  return def.kind === 'color' ? '#000000' : def.kind === 'string' ? '' : undefined;
}

/** Number and color properties can be keyed by the timeline's auto-key. */
export const isAnimatable = (def: PropDef | undefined): boolean => def?.kind === 'number' || def?.kind === 'color';

/** The class of a type: names are unique, and references resolve, among the objects of one class. */
export const classOf = (schema: Schema, type: string): string | undefined => schema.types.get(type)?.class;
/** Classes of objects that live at the document root (assets, artboards) are named document-wide, the rest per artboard. */
export const isGlobal = (schema: Schema, cls: string | undefined): boolean => [...schema.types.values()].some((t) => t.class === cls && t.parents.length === 0);
/** Assets are the document roots that are not artboards. */
export const isAsset = (schema: Schema, type: string): boolean => type !== 'Artboard' && canParent(schema, type, null);

// ---- what the schema does not carry -------------------------------------------------------

export const GEOMETRY_TYPES: ReadonlySet<string> = new Set(['Rectangle', 'Ellipse', 'Polygon', 'Star', 'Path']);

/**
 * The node a reference resolves to, exactly as the compiler does it: a matching name in
 * the owner's artboard first, then among the document roots. Undefined when the name is
 * missing or ambiguous.
 */
export function resolveRef(schema: Schema, roots: readonly Node[], ownerId: string, prop: string, name: unknown): Node | undefined {
  const path = pathTo(roots, ownerId);
  const cls = path && propDef(schema, path.at(-1)!.type, prop)?.target;
  if (!path || !cls || typeof name !== 'string') return undefined;
  const match = (n: Node) => classOf(schema, n.type) === cls && n.props.name === name;
  const local = [...walk([path[0]])].filter(match);
  if (local.length) return local.length === 1 ? local[0] : undefined;
  const global = roots.filter(match);
  return global.length === 1 ? global[0] : undefined;
}

/** Named objects a reference property of `ownerId` may sensibly point at, in document order. */
export function refCandidates(schema: Schema, roots: readonly Node[], ownerId: string, prop: string): string[] {
  const path = pathTo(roots, ownerId);
  const owner = path?.at(-1);
  const def = owner && propDef(schema, owner.type, prop);
  const cls = def?.target;
  if (!path || !owner || !cls) return [];
  const scope: readonly Node[] = isGlobal(schema, cls)
    ? roots
    : prop === 'to'
      ? (path.find((n) => n.type === 'Layer')?.children ?? []) // a Transition stays inside its Layer
      : [...walk([path[0]])];
  const fits = (n: Node): boolean => {
    if (classOf(schema, n.type) !== cls || n.id === ownerId) return false;
    if (def.types) return def.types.includes(n.type);
    if (prop === 'target') return schema.types.get(n.type)!.scene;
    if (prop === 'artboard') return n.id !== path[0].id; // an artboard cannot nest itself
    return true;
  };
  const names = scope
    .filter(fits)
    .map((n) => n.props.name)
    .filter((n): n is string => typeof n === 'string' && n !== '');
  return [...new Set(names)];
}
