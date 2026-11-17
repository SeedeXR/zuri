// The one application store: document, selection, view, tools, playback and undo
// history. Plain TypeScript + `useSyncExternalStore`; the document tree is immutable, so
// an undo snapshot is just a reference to an old tree.

import { useSyncExternalStore } from 'react';
import { type Node, find, num } from './doc/model.ts';
import type { PenPoint } from './doc/ops.ts';

export type Tool = 'select' | 'hand' | 'Rectangle' | 'Ellipse' | 'Polygon' | 'Star' | 'pen' | 'text' | 'artboard';
export type Mode = 'design' | 'animate';

/** Artboard-to-stage transform in CSS pixels: screen = artboard * zoom + (x, y). */
export interface View {
  zoom: number;
  x: number;
  y: number;
}

export interface State {
  doc: Node[];
  /** The document as last opened or saved; `doc !== saved` means unsaved changes. */
  saved: Node[];
  fileName: string;
  /** Path on disk (Tauri only). */
  filePath: string | null;
  artboardId: string;
  selection: string[];
  mode: Mode;
  tool: Tool;
  /** Space is held: drags pan the stage. */
  spacePan: boolean;
  view: View;
  /** Bumped to ask the stage to zoom to fit. */
  fitRequest: number;
  /** Points of the pen drawing in progress (artboard space). */
  pen: PenPoint[];
  animationId: string | null;
  /** Playhead, in frames of the current animation. */
  frame: number;
  playing: boolean;
  machineId: string | null;
  layerId: string | null;
  /** The stage is running the current state machine and receives the pointer. */
  previewing: boolean;
  /** The preview reports the events that have a sound without playing it. */
  muted: boolean;
  leftTab: 'hierarchy' | 'assets';
  bottomTab: 'timeline' | 'machine' | 'source';
  bottomOpen: boolean;
  /** Why the document does not compile, if it does not. */
  error: string | null;
  toast: { text: string; kind: 'info' | 'error'; id: number } | null;
  canUndo: boolean;
  canRedo: boolean;
}

interface Snapshot {
  doc: Node[];
  artboardId: string;
  selection: string[];
}

const HISTORY_LIMIT = 200;
/** Edits with the same coalesce key closer together than this share one undo step. */
const COALESCE_MS = 600;

let state: State = {
  doc: [],
  saved: [],
  fileName: 'Untitled.json',
  filePath: null,
  artboardId: '',
  selection: [],
  mode: 'design',
  tool: 'select',
  spacePan: false,
  view: { zoom: 1, x: 0, y: 0 },
  fitRequest: 0,
  pen: [],
  animationId: null,
  frame: 0,
  playing: false,
  machineId: null,
  layerId: null,
  previewing: false,
  muted: false,
  leftTab: 'hierarchy',
  bottomTab: 'machine',
  bottomOpen: false,
  error: null,
  toast: null,
  canUndo: false,
  canRedo: false,
};

const listeners = new Set<() => void>();
let past: Snapshot[] = [];
let future: Snapshot[] = [];
let gesture: 'idle' | 'armed' | 'open' = 'idle';
let lastEdit = { key: '', time: 0 };

const firstOf = (nodes: readonly Node[] | undefined, type: string): string | null => nodes?.find((n) => n.type === type)?.id ?? null;
const keep = (nodes: readonly Node[] | undefined, id: string | null, type: string): string | null => (nodes?.some((n) => n.id === id && n.type === type) ? id : firstOf(nodes, type));

/** Keep every id in the state pointing at something that exists. */
function normalize(s: State): State {
  const artboardId = keep(s.doc, s.artboardId, 'Artboard') ?? '';
  const artboard = s.doc.find((n) => n.id === artboardId);
  const animationId = keep(artboard?.children, s.animationId, 'Animation');
  const machineId = keep(artboard?.children, s.machineId, 'StateMachine');
  const layerId = keep(artboard?.children.find((n) => n.id === machineId)?.children, s.layerId, 'Layer');
  const alive = s.selection.filter((id) => find(s.doc, id));
  const end = num(artboard?.children.find((n) => n.id === animationId)?.props.duration, 60);
  const playing = s.playing && animationId !== null && s.mode === 'animate';
  return {
    ...s,
    artboardId,
    animationId,
    machineId,
    layerId,
    selection: alive.length === s.selection.length ? s.selection : alive,
    // Only playback sits between frames: at rest the playhead is on the frame keys are written to.
    frame: Math.min(Math.max(0, playing ? s.frame : Math.round(s.frame)), end),
    playing,
    previewing: s.previewing && machineId !== null,
    canUndo: past.length > 0,
    canRedo: future.length > 0,
  };
}

function commit(next: State): void {
  state = normalize(next);
  for (const l of listeners) l();
}

const snapshot = (): Snapshot => ({ doc: state.doc, artboardId: state.artboardId, selection: state.selection });

export const store = {
  get: (): State => state,

  subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },

  /** Change anything but the document. Not undoable. */
  set(patch: Partial<Omit<State, 'doc'>>): void {
    commit({ ...state, ...patch });
  },

  /**
   * Change the document (one undo step). Inside a gesture, or when `coalesce` matches
   * the previous edit's key within a short time, the edit joins the previous step.
   */
  edit(f: (doc: Node[]) => Node[], patch: Partial<Omit<State, 'doc'>> = {}, coalesce = ''): void {
    const doc = f(state.doc);
    if (doc !== state.doc) {
      const now = performance.now();
      const joins = gesture === 'open' || (coalesce !== '' && coalesce === lastEdit.key && now - lastEdit.time < COALESCE_MS);
      if (!joins) {
        past = [...past.slice(-HISTORY_LIMIT + 1), snapshot()];
        future = [];
      }
      if (gesture === 'armed') gesture = 'open';
      lastEdit = { key: coalesce, time: now };
    }
    commit({ ...state, ...patch, doc });
  },

  /** Between these two calls every edit is one undo step (a drag). */
  beginGesture(): void {
    gesture = 'armed';
  },
  endGesture(): void {
    gesture = 'idle';
    lastEdit = { key: '', time: 0 };
  },

  undo(): void {
    const prev = past.at(-1);
    if (!prev) return;
    past = past.slice(0, -1);
    future = [...future, snapshot()];
    commit({ ...state, ...prev });
  },
  redo(): void {
    const next = future.at(-1);
    if (!next) return;
    future = future.slice(0, -1);
    past = [...past, snapshot()];
    commit({ ...state, ...next });
  },

  /** Replace the document with another file's. Clears the history. */
  open(doc: Node[], fileName: string, filePath: string | null): void {
    past = [];
    future = [];
    commit({
      ...state,
      doc,
      saved: doc,
      fileName,
      filePath,
      selection: [],
      artboardId: '',
      animationId: null,
      machineId: null,
      layerId: null,
      frame: 0,
      playing: false,
      previewing: false,
      pen: [],
      error: null,
      fitRequest: state.fitRequest + 1,
    });
  },
};

/** Subscribe a component to a slice of the state. The selector must return a stable value. */
export function useStore<T>(selector: (s: State) => T): T {
  return useSyncExternalStore(store.subscribe, () => selector(state));
}

/**
 * The whole state, re-rendering only when something that decides what a property shows
 * or where a write goes has changed: the document, the mode, a preview, the animation, the playhead.
 */
export function useKeyingState(): State {
  useStore((s) => s.doc);
  useStore((s) => s.mode);
  useStore((s) => s.previewing);
  useStore((s) => s.artboardId);
  useStore((s) => s.animationId);
  useStore((s) => Math.round(s.frame));
  return state;
}
