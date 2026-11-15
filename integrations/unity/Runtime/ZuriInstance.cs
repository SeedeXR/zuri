using System;
using System.Collections.Generic;
using System.Text;
using System.Threading;

namespace Zuri
{
    /// <summary>How an artboard is placed in a render target of a different size.</summary>
    public enum ZuriFit
    {
        /// <summary>Scale uniformly so the whole artboard is visible.</summary>
        Contain = 0,
        /// <summary>Scale uniformly so the target is covered, cropping the overflow.</summary>
        Cover = 1,
        /// <summary>Stretch to the target, ignoring the aspect ratio.</summary>
        Fill = 2,
        /// <summary>Do not scale: the artboard keeps its own size.</summary>
        None = 3,
        /// <summary>Scale uniformly so the artboard is as wide as the target.</summary>
        FitWidth = 4,
        /// <summary>Scale uniformly so the artboard is as tall as the target.</summary>
        FitHeight = 5,
        /// <summary><see cref="Contain"/> when the artboard is larger than the target, its own size otherwise.</summary>
        ScaleDown = 6,
    }

    /// <summary>
    /// Where the artboard sits in the room a <see cref="ZuriFit"/> leaves over, or which part of it
    /// shows when it overflows the target.
    /// </summary>
    public enum ZuriAlignment
    {
        /// <summary>The middle of the target.</summary>
        Center = 0,
        /// <summary>The top-left corner.</summary>
        TopLeft = 1,
        /// <summary>The middle of the top edge.</summary>
        TopCenter = 2,
        /// <summary>The top-right corner.</summary>
        TopRight = 3,
        /// <summary>The middle of the left edge.</summary>
        CenterLeft = 4,
        /// <summary>The middle of the right edge.</summary>
        CenterRight = 5,
        /// <summary>The bottom-left corner.</summary>
        BottomLeft = 6,
        /// <summary>The middle of the bottom edge.</summary>
        BottomCenter = 7,
        /// <summary>The bottom-right corner.</summary>
        BottomRight = 8,
    }

    /// <summary>The kind of a pointer event fed to <see cref="ZuriInstance.Pointer"/>.</summary>
    public enum ZuriPointerEvent
    {
        /// <summary>The pointer moved (or entered the surface).</summary>
        Move = 0,
        /// <summary>The pointer was pressed.</summary>
        Down = 1,
        /// <summary>The pointer was released.</summary>
        Up = 2,
        /// <summary>The pointer left the surface.</summary>
        Exit = 3,
    }

    /// <summary>The type of a view-model property.</summary>
    public enum ZuriPropertyType
    {
        /// <summary>A float.</summary>
        Number = 0,
        /// <summary>A bool, stored as the number 0 or 1.</summary>
        Bool = 1,
        /// <summary>A one-shot signal: see <see cref="ZuriInstance.Fire"/>.</summary>
        Trigger = 2,
        /// <summary>A color, 0xAARRGGBB.</summary>
        Color = 3,
        /// <summary>UTF-8 text.</summary>
        String = 4,
        /// <summary>
        /// One of the property's <see cref="ZuriProperty.Options"/>: read and set it by position
        /// with the number accessors or by name with the string accessors.
        /// </summary>
        Enum = 5,
    }

    /// <summary>The name and type of a view-model property, and the Options of an enum.</summary>
    [Serializable]
    public struct ZuriProperty
    {
        // Public fields (not properties) so Unity can serialize this into a ZuriAsset.

        /// <summary>The property's name, as passed to the typed accessors.</summary>
        public string Name;

        /// <summary>The property's type.</summary>
        public ZuriPropertyType Type;

        /// <summary>The names of an enum's Options, in order. Empty for every other type.</summary>
        public string[] Options;
    }

    /// <summary>
    /// A playing artboard: animation or state-machine playback, view-model properties,
    /// pointer input, events and CPU rendering to RGBA8 pixels.
    /// </summary>
    /// <remarks>
    /// The frame loop is: <c>if (instance.Advance(dt)) instance.Render(...)</c>. <see cref="Advance"/>
    /// returns false once nothing is moving, so an idle instance costs no rendering.
    /// Use an instance from one thread at a time.
    /// </remarks>
    public sealed unsafe class ZuriInstance : IDisposable
    {
        const int StackBuffer = 512;

        IntPtr _ptr;
        readonly string[] _animations;
        readonly string[] _stateMachines;
        readonly string[] _declaredEvents;
        readonly ZuriProperty[] _properties;

        // UTF-8 encodings of every name passed to the accessors, so per-frame sets do not allocate.
        readonly Dictionary<string, byte[]> _utf8Names = new Dictionary<string, byte[]>();

