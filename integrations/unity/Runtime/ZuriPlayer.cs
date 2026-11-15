using System;
using System.Collections.Generic;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using UnityEngine;
using UnityEngine.Events;

namespace Zuri
{
    /// <summary>What a <see cref="ZuriPlayer"/> plays when it starts.</summary>
    public enum ZuriPlayback
    {
        /// <summary>The first state machine or, if there is none, the first animation.</summary>
        Auto,
        /// <summary>The state machine named by the player (the first one if no name is set).</summary>
        StateMachine,
        /// <summary>The animation named by the player (the first one if no name is set).</summary>
        Animation,
    }

    /// <summary>
    /// Plays one artboard of a <see cref="ZuriAsset"/> into a <see cref="Texture2D"/>.
    /// Show the texture with a <see cref="ZuriRawImage"/> (uGUI) or a <see cref="ZuriQuad"/> (world space).
    /// </summary>
    /// <remarks>
    /// <para>Every frame the player advances the instance and renders only if something changed,
    /// so an idle artboard costs one cheap native call per frame.</para>
    /// <para>The texture's rows run top to bottom (Zuri's convention), which is upside down for
    /// Unity. Sample it with a flipped V: UV rect (0, 1, 1, -1), or texture scale (1, -1) and
    /// offset (0, 1). <see cref="ZuriRawImage"/> and <see cref="ZuriQuad"/> do this for you.</para>
    /// <para>In edit mode the player shows the first frame and does not animate.</para>
    /// </remarks>
    [ExecuteAlways]
    [AddComponentMenu("Zuri/Zuri Player")]
    public sealed class ZuriPlayer : MonoBehaviour
    {
        // The runtime's limit on a render target's side.
        const int MaxSize = 16384;

        [SerializeField, Tooltip("The .zur file to play.")]
        ZuriAsset asset;

        [SerializeField, Tooltip("Name of the artboard to play. Empty plays the first one.")]
        string artboard = "";

        [SerializeField, Tooltip("What to play when the player starts.")]
        ZuriPlayback playback = ZuriPlayback.Auto;

        [SerializeField, Tooltip("Name of the state machine or animation to play. Empty plays the first one.")]
        string playbackName = "";

        [SerializeField, Tooltip("How the artboard is scaled when the resolution has a different aspect ratio.")]
        ZuriFit fit = ZuriFit.Contain;

        [SerializeField, Tooltip("Where the artboard sits in the room the fit leaves over, or which part shows when it overflows.")]
        ZuriAlignment alignment = ZuriAlignment.Center;

        [SerializeField, Tooltip("Texture size in pixels. Zero uses the artboard's size.")]
        Vector2Int resolution;

        [SerializeField, Min(0.01f), Tooltip("Multiplies the texture size, for example 2 for high-density displays.")]
        float resolutionScale = 1f;

        [SerializeField, Tooltip("Color the texture is cleared to before drawing. Transparent by default.")]
        Color32 background = new Color32(0, 0, 0, 0);

        [SerializeField, Tooltip("Output premultiplied alpha. Use with a material that blends One / OneMinusSrcAlpha.")]
        bool premultipliedAlpha;

        [SerializeField, Tooltip("Advance with unscaled time, so the artboard keeps playing while Time.timeScale is 0.")]
        bool unscaledTime;

        [SerializeField, Tooltip("Invoked with the name of each event the artboard emits.")]
        UnityEvent<string> onEvent = new UnityEvent<string>();

        readonly List<string> _events = new List<string>();
        ZuriInstance _instance;
        Texture2D _texture;

        /// <summary>The asset being played. Setting it restarts playback.</summary>
        public ZuriAsset Asset
        {
            get => asset;
            set
            {
                asset = value;
                Rebuild();
            }
        }

        /// <summary>The playing instance, or null while the player is disabled or has no valid asset.</summary>
        public ZuriInstance Instance => _instance;

        /// <summary>
        /// The texture the artboard is rendered into (RGBA32, sRGB, no mipmaps, rows top to bottom:
        /// sample it with a flipped V), or null while nothing is playing.
        /// </summary>
        public Texture2D Texture => _texture;

        /// <summary>Invoked with the name of each event the artboard emits.</summary>
        public UnityEvent<string> OnEvent => onEvent;

        /// <summary>Raised when <see cref="Texture"/> is created, replaced or released.</summary>
        public event Action TextureChanged;

        void OnEnable()
        {
            Rebuild();
        }

        void OnDisable()
        {
            Release();
        }

        void Update()
        {
            if (Application.isPlaying)
            {
                Tick(unscaledTime ? Time.unscaledDeltaTime : Time.deltaTime);
            }
        }

        /// <summary>
        /// Recreates the instance and the texture from the current settings and restarts playback.
        /// Call it after changing serialized settings from code. View-model values are reset.
        /// </summary>
        public void Rebuild()
        {
            Release();
            if (asset == null || !isActiveAndEnabled)
            {
                return;
            }
            try
            {
                ZuriFile file = asset.File;
                _instance = string.IsNullOrEmpty(artboard) ? file.CreateInstance() : file.CreateInstance(artboard);
            }
            catch (Exception e) when (e is ZuriException || e is DllNotFoundException)
            {
                Debug.LogError("Zuri: cannot play '" + asset.name + "': " + e.Message, this);
                return;
            }
            StartPlayback();

            int width = TargetSize(resolution.x, _instance.Width);
            int height = TargetSize(resolution.y, _instance.Height);
            // Not linear: the runtime writes sRGB-encoded colors.
            _texture = new Texture2D(width, height, TextureFormat.RGBA32, false)
            {
                name = asset.name,
                wrapMode = TextureWrapMode.Clamp,
                hideFlags = HideFlags.HideAndDontSave,
            };
            _instance.Advance(0f);
            Render();
            TextureChanged?.Invoke();
        }

