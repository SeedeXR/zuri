// Bottom panel, Source tab: the whole document as editable JSON source. Every edit is
// checked by the real compiler; the error is shown with its line marked. Applying valid
// JSON replaces the document (one undo step): the path for pasting a document from an
// AI agent and seeing it live.

import { Check, RotateCcw } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { errorLine } from '../doc/jsonpath.ts';
import { fromSource, toSource } from '../doc/model.ts';
import { runtime } from '../runtime.ts';
import { store, useStore } from '../store.ts';
import { Button, cx } from './ui.tsx';

/** Why `text` is not a valid document, or null when it compiles. */
function validate(text: string): string | null {
  try {
    fromSource(text);
    runtime.compile(text);
    return null;
  } catch (e) {
    return (e as Error).message.replace(/^Zuri: could not compile: /, '');
  }
}

export function Source() {
  const doc = useStore((s) => s.doc);
  const [text, setText] = useState(() => toSource(doc));
  const [edited, setEdited] = useState(false);
  const area = useRef<HTMLTextAreaElement>(null);
  const gutter = useRef<HTMLDivElement>(null);

  // Follow the document unless there are unapplied edits here.
  useEffect(() => {
    if (!edited) setText(toSource(doc));
  }, [doc, edited]);

  // ponytail: validates the whole text on every keystroke (about a millisecond for
  // ordinary documents); debounce it if documents with large embedded assets feel slow.
  const error = useMemo(() => (edited ? validate(text) : null), [text, edited]);
  const badLine = error ? errorLine(text, error) : undefined;
  const lineCount = useMemo(() => text.split('\n').length, [text]);

  const apply = () => {
    if (!edited || error) return;
    const s = store.get();
    const roots = fromSource(text);
    // Ids are new after a parse: stay on the artboard at the same position.
    const artboardId = roots[s.doc.findIndex((n) => n.id === s.artboardId)]?.id ?? '';
    store.edit(() => roots, { selection: [], artboardId });
    setEdited(false);
  };
  const revert = () => {
    setText(toSource(doc));
    setEdited(false);
  };
  const goToError = () => {
    const el = area.current;
    if (!el || !badLine) return;
    const lines = text.split('\n');
    const start = lines.slice(0, badLine - 1).join('\n').length + (badLine > 1 ? 1 : 0);
    el.focus();
    el.setSelectionRange(start, start + lines[badLine - 1].length);
    el.scrollTop = Math.max(0, (badLine - 4) * parseFloat(getComputedStyle(el).lineHeight));
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8.5 shrink-0 items-center gap-1.5 border-b border-line px-2">
        <Button tone="accent" disabled={!edited || error !== null} onClick={apply} title="Replace the document with this JSON (Cmd/Ctrl+Enter)">
          <Check size={13} aria-hidden /> Apply
        </Button>
        <Button disabled={!edited} onClick={revert} title="Discard the edits made here">
          <RotateCcw size={13} aria-hidden /> Revert
        </Button>
        {error ? (
          <button
            type="button"
            className="ml-1.5 min-w-0 flex-1 truncate rounded-s bg-danger/15 px-2 py-0.5 text-left text-danger"
            role="alert"
            onClick={goToError}
            title={badLine ? 'Go to the line' : undefined}
          >
            {badLine ? `Line ${badLine}: ` : ''}
            {error}
          </button>
        ) : (
          <span className={cx('ml-1.5 text-[11px]', edited ? 'text-success' : 'text-fg-faint')}>
            {edited ? 'Valid. Apply to replace the document.' : 'Edit or paste JSON source. Run `zuri schema` for every type and property.'}
          </span>
        )}
      </div>
      <div className="flex min-h-0 flex-1 bg-stage font-mono text-[11px] leading-4">
        <div className="min-w-10 shrink-0 overflow-hidden py-2 text-right text-fg-faint" ref={gutter} aria-hidden>
          {Array.from({ length: lineCount }, (_, i) => (
            <div key={i} className={cx('h-4 pr-2 pl-1.5', i + 1 === badLine && 'bg-danger text-accent-fg')}>
              {i + 1}
            </div>
          ))}
        </div>
        <textarea
          ref={area}
          className="min-w-0 flex-1 resize-none bg-transparent py-2 pr-2 pl-1 whitespace-pre outline-none select-text [tab-size:2] focus-visible:bg-app"
          aria-label="Document JSON source"
          aria-invalid={error !== null}
          spellCheck={false}
          wrap="off"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setEdited(true);
          }}
          onScroll={(e) => {
            if (gutter.current) gutter.current.scrollTop = e.currentTarget.scrollTop;
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) apply();
          }}
        />
      </div>
    </div>
  );
}
