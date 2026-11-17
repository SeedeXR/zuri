// Top toolbar: file menu, tools, Design / Animate switch, play, zoom, undo / redo.

import { ChevronDown, Circle, Download, Frame, Hand, Menu as MenuIcon, MousePointer2, Pause, PenTool, Pentagon, Play, Redo2, Square, Star, Type, Undo2, Volume2, VolumeX, type LucideIcon } from 'lucide-react';
import { setMode, setTool, togglePlay, zoomBy, zoomToFit } from '../actions.ts';
import { exampleNames, exportDocument, newDocument, openDocument, openExample, saveDocument } from '../io/files.ts';
import { type Tool, store, useStore } from '../store.ts';
import { Button, IconButton, Menu, type MenuItem, cx } from './ui.tsx';
import markUrl from '../../../../brand/zuri-logo.svg';

const MOD = navigator.platform.startsWith('Mac') ? '⌘' : 'Ctrl+';

const TOOLS: [tool: Tool, icon: LucideIcon, label: string, key: string][] = [
  ['select', MousePointer2, 'Select', 'V'],
  ['Rectangle', Square, 'Rectangle', 'R'],
  ['Ellipse', Circle, 'Ellipse', 'O'],
  ['Polygon', Pentagon, 'Polygon', ''],
  ['Star', Star, 'Star', ''],
  ['pen', PenTool, 'Pen', 'P'],
  ['text', Type, 'Text', 'T'],
  ['artboard', Frame, 'Artboard', 'A'],
  ['hand', Hand, 'Hand', 'H'],
];

export function Toolbar() {
  const tool = useStore((s) => s.tool);
  const mode = useStore((s) => s.mode);
  const zoom = useStore((s) => s.view.zoom);
  const running = useStore((s) => s.playing || s.previewing);
  // Sounds are the file's own objects, beside its artboards: no button without one.
  const hasSound = useStore((s) => s.doc.some((node) => node.type === 'AudioAsset'));
  const muted = useStore((s) => s.muted);
  const canUndo = useStore((s) => s.canUndo);
  const canRedo = useStore((s) => s.canRedo);
  const fileName = useStore((s) => s.fileName);
  const dirty = useStore((s) => s.doc !== s.saved);

  const file: MenuItem[] = [
    { label: 'New', run: newDocument, shortcut: `${MOD}N` },
    { label: 'Open…', run: openDocument, shortcut: `${MOD}O` },
    ...exampleNames.map((name): MenuItem => ({ label: `Open example: ${name}`, run: () => openExample(name) })),
    'separator',
    { label: 'Save', run: () => saveDocument(), shortcut: `${MOD}S` },
    { label: 'Save As…', run: () => saveDocument(true), shortcut: `⇧${MOD}S` },
    { label: 'Export .zur…', run: exportDocument, shortcut: `${MOD}E` },
  ];
  const zoomItems: MenuItem[] = [
    { label: 'Zoom in', run: () => zoomBy(1.25), shortcut: `${MOD}+` },
    { label: 'Zoom out', run: () => zoomBy(0.8), shortcut: `${MOD}−` },
    { label: 'Zoom to fit', run: zoomToFit, shortcut: '⇧1' },
    { label: 'Zoom to 100%', run: () => zoomBy(1 / store.get().view.zoom), shortcut: `${MOD}0` },
  ];

  return (
    <header className="col-span-3 grid grid-cols-[1fr_auto_1fr] items-center gap-3.5 border-b border-line bg-panel px-2" role="toolbar" aria-label="Main toolbar">
      <div className="flex min-w-0 items-center gap-2">
        {/* The mark is monochrome artwork: a mask lets it take the text color. */}
        <span role="img" aria-label="Zuri" title="Zuri" className="mx-1 size-5.5 shrink-0 bg-fg" style={{ mask: `url(${markUrl}) center / contain no-repeat` }} />
        <Menu label="File" trigger={<MenuIcon size={16} aria-hidden />} items={file} />
        <span className="flex max-w-56 items-center gap-1.5 truncate px-1 text-fg-dim" title={dirty ? 'Unsaved changes' : 'Saved'}>
          {fileName}
          {dirty && <span className="size-1.5 shrink-0 rounded-full bg-warning" aria-label="unsaved changes" />}
        </span>
        <div className="ml-1 flex items-center gap-0.5 border-l border-line pl-3" role="group" aria-label="Tools">
          {TOOLS.map(([id, icon, label, key]) => (
            <IconButton key={id} icon={icon} label={label} shortcut={key} active={tool === id} onClick={() => setTool(id)} />
          ))}
        </div>
      </div>
      <div className="flex rounded-m bg-app p-0.5" role="group" aria-label="Mode">
        {(['design', 'animate'] as const).map((m) => (
          <button
            key={m}
            type="button"
            className={cx('h-6 rounded-[6px] px-3.5 font-medium', mode === m ? 'bg-hover text-fg' : 'text-fg-dim hover:text-fg')}
            aria-pressed={mode === m}
            onClick={() => setMode(m)}
          >
            {m === 'design' ? 'Design' : 'Animate'}
          </button>
        ))}
      </div>
      <div className="flex min-w-0 items-center justify-end gap-0.5">
        {hasSound && <IconButton icon={muted ? VolumeX : Volume2} label="Mute the preview" active={muted} onClick={() => store.set({ muted: !muted })} />}
        <IconButton icon={running ? Pause : Play} label={running ? 'Stop' : mode === 'animate' ? 'Play' : 'Preview the state machine'} shortcut="⇧Space" active={running} onClick={togglePlay} />
        <Menu
          label="Zoom"
          align="right"
          items={zoomItems}
          trigger={
            <>
              {Math.round(zoom * 100)}% <ChevronDown size={12} aria-hidden />
            </>
          }
        />
        <IconButton icon={Undo2} label="Undo" shortcut={`${MOD}Z`} disabled={!canUndo} onClick={store.undo} />
        <IconButton icon={Redo2} label="Redo" shortcut={`⇧${MOD}Z`} disabled={!canRedo} onClick={store.redo} />
        <span className="ml-2">
          <Button tone="brand" onClick={exportDocument} title={`Export a .zur file (${MOD}E)`}>
            <Download size={13} aria-hidden /> Export
          </Button>
        </span>
      </div>
    </header>
  );
}
