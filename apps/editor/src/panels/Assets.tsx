// Left panel, Assets tab: the document's top-level assets (images, fonts, meshes,
// shaders) and the import buttons.

import { CaseSensitive, FileUp, ImagePlus, Plus, Trash2 } from 'lucide-react';
import { deleteNodes, select } from '../actions.ts';
import { ASSET_TYPES } from '../doc/schema.ts';
import { labelOf } from '../doc/ops.ts';
import { importExternal, importFont, importImage, placeImage } from '../io/assets.ts';
import { isTauri } from '../io/tauri.ts';
import { useStore } from '../store.ts';
import { TypeIcon } from './icons.tsx';
import { Button, IconButton, cx } from './ui.tsx';

/** A data URL for an embedded image, from its base64 payload's signature. */
function thumbnail(bytes: unknown): string | null {
  if (typeof bytes !== 'string' || bytes.startsWith('@')) return null;
  const mime = bytes.startsWith('iVBOR') ? 'image/png' : bytes.startsWith('/9j/') ? 'image/jpeg' : bytes.startsWith('UklGR') ? 'image/webp' : null;
  return mime && `data:${mime};base64,${bytes}`;
}

export function Assets() {
  const doc = useStore((s) => s.doc);
  const selection = useStore((s) => s.selection);
  const assets = doc.filter((n) => ASSET_TYPES.has(n.type));
  return (
    <div className="p-2">
      <div className="mb-2 flex gap-1">
        <Button onClick={importImage} title="Embed a PNG, JPEG or WebP and place it on the artboard">
          <ImagePlus size={13} aria-hidden /> Image
        </Button>
        <Button onClick={importFont} title="Embed a TTF or OTF font for Text">
          <CaseSensitive size={13} aria-hidden /> Font
        </Button>
        <Button
          disabled={!isTauri}
          onClick={importExternal}
          title={isTauri ? 'Import an SVG, OBJ, glTF / GLB or FBX file into the artboard' : 'Importing SVG and 3D models needs the desktop app (it runs the native importer)'}
        >
          <FileUp size={13} aria-hidden /> SVG / 3D
        </Button>
      </div>
      {!isTauri && <p className="mx-0.5 mb-2 text-[11px] text-fg-faint">SVG and 3D import is available in the desktop app.</p>}
      {assets.length === 0 && <p className="empty px-0.5 py-1.5">No assets yet. Import an image or a font.</p>}
      <ul>
        {assets.map((asset) => {
          const src = asset.type === 'ImageAsset' ? thumbnail(asset.props.bytes) : null;
          return (
            <li key={asset.id} className={cx('flex h-8.5 items-center gap-0.5 rounded-s px-1', selection.includes(asset.id) ? 'bg-accent/20' : 'hover:bg-raised')}>
              <button
                type="button"
                className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-s text-left"
                onClick={() => select([asset.id])}
                title={`${asset.type}: select to edit in the inspector`}
              >
                <span className="checker flex size-6.5 shrink-0 items-center justify-center overflow-hidden rounded-s [background-size:8px_8px]">
                  {src ? <img src={src} alt="" className="max-h-full max-w-full" /> : <TypeIcon node={asset} size={16} />}
                </span>
                <span className="flex-1 truncate">{labelOf(asset)}</span>
                <span className="text-[11px] text-fg-faint">{asset.type.replace('Asset', '')}</span>
              </button>
              {asset.type === 'ImageAsset' && <IconButton icon={Plus} size={13} label={`Place ${labelOf(asset)} on the artboard`} onClick={() => placeImage(asset)} />}
              <IconButton icon={Trash2} size={13} label={`Delete ${labelOf(asset)}`} onClick={() => deleteNodes([asset.id])} />
            </li>
          );
        })}
      </ul>
    </div>
  );
}
