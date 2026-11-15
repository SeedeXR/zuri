// The fonts the picker offers, as plain data: the catalogue of open-licence fonts
// (catalog.json, written by scripts/font-catalog.mjs) and the fonts installed on this
// machine, in one shape. No UI and no IO here, so the tests run it as it is.

import catalog from './catalog.json' with { type: 'json' };

export interface FontStyle {
  /** "Regular", "Bold Italic", ... */
  style: string;
  /** Size of the file, when known before it is read. */
  bytes?: number;
  /** Where the font is: a path in the catalogue's repository, or a file on this machine. */
  path: string;
  /** Catalogue: the git hash of the file, which a download is checked against. */
  sha?: string;
  /** Installed: its number in its file (a collection holds several). */
  index?: number;
}

export interface FontFamily {
  family: string;
  /** Catalogue: sans-serif, serif, monospace, display or handwriting. */
  category?: string;
  /** Catalogue: the licence's SPDX name, and where its text is. */
  license?: string;
  licenseUrl?: string;
  /** The axes of a variable font ("wght", "wdth", ...). */
  axes: string[];
  styles: FontStyle[];
}

const RAW = catalog.source.replace('github.com', 'raw.githubusercontent.com');
/** Where a catalogue file is downloaded from: the repository at the catalogue's commit. */
export const fontUrl = (path: string): string => `${RAW}/${catalog.commit}/${path.split('/').map(encodeURIComponent).join('/')}`;

/** The catalogue's licences by SPDX identifier: the name, and what each asks of someone who passes the font on (read from the licence texts, 2026-11-15). */
export const LICENSES: Record<string, { name: string; terms: string }> = {
  'OFL-1.1': { name: 'SIL Open Font License 1.1', terms: 'keep its copyright notice and the licence with it, do not sell the font by itself, and do not use a reserved name for a changed font' },
  'Apache-2.0': { name: 'Apache License 2.0', terms: 'give a copy of the licence with it and keep its notices' },
  'Ubuntu-font-1.0': { name: 'Ubuntu Font Licence 1.0', terms: 'the licence goes with it, and a changed font is renamed as the licence says' },
};

/** The catalogue: families under licences that allow commercial use and embedding. */
export const openFamilies: FontFamily[] = catalog.families.map((entry) => ({ ...entry, licenseUrl: `${catalog.source}/blob/${catalog.commit}/${entry.licenseFile}` }));

/** Installed fonts as the desktop shell lists them, `[family, style, path, index]`, by family. */
export function groupInstalled(fonts: [string, string, string, number][]): FontFamily[] {
  const families = new Map<string, FontFamily>();
  for (const [family, style, path, index] of fonts) {
    if (!families.has(family)) families.set(family, { family, axes: [], styles: [] });
    families.get(family)!.styles.push({ style, path, index });
  }
  const order = (style: string) => ['Regular', 'Italic', 'Bold', 'Bold Italic'].indexOf(style) + 1 || 5;
  for (const family of families.values()) family.styles.sort((a, b) => order(a.style) - order(b.style) || a.style.localeCompare(b.style));
  return [...families.values()].sort((a, b) => a.family.localeCompare(b.family));
}

/** Families whose name contains every word of `query`, of `category` if one is given. */
export function searchFamilies(families: FontFamily[], query: string, category = ''): FontFamily[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return families.filter((f) => (!category || f.category === category) && words.every((word) => f.family.toLowerCase().includes(word)));
}

/** Git's name for a file's content: the SHA-1 of "blob <length>\0" and the bytes. */
export async function blobSha(bytes: Uint8Array): Promise<string> {
  const all = await new Blob([`blob ${bytes.length}\0`, bytes as BlobPart]).arrayBuffer();
  return [...new Uint8Array(await crypto.subtle.digest('SHA-1', all))].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** `bytes` if they are the file the catalogue lists as `style`; an error otherwise. */
export async function verified(style: FontStyle, bytes: Uint8Array): Promise<Uint8Array> {
  if (style.sha && (await blobSha(bytes)) !== style.sha) throw new Error('the download is not the file the catalogue lists (it was changed or cut short)');
  return bytes;
}
