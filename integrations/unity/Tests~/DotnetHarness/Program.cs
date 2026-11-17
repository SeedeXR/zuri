using System;
using System.Collections.Generic;
using System.IO;

namespace Zuri.Harness
{
    /// <summary>
    /// Drives the button example through the same wrappers Unity uses, against the real native
    /// library. Prints one line per check and exits non-zero on the first failure.
    /// </summary>
    static class Program
    {
        const int Width = 320;
        const int Height = 160;

        static int Main(string[] args)
        {
            if (args.Length != 4)
            {
                Console.Error.WriteLine("usage: DotnetHarness <button.zur> <enum.zur> <sound.zur> <scroll.zur>");
                return 2;
            }
            try
            {
                Run(File.ReadAllBytes(args[0]));
                Keys(File.ReadAllBytes(args[0]));
                Enums(File.ReadAllBytes(args[1]));
                Sounds(File.ReadAllBytes(args[2]));
                Scrolls(File.ReadAllBytes(args[3]));
            }
            catch (Exception e)
            {
                Console.Error.WriteLine("FAILED: " + e);
                return 1;
            }
            Console.WriteLine("OK");
            return 0;
        }

        static void Run(byte[] zur)
        {
            // Errors are exceptions carrying the runtime's message.
            try
            {
                ZuriFile.Load(new byte[] { (byte)'n', (byte)'o', (byte)'p', (byte)'e' });
                throw new Exception("loading garbage did not throw");
            }
            catch (ZuriException e)
            {
                Check(e.Message.Contains("ZURI"), "garbage is rejected: \"" + e.Message + "\"");
            }

            ZuriInstance instance;
            using (ZuriFile file = ZuriFile.Load(zur))
            {
                Check(file.ArtboardNames.Count == 1 && file.ArtboardNames[0] == "Button", "artboards: " + string.Join(", ", file.ArtboardNames));
                Check(file.ImageCount == 0, "image assets: 0");
                Check(Throws<ZuriException>(() => file.CreateInstance(7)), "a missing artboard index throws");
                Check(Throws<ZuriException>(() => file.CreateInstance("Nope")), "a missing artboard name throws");
                instance = file.CreateInstance("Button");
            }

            // The file is disposed: an instance keeps it alive.
            using (instance)
            {
                Check(instance.Width == 320f && instance.Height == 160f, "size: " + instance.Width + " x " + instance.Height);
                Check(instance.Animations.Count == 5, "animations: " + string.Join(", ", instance.Animations));
                Check(instance.StateMachines.Count == 1 && instance.StateMachines[0] == "Button", "state machines: " + string.Join(", ", instance.StateMachines));
                Check(instance.DeclaredEvents.Count == 1 && instance.DeclaredEvents[0] == "clicked", "declared events: " + string.Join(", ", instance.DeclaredEvents));
                var properties = new List<string>();
                foreach (ZuriProperty property in instance.Properties)
                {
                    properties.Add(property.Name + ":" + property.Type);
                }
                Check(properties.Count == 5 && instance.Properties[3].Name == "accent" && instance.Properties[3].Type == ZuriPropertyType.Color, "properties: " + string.Join(", ", properties));
                Check(instance.TryGetPropertyType("accent", out ZuriPropertyType found) && found == ZuriPropertyType.Color && !instance.TryGetPropertyType("nobody/accent", out ZuriPropertyType none) && none == default, "a property is found by name, with its type: " + found);

                Check(!instance.PlayStateMachine("Nope") && instance.PlayStateMachine("Button"), "play state machine \"Button\"");
                uint[] layers = instance.LayerStates;
                Check(layers.Length > 0 && layers[0] != uint.MaxValue, "every layer is in a state: " + string.Join(", ", layers));
                Check(!instance.Seek(1f), "seek is refused while a state machine plays");

                var pixels = new byte[Width * Height * 4];
                Check(instance.Advance(0f), "first advance reports a change");
                instance.Render(pixels, Width, Height);
                Check(Pixel(pixels, 100, 80) == (0x4f, 0x7c, 0xff, 0xff), "pixel (100, 80) is the button's blue #4f7cff: " + Pixel(pixels, 100, 80));

                bool active = true;
                for (int i = 0; i < 30; i++)
                {
                    active = instance.Advance(1f / 60f);
                }
                Check(!active, "settles: advance returns false after 0.5 s idle");

                instance.Pointer(ZuriPointerEvent.Move, 160, 80);
                Check(instance.GetBool("hover"), "pointer move sets hover");
                instance.Pointer(ZuriPointerEvent.Down, 160, 80);
                Check(instance.GetBool("pressed"), "pointer down sets pressed");
                instance.Pointer(ZuriPointerEvent.Up, 160, 80);
                Check(instance.GetNumber("clicks") == 1f, "pointer up counts a click: clicks = " + instance.GetNumber("clicks"));
                Check(instance.Advance(1f / 60f), "the click wakes the instance");
                var events = new List<string>();
                Check(instance.PollEvents(events) == 1 && events[0] == "clicked", "one event: " + string.Join(", ", events));
                Check(instance.PollEvents(events) == 0 && events.Count == 0, "events are delivered once");

                Check(instance.SetColor("accent", 0xffff3366u) && instance.GetColor("accent") == 0xffff3366u, "accent = 0xffff3366");
                Check(!instance.SetColor("missing", 0) && Throws<ZuriException>(() => instance.GetColor("missing")), "a missing property: set returns false, get throws");

                // Read back what objects draw with. Object ids are file indices; schema key 3 is "x", 15 is "color".
                uint shape = 0;
                float x = 0f;
                while (shape < 1000 && !instance.TryGetProperty(shape, 3, out x))
                {
                    shape++;
                }
                Check(x == 160f && !instance.TryGetPropertyColor(shape, 3, out _), "object " + shape + " draws at x = " + x + ", which is not a color");
                instance.Advance(0f);
                uint fill = 0;
                uint argb = 0;
                while (fill < 1000 && !(instance.TryGetPropertyColor(fill, 15, out argb) && argb == 0xffff3366u))
                {
                    fill++;
                }
                Check(argb == 0xffff3366u && !instance.TryGetProperty(fill, 15, out _), "object " + fill + " draws with the bound accent, which is not a number");
                Check(!instance.TryGetProperty(uint.MaxValue, 3, out _), "an unknown object has no properties");
                Check(instance.SetNumber("clicks", 41f) && instance.GetNumber("clicks") == 41f, "clicks = 41");
                Check(instance.Fire("pulse") && !instance.Fire("missing"), "fire pulse");
                Check(!instance.SetString("missing", "héllo") && Throws<ZuriException>(() => instance.GetString("missing")), "a missing string property: set returns false, get throws");

                long before = GC.GetAllocatedBytesForCurrentThread();
                for (int i = 0; i < 1000; i++)
                {
                    instance.SetNumber("clicks", i);
                    instance.SetColor("accent", 0xffff3366u);
                    instance.Advance(1f / 60f);
                    instance.Render(pixels, Width, Height, ZuriFit.Contain, 0xff000000u, true);
                }
                long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
                Check(allocated == 0, "1000 frames of set + advance + render allocate " + allocated + " managed bytes");

                var after = Pixel(pixels, 100, 80);
                Check((after.r, after.g, after.b) == (0xff, 0x33, 0x66), "pixel (100, 80) follows accent: " + after);
                Check(pixels[3] == 0xff, "the background was applied");

                // Fit and alignment: the artboard at its own size in the bottom-right corner of a surface twice as large.
                var large = new byte[Width * 2 * Height * 2 * 4];
                instance.Render(large, Width * 2, Height * 2, ZuriFit.None, alignment: ZuriAlignment.BottomRight);
                Check(large[(80 * Width * 2 + 100) * 4 + 3] == 0 && large[((Height + 80) * Width * 2 + Width + 100) * 4 + 3] == 0xff, "fit None + BottomRight: the top-left quadrant is transparent, the bottom-right one is drawn");

                Check(instance.PlayAnimation("idle") && instance.Seek(0.5f) && instance.LayerStates.Length == 0, "an animation can be sought and has no layers");

                Check(Throws<ArgumentException>(() => instance.Render(new byte[16], Width, Height)), "a too-small buffer is rejected before the native call");
                Check(Throws<ZuriException>(() => instance.Render(pixels, 20000, 1)), "an oversized target throws the runtime's error");
            }
            Check(Throws<ObjectDisposedException>(() => instance.Advance(0f)), "use after dispose throws");

            RawBindings(zur);
        }

