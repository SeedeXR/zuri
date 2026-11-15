// Auto-layout of a state machine layer: the format stores no positions, so states are
// placed left to right by their distance from the entry state. Framework-free.

import type { Node } from '../doc/model.ts';

export const STATE_W = 112;
export const STATE_H = 40;
const GAP_X = 60;
const GAP_Y = 22;
const PAD = 24;

export interface Placed {
  state: Node;
  x: number;
  y: number;
}

export interface Edge {
  transition: Node;
  /** SVG path data of the arrow. */
  path: string;
  /** Where a label for the arrow can sit. */
  mid: [number, number];
}

export interface Graph {
  states: Placed[];
  edges: Edge[];
  width: number;
  height: number;
}

const targetsOf = (state: Node, byName: Map<unknown, Node>): Node[] => state.children.flatMap((t) => (t.type === 'Transition' && byName.has(t.props.to) ? [byName.get(t.props.to)!] : []));

export function layoutLayer(layer: Node): Graph {
  const states = layer.children.filter((n) => n.type === 'State');
  const byName = new Map<unknown, Node>(states.map((s) => [s.props.name, s]));

  // Column = number of transitions from the entry state (breadth first). `any` states sit with the entry.
  const column = new Map<string, number>();
  const queue = states.filter((s) => s.props.kind === 'entry' || s.props.kind === 'any');
  if (!queue.length && states.length) queue.push(states[0]);
  for (const s of queue) column.set(s.id, 0);
  for (let i = 0; i < queue.length; i++) {
    for (const next of targetsOf(queue[i], byName)) {
      if (column.has(next.id)) continue;
      column.set(next.id, column.get(queue[i].id)! + 1);
      queue.push(next);
    }
  }
  // States nothing leads to go in a last column of their own.
  const orphanColumn = Math.max(-1, ...column.values()) + 1;
  const rows = new Map<number, number>();
  const placed: Placed[] = states.map((state) => {
    const col = column.get(state.id) ?? orphanColumn;
    const row = rows.get(col) ?? 0;
    rows.set(col, row + 1);
    return { state, x: PAD + col * (STATE_W + GAP_X), y: PAD + row * (STATE_H + GAP_Y) };
  });
  const at = new Map(placed.map((p) => [p.state.id, p]));

  const pairs = new Map<string, number>();
  const edges: Edge[] = placed.flatMap((from) =>
    from.state.children.flatMap((transition): Edge[] => {
      const target = transition.type === 'Transition' ? byName.get(transition.props.to) : undefined;
      const to = target && at.get(target.id);
      if (!to) return [];
      // Parallel transitions between the same two states are spread apart.
      const pair = `${from.state.id}>${to.state.id}`;
      const lane = pairs.get(pair) ?? 0;
      pairs.set(pair, lane + 1);
      return [{ transition, ...route(from, to, lane) }];
    }),
  );

  const width = Math.max(0, ...placed.map((p) => p.x + STATE_W)) + PAD + 16;
  const height = Math.max(0, ...placed.map((p) => p.y + STATE_H)) + PAD + 16;
  return { states: placed, edges, width, height };
}

/** Forward arrows leave the right side and run above center; backward ones return below it. */
function route(from: Placed, to: Placed, lane: number): Pick<Edge, 'path' | 'mid'> {
  const shift = 7 + lane * 8;
  const cy = (p: Placed) => p.y + STATE_H / 2;
  if (from === to) {
    const [x, y] = [from.x + STATE_W / 2, from.y];
    return { path: `M${x - 14} ${y} C${x - 40} ${y - 34 - lane * 8} ${x + 40} ${y - 34 - lane * 8} ${x + 14} ${y}`, mid: [x, y - 26 - lane * 8] };
  }
  const forward = to.x > from.x;
  const sameColumn = to.x === from.x;
  const [x1, y1] = forward || sameColumn ? [from.x + STATE_W, cy(from) - (sameColumn ? 0 : shift)] : [from.x, cy(from) + shift];
  const [x2, y2] = forward ? [to.x, cy(to) - shift] : [to.x + STATE_W, cy(to) + (sameColumn ? 0 : shift)];
  const bend = sameColumn ? 46 + lane * 10 : Math.max(36, Math.abs(x2 - x1) / 2);
  const [c1, c2] = forward ? [x1 + bend, x2 - bend] : sameColumn ? [x1 + bend, x2 + bend] : [x1 - bend, x2 + bend];
  return { path: `M${x1} ${y1} C${c1} ${y1} ${c2} ${y2} ${x2} ${y2}`, mid: [(x1 + x2 + (sameColumn ? bend * 1.5 : 0)) / 2, (y1 + y2) / 2 + (forward ? -6 : 12)] };
}