        internal ZuriInstance(IntPtr ptr)
        {
            _ptr = ptr;
            Width = ZuriNative.zuri_instance_width(ptr);
            Height = ZuriNative.zuri_instance_height(ptr);
            _animations = ZuriNative.ReadNames(ZuriNative.AnimationName, ptr, ZuriNative.zuri_instance_animation_count(ptr));
            _stateMachines = ZuriNative.ReadNames(ZuriNative.StateMachineName, ptr, ZuriNative.zuri_instance_state_machine_count(ptr));
            _declaredEvents = ZuriNative.ReadNames(ZuriNative.DeclaredEventName, ptr, ZuriNative.zuri_instance_declared_event_count(ptr));
            _properties = new ZuriProperty[ZuriNative.zuri_instance_property_count(ptr)];
            for (uint i = 0; i < _properties.Length; i++)
            {
                uint property = i; // the loop variable must not be captured
                _properties[i] = new ZuriProperty
                {
                    Name = ZuriNative.ReadName(ZuriNative.PropertyName, ptr, i),
                    Type = (ZuriPropertyType)ZuriNative.zuri_instance_property_type(ptr, i),
                    Options = ZuriNative.ReadNames((handle, option, buf, cap) => ZuriNative.zuri_instance_property_option_name(handle, property, option, buf, cap), ptr, ZuriNative.zuri_instance_property_option_count(ptr, i)),
                };
            }
        }

        /// <summary>Releases the native instance if <see cref="Dispose"/> was not called.</summary>
        ~ZuriInstance()
        {
            Free();
        }

        /// <summary>Artboard width in artboard units.</summary>
        public float Width { get; }

        /// <summary>Artboard height in artboard units.</summary>
        public float Height { get; }

        /// <summary>Names of the artboard's animations.</summary>
        public IReadOnlyList<string> Animations => _animations;

        /// <summary>Names of the artboard's state machines.</summary>
        public IReadOnlyList<string> StateMachines => _stateMachines;

        /// <summary>
        /// Names of the events <see cref="PollEvents"/> can report: the artboard's own and those of
        /// artboards nested in it.
        /// </summary>
        public IReadOnlyList<string> DeclaredEvents => _declaredEvents;

        /// <summary>The artboard's view-model properties.</summary>
        public IReadOnlyList<ZuriProperty> Properties => _properties;

        // ---- playback ----

