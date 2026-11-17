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
            _properties = ReadProperties(ptr, "");
        }

        static ZuriProperty[] ReadProperties(IntPtr ptr, string artboard)
        {
            // Not through Utf8, which keeps every name: the copies of a list come and go.
            byte[] prefix = Encoding.UTF8.GetBytes(artboard);
            fixed (byte* pinned = prefix)
            {
                // A copy, because a lambda cannot capture what `fixed` declares.
                var at = (IntPtr)pinned;
                var properties = new ZuriProperty[ZuriNative.zuri_instance_property_count(ptr, pinned, (nuint)prefix.Length)];
                for (uint i = 0; i < properties.Length; i++)
                {
                    string name = ZuriNative.ReadName((handle, index, buf, cap) => ZuriNative.zuri_instance_property_name(handle, (byte*)at, (nuint)prefix.Length, index, buf, cap), ptr, i);
                    byte[] path = Encoding.UTF8.GetBytes(artboard.Length == 0 ? name : artboard + "/" + name);
                    fixed (byte* pinnedPath = path)
                    {
                        var to = (IntPtr)pinnedPath;
                        properties[i] = new ZuriProperty
                        {
                            Name = name,
                            Type = (ZuriPropertyType)ZuriNative.zuri_instance_property_type_of(ptr, pinnedPath, (nuint)path.Length),
                            Options = ZuriNative.ReadNames((handle, option, buf, cap) => ZuriNative.zuri_instance_property_option_name(handle, (byte*)to, (nuint)path.Length, option, buf, cap), ptr, ZuriNative.zuri_instance_property_option_count(ptr, pinnedPath, (nuint)path.Length)),
                        };
                    }
                }
                return properties;
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

        /// <summary>
        /// The view-model properties of a nested artboard, by its path: "card" is the artboard
        /// nested under that name, "rows/2" the third copy of a list ("rows" is its first).
        /// Empty where nothing is nested by that path, which a list without copies is too.
        /// Each is set as the path, "/" and its name. Read anew at every call: the copies
        /// of a list come and go.
        /// </summary>
        public IReadOnlyList<ZuriProperty> PropertiesIn(string artboard)
        {
            if (artboard == null)
            {
                throw new ArgumentNullException(nameof(artboard));
            }
            ZuriProperty[] properties = ReadProperties(Ptr, artboard);
            GC.KeepAlive(this);
            return properties;
        }

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
        /// target, origin top-left, y down. A press that moves a Layout that scrolls is a
        /// scroll, and until it is let go the scene's Listeners hear only that the pointer has
        /// left. Send an Up or an Exit for every Down: until then the press is open and the
        /// Layouts it holds follow every Move.
        /// <para>
        /// The scene has one pointer and one button, and the runtime keeps to them. Pass every
        /// pointer's events with <paramref name="pointer"/>, your number for it, and for Down and
        /// Up the <paramref name="button"/> that went down or up (0 is the primary one): a press
        /// begins when a primary button goes down and ends when it goes up or its pointer leaves,
        /// and meanwhile no other pointer is the scene's. Returns whether the event was the scene's.
        /// </para>
        /// </summary>
        public bool Pointer(ZuriPointerEvent kind, float x, float y, uint pointer = 0, uint button = 0)
        {
            bool taken = ZuriNative.zuri_instance_pointer(Ptr, (uint)kind, x, y, pointer, button);
            GC.KeepAlive(this);
            return taken;
        }

        /// <summary>
        /// Sets how far, in pixels of the last render target, a press may go along a Layout
        /// that scrolls before the Layout goes with it and the press is a scroll, no longer a
        /// tap. It is 8 until set. Give a finger more than a mouse, and a dense screen more
        /// pixels; a negative number or NaN is not taken.
        /// </summary>
        public void SetDragThreshold(float pixels)
        {
            ZuriNative.zuri_instance_set_drag_threshold(Ptr, pixels);
            GC.KeepAlive(this);
        }

        /// <summary>
        /// The wheel was turned, or two fingers went over a trackpad, with the pointer at
        /// (<paramref name="x"/>, <paramref name="y"/>): <paramref name="dx"/> and
        /// <paramref name="dy"/> are how far to scroll, in pixels of the last render target,
        /// down and to the right being further into the content. The innermost Layout there
        /// that scrolls along it and has further to go that way goes by it. Returns whether a
        /// Layout moved: where none did, the wheel is for what is around the scene.
        /// </summary>
        public bool Wheel(float x, float y, float dx, float dy)
        {
            bool moved = ZuriNative.zuri_instance_wheel(Ptr, x, y, dx, dy);
            GC.KeepAlive(this);
            return moved;
        }

        /// <summary>
        /// Tells the playing state machine that a key went down (<paramref name="down"/> true) or up,
        /// for its keyDown / keyUp listeners. Returns true if a listener reacted.
        /// </summary>
        /// <param name="key">
        /// The key's name as the web's <c>KeyboardEvent.key</c> gives it: "Enter", "Escape", "Tab",
        /// "Backspace", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", " " for the space bar, or
        /// the character typed, such as "a", "A" or "1". Names are case-sensitive.
        /// </param>
        /// <param name="down">True when the key went down, false when it was released.</param>
        public bool Key(string key, bool down)
        {
            bool reacted;
            fixed (byte* utf8 = Utf8(key, out nuint length))
            {
                reacted = ZuriNative.zuri_instance_key(Ptr, utf8, length, down);
            }
            GC.KeepAlive(this);
            return reacted;
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

        /// <summary>
        /// The sound that event <paramref name="index"/> of the last <see cref="PollEvents"/> plays:
        /// the id of an audio asset of the file (see <see cref="ZuriFile.SoundId"/>) and how loud,
        /// 0 to 1. Returns false for an event without a sound. The runtime makes no sound itself.
        /// </summary>
        public bool TryGetEventSound(int index, out uint sound, out float volume)
        {
            float loud = 1f;
            sound = ZuriNative.zuri_instance_event_sound(Ptr, unchecked((uint)index), &loud);
            GC.KeepAlive(this);
            volume = loud;
            return sound != ZuriNative.None;
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

        /// <summary>
        /// Whether the artboard has a view-model property of this name, and its type. The name
        /// can be a path into a nested artboard: "card/title" is the property "title" of the
        /// artboard nested under the name "card" (every getter and setter here takes one);
        /// <see cref="Properties"/> lists the outer artboard's, <see cref="PropertiesIn"/>
        /// those of one nested in it.
        /// </summary>
        public bool TryGetPropertyType(string name, out ZuriPropertyType type)
        {
            byte[] utf8 = Utf8(name, out nuint length);
            uint found;
            fixed (byte* bytes = utf8)
            {
                found = ZuriNative.zuri_instance_property_type_of(Ptr, bytes, length);
            }
            GC.KeepAlive(this);
            bool any = found != ZuriNative.None;
            type = any ? (ZuriPropertyType)found : default;
            return any;
        }

        bool HasProperty(string name)
        {
            return TryGetPropertyType(name, out _);
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
