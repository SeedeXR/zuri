// Document files: New, Open (.json source or .zur), Save / Save As (JSON source) and
// Export (.zur). Uses native dialogs inside Tauri and file inputs / downloads in a browser.

import { toast } from '../actions.ts';
import { fromSource, toSource } from '../doc/model.ts';
import { defaultDocument } from '../doc/ops.ts';
import { runtime } from '../runtime.ts';
import { store } from '../store.ts';
import { baseName, dirName, exportZur, isTauri, pickOpenPath, pickSavePath, readFile, writeText } from './tauri.ts';

/** Example documents from the repository, bundled for the File menu and the `?example=` dev hook. */
const EXAMPLES = import.meta.glob<string>('../../../../examples/*.json', { query: '?raw', import: 'default' });
export const exampleNames: string[] = Object.keys(EXAMPLES).map((path) => baseName(path).replace('.json', ''));

/** Ask the browser for a file. Resolves null when the picker is cancelled. */
function pickFile(accept: string): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept;
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/** Pick a file with one of `extensions`: a native dialog in Tauri (which also knows its path), a file input in a browser. */
export async function pick(kind: string, extensions: string[]): Promise<{ name: string; bytes: Uint8Array; path: string | null } | null> {
  if (isTauri) {
    const path = await pickOpenPath([{ name: kind, extensions }]);
    return path ? { name: baseName(path), bytes: await readFile(path), path } : null;
  }
  const file = await pickFile(extensions.map((e) => `.${e}`).join(','));
  return file ? { name: file.name, bytes: new Uint8Array(await file.arrayBuffer()), path: null } : null;
}

function download(name: string, data: BlobPart, type: string): void {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

const mayDiscard = (): boolean => store.get().doc === store.get().saved || confirm('Discard the unsaved changes to this document?');

/** Run a file command, reporting any failure instead of throwing into the UI. */
export async function guarded(what: string, f: () => Promise<void> | void): Promise<void> {
  try {
    await f();
  } catch (e) {
    toast(`${what}: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

/** Load file contents as the document. `.zur` files are decompiled to source first. */
function openBytes(name: string, bytes: Uint8Array, path: string | null): void {
  const binary = name.toLowerCase().endsWith('.zur');
  const text = binary ? runtime.decompile(bytes) : new TextDecoder().decode(bytes);
  // A decompiled .zur has no source file yet: saving asks where to put it.
  store.open(fromSource(text), binary ? name.replace(/\.zur$/i, '.json') : name, binary ? null : path);
}

export function newDocument(): void {
  if (mayDiscard()) store.open(defaultDocument(), 'Untitled.json', null);
}

export const openDocument = (): Promise<void> =>
  guarded('Could not open the file', async () => {
    if (!mayDiscard()) return;
    const file = await pick('Zuri documents', ['json', 'zur']);
    if (file) openBytes(file.name, file.bytes, file.path);
  });

export const openExample = (name: string): Promise<void> =>
  guarded('Could not open the example', async () => {
    const load = Object.entries(EXAMPLES).find(([path]) => baseName(path) === `${name}.json`)?.[1];
    if (!load) throw new Error(`there is no example named "${name}"`);
    if (mayDiscard()) store.open(fromSource(await load()), `${name}.json`, null);
  });

export const saveDocument = (saveAs = false): Promise<void> =>
  guarded('Could not save', async () => {
    const { doc, fileName, filePath } = store.get();
    const text = toSource(doc);
    if (!isTauri) {
      download(fileName, text, 'application/json');
      store.set({ saved: doc });
      return;
    }
    const path = (!saveAs && filePath) || (await pickSavePath(filePath ?? fileName, [{ name: 'Zuri source', extensions: ['json'] }]));
    if (!path) return;
    await writeText(path, text);
    store.set({ saved: doc, filePath: path, fileName: baseName(path) });
  });

export const exportDocument = (): Promise<void> =>
  guarded('Could not export', async () => {
    const { doc, fileName, filePath } = store.get();
    const name = fileName.replace(/\.json$/i, '') + '.zur';
    if (!isTauri) {
      download(name, runtime.compile(toSource(doc)) as BlobPart, 'application/octet-stream');
      return;
    }
    const path = await pickSavePath(filePath ? `${dirName(filePath)}/${name}` : name, [{ name: 'Zuri file', extensions: ['zur'] }]);
    if (!path) return;
    await exportZur(toSource(doc), filePath ? dirName(filePath) : '.', path);
    toast(`Exported ${baseName(path)}`);
  });