        /// <summary>Plays the animation called <paramref name="name"/> from its start. Returns false if there is none.</summary>
        public bool PlayAnimation(string name)
        {
            int index = Array.IndexOf(_animations, name);
            bool ok = index >= 0 && ZuriNative.zuri_instance_play_animation(Ptr, (uint)index);
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>Plays the state machine called <paramref name="name"/> from its entry state. Returns false if there is none.</summary>
        public bool PlayStateMachine(string name)
        {
            int index = Array.IndexOf(_stateMachines, name);
            bool ok = index >= 0 && ZuriNative.zuri_instance_play_state_machine(Ptr, (uint)index);
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>Plays the first state machine or, if there is none, the first animation.</summary>
        public void Autoplay()
        {
            ZuriNative.zuri_instance_autoplay(Ptr);
            GC.KeepAlive(this);
        }

        /// <summary>
        /// Jumps the animation started with <see cref="PlayAnimation"/> to <paramref name="seconds"/> on its
        /// timeline; the next <see cref="Advance"/> (0 is fine) shows it. Nested artboards restart and run
        /// to the same time. Returns false if no animation is playing directly: a state machine has no
        /// single timeline.
        /// </summary>
        public bool Seek(float seconds)
        {
            bool ok = ZuriNative.zuri_instance_seek(Ptr, seconds);
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>
        /// The object id of the state each layer of the playing state machine is in
        /// (<see cref="uint.MaxValue"/> for a layer in none), as a new array. Empty while no state machine is playing.
        /// </summary>
        public uint[] LayerStates
        {
            get
            {
                var states = new uint[ZuriNative.zuri_instance_layer_count(Ptr)];
                for (uint i = 0; i < states.Length; i++)
                {
                    states[i] = ZuriNative.zuri_instance_layer_state(_ptr, i);
                }
                GC.KeepAlive(this);
                return states;
            }
        }

        /// <summary>
        /// Advances time by <paramref name="deltaSeconds"/>. Returns true if the picture may have
        /// changed and should be rendered again; false means the last render is still current.
        /// </summary>
        public bool Advance(float deltaSeconds)
        {
            bool changed = ZuriNative.zuri_instance_advance(Ptr, deltaSeconds);
            GC.KeepAlive(this);
            return changed;
        }

        // ---- view model ----
        // Setters return false when the artboard has no property with that name.
        // Getters throw, because there is no value to return.
        // An enum is a number (its Option's position, from 0) and a string (its Option's name).

        /// <summary>
        /// Sets a number property, or an enum to the Option at that position. Returns false if there
        /// is no such property or the value is not finite.
        /// </summary>
        public bool SetNumber(string name, float value)
        {
            bool ok;
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                ok = ZuriNative.zuri_instance_set_number(Ptr, utf8, length, value);
            }
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>Reads a number property, or the position of an enum's Option.</summary>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public float GetNumber(string name)
        {
            float value;
            bool ok;
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                ok = ZuriNative.zuri_instance_get_number(Ptr, utf8, length, &value);
            }
            GC.KeepAlive(this);
            return ok ? value : throw Missing(name);
        }

        /// <summary>Sets a bool property (the number 0 or 1). Returns false if there is no such property.</summary>
        public bool SetBool(string name, bool value)
        {
            return SetNumber(name, value ? 1f : 0f);
        }

        /// <summary>Reads a bool property.</summary>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public bool GetBool(string name)
        {
            return GetNumber(name) != 0f;
        }

        /// <summary>Sets a color property to <paramref name="argb"/> (0xAARRGGBB). Returns false if there is no such property.</summary>
        public bool SetColor(string name, uint argb)
        {
            bool ok;
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                ok = ZuriNative.zuri_instance_set_color(Ptr, utf8, length, argb);
            }
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>Reads a color property as 0xAARRGGBB.</summary>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public uint GetColor(string name)
        {
            uint argb;
            bool ok;
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                ok = ZuriNative.zuri_instance_get_color(Ptr, utf8, length, &argb);
            }
            GC.KeepAlive(this);
            return ok ? argb : throw Missing(name);
        }

        /// <summary>
        /// Sets a string property, or an enum to the Option with that name. Returns false if there is
        /// no such property, or the enum has no such Option.
        /// </summary>
        public bool SetString(string name, string value)
        {
            if (value == null)
            {
                throw new ArgumentNullException(nameof(value));
            }
            int capacity = Encoding.UTF8.GetMaxByteCount(value.Length);
            Span<byte> encoded = capacity <= StackBuffer ? stackalloc byte[capacity] : new byte[capacity];
            bool ok;
            fixed (char* chars = value)
            fixed (byte* bytes = encoded)
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                // `chars` is null for an empty string, which GetBytes rejects.
                int written = value.Length == 0 ? 0 : Encoding.UTF8.GetBytes(chars, value.Length, bytes, capacity);
                ok = ZuriNative.zuri_instance_set_string(Ptr, utf8, length, bytes, (nuint)written);
            }
            GC.KeepAlive(this);
            return ok;
        }

        /// <summary>Reads a string property, or the name of an enum's Option.</summary>
        /// <exception cref="ZuriException">There is no such property.</exception>
        public string GetString(string name)
        {
            byte[] buffer;
            fixed (byte* utf8 = Utf8(name, out nuint nameLength))
            {
                // Measure, then read. An empty array pins to NULL, which the C API accepts with cap = 0.
                buffer = new byte[checked((int)ZuriNative.zuri_instance_get_string(Ptr, utf8, nameLength, null, 0))];
                fixed (byte* bytes = buffer)
                {
                    ZuriNative.zuri_instance_get_string(Ptr, utf8, nameLength, bytes, (nuint)buffer.Length);
                }
            }
            GC.KeepAlive(this);
            // The C API returns 0 both for an empty string and for a missing property.
            if (buffer.Length == 0 && !HasProperty(name))
            {
                throw Missing(name);
            }
            return Encoding.UTF8.GetString(buffer);
        }

        /// <summary>Fires a trigger property. Returns false if there is no such property.</summary>
        public bool Fire(string name)
        {
            bool ok;
            fixed (byte* utf8 = Utf8(name, out nuint length))
            {
                ok = ZuriNative.zuri_instance_fire(Ptr, utf8, length);
            }
            GC.KeepAlive(this);
            return ok;
        }

        // ---- input and events ----

        /// <summary>
        /// Feeds a pointer event to the playing state machine. <paramref name="x"/> and
        /// <paramref name="y"/> are pixels of the last <see cref="Render(Span{byte}, int, int, ZuriFit, uint, bool, ZuriAlignment)"/>
        /// target, origin top-left, y down. Does nothing while no state machine is playing.
        /// </summary>
        public void Pointer(ZuriPointerEvent kind, float x, float y)
        {
            ZuriNative.zuri_instance_pointer(Ptr, (uint)kind, x, y);
            GC.KeepAlive(this);
        }

