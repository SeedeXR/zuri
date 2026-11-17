using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Zuri
{
    /// <summary>
    /// Raw P/Invoke bindings for the Zuri C API (<c>crates/zuri-ffi/include/zuri.h</c>, ABI version 1).
    /// One declaration per C function, same names, blittable arguments only.
    /// Use <see cref="ZuriFile"/> and <see cref="ZuriInstance"/> instead of calling these directly.
    /// </summary>
    /// <remarks>
    /// Conventions of the C API: handles are opaque pointers (NULL is always accepted); strings in
    /// are UTF-8 pointer + byte length; "string out" functions write at most <c>cap</c> bytes and
    /// return the full length; colors are 0xAARRGGBB; pixels are RGBA8 with rows top to bottom.
    /// </remarks>
    internal static unsafe class ZuriNative
    {
#if (UNITY_IOS || UNITY_TVOS || UNITY_WEBGL) && !UNITY_EDITOR
        // Statically linked into the player.
        const string Lib = "__Internal";
#else
        const string Lib = "zuri";
#endif

        /// <summary>The ABI version these bindings were written against.</summary>
        public const uint AbiVersion = 3;

        /// <summary><c>ZURI_NONE</c>: "no object" / "out of range".</summary>
        public const uint None = uint.MaxValue;

        /// <summary><c>ZuriShaderRequest</c>: a shader paint the host should draw.</summary>
        [StructLayout(LayoutKind.Sequential)]
        public struct ShaderRequest
        {
            public uint Paint;
            public uint Asset;
            public uint Width;
            public uint Height;
            public fixed float Params[4];
            public float Time;
        }

        // ---- memory, version, errors ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_alloc(nuint len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern void zuri_free(byte* ptr, nuint len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_last_error(byte* buf, nuint cap);

        // ---- files ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr zuri_file_load(byte* bytes, nuint len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern void zuri_file_free(IntPtr file);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_file_artboard_count(IntPtr file);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_file_artboard_name(IntPtr file, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_file_image_count(IntPtr file);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_file_image_id(IntPtr file, uint index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_file_image_bytes(IntPtr file, uint id, nuint* outLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_file_sound_count(IntPtr file);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_file_sound_id(IntPtr file, uint index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_file_sound_bytes(IntPtr file, uint id, nuint* outLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_file_set_image(IntPtr file, uint id, uint width, uint height, byte* rgba);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_file_shader_source(IntPtr file, uint asset, uint language, byte* buf, nuint cap);

        // ---- instances ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr zuri_instance_new(IntPtr file, uint artboard);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern void zuri_instance_free(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern float zuri_instance_width(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern float zuri_instance_height(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_animation_count(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_animation_name(IntPtr inst, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_state_machine_count(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_state_machine_name(IntPtr inst, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_play_animation(IntPtr inst, uint index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_play_state_machine(IntPtr inst, uint index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern void zuri_instance_autoplay(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_seek(IntPtr inst, float seconds);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_layer_count(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_layer_state(IntPtr inst, uint layer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_advance(IntPtr inst, float dt);

        // ---- view model (data binding) ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_property_count(IntPtr inst, byte* within, nuint withinLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_property_name(IntPtr inst, byte* within, nuint withinLen, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_property_type_of(IntPtr inst, byte* name, nuint nameLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_property_option_count(IntPtr inst, byte* name, nuint nameLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_property_option_name(IntPtr inst, byte* name, nuint nameLen, uint option, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_set_number(IntPtr inst, byte* name, nuint nameLen, float value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_get_number(IntPtr inst, byte* name, nuint nameLen, float* value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_set_color(IntPtr inst, byte* name, nuint nameLen, uint argb);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_get_color(IntPtr inst, byte* name, nuint nameLen, uint* argb);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_set_string(IntPtr inst, byte* name, nuint nameLen, byte* value, nuint valueLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_get_string(IntPtr inst, byte* name, nuint nameLen, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_fire(IntPtr inst, byte* name, nuint nameLen);

        // ---- input and events ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_pointer(IntPtr inst, uint kind, float x, float y, uint pointer, uint button);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern void zuri_instance_set_drag_threshold(IntPtr inst, float pixels);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_wheel(IntPtr inst, float x, float y, float dx, float dy);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_key(IntPtr inst, byte* key, nuint keyLen, [MarshalAs(UnmanagedType.U1)] bool down);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_declared_event_count(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_declared_event_name(IntPtr inst, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_poll_events(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern nuint zuri_instance_event_name(IntPtr inst, uint index, byte* buf, nuint cap);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_event_sound(IntPtr inst, uint index, float* outVolume);

        // ---- rendering ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_render(IntPtr inst, byte* rgba, uint width, uint height, uint fit, uint background, [MarshalAs(UnmanagedType.U1)] bool premultiplied);

        /// <summary><paramref name="view"/> points at 6 floats: <c>[a, b, c, d, tx, ty]</c>.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_render_view(IntPtr inst, byte* rgba, uint width, uint height, float* view, uint background, [MarshalAs(UnmanagedType.U1)] bool premultiplied);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_shader_request_count(IntPtr inst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_shader_request(IntPtr inst, uint index, ShaderRequest* request);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_set_shader_texture(IntPtr inst, uint paint, uint width, uint height, byte* rgba);

        // ---- editor helpers ----

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern uint zuri_instance_hit_test(IntPtr inst, float x, float y);

        /// <summary><paramref name="bounds"/> receives 4 floats: <c>[x0, y0, x1, y1]</c>.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_bounds(IntPtr inst, uint obj, float* bounds);

        /// <summary><paramref name="matrix"/> receives 6 floats: <c>[a, b, c, d, tx, ty]</c>.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_world_matrix(IntPtr inst, uint obj, float* matrix);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_get_property(IntPtr inst, uint obj, uint key, float* value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        public static extern bool zuri_instance_get_property_color(IntPtr inst, uint obj, uint key, uint* argb);

#if ZURI_JSON
        // ---- authoring ----
        // Exported only by libraries built with the "json" cargo feature, which the player plugin
        // is not. Define ZURI_JSON when shipping such a build; without it these stay undeclared
        // so that static linking ("__Internal") never asks for symbols that are not there.

        /// <summary>Returns a buffer to release with <c>zuri_free(ptr, *outLen)</c>, or NULL.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_json_to_zur(byte* json, nuint len, nuint* outLen);

        /// <summary>Returns a buffer to release with <c>zuri_free(ptr, *outLen)</c>, or NULL.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_zur_to_json(byte* bytes, nuint len, nuint* outLen);

        /// <summary>Returns a buffer to release with <c>zuri_free(ptr, *outLen)</c>.</summary>
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        public static extern byte* zuri_schema_json(nuint* outLen);
#endif

        // ---- helpers shared by the wrappers ----

        /// <summary>The shape of every indexed "string out" function.</summary>
        public delegate nuint IndexedName(IntPtr handle, uint index, byte* buf, nuint cap);

        public static readonly IndexedName ArtboardName = (handle, index, buf, cap) => zuri_file_artboard_name(handle, index, buf, cap);
        public static readonly IndexedName AnimationName = (handle, index, buf, cap) => zuri_instance_animation_name(handle, index, buf, cap);
        public static readonly IndexedName StateMachineName = (handle, index, buf, cap) => zuri_instance_state_machine_name(handle, index, buf, cap);
        public static readonly IndexedName DeclaredEventName = (handle, index, buf, cap) => zuri_instance_declared_event_name(handle, index, buf, cap);
        public static readonly IndexedName EventName = (handle, index, buf, cap) => zuri_instance_event_name(handle, index, buf, cap);

        /// <summary>Reads one name through a "string out" function: measure, then read.</summary>
        public static string ReadName(IndexedName function, IntPtr handle, uint index)
        {
            // An empty array pins to NULL, which the C API accepts with cap = 0.
            var buffer = new byte[checked((int)function(handle, index, null, 0))];
            fixed (byte* bytes = buffer)
            {
                function(handle, index, bytes, (nuint)buffer.Length);
            }
            return Encoding.UTF8.GetString(buffer);
        }

        /// <summary>Reads <c>count</c> names through a "string out" function.</summary>
        public static string[] ReadNames(IndexedName function, IntPtr handle, uint count)
        {
            var names = new string[count];
            for (uint i = 0; i < count; i++)
            {
                names[i] = ReadName(function, handle, i);
            }
            return names;
        }

        /// <summary>The calling thread's last error message, or an empty string.</summary>
        public static string LastError()
        {
            var buffer = new byte[checked((int)zuri_last_error(null, 0))];
            fixed (byte* bytes = buffer)
            {
                zuri_last_error(bytes, (nuint)buffer.Length);
            }
            return Encoding.UTF8.GetString(buffer);
        }
    }
}