        // The button has one key listener: Enter going down adds 1 to clicks and fires pulse.
        static void Keys(byte[] zur)
        {
            using (ZuriFile file = ZuriFile.Load(zur))
            using (ZuriInstance instance = file.CreateInstance())
            {
                instance.Autoplay();
                instance.Advance(1f);
                var events = new List<string>();
                Check(!instance.Key("q", true) && !instance.Key("Enter", false) && instance.GetNumber("clicks") == 0f, "key: q down and Enter up have no listener");
                Check(!instance.Advance(1f / 60f), "key: a key without a listener does not wake the instance");
                Check(instance.Key("Enter", true) && instance.GetNumber("clicks") == 1f, "key: Enter down reacts, clicks = " + instance.GetNumber("clicks"));
                Check(instance.Advance(1f / 60f) && instance.PollEvents(events) == 1 && events[0] == "clicked", "key: the advance after it reports: " + string.Join(", ", events));
                Check(Throws<ArgumentNullException>(() => instance.Key(null, true)), "key: a null key throws");
            }
        }

        /// <summary>A Layout that scrolls, and how far a press may go before it does.</summary>
        static void Scrolls(byte[] zur)
        {
            using (ZuriFile file = ZuriFile.Load(zur))
            using (ZuriInstance instance = file.CreateInstance())
            {
                instance.Autoplay();
                instance.Advance(0f);
                // Nothing is rendered: a pixel is a unit of the artboard. A press that goes 6 pixels up.
                Func<float> drag = () =>
                {
                    instance.Pointer(ZuriPointerEvent.Down, 40f, 40f);
                    instance.Pointer(ZuriPointerEvent.Move, 40f, 34f);
                    instance.Advance(0f);
                    instance.Pointer(ZuriPointerEvent.Up, 40f, 34f);
                    instance.Advance(0f);
                    return instance.GetNumber("scrolled");
                };
                float tap = drag();
                Check(tap == 0f, "scroll: a press that goes 6 pixels is a tap while the threshold is 8: scrolled " + tap);
                instance.SetDragThreshold(2f);
                float scrolled = drag();
                Check(scrolled == 4f, "scroll: with a threshold of 2 it scrolls by the 4 beyond: " + scrolled);
                // The scene has one pointer and one button: a host with several says which each event is of.
                string taken = string.Join(" ", instance.Pointer(ZuriPointerEvent.Down, 40f, 40f, 7), instance.Pointer(ZuriPointerEvent.Down, 40f, 40f, 8), instance.Pointer(ZuriPointerEvent.Down, 40f, 40f, 7, 2), instance.Pointer(ZuriPointerEvent.Exit, 0f, 0f, 7), instance.Pointer(ZuriPointerEvent.Down, 40f, 40f, 8));
                Check(taken == "True False False True True", "pointer: a press, a pointer beside it, another button, the pressing pointer leaving, the other pressing: " + taken);
            }
        }

