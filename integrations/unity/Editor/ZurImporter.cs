using System;
using System.IO;
using UnityEditor.AssetImporters;
using UnityEngine;

namespace Zuri.Editor
{
    /// <summary>
    /// Imports <c>.zur</c> files as <see cref="ZuriAsset"/>s, with the first frame of the first
    /// artboard as the asset's thumbnail.
    /// </summary>
    [ScriptedImporter(2, "zur")]
    public sealed class ZurImporter : ScriptedImporter
    {
        const int ThumbnailSize = 128;

        /// <inheritdoc/>
        public override void OnImportAsset(AssetImportContext ctx)
        {
            try
            {
                ZuriAsset asset = ZuriAsset.Create(File.ReadAllBytes(ctx.assetPath));
                Texture2D thumbnail = asset.Artboards.Count > 0 ? asset.RenderPreview(ThumbnailSize, ThumbnailSize) : null;
                ctx.AddObjectToAsset("asset", asset, thumbnail);
                ctx.SetMainObject(asset);
                for (int i = 0; i < asset.File.SoundCount; i++)
                {
                    if (!ZuriWav.TryDecode(asset.File.GetEncodedSound(asset.File.SoundId(i)), out _, out _, out _))
                    {
                        ctx.LogImportWarning("Zuri: sound " + i + " of '" + ctx.assetPath + "' is not an uncompressed WAV file, the one format Unity can decode at run time, and will not be played.");
                    }
                }
            }
            catch (Exception e) when (e is ZuriException || e is DllNotFoundException)
            {
                // DllNotFoundException: the native plugin has not been built for this editor platform.
                ctx.LogImportError("Zuri: cannot import '" + ctx.assetPath + "': " + e.Message);
            }
        }
    }
}
