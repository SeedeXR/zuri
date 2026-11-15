// Source-format colors (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`) as RGBA channels and back.
// Framework-free.

export type Rgba = [r: number, g: number, b: number, a: number];

/** Channels 0..255 of a source color, or null when the text is not a color. */
export function parseHex(text: unknown): Rgba | null {
  if (typeof text !== 'string' || !/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(text)) return null;
  let hex = text.slice(1);
  if (hex.length <= 4) hex = [...hex].map((c) => c + c).join('');
  const ch = (i: number) => parseInt(hex.slice(i, i + 2), 16);
  return [ch(0), ch(2), ch(4), hex.length === 8 ? ch(6) : 255];
}

/** `#rrggbb`, or `#rrggbbaa` when not opaque: the form the compiler and decompiler use. */
export function formatHex([r, g, b, a]: Rgba): string {
  const h = (v: number) =>
    Math.round(Math.min(255, Math.max(0, v)))
      .toString(16)
      .padStart(2, '0');
  return `#${h(r)}${h(g)}${h(b)}${a >= 254.5 ? '' : h(a)}`;
}
