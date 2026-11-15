// Value controls used by the inspector and the other panels: number field with
// drag-to-change, color picker with alpha and hex, text, select and checkbox.

import { ChevronDown } from 'lucide-react';
import { type PointerEvent as ReactPointerEvent, useState } from 'react';
import { type Rgba, formatHex, parseHex } from '../doc/color.ts';
import { store } from '../store.ts';
import { cx, drag } from './ui.tsx';

const format = (v: number): string => String(Math.round(v * 1000) / 1000);

/**
 * Pointer handlers that turn a horizontal drag on a label into value changes
 * (Shift = 10x). The whole drag is one undo step.
 */
export function scrub(value: number, onChange: (v: number) => void, step: number) {
  return {
    onPointerDown(e: ReactPointerEvent<HTMLElement>) {
      if (e.button !== 0) return;
      const x0 = e.clientX;
      let dragging = false;
      const move = (ev: PointerEvent) => {
        const dx = ev.clientX - x0;
        if (!dragging && Math.abs(dx) < 3) return;
        if (!dragging) store.beginGesture();
        dragging = true;
        onChange(Math.round((value + dx * step * (ev.shiftKey ? 10 : 1)) * 1000) / 1000);
      };
      drag(e, move, () => dragging && store.endGesture());
    },
  };
}

interface NumberFieldProps {
  value: number;
  onChange: (v: number) => void;
  label: string;
  step?: number;
  min?: number;
  max?: number;
  integer?: boolean;
  suffix?: string;
  /** Extra classes for the field shell (width, corner rounding). */
  className?: string;
}

export function NumberField({ value, onChange, label, step = 1, min = -Infinity, max = Infinity, integer, suffix, className }: NumberFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const clamp = (v: number) => Math.min(max, Math.max(min, integer ? Math.round(v) : v));
  const commit = () => {
    const parsed = draft === null ? NaN : Number(draft.replace(',', '.'));
    setDraft(null);
    if (Number.isFinite(parsed) && clamp(parsed) !== value) onChange(clamp(parsed));
  };
  return (
    <span className={cx('field', className)}>
      <input
        type="text"
        inputMode="decimal"
        className="field-input tabular-nums"
        aria-label={label}
        value={draft ?? format(value)}
        onChange={(e) => setDraft(e.target.value)}
        onFocus={(e) => e.target.select()}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          else if (e.key === 'Escape') {
            // Blur after the draft is gone: blurring now would commit it (as TextField does below).
            const el = e.currentTarget;
            setDraft(null);
            requestAnimationFrame(() => el.blur());
          } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            setDraft(null);
            onChange(clamp(Math.round((value + (e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1)) * 1000) / 1000));
          }
        }}
      />
      {suffix && <span className="shrink-0 pr-1.5 text-[11px] text-fg-faint">{suffix}</span>}
    </span>
  );
}

interface TextFieldProps {
  value: string;
  onChange: (v: string) => void;
  label: string;
  placeholder?: string;
  multiline?: boolean;
  mono?: boolean;
  className?: string;
}

/** Commits on blur or Enter (Cmd/Ctrl+Enter when multiline); Escape reverts. */
export function TextField({ value, onChange, label, placeholder, multiline, mono, className }: TextFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null && draft !== value) onChange(draft);
    setDraft(null);
  };
  const shared = {
    'aria-label': label,
    placeholder,
    spellCheck: false,
    value: draft ?? value,
    onBlur: commit,
    onChange: (e: { target: { value: string } }) => setDraft(e.target.value),
    onKeyDown: (e: React.KeyboardEvent<HTMLElement>) => {
      if (e.key === 'Escape') {
        setDraft(null);
        requestAnimationFrame(() => (e.target as HTMLElement).blur());
      } else if (e.key === 'Enter' && (!multiline || e.metaKey || e.ctrlKey)) {
        (e.target as HTMLElement).blur();
      }
    },
  };
  const type = mono ? 'font-mono text-[11px]' : '';
  if (multiline) {
    return (
      <textarea
        rows={3}
        className={cx(
          'min-h-13 w-full min-w-0 resize-y rounded-s border border-transparent bg-input px-1.5 py-1 leading-snug outline-none select-text placeholder:text-fg-faint hover:border-line-strong focus:border-accent',
          type,
          className,
        )}
        {...shared}
      />
    );
  }
  return (
    <span className={cx('field', className)}>
      <input type="text" className={cx('field-input placeholder:text-fg-faint', type)} {...shared} />
    </span>
  );
}

