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
    [ScriptedImporter(1, "zur")]
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
            }
            catch (Exception e) when (e is ZuriException || e is DllNotFoundException)
            {
                // DllNotFoundException: the native plugin has not been built for this editor platform.
                ctx.LogImportError("Zuri: cannot import '" + ctx.assetPath + "': " + e.Message);
            }
        }
    }
}
