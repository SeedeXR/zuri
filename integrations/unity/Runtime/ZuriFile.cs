using System;
using System.Collections.Generic;
using System.Threading;

namespace Zuri
{
    /// <summary>An error reported by the Zuri runtime. The message is the runtime's own (<c>zuri_last_error</c>).</summary>
    public sealed class ZuriException : Exception
    {
        /// <summary>Creates an exception with the given message.</summary>
        public ZuriException(string message) : base(message)
        {
        }

        /// <summary>
        /// Builds an exception from the calling thread's last native error. Call it right after
        /// the failing native call, on the same thread.
        /// </summary>
        internal static ZuriException FromLastError(string what)
        {
            string detail = ZuriNative.LastError();
            return new ZuriException(detail.Length == 0 ? what : what + ": " + detail);
        }
    }

    /// <summary>
    /// A loaded <c>.zur</c> file: the immutable, shareable description of one or more artboards.
    /// Create <see cref="ZuriInstance"/>s from it to play them. Instances keep their file alive,
    /// so a file may be disposed as soon as its instances exist.
    /// </summary>
    /// <remarks>Use a file from one thread at a time.</remarks>
    public sealed unsafe class ZuriFile : IDisposable
    {
        IntPtr _ptr;
        readonly string[] _artboardNames;

        ZuriFile(IntPtr ptr)
        {
            _ptr = ptr;
            _artboardNames = ZuriNative.ReadNames(ZuriNative.ArtboardName, ptr, ZuriNative.zuri_file_artboard_count(ptr));
        }

        /// <summary>Releases the native file if <see cref="Dispose"/> was not called.</summary>
        ~ZuriFile()
        {
            Free();
        }

        /// <summary>Parses the bytes of a <c>.zur</c> file. The bytes are copied; the caller keeps ownership.</summary>
        /// <exception cref="ZuriException">The bytes are not a valid file, or the native library has a different ABI version.</exception>
        /// <exception cref="DllNotFoundException">The native library is missing for this platform.</exception>
        public static ZuriFile Load(ReadOnlySpan<byte> bytes)
        {
            uint abi = ZuriNative.zuri_abi_version();
            if (abi != ZuriNative.AbiVersion)
            {
                throw new ZuriException("The native zuri library has ABI version " + abi + " but these bindings need version " + ZuriNative.AbiVersion + ". Rebuild the plugin.");
            }
            IntPtr ptr;
            fixed (byte* data = bytes)
            {
                ptr = ZuriNative.zuri_file_load(data, (nuint)bytes.Length);
            }
            if (ptr == IntPtr.Zero)
            {
                throw ZuriException.FromLastError("Could not load the Zuri file");
            }
            return new ZuriFile(ptr);
        }

        /// <summary>Names of the artboards, in file order.</summary>
        public IReadOnlyList<string> ArtboardNames => _artboardNames;

        /// <summary>Number of image assets. The host decodes each one and hands the pixels back with <see cref="SetImage"/>.</summary>
        public int ImageCount
        {
            get
            {
                int count = (int)ZuriNative.zuri_file_image_count(Ptr);
                GC.KeepAlive(this);
                return count;
            }
        }

        /// <summary>
        /// The encoded bytes (PNG, JPEG or WebP) of image asset <paramref name="index"/>.
        /// The memory belongs to the file: do not use the span after the file is disposed, and keep the
        /// file referenced for as long as you read it (a file nothing refers to is freed when collected).
        /// </summary>
        public ReadOnlySpan<byte> GetEncodedImage(int index)
        {
            nuint length;
            byte* bytes = ZuriNative.zuri_file_image_bytes(Ptr, ImageId(index), &length);
            GC.KeepAlive(this);
            return new ReadOnlySpan<byte>(bytes, checked((int)length));
        }

        /// <summary>Number of audio assets: the sounds that events of this file play. The host decodes and plays them.</summary>
        public int SoundCount
        {
            get
            {
                int count = (int)ZuriNative.zuri_file_sound_count(Ptr);
                GC.KeepAlive(this);
                return count;
            }
        }

