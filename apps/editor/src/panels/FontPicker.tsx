// The font picker: open-licence fonts from the catalogue, the fonts installed on this
// machine (desktop app), or a file. Choosing a font shows it; "Add to document" embeds it.

import { FolderOpen, Search, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { type FontFamily, LICENSES, openFamilies, searchFamilies } from '../fonts/catalog.ts';
import { embedFont, importFont, installedFamilies, loadFont } from '../io/assets.ts';
import { isTauri } from '../io/tauri.ts';
import { Button, IconButton, cx } from './ui.tsx';

const CATEGORIES = [
  ['', 'All'],
  ['sans-serif', 'Sans'],
  ['serif', 'Serif'],
  ['monospace', 'Mono'],
  ['display', 'Display'],
  ['handwriting', 'Script'],
];
const SAMPLE = 'The quick brown fox jumps over the lazy dog 0123456789';
const fileSize = (bytes: number): string => (bytes < 1e6 ? `${Math.round(bytes / 1024)} KB` : `${(bytes / 1048576).toFixed(1)} MB`);
const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** The name the chosen font has on the page while the picker shows it. */
const PREVIEW = 'zuri-font-preview';
/** What is known of the chosen style: being fetched (neither), ready, or failed. */
type Loaded = { bytes?: Uint8Array; error?: string };

export function FontPicker({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const [tab, setTab] = useState<'open' | 'installed'>('open');
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('');
  const [installed, setInstalled] = useState<FontFamily[] | string | null>(null);
  const [family, setFamily] = useState<FontFamily | null>(null);
  const [styleAt, setStyleAt] = useState(0);
  const [loaded, setLoaded] = useState<Loaded>({});

  // Open as a modal (once: in development React runs effects twice), then put the
  // caret in the search box. Closing gives the focus back to what had it.
  useEffect(() => {
    if (!dialog.current?.open) dialog.current?.showModal();
    search.current?.focus();
  }, []);
  // The installed fonts are read when their tab is first opened.
  useEffect(() => {
    if (tab === 'installed' && installed === null) installedFamilies().then(setInstalled, (e) => setInstalled(message(e)));
  }, [tab, installed]);

  const style = family?.styles[styleAt];
  // Fetch the chosen style and show it: the font is registered with the page, and taken
  // away again (and its download dropped) when another is chosen or the picker closes.
  useEffect(() => {
    setLoaded({});
    if (!style) return;
    const stop = new AbortController();
    let face: FontFace | undefined;
    loadFont(style, stop.signal)
      .then(async (bytes) => {
        face = new FontFace(PREVIEW, bytes.buffer as ArrayBuffer);
        await face.load();
        if (stop.signal.aborted) return;
        document.fonts.add(face);
        setLoaded({ bytes });
      })
      .catch((e) => stop.signal.aborted || setLoaded({ error: message(e) }));
    return () => {
      stop.abort();
      if (face) document.fonts.delete(face);
    };
  }, [style]);

  const families = tab === 'open' ? openFamilies : Array.isArray(installed) ? installed : [];
  const shown = searchFamilies(families, query, tab === 'open' ? category : '');
  const choose = (next: FontFamily) => {
    setFamily(next);
    setStyleAt(0);
  };
  const add = () => {
    if (!family || !style || !loaded.bytes) return;
    embedFont(style.style === 'Regular' ? family.family : `${family.family} ${style.style}`, loaded.bytes);
    dialog.current?.close();
  };
  // The picker makes way for the file dialog (and for the message, if the file is not a font).
  const fromFile = () => {
    dialog.current?.close();
    void importFont();
  };

  return (
    <dialog
      ref={dialog}
      aria-labelledby="font-picker-title"
      className="m-auto h-[540px] max-h-[92vh] w-[780px] max-w-[94vw] flex-col rounded-m border border-line-strong bg-panel p-0 text-fg shadow-2xl backdrop:bg-black/50 open:flex"
      onClose={onClose}
    >
      <header className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
        <h2 id="font-picker-title" className="text-[13px] font-semibold">
          Add a font
        </h2>
        <div role="tablist" aria-label="Where the font comes from" className="ml-3 flex gap-0.5">
          {(
            [
              ['open', `Open fonts (${openFamilies.length})`],
              ['installed', Array.isArray(installed) ? `Installed (${installed.length})` : 'Installed'],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={cx('h-6 rounded-s px-2.5', tab === id ? 'bg-accent/25 text-fg' : 'text-fg-dim hover:bg-hover hover:text-fg')}
              onClick={() => {
                setTab(id);
                setFamily(null);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <span className="flex-1" />
        <Button onClick={fromFile} title="Embed a TTF or OTF file from this machine">
          <FolderOpen size={13} aria-hidden /> From file…
        </Button>
        <IconButton icon={X} size={14} label="Close" onClick={() => dialog.current?.close()} />
      </header>

      <div className="flex min-h-0 flex-1">
        <div className="flex w-[300px] shrink-0 flex-col border-r border-line">
          <label className="m-2 flex h-6.5 items-center gap-1.5 rounded-s bg-input px-2 text-fg-dim focus-within:outline-2 focus-within:outline-accent">
            <Search size={13} aria-hidden />
            <input
              ref={search}
              type="search"
              value={query}
              placeholder="Search fonts"
              aria-label="Search fonts by name"
              className="min-w-0 flex-1 bg-transparent text-fg outline-none select-text placeholder:text-fg-faint"
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          {tab === 'open' && (
            <div className="mx-2 mb-1.5 flex flex-wrap gap-1" role="group" aria-label="Kind of font">
              {CATEGORIES.map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  aria-pressed={category === id}
                  className={cx('h-5 rounded-s px-1.5 text-[11px]', category === id ? 'bg-accent/25 text-fg' : 'bg-input text-fg-dim hover:bg-hover hover:text-fg')}
                  onClick={() => setCategory(id)}
                >
                  {label}
                </button>
              ))}
            </div>
          )}
          <ul className="min-h-0 flex-1 overflow-y-auto pb-1" aria-label="Fonts">
            {shown.map((f) => (
              <li key={f.family}>
                <button
                  type="button"
                  aria-pressed={f === family}
                  className={cx('flex h-6.5 w-full items-center gap-2 px-3 text-left -outline-offset-2', f === family ? 'bg-accent/20' : 'hover:bg-raised')}
                  onClick={() => choose(f)}
                >
                  <span className="flex-1 truncate">{f.family}</span>
                  <span className="text-[11px] text-fg-faint">{f.category ?? (f.styles.length > 1 ? `${f.styles.length} styles` : '')}</span>
                </button>
              </li>
            ))}
            {tab === 'installed' && !isTauri && <li className="empty px-3 py-2">Installed fonts are listed in the desktop app. Here, use "From file…" to embed one.</li>}
            {tab === 'installed' && isTauri && installed === null && <li className="empty px-3 py-2">Reading the fonts on this machine…</li>}
            {tab === 'installed' && typeof installed === 'string' && <li className="px-3 py-2 text-danger">Could not list the installed fonts: {installed}</li>}
            {shown.length === 0 && families.length > 0 && <li className="empty px-3 py-2">No font is called that.</li>}
          </ul>
        </div>

        <div className="flex min-w-0 flex-1 flex-col p-4">
          {!family || !style ? (
            <p className="empty m-auto max-w-[280px] text-center">
              {tab === 'open'
                ? 'Fonts under open licences that allow commercial use. Choose one to see it; it is downloaded when you do.'
                : 'The fonts installed on this machine. Choose one to see it.'}
            </p>
          ) : (
            <>
              <h3 className="text-[16px] font-semibold">{family.family}</h3>
              <p className="mt-0.5 text-fg-dim">
                {[family.category, family.axes.length ? `variable (${family.axes.join(', ')})` : '', style.bytes ? fileSize(style.bytes) : ''].filter(Boolean).join(' · ')}
              </p>
              {family.styles.length > 1 && (
                <div className="mt-2.5 flex flex-wrap gap-1" role="group" aria-label="Style">
                  {family.styles.map((s, i) => (
                    <button
                      key={`${s.style} ${s.path} ${s.index}`}
                      type="button"
                      aria-pressed={i === styleAt}
                      className={cx('h-5.5 rounded-s px-2', i === styleAt ? 'bg-accent/25 text-fg' : 'bg-input text-fg-dim hover:bg-hover hover:text-fg')}
                      onClick={() => setStyleAt(i)}
                    >
                      {s.style}
                    </button>
                  ))}
                </div>
              )}

              <div className="mt-3 flex min-h-0 flex-1 flex-col justify-center overflow-hidden rounded-s bg-stage p-4" aria-live="polite">
                {loaded.bytes ? (
                  <div style={{ fontFamily: PREVIEW }}>
                    <p className="text-[56px] leading-none">Aa</p>
                    <p className="mt-3 text-[20px] leading-snug break-words">{SAMPLE}</p>
                  </div>
                ) : loaded.error ? (
                  <p className="text-danger">
                    {style.index === undefined ? 'Could not download this font' : 'Could not read this font'}: {loaded.error}
                  </p>
                ) : (
                  <p className="text-fg-dim">{style.index === undefined ? `Downloading${style.bytes ? ` ${fileSize(style.bytes)}` : ''}…` : 'Reading…'}</p>
                )}
              </div>
              <div className="mt-3 text-[11px] leading-relaxed text-fg-dim select-text">
                {family.license ? (
                  <>
                    <p>
                      <span className="text-fg">{LICENSES[family.license].name}</span>: free to use, in commercial work too. When you pass the font on (a document or a .zur carries it):{' '}
                      {LICENSES[family.license].terms}.
                    </p>
                    <p className="truncate" title={family.licenseUrl}>
                      Licence text: {family.licenseUrl}
                    </p>
                  </>
                ) : (
                  <>
                    <p className="truncate" title={style.path}>
                      {style.path}
                    </p>
                    <p className="text-warning">Installed fonts come under their own licences, and many system fonts may not be embedded in what you ship. Check before you publish.</p>
                  </>
                )}
              </div>
              <div className="mt-3 flex justify-end">
                <Button tone="accent" disabled={!loaded.bytes} onClick={add} title="Embed this font in the document, for Text to use">
                  Add to document
                </Button>
              </div>
            </>
          )}
        </div>
      </div>
    </dialog>
  );
}
