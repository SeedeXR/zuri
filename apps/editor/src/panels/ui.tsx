// Small shared controls: class joining, icon buttons, text buttons and popup menus.

import type { LucideIcon } from 'lucide-react';
import { type PointerEvent as ReactPointerEvent, type ReactNode, useEffect, useRef, useState } from 'react';

/** Join class names, skipping the falsy ones. */
export const cx = (...parts: (string | false | null | undefined)[]): string => parts.filter(Boolean).join(' ');

/** Run `move` for every pointer move until the pointer is released or cancelled; `done` gets that last event. */
export function drag(e: ReactPointerEvent<Element>, move: (ev: PointerEvent) => void, done?: (ev: PointerEvent) => void): void {
  const el = e.currentTarget as HTMLElement; // SVG elements take the same pointer events
  el.setPointerCapture(e.pointerId);
  const up = (ev: PointerEvent) => {
    el.removeEventListener('pointermove', move);
    el.removeEventListener('pointerup', up);
    el.removeEventListener('pointercancel', up);
    done?.(ev);
  };
  el.addEventListener('pointermove', move);
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
}

interface IconButtonProps {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
  /** Shown in the tooltip after the label. */
  shortcut?: string;
  /** Icon size in pixels. Icons of 14 or less get the compact 22px button used inside panels. */
  size?: number;
  className?: string;
}

export function IconButton({ icon: Icon, label, onClick, active, disabled, shortcut, size = 16, className }: IconButtonProps) {
  return (
    <button
      type="button"
      className={cx(
        'inline-flex shrink-0 items-center justify-center rounded-s disabled:pointer-events-none disabled:opacity-40',
        size <= 14 ? 'size-5.5' : 'size-7',
        active ? 'bg-accent text-accent-fg' : 'text-fg-dim hover:bg-hover hover:text-fg',
        className,
      )}
      aria-label={label}
      aria-pressed={active}
      title={shortcut ? `${label} (${shortcut})` : label}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon size={size} aria-hidden />
    </button>
  );
}

interface ButtonProps {
  children: ReactNode;
  onClick: () => void;
  /** `accent` for the main action of a panel, `brand` for the identity call to action. */
  tone?: 'neutral' | 'accent' | 'brand';
  disabled?: boolean;
  title?: string;
}

const TONES = {
  neutral: 'bg-input text-fg hover:bg-hover',
  accent: 'bg-accent font-semibold text-accent-fg hover:brightness-110',
  brand: 'bg-brand font-semibold text-accent-fg hover:brightness-110',
};

/** A labelled button, usually with a leading icon. */
export function Button({ children, onClick, tone = 'neutral', disabled, title }: ButtonProps) {
  return (
    <button
      type="button"
      className={cx('inline-flex h-5.5 shrink-0 items-center gap-1.5 rounded-s px-2 whitespace-nowrap disabled:pointer-events-none disabled:opacity-40', TONES[tone])}
      disabled={disabled}
      title={title}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export type MenuItem = { label: string; run: () => void; shortcut?: string; title?: string } | 'separator';

interface MenuProps {
  trigger: ReactNode;
  label: string;
  items: MenuItem[];
  align?: 'left' | 'right';
  /** Classes for the trigger button (its size and resting colors). */
  className?: string;
}

/** A button that opens a list of commands. Closes on outside click, Escape or choosing. */
export function Menu({ trigger, label, items, align = 'left', className = 'h-7 px-2 text-fg-dim hover:bg-hover hover:text-fg' }: MenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className={cx('inline-flex items-center gap-1 rounded-s tabular-nums aria-expanded:bg-hover aria-expanded:text-fg', className)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={label}
        title={label}
        onClick={() => setOpen(!open)}
      >
        {trigger}
      </button>
      {open && (
        <div
          className={cx('absolute top-full z-30 mt-1 flex min-w-52 flex-col rounded-m border border-line-strong bg-raised p-1 shadow-xl shadow-black/60', align === 'right' ? 'right-0' : 'left-0')}
          role="menu"
        >
          {items.map((item, i) =>
            item === 'separator' ? (
              <hr key={i} className="my-1 border-line" />
            ) : (
              <button
                key={i}
                type="button"
                role="menuitem"
                className="group flex h-6.5 items-center justify-between gap-6 rounded-s px-2 text-left whitespace-nowrap hover:bg-accent hover:text-accent-fg focus-visible:bg-accent focus-visible:text-accent-fg"
                title={item.title}
                onClick={() => {
                  setOpen(false);
                  item.run();
                }}
              >
                <span>{item.label}</span>
                {item.shortcut && <kbd className="font-ui text-fg-faint group-hover:text-accent-fg">{item.shortcut}</kbd>}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}