        /// <summary>
        /// The id of audio asset <paramref name="index"/>, which is what
        /// <see cref="ZuriInstance.TryGetEventSound"/> reports for an event that plays it.
        /// </summary>
        /// <exception cref="ArgumentOutOfRangeException">The file has no audio asset at this index.</exception>
        public uint SoundId(int index)
        {
            // A negative index wraps to one the runtime rejects.
            uint id = ZuriNative.zuri_file_sound_id(Ptr, unchecked((uint)index));
            GC.KeepAlive(this);
            if (id == ZuriNative.None)
            {
                throw new ArgumentOutOfRangeException(nameof(index), index, "The file has no audio asset at this index.");
            }
            return id;
        }

        /// <summary>
        /// The encoded bytes (WAV, MP3, OGG...) of the audio asset with this id (from <see cref="SoundId"/>
        /// or <see cref="ZuriInstance.TryGetEventSound"/>); empty if the file has no such sound.
        /// The memory belongs to the file: do not use the span after the file is disposed, and keep the
        /// file referenced for as long as you read it (a file nothing refers to is freed when collected).
        /// </summary>
        public ReadOnlySpan<byte> GetEncodedSound(uint id)
        {
            nuint length;
            byte* bytes = ZuriNative.zuri_file_sound_bytes(Ptr, id, &length);
            GC.KeepAlive(this);
            return new ReadOnlySpan<byte>(bytes, checked((int)length));
        }

        /// <summary>
        /// Supplies the decoded pixels of image asset <paramref name="index"/>: straight-alpha RGBA8,
        /// rows top to bottom, exactly <c>width * height * 4</c> bytes. Affects every instance of this file.
        /// </summary>
        /// <exception cref="ArgumentException">The span is not <c>width * height * 4</c> bytes long.</exception>
        /// <exception cref="ZuriException">The runtime rejected the image.</exception>
        public void SetImage(int index, int width, int height, ReadOnlySpan<byte> rgba)
        {
            if (width <= 0 || height <= 0 || rgba.Length != (long)width * height * 4)
            {
                throw new ArgumentException("Image pixels must be exactly width * height * 4 bytes.", nameof(rgba));
            }
            bool ok;
            fixed (byte* pixels = rgba)
            {
                ok = ZuriNative.zuri_file_set_image(Ptr, ImageId(index), (uint)width, (uint)height, pixels);
            }
            GC.KeepAlive(this);
            if (!ok)
            {
                throw ZuriException.FromLastError("Could not set image " + index);
            }
        }

        /// <summary>Creates a playable instance of the artboard at <paramref name="artboard"/>.</summary>
        /// <exception cref="ZuriException">There is no such artboard.</exception>
        public ZuriInstance CreateInstance(int artboard = 0)
        {
            // A negative index wraps to one the runtime rejects.
            IntPtr instance = ZuriNative.zuri_instance_new(Ptr, unchecked((uint)artboard));
            GC.KeepAlive(this);
            if (instance == IntPtr.Zero)
            {
                throw ZuriException.FromLastError("Could not create an instance of artboard " + artboard);
            }
            return new ZuriInstance(instance);
        }

        /// <summary>Creates a playable instance of the artboard called <paramref name="artboardName"/>.</summary>
        /// <exception cref="ZuriException">There is no artboard with that name.</exception>
        public ZuriInstance CreateInstance(string artboardName)
        {
            int index = Array.IndexOf(_artboardNames, artboardName);
            if (index < 0)
            {
                throw new ZuriException("The file has no artboard named '" + artboardName + "'.");
            }
            return CreateInstance(index);
        }

        /// <summary>Releases the native file. Instances created from it stay valid.</summary>
        public void Dispose()
        {
            Free();
            GC.SuppressFinalize(this);
        }

        IntPtr Ptr => _ptr != IntPtr.Zero ? _ptr : throw new ObjectDisposedException(nameof(ZuriFile));

        uint ImageId(int index)
        {
            // A negative index wraps to one the runtime rejects.
            uint id = ZuriNative.zuri_file_image_id(Ptr, unchecked((uint)index));
            if (id == ZuriNative.None)
            {
                throw new ArgumentOutOfRangeException(nameof(index), index, "The file has no image asset at this index.");
            }
            return id;
        }

        void Free()
        {
            IntPtr ptr = Interlocked.Exchange(ref _ptr, IntPtr.Zero);
            if (ptr != IntPtr.Zero)
            {
                ZuriNative.zuri_file_free(ptr);
            }
        }
    }
}
