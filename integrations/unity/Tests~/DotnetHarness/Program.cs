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
            if (args.Length != 1)
            {
                Console.Error.WriteLine("usage: DotnetHarness <button.zur>");
                return 2;
            }
            try
            {
                Run(File.ReadAllBytes(args[0]));
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
                var properties = new List<string>();
                foreach (ZuriProperty property in instance.Properties)
                {
                    properties.Add(property.Name + ":" + property.Type);
                }
                Check(properties.Count == 5 && instance.Properties[3].Name == "accent" && instance.Properties[3].Type == ZuriPropertyType.Color, "properties: " + string.Join(", ", properties));

                Check(!instance.PlayStateMachine("Nope") && instance.PlayStateMachine("Button"), "play state machine \"Button\"");

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

                Check(Throws<ArgumentException>(() => instance.Render(new byte[16], Width, Height)), "a too-small buffer is rejected before the native call");
                Check(Throws<ZuriException>(() => instance.Render(pixels, 20000, 1)), "an oversized target throws the runtime's error");
            }
            Check(Throws<ObjectDisposedException>(() => instance.Advance(0f)), "use after dispose throws");

            RawBindings(zur);
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