        // examples/sound.json: firing "tap" reports the event "tapped", which plays the file's one
        // sound (a 60 ms click: 1,323 samples of 16-bit mono at 22,050 Hz) at volume 0.8.
        static void Sounds(byte[] zur)
        {
            using (ZuriFile file = ZuriFile.Load(zur))
            using (ZuriInstance instance = file.CreateInstance())
            {
                instance.Autoplay();
                instance.Advance(0f);
                var events = new List<string>();
                Check(file.SoundCount == 1 && file.GetEncodedSound(file.SoundId(0)).Length == 2690, "sound: the file has one sound of " + file.GetEncodedSound(file.SoundId(0)).Length + " bytes");
                Check(Throws<ArgumentOutOfRangeException>(() => file.SoundId(1)) && Throws<ArgumentOutOfRangeException>(() => file.SoundId(-1)), "sound: no sound at another index");
                Check(file.GetEncodedSound(file.SoundId(0) + 1).IsEmpty && file.GetEncodedSound(uint.MaxValue).IsEmpty, "sound: an id that is not a sound's has no bytes");
                instance.Fire("tap");
                instance.Advance(1f / 60f);
                Check(instance.PollEvents(events) == 1 && events[0] == "tapped", "sound: the tap is reported: " + string.Join(", ", events));
                bool has = instance.TryGetEventSound(0, out uint sound, out float volume);
                Check(has && sound == file.SoundId(0) && volume == 0.8f, "sound: the event plays sound " + sound + " at volume " + volume);
                Check(!instance.TryGetEventSound(1, out sound, out volume) && !instance.TryGetEventSound(-1, out sound, out volume), "sound: an event that was not reported has none");

                bool decoded = ZuriWav.TryDecode(file.GetEncodedSound(file.SoundId(0)), out float[] samples, out int channels, out int sampleRate);
                Check(decoded && samples.Length == 1323 && channels == 1 && sampleRate == 22050, "wav: " + samples?.Length + " samples, " + channels + " channel, " + sampleRate + " Hz");
                // Each sample size, read by hand: 16-bit, 8-bit (128 is silence), 24-bit and float.
                Check(ZuriWav.TryDecode(Wav(1, 16, new byte[] { 0, 0, 0, 0x40, 0, 0x80 }), out samples, out channels, out sampleRate) && samples[0] == 0f && samples[1] == 0.5f && samples[2] == -1f, "wav: 16-bit");
                Check(ZuriWav.TryDecode(Wav(1, 8, new byte[] { 128, 255, 0 }), out samples, out channels, out sampleRate) && samples[0] == 0f && samples[1] > 0.99f && samples[2] == -1f, "wav: 8-bit");
                Check(ZuriWav.TryDecode(Wav(1, 24, new byte[] { 0, 0, 0x40, 0, 0, 0x80 }), out samples, out channels, out sampleRate) && samples[0] == 0.5f && samples[1] == -1f, "wav: 24-bit");
                Check(ZuriWav.TryDecode(Wav(3, 32, BitConverter.GetBytes(0.25f)), out samples, out channels, out sampleRate) && samples[0] == 0.25f, "wav: 32-bit float");
                // Not a sound this reads: nothing is thrown.
                Check(!ZuriWav.TryDecode(Wav(85, 16, new byte[4]), out samples, out channels, out sampleRate), "wav: a compressed format is refused");
                Check(!ZuriWav.TryDecode(new byte[] { 1, 2, 3 }, out samples, out channels, out sampleRate) && !ZuriWav.TryDecode(file.GetEncodedSound(file.SoundId(0)).Slice(0, 40), out samples, out channels, out sampleRate), "wav: junk and a cut-off file are refused");
            }
        }

