// Global keyboard shortcuts (Figma / Rive conventions). Ignored while typing in a field.

import { deleteSelection, duplicateSelection, finishPen, groupSelection, nudge, select, setTool, togglePlay, zoomBy, zoomToFit } from './actions.ts';
import { exportDocument, newDocument, openDocument, saveDocument } from './io/files.ts';
import { type Tool, store } from './store.ts';

const TOOL_KEYS: Record<string, Tool> = { v: 'select', r: 'Rectangle', o: 'Ellipse', p: 'pen', t: 'text', a: 'artboard', h: 'hand' };
const ARROWS: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
const FILE_KEYS: Record<string, (e: KeyboardEvent) => unknown> = { s: (e) => saveDocument(e.shiftKey), o: openDocument, e: exportDocument };

const isTyping = (target: EventTarget | null): boolean => target instanceof HTMLElement && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));

function onKeyDown(e: KeyboardEvent): void {
  const mod = e.metaKey || e.ctrlKey;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  // File shortcuts work everywhere, fields included.
  if (mod && key in FILE_KEYS) {
    e.preventDefault();
    return void FILE_KEYS[key](e);
  }
  if (isTyping(e.target)) return;
  const s = store.get();
  const run = (f: () => void): void => {
    e.preventDefault();
    f();
  };
  if (mod) {
    if (key === 'z') run(e.shiftKey ? store.redo : store.undo);
    else if (key === 'y') run(store.redo);
    else if (key === 'd') run(duplicateSelection);
    else if (key === 'g') run(groupSelection);
    else if (key === 'n') run(newDocument);
    else if (key === '0') run(() => zoomBy(1 / s.view.zoom));
    else if (key === '=' || key === '+') run(() => zoomBy(1.25));
    else if (key === '-') run(() => zoomBy(0.8));
    return;
  }
  if (key === ' ') {
    // Shift+Space plays; plain Space held down pans. Either way the page must not scroll.
    e.preventDefault();
    if (e.shiftKey) {
      if (!e.repeat) togglePlay();
    } else if (!s.spacePan) store.set({ spacePan: true });
  } else if (e.shiftKey && (key === '1' || key === '!')) run(zoomToFit);
  else if (key === 'Enter' && s.pen.length) run(() => finishPen(false));
  else if (key === 'Escape') {
    if (s.pen.length) finishPen(false);
    else if (s.previewing) store.set({ previewing: false });
    else if (s.tool !== 'select') setTool('select');
    else select([]);
  } else if (s.previewing) return;
  else if (key === 'Delete' || key === 'Backspace') run(deleteSelection);
  else if (key in ARROWS) run(() => nudge(ARROWS[key][0] * (e.shiftKey ? 10 : 1), ARROWS[key][1] * (e.shiftKey ? 10 : 1)));
  else if (key in TOOL_KEYS && !e.altKey) run(() => setTool(TOOL_KEYS[key]));
}

function onKeyUp(e: KeyboardEvent): void {
  if (e.key === ' ' && store.get().spacePan) store.set({ spacePan: false });
}

export function installShortcuts(): void {
  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
}