        /// <summary>
        /// Replaces the contents of <paramref name="events"/> with the names of the events emitted
        /// since the last call, in order, and returns how many there are.
        /// </summary>
        public int PollEvents(List<string> events)
        {
            events.Clear();
            uint count = ZuriNative.zuri_instance_poll_events(Ptr);
            for (uint i = 0; i < count; i++)
            {
                events.Add(ZuriNative.ReadName(ZuriNative.EventName, _ptr, i));
            }
            GC.KeepAlive(this);
            return (int)count;
        }

        // ---- object properties ----

        /// <summary>
        /// Reads the value object <paramref name="objectId"/> currently draws with (after animation and
        /// binds) for schema property <paramref name="key"/>: numbers, bools and enums as a float.
        /// Returns false if the object has no such property.
        /// </summary>
        public bool TryGetProperty(uint objectId, uint key, out float value)
        {
            float result = 0f;
            bool ok = ZuriNative.zuri_instance_get_property(Ptr, objectId, key, &result);
            GC.KeepAlive(this);
            value = result;
            return ok;
        }

        /// <summary>Like <see cref="TryGetProperty"/>, for color properties (0xAARRGGBB).</summary>
        public bool TryGetPropertyColor(uint objectId, uint key, out uint argb)
        {
            uint result = 0;
            bool ok = ZuriNative.zuri_instance_get_property_color(Ptr, objectId, key, &result);
            GC.KeepAlive(this);
            argb = result;
            return ok;
        }

        // ---- rendering ----

        /// <summary>
        /// Renders the current frame into <paramref name="rgba"/>: RGBA8, <paramref name="width"/> x
        /// <paramref name="height"/> pixels, rows top to bottom. The target is first cleared to
        /// <paramref name="background"/> (0xAARRGGBB; 0 is transparent).
        /// </summary>
        /// <param name="rgba">At least <c>width * height * 4</c> bytes.</param>
        /// <param name="width">Target width in pixels, 1 to 16384.</param>
        /// <param name="height">Target height in pixels, 1 to 16384.</param>
        /// <param name="fit">How the artboard is scaled into the target.</param>
        /// <param name="background">Clear color, 0xAARRGGBB.</param>
        /// <param name="premultiplied">True for premultiplied alpha output, false for straight alpha.</param>
        /// <param name="alignment">Where the artboard sits in the target.</param>
        /// <exception cref="ArgumentException">The span is too small for the target.</exception>
        /// <exception cref="ZuriException">The runtime rejected the target.</exception>
        public void Render(Span<byte> rgba, int width, int height, ZuriFit fit = ZuriFit.Contain, uint background = 0, bool premultiplied = false, ZuriAlignment alignment = ZuriAlignment.Center)
        {
            if (width <= 0 || height <= 0 || rgba.Length < (long)width * height * 4)
            {
                throw new ArgumentException("The pixel buffer must hold at least width * height * 4 bytes.", nameof(rgba));
            }
            bool ok;
            fixed (byte* pixels = rgba)
            {
                ok = ZuriNative.zuri_instance_render(Ptr, pixels, (uint)width, (uint)height, (uint)fit | ((uint)alignment << 8), background, premultiplied);
            }
            GC.KeepAlive(this);
            if (!ok)
            {
                throw ZuriException.FromLastError("Could not render");
            }
        }

        /// <summary>Releases the native instance.</summary>
        public void Dispose()
        {
            Free();
            GC.SuppressFinalize(this);
        }

        IntPtr Ptr => _ptr != IntPtr.Zero ? _ptr : throw new ObjectDisposedException(nameof(ZuriInstance));

        byte[] Utf8(string name, out nuint length)
        {
            if (name == null)
            {
                throw new ArgumentNullException(nameof(name));
            }
            if (!_utf8Names.TryGetValue(name, out byte[] utf8))
            {
                utf8 = Encoding.UTF8.GetBytes(name);
                _utf8Names.Add(name, utf8);
            }
            length = (nuint)utf8.Length;
            return utf8;
        }

        bool HasProperty(string name)
        {
            return Array.Exists(_properties, property => property.Name == name);
        }

        static ZuriException Missing(string name)
        {
            return new ZuriException("The artboard has no view-model property named '" + name + "'.");
        }

        void Free()
        {
            IntPtr ptr = Interlocked.Exchange(ref _ptr, IntPtr.Zero);
            if (ptr != IntPtr.Zero)
            {
                ZuriNative.zuri_instance_free(ptr);
            }
        }
    }
}