        // A mono WAV file at 8,000 Hz holding `data`.
        static byte[] Wav(ushort format, ushort bits, byte[] data)
        {
            var wav = new MemoryStream();
            var w = new BinaryWriter(wav);
            w.Write(new[] { (byte)'R', (byte)'I', (byte)'F', (byte)'F' });
            w.Write(36 + data.Length);
            w.Write(new[] { (byte)'W', (byte)'A', (byte)'V', (byte)'E', (byte)'f', (byte)'m', (byte)'t', (byte)' ' });
            w.Write(16);
            w.Write(format);
            w.Write((ushort)1);
            w.Write(8000);
            w.Write(8000 * bits / 8);
            w.Write((ushort)(bits / 8));
            w.Write(bits);
            w.Write(new[] { (byte)'d', (byte)'a', (byte)'t', (byte)'a' });
            w.Write(data.Length);
            w.Write(data);
            return wav.ToArray();
        }

        // Tests~/Fixtures/enum.json: one enum property, "mode", with the Options idle, busy and done, set to busy,
        // and an artboard nested as "card" with a string, "title", and an enum, "size".
        static void Enums(byte[] zur)
        {
            using (ZuriFile file = ZuriFile.Load(zur))
            using (ZuriInstance instance = file.CreateInstance())
            {
                ZuriProperty mode = instance.Properties[0];
                Check(mode.Name == "mode" && mode.Type == ZuriPropertyType.Enum && string.Join(", ", mode.Options) == "idle, busy, done", "enum: " + mode.Name + ":" + mode.Type + " with the Options " + string.Join(", ", mode.Options));
                Check(instance.GetNumber("mode") == 1f && instance.GetString("mode") == "busy", "enum: starts as Option 1, \"busy\"");
                Check(instance.SetString("mode", "done") && instance.GetNumber("mode") == 2f, "enum: set by name, read by position");
                Check(instance.SetNumber("mode", 0f) && instance.GetString("mode") == "idle", "enum: set by position, read by name");
                Check(!instance.SetString("mode", "nope") && instance.GetString("mode") == "idle", "enum: a name that is not an Option is refused");
                // The artboard nested in it as "card" has a string and an enum of its own, listed by its path.
                IReadOnlyList<ZuriProperty> card = instance.PropertiesIn("card");
                Check(card.Count == 2 && card[0].Name == "title" && card[0].Type == ZuriPropertyType.String && card[0].Options.Length == 0, "nested: " + card.Count + " properties in \"card\", the first a string");
                Check(card.Count == 2 && card[1].Name == "size" && card[1].Type == ZuriPropertyType.Enum && string.Join(", ", card[1].Options) == "small, large", "nested: an enum there lists its Options");
                Check(instance.PropertiesIn("").Count == 1 && instance.PropertiesIn("nobody").Count == 0 && instance.PropertiesIn("card/title").Count == 0, "nested: \"\" is the instance's own artboard, and a path that names no artboard lists nothing");
            }
        }

