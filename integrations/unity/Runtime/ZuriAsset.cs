using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Zuri
{
    /// <summary>What a <see cref="ZuriAsset"/> knows about one artboard without loading it.</summary>
    [Serializable]
    public struct ZuriArtboardInfo
    {
        /// <summary>The artboard's name.</summary>
        public string Name;

        /// <summary>Width in artboard units.</summary>
        public float Width;

        /// <summary>Height in artboard units.</summary>
        public float Height;

        /// <summary>Names of the artboard's animations.</summary>
        public string[] Animations;

        /// <summary>Names of the artboard's state machines.</summary>
        public string[] StateMachines;

        /// <summary>The artboard's view-model properties.</summary>
        public ZuriProperty[] Properties;
    }

    /// <summary>
    /// A <c>.zur</c> file as a Unity asset: the raw bytes plus a description of its artboards.
    /// Dropping a <c>.zur</c> into the project creates one; <see cref="Create"/> makes one at
    /// runtime, for example from downloaded bytes.
    /// </summary>
    public sealed class ZuriAsset : ScriptableObject
    {
        [SerializeField, HideInInspector] byte[] bytes = Array.Empty<byte>();
        [SerializeField] ZuriArtboardInfo[] artboards = Array.Empty<ZuriArtboardInfo>();

        ZuriFile _file;
        byte[] _fileBytes;
        // The file's sounds as clips, by audio asset id: null for one that cannot be decoded.
        readonly Dictionary<uint, AudioClip> _sounds = new Dictionary<uint, AudioClip>();

        /// <summary>The file's artboards, in file order.</summary>
        public IReadOnlyList<ZuriArtboardInfo> Artboards => artboards;

        /// <summary>
        /// The loaded file, shared by every player of this asset, with its image assets decoded.
        /// Owned by the asset: do not dispose it.
        /// </summary>
        /// <exception cref="ZuriException">The bytes are not a valid <c>.zur</c> file.</exception>
        public ZuriFile File
        {
            get
            {
                // A reimport deserializes a new array into `bytes`; that invalidates the cache.
                if (_file == null || !ReferenceEquals(_fileBytes, bytes))
                {
                    ReleaseFile();
                    ZuriFile file = ZuriFile.Load(bytes);
                    try
                    {
                        DecodeImages(file);
                    }
                    catch
                    {
                        // Not cached: a file missing some of its images would be handed out silently.
                        file.Dispose();
                        throw;
                    }
                    _file = file;
                    _fileBytes = bytes;
                }
                return _file;
            }
        }

        /// <summary>Creates an asset from the bytes of a <c>.zur</c> file. The array is kept, not copied.</summary>
        /// <exception cref="ZuriException">The bytes are not a valid <c>.zur</c> file.</exception>
        public static ZuriAsset Create(byte[] zurBytes)
        {
            ZuriArtboardInfo[] infos;
            using (ZuriFile file = ZuriFile.Load(zurBytes))
            {
                infos = new ZuriArtboardInfo[file.ArtboardNames.Count];
                for (int i = 0; i < infos.Length; i++)
                {
                    using (ZuriInstance instance = file.CreateInstance(i))
                    {
                        infos[i] = new ZuriArtboardInfo
                        {
                            Name = file.ArtboardNames[i],
                            Width = instance.Width,
                            Height = instance.Height,
                            Animations = instance.Animations.ToArray(),
                            StateMachines = instance.StateMachines.ToArray(),
                            Properties = instance.Properties.ToArray(),
                        };
                    }
                }
            }
            ZuriAsset asset = CreateInstance<ZuriAsset>();
            asset.bytes = zurBytes;
            asset.artboards = infos;
            return asset;
        }

        /// <summary>
        /// Renders the first frame of the first artboard into a new, upright texture
        /// (unlike <see cref="ZuriPlayer.Texture"/>, row 0 is the bottom row). The caller owns the texture.
        /// </summary>
        public Texture2D RenderPreview(int width, int height)
        {
            var pixels = new Color32[width * height];
            using (ZuriInstance instance = File.CreateInstance())
            {
                instance.Autoplay();
                instance.Advance(0f);
                instance.Render(MemoryMarshal.AsBytes(pixels.AsSpan()), width, height);
            }
            FlipRows(pixels, width, height);
            var texture = new Texture2D(width, height, TextureFormat.RGBA32, false);
            texture.SetPixels32(pixels);
            texture.Apply();
            return texture;
        }

        /// <summary>
        /// The clip for the audio asset with this id (what <see cref="ZuriInstance.TryGetEventSound"/>
        /// reports), decoded on first use and shared by every player of this asset. Null for a
        /// sound Unity cannot decode from bytes at run time: only uncompressed WAV can be.
        /// </summary>
        public AudioClip GetSound(uint id)
        {
            ZuriFile file = File;
            if (_sounds.TryGetValue(id, out AudioClip clip))
            {
                return clip;
            }
            if (ZuriWav.TryDecode(file.GetEncodedSound(id), out float[] samples, out int channels, out int sampleRate))
            {
                clip = AudioClip.Create(name + " sound " + id, samples.Length / channels, channels, sampleRate, false);
                clip.hideFlags = HideFlags.HideAndDontSave;
                clip.SetData(samples, 0);
            }
            else
            {
                Debug.LogWarning("Zuri: sound " + id + " of '" + name + "' is not an uncompressed WAV file, the one format that can be decoded at run time, and will not be played.", this);
            }
            _sounds[id] = clip;
            return clip;
        }

        void OnDisable()
        {
            ReleaseFile();
        }

        void ReleaseFile()
        {
            _file?.Dispose();
            _file = null;
            foreach (AudioClip clip in _sounds.Values)
            {
                if (clip != null)
                {
                    DestroySafely(clip);
                }
            }
            _sounds.Clear();
        }

        /// <summary>Destroys a Unity object the right way for play mode and edit mode.</summary>
        internal static void DestroySafely(UnityEngine.Object target)
        {
            if (Application.isPlaying)
            {
                Destroy(target);
            }
            else
            {
                DestroyImmediate(target);
            }
        }

        // The runtime does not decode images: the host does, and hands the pixels back.
        static void DecodeImages(ZuriFile file)
        {
            int count = file.ImageCount;
            if (count == 0)
            {
                return;
            }
            var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            try
            {
                for (int i = 0; i < count; i++)
                {
                    if (!texture.LoadImage(file.GetEncodedImage(i).ToArray()))
                    {
                        Debug.LogWarning("Zuri: image asset " + i + " could not be decoded (Unity reads PNG and JPEG) and will not be drawn.");
                        continue;
                    }
                    // Unity's rows run bottom to top, Zuri's top to bottom.
                    Color32[] pixels = texture.GetPixels32();
                    FlipRows(pixels, texture.width, texture.height);
                    file.SetImage(i, texture.width, texture.height, MemoryMarshal.AsBytes(pixels.AsSpan()));
                }
            }
            finally
            {
                DestroySafely(texture);
            }
        }

        static void FlipRows(Color32[] pixels, int width, int height)
        {
            for (int top = 0, bottom = height - 1; top < bottom; top++, bottom--)
            {
                for (int x = 0; x < width; x++)
                {
                    Color32 swap = pixels[top * width + x];
                    pixels[top * width + x] = pixels[bottom * width + x];
                    pixels[bottom * width + x] = swap;
                }
            }
        }
    }
}
