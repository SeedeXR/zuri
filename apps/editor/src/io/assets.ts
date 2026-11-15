// Asset import: images and fonts are embedded as base64 assets in the document;
// SVG and 3D files go through the desktop shell's importer and are merged in.

import { activeArtboard, freshName, toast } from '../actions.ts';
import { type Node, append, fromSource, node } from '../doc/model.ts';
import { mergeImport } from '../doc/ops.ts';
import { uniqueName } from '../doc/refs.ts';
import { schema } from '../runtime.ts';
import { store } from '../store.ts';
import { guarded, pick } from './files.ts';
import { baseName, importFile, pickOpenPath } from './tauri.ts';

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

/** Embed a TTF / OTF as a FontAsset. */
export const importFont = (): Promise<void> =>
  guarded('Could not import the font', async () => {
    const file = await pick('Fonts', FONTS);
    if (!file) return;
    const asset = node('FontAsset', { name: assetName('FontAsset', file.name), bytes: base64(file.bytes) });
    store.edit((doc) => append(doc, null, asset), { selection: [asset.id] });
    toast(`Imported font "${asset.props.name}". The Text tool (T) uses it.`);
  });

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