        /// <summary>
        /// Advances the artboard by <paramref name="deltaSeconds"/>, renders if something changed and
        /// dispatches events. The player calls this from Update in play mode; call it yourself to
        /// drive the artboard manually. Returns true if the texture was redrawn.
        /// </summary>
        public bool Tick(float deltaSeconds)
        {
            if (_instance == null)
            {
                return false;
            }
            bool changed = _instance.Advance(deltaSeconds);
            if (changed)
            {
                Render();
            }
            _instance.PollEvents(_events);
            for (int i = 0; i < _events.Count; i++)
            {
                onEvent.Invoke(_events[i]);
            }
            return changed;
        }

        /// <summary>
        /// Feeds a pointer event to the artboard. <paramref name="pixel"/> is in <see cref="Texture"/>
        /// pixels with the origin at the top-left of the picture and y pointing down.
        /// </summary>
        public void Pointer(ZuriPointerEvent kind, Vector2 pixel)
        {
            _instance?.Pointer(kind, pixel.x, pixel.y);
        }

        // ---- view-model properties ----
        // Setters return false if nothing is playing or the artboard has no such property.
        // Getters throw in those cases.

        /// <summary>Sets a number property.</summary>
        public bool SetNumber(string propertyName, float value)
        {
            return _instance != null && _instance.SetNumber(propertyName, value);
        }

        /// <summary>Sets a bool property.</summary>
        public bool SetBool(string propertyName, bool value)
        {
            return _instance != null && _instance.SetBool(propertyName, value);
        }

        /// <summary>Sets a color property.</summary>
        public bool SetColor(string propertyName, Color32 color)
        {
            return _instance != null && _instance.SetColor(propertyName, ToArgb(color));
        }

        /// <summary>Sets a string property.</summary>
        public bool SetString(string propertyName, string value)
        {
            return _instance != null && _instance.SetString(propertyName, value);
        }

        /// <summary>Fires a trigger property.</summary>
        public bool Fire(string propertyName)
        {
            return _instance != null && _instance.Fire(propertyName);
        }

        /// <summary>Reads a number property.</summary>
        /// <exception cref="InvalidOperationException">Nothing is playing.</exception>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public float GetNumber(string propertyName)
        {
            return Playing().GetNumber(propertyName);
        }

        /// <summary>Reads a bool property.</summary>
        /// <exception cref="InvalidOperationException">Nothing is playing.</exception>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public bool GetBool(string propertyName)
        {
            return Playing().GetBool(propertyName);
        }

        /// <summary>Reads a color property.</summary>
        /// <exception cref="InvalidOperationException">Nothing is playing.</exception>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public Color32 GetColor(string propertyName)
        {
            uint argb = Playing().GetColor(propertyName);
            return new Color32((byte)(argb >> 16), (byte)(argb >> 8), (byte)argb, (byte)(argb >> 24));
        }

        /// <summary>Reads a string property.</summary>
        /// <exception cref="InvalidOperationException">Nothing is playing.</exception>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public string GetString(string propertyName)
        {
            return Playing().GetString(propertyName);
        }

        ZuriInstance Playing()
        {
            return _instance ?? throw new InvalidOperationException("The ZuriPlayer is not playing: it is disabled or has no valid asset.");
        }

        void StartPlayback()
        {
            bool named = !string.IsNullOrEmpty(playbackName);
            bool started = true;
            switch (playback)
            {
                case ZuriPlayback.StateMachine:
                    started = _instance.StateMachines.Count > 0 && _instance.PlayStateMachine(named ? playbackName : _instance.StateMachines[0]);
                    break;
                case ZuriPlayback.Animation:
                    started = _instance.Animations.Count > 0 && _instance.PlayAnimation(named ? playbackName : _instance.Animations[0]);
                    break;
                default:
                    _instance.Autoplay();
                    break;
            }
            if (!started)
            {
                Debug.LogWarning("Zuri: '" + asset.name + "' has no " + (playback == ZuriPlayback.Animation ? "animation" : "state machine") + (named ? " named '" + playbackName + "'" : "") + "; nothing is playing.", this);
            }
        }

        int TargetSize(int pixels, float artboardSize)
        {
            float size = pixels > 0 ? pixels : artboardSize;
            return Mathf.Clamp(Mathf.RoundToInt(size * resolutionScale), 1, MaxSize);
        }

        // Renders straight into the texture's memory: no managed allocation, no copy.
        unsafe void Render()
        {
            NativeArray<byte> pixels = _texture.GetRawTextureData<byte>();
            var target = new Span<byte>(NativeArrayUnsafeUtility.GetUnsafePtr(pixels), pixels.Length);
            _instance.Render(target, _texture.width, _texture.height, fit, ToArgb(background), premultipliedAlpha, alignment);
            _texture.Apply(false);
        }

        void Release()
        {
            _instance?.Dispose();
            _instance = null;
            if (_texture != null)
            {
                ZuriAsset.DestroySafely(_texture);
                _texture = null;
                TextureChanged?.Invoke();
            }
        }

        static uint ToArgb(Color32 color)
        {
            return (uint)(color.a << 24 | color.r << 16 | color.g << 8 | color.b);
        }
    }
}