        // The C functions the wrappers do not use yet, called directly so that every
        // signature in ZuriNative.cs is exercised (the JSON ones need a "json" build).
        static unsafe void RawBindings(byte[] zur)
        {
            byte* block = ZuriNative.zuri_alloc(64);
            Check(block != null && block[0] == 0 && block[63] == 0, "raw: zuri_alloc returns zeroed memory");
            ZuriNative.zuri_free(block, 64);

            IntPtr file;
            fixed (byte* bytes = zur)
            {
                file = ZuriNative.zuri_file_load(bytes, (nuint)zur.Length);
            }
            IntPtr instance = ZuriNative.zuri_instance_new(file, 0);
            Check(instance != IntPtr.Zero, "raw: file and instance created");
            Check(ZuriNative.zuri_file_shader_source(file, 0, 0, null, 0) == 0, "raw: object 0 is not a shader asset");
            ZuriNative.zuri_file_free(file);
            ZuriNative.zuri_instance_autoplay(instance);
            ZuriNative.zuri_instance_advance(instance, 0f);

            var pixels = new byte[Width * Height * 4];
            float* view = stackalloc float[6] { 2f, 0f, 0f, 2f, 0f, 0f };
            fixed (byte* rgba = pixels)
            {
                Check(ZuriNative.zuri_instance_render_view(instance, rgba, Width, Height, view, 0, false), "raw: render_view at 2x");
                Check(!ZuriNative.zuri_instance_set_shader_texture(instance, 0, 0, 0, rgba), "raw: an empty shader texture is refused");
            }
            Check(Pixel(pixels, 200, 160 - 1) == (0x4f, 0x7c, 0xff, 0xff), "raw: the 2x view moved the button: " + Pixel(pixels, 200, 159));

            uint hit = ZuriNative.zuri_instance_hit_test(instance, 320f, 160f);
            float* box = stackalloc float[4];
            float* matrix = stackalloc float[6];
            Check(hit != ZuriNative.None && ZuriNative.zuri_instance_bounds(instance, hit, box) && box[0] < 160f && box[2] > 160f, "raw: hit_test finds object " + hit + ", bounds x " + box[0] + ".." + box[2]);
            Check(ZuriNative.zuri_instance_world_matrix(instance, hit, matrix) && matrix[4] == 160f && matrix[5] == 80f, "raw: world_matrix translation (" + matrix[4] + ", " + matrix[5] + ")");
            Check(ZuriNative.zuri_instance_hit_test(instance, 2f, 2f) == ZuriNative.None, "raw: nothing at (2, 2)");

            ZuriNative.ShaderRequest request;
            Check(ZuriNative.zuri_instance_shader_request_count(instance) == 0 && !ZuriNative.zuri_instance_shader_request(instance, 0, &request), "raw: no shader requests");
            Check(sizeof(ZuriNative.ShaderRequest) == 36, "raw: ZuriShaderRequest is 36 bytes");
            ZuriNative.zuri_instance_free(instance);
        }

        static (int r, int g, int b, int a) Pixel(byte[] pixels, int x, int y)
        {
            int i = (y * Width + x) * 4;
            return (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]);
        }

        static bool Throws<T>(Action action) where T : Exception
        {
            try
            {
                action();
                return false;
            }
            catch (T)
            {
                return true;
            }
        }

        static void Check(bool condition, string what)
        {
            if (!condition)
            {
                throw new Exception("check failed: " + what);
            }
            Console.WriteLine("ok  " + what);
        }
    }
}
