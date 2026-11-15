// Typed view of the runtime's schema (`Runtime.schema()` / `zuri schema`) plus the few
// facts the schema JSON does not carry yet (which types are scene objects, and which
// class of object each reference property points at). Those mirror
// crates/zuri-core/src/schema.rs and are checked against the real schema in the tests.

import { type Node, pathTo, walk } from './model.ts';

export type PropKind = 'number' | 'integer' | 'boolean' | 'color' | 'string' | 'bytes' | 'reference' | 'property-name' | 'enum';

export interface PropDef {
  name: string;
  kind: PropKind;
  doc: string;
  default?: number | boolean | string;
  values?: string[];
}

export interface TypeDef {
  type: string;
  doc: string;
  /** Types this one may be a child of. Empty = document root. */
  parents: string[];
  properties: PropDef[];
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

// ---- mirrors of schema.rs ---------------------------------------------------------------

/** Objects that live in an artboard instance's scene tree (`is_scene`): tracks can target them. */
export const SCENE_TYPES: ReadonlySet<string> = new Set([
  'Artboard',
  'Node',
  'Shape',
  'Rectangle',
  'Ellipse',
  'Polygon',
  'Star',
  'Path',
  'Fill',
  'Stroke',
  'LinearGradient',
  'RadialGradient',
  'GradientStop',
  'Clip',
  'Image',
  'Text',
  'NestedArtboard',
  'Model',
  'ShaderPaint',
]);
export const GEOMETRY_TYPES: ReadonlySet<string> = new Set(['Rectangle', 'Ellipse', 'Polygon', 'Star', 'Path']);
export const ASSET_TYPES: ReadonlySet<string> = new Set(['ImageAsset', 'FontAsset', 'MeshAsset', 'ShaderAsset']);
/** Types the runtime hit-tests: pointer listeners make sense on these and on groups of them. */
const POINTER_TYPES: ReadonlySet<string> = new Set(['Node', 'Shape', 'Text', 'Image', 'Model']);

/** Reference classes (`ref_class`): names are resolved per class within an artboard. */
export const CLASS = { drawable: 0, state: 1, animation: 2, property: 3, event: 4, asset: 5, artboard: 6 } as const;

export function refClass(type: string): number {
  if (type === 'State') return CLASS.state;
  if (type === 'Animation') return CLASS.animation;
  if (type === 'Property') return CLASS.property;
  if (type === 'Event') return CLASS.event;
  if (ASSET_TYPES.has(type)) return CLASS.asset;
  if (type === 'Artboard') return CLASS.artboard;
  return CLASS.drawable;
}

/** The class a reference property of an `owner` type must point at (`expected_class`). */
export function expectedClass(owner: string, prop: string): number {
  switch (prop) {
    case 'to':
      return CLASS.state;
    case 'animation':
      return CLASS.animation;
    case 'input':
      return CLASS.property;
    case 'source':
      return owner === 'Bind' ? CLASS.property : CLASS.drawable;
    case 'emit':
      return CLASS.event;
    case 'asset':
      return CLASS.asset;
    case 'artboard':
      return CLASS.artboard;
    default:
      return CLASS.drawable;
  }
}

const ASSET_FOR: Record<string, string> = { Image: 'ImageAsset', Text: 'FontAsset', Model: 'MeshAsset', ShaderPaint: 'ShaderAsset' };

/**
 * The node a reference resolves to, exactly as the compiler does it: a matching name in
 * the owner's artboard first, then among the document roots. Undefined when the name is
 * missing or ambiguous.
 */
export function resolveRef(roots: readonly Node[], ownerId: string, prop: string, name: unknown): Node | undefined {
  const path = pathTo(roots, ownerId);
  if (!path || typeof name !== 'string') return undefined;
  const cls = expectedClass(path.at(-1)!.type, prop);
  const match = (n: Node) => refClass(n.type) === cls && n.props.name === name;
  const local = [...walk([path[0]])].filter(match);
  if (local.length) return local.length === 1 ? local[0] : undefined;
  const global = roots.filter(match);
  return global.length === 1 ? global[0] : undefined;
}

/** Named objects a reference property of `ownerId` may sensibly point at, in document order. */
export function refCandidates(roots: readonly Node[], ownerId: string, prop: string): string[] {
  const path = pathTo(roots, ownerId);
  if (!path) return [];
  const owner = path.at(-1)!;
  const cls = expectedClass(owner.type, prop);
  const scope: readonly Node[] =
    cls === CLASS.asset || cls === CLASS.artboard
      ? roots
      : prop === 'to'
        ? (path.find((n) => n.type === 'Layer')?.children ?? []) // a Transition stays inside its Layer
        : [...walk([path[0]])];
  const fits = (n: Node): boolean => {
    if (refClass(n.type) !== cls || n.id === ownerId) return false;
    if (prop === 'asset') return n.type === ASSET_FOR[owner.type];
    if (prop === 'source' && owner.type === 'Clip') return n.type === 'Shape';
    if (prop === 'target' && owner.type === 'Listener') return POINTER_TYPES.has(n.type);
    if (prop === 'target') return SCENE_TYPES.has(n.type) && n.type !== 'Artboard';
    if (prop === 'artboard') return n.id !== path[0].id; // an artboard cannot nest itself
    return true;
  };
  const names = scope
    .filter(fits)
    .map((n) => n.props.name)
    .filter((n): n is string => typeof n === 'string' && n !== '');
  return [...new Set(names)];
}
