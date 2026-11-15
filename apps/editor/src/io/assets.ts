// Asset import: images and fonts are embedded as base64 assets in the document (a font
// from a file, from the catalogue of open fonts, or from this machine's installed
// fonts); SVG and 3D files go through the desktop shell's importer and are merged in.

import { activeArtboard, freshName, toast } from '../actions.ts';
import { type Node, append, fromSource, node } from '../doc/model.ts';
import { mergeImport } from '../doc/ops.ts';
import { uniqueName } from '../doc/refs.ts';
import { type FontFamily, type FontStyle, fontUrl, groupInstalled, verified } from '../fonts/catalog.ts';
import { schema } from '../runtime.ts';
import { store } from '../store.ts';
import { guarded, pick } from './files.ts';
import { baseName, importFile, isTauri, pickOpenPath, readFont, systemFonts } from './tauri.ts';

const IMAGES = ['png', 'jpg', 'jpeg', 'webp'];
const FONTS = ['ttf', 'otf'];
const IMPORTABLE = ['svg', 'obj', 'gltf', 'glb', 'fbx'];

function base64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

const stem = (fileName: string): string => fileName.replace(/\.[^.]*$/, '') || 'asset';
const assetName = (type: string, fileName: string): string => uniqueName(schema, store.get().doc, undefined, type, stem(fileName));

/** An Image node showing `asset`, centered on the active artboard. */
export function placeImage(asset: Node): void {
  const artboard = activeArtboard();
  if (!artboard) return;
  const center = (prop: string) => (typeof artboard.props[prop] === 'number' ? (artboard.props[prop] as number) / 2 : 0);
  const image = node('Image', { name: freshName(String(asset.props.name)), asset: asset.props.name, x: center('width'), y: center('height') });
  store.edit((doc) => append(doc, artboard.id, image), { selection: [image.id] });
}

/** Embed a PNG / JPEG / WebP as an ImageAsset and place it on the artboard. */
export const importImage = (): Promise<void> =>
  guarded('Could not import the image', async () => {
    const file = await pick('Images', IMAGES);
    if (!file) return;
    const bitmap = await createImageBitmap(new Blob([file.bytes as BlobPart]));
    const asset = node('ImageAsset', { name: assetName('ImageAsset', file.name), width: bitmap.width, height: bitmap.height, bytes: base64(file.bytes) });
    bitmap.close();
    // The asset and the Image that shows it are one undo step.
    store.beginGesture();
    store.edit((doc) => append(doc, null, asset));
    placeImage(asset);
    store.endGesture();
  });

/** Embed a font's bytes as a FontAsset called `name` ("Inter Italic"). */
export function embedFont(name: string, bytes: Uint8Array): void {
  const asset = node('FontAsset', { name: uniqueName(schema, store.get().doc, undefined, 'FontAsset', name), bytes: base64(bytes) });
  store.edit((doc) => append(doc, null, asset), { selection: [asset.id] });
  toast(`Added font "${asset.props.name}". The Text tool (T) uses it.`);
}

/** Embed a TTF / OTF file as a FontAsset. */
export const importFont = (): Promise<void> =>
  guarded('Could not import the font', async () => {
    const file = await pick('Fonts', FONTS);
    if (file) embedFont(stem(file.name), file.bytes);
  });

/** The fonts installed on this machine. Only the desktop app can list them. */
export const installedFamilies = async (): Promise<FontFamily[]> => (isTauri ? groupInstalled(await systemFonts()) : []);

/** The font file of `style`, as one font: a catalogue font is downloaded and checked against the hash the catalogue lists, an installed one is read by the desktop shell. */
export async function loadFont(style: FontStyle, signal: AbortSignal): Promise<Uint8Array> {
  if (style.index !== undefined) return readFont(style.path, style.index);
  const response = await fetch(fontUrl(style.path), { signal });
  if (!response.ok) throw new Error(`the download failed (${response.status})`);
  return verified(style, new Uint8Array(await response.arrayBuffer()));
}

/** Import SVG / OBJ / glTF / GLB / FBX through the desktop importer and merge the result. */
export const importExternal = (): Promise<void> =>
  guarded('Could not import the file', async () => {
    const path = await pickOpenPath([{ name: 'SVG and 3D models', extensions: IMPORTABLE }]);
    const artboard = activeArtboard();
    if (!path || !artboard) return;
    const imported = fromSource(await importFile(path));
    const merged = mergeImport(store.get().doc, imported, artboard.id, freshName(stem(baseName(path))), schema);
    store.edit(() => merged.roots, merged.id ? { selection: [merged.id] } : {});
  });