export type Option = string | { value: string; label: string };

interface SelectFieldProps {
  value: string;
  options: readonly Option[];
  onChange: (v: string) => void;
  label: string;
  /** Text of an empty first choice, when "nothing" is a valid value. */
  placeholder?: string;
  className?: string;
}

export function SelectField({ value, options, onChange, label, placeholder, className }: SelectFieldProps) {
  const choices = options.map((o) => (typeof o === 'string' ? { value: o, label: o } : o));
  // A value that is not among the options (a dangling reference) stays visible so it can be fixed.
  const missing = value !== '' && !choices.some((o) => o.value === value);
  return (
    <span className={cx('field relative', missing && 'border-danger text-danger', className)}>
      <select className="field-input appearance-none truncate pr-5 pl-1.5" aria-label={label} value={value} onChange={(e) => onChange(e.target.value)}>
        {placeholder !== undefined && <option value="">{placeholder}</option>}
        {missing && <option value={value}>{value} (missing)</option>}
        {choices.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronDown size={12} aria-hidden className="pointer-events-none absolute right-1 text-fg-dim" />
    </span>
  );
}

export function Checkbox({ value, onChange, label }: { value: boolean; onChange: (v: boolean) => void; label: string }) {
  return <input type="checkbox" className="m-0 mt-1 size-3.5 accent-accent" aria-label={label} checked={value} onChange={(e) => onChange(e.target.checked)} />;
}

/** Swatch (opens the system color picker), hex field and alpha percentage. */
export function ColorField({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const rgba: Rgba = parseHex(value) ?? [0, 0, 0, 255];
  const opaque = formatHex([rgba[0], rgba[1], rgba[2], 255]);
  return (
    <span className="flex min-w-0 items-center gap-px">
      <span className="checker relative size-5.5 shrink-0 overflow-hidden rounded-l-s [background-size:8px_8px] focus-within:outline-2 focus-within:outline-accent">
        <span className="absolute inset-0 ring-1 ring-line ring-inset" style={{ background: `rgba(${rgba[0]}, ${rgba[1]}, ${rgba[2]}, ${rgba[3] / 255})` }} />
        <input
          type="color"
          className="absolute inset-0 size-full opacity-0"
          aria-label={`${label} color`}
          value={opaque}
          onChange={(e) => {
            const picked = parseHex(e.target.value);
            if (picked) onChange(formatHex([picked[0], picked[1], picked[2], rgba[3]]));
          }}
        />
      </span>
      <TextField
        label={`${label} hex`}
        mono
        className="flex-1 rounded-none"
        value={opaque.slice(1).toUpperCase()}
        onChange={(text) => {
          const typed = parseHex(text.startsWith('#') ? text : `#${text}`);
          // Typing 6 digits keeps the current alpha; 8 digits sets it.
          if (typed) onChange(formatHex(text.replace('#', '').length > 6 ? typed : [typed[0], typed[1], typed[2], rgba[3]]));
        }}
      />
      <NumberField
        label={`${label} opacity`}
        className="w-13 shrink-0 rounded-l-none"
        value={Math.round((rgba[3] / 255) * 100)}
        min={0}
        max={100}
        integer
        suffix="%"
        onChange={(a) => onChange(formatHex([rgba[0], rgba[1], rgba[2], (a / 100) * 255]))}
      />
    </span>
  );
}
