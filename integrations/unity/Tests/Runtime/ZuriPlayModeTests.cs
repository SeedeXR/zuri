using System.Collections;
using System.Collections.Generic;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.Rendering;
using UnityEngine.TestTools;
using UnityEngine.UI;

namespace Zuri.Tests
{
    /// <summary>
    /// What the EditMode tests cannot see: the pixels a camera draws for a ZuriRawImage and a
    /// ZuriQuad, read back from a render texture, and pointer input found by the EventSystem's
    /// raycasters. Every test needs a graphics device and is skipped under -nographics.
    /// </summary>
    public sealed class ZuriPlayModeTests
    {
        const string Fixtures = "Packages/com.seedexr.zuri/Tests/Editor/";

        // The camera's target. Canvas and world units are laid out so that one pixel of a
        // 320 x 160 artboard is one screen pixel.
        const int Width = 640, Height = 320;
        // The view's left edge and its top edge (screen y points up). Off-centre, so a picture
        // that is mirrored or drawn somewhere else cannot pass.
        const int Left = 260, Top = 280;

        static readonly Color32 Blue = new Color32(0x4f, 0x7c, 0xff, 0xff);
        static readonly Color32 ArtboardFill = new Color32(0x0f, 0x12, 0x20, 0xff);

        readonly List<Object> _created = new List<Object>();
        Camera _camera;
        Texture2D _readback;

        [OneTimeSetUp]
        public void ReportTheDevice()
        {
            // scripts/verify-engines.sh reads this line to say what the tests ran on.
            Debug.Log("Zuri PlayMode tests: device=" + SystemInfo.graphicsDeviceType + " (" + SystemInfo.graphicsDeviceName + "), colorSpace=" + QualitySettings.activeColorSpace);
        }

        [SetUp]
        public void MakeTheCamera()
        {
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
            {
                Assert.Ignore("Needs a graphics device: run Unity without -nographics.");
            }
            _camera = Make<Camera>("Camera");
            _camera.transform.position = new Vector3(0f, 0f, -5f);
            _camera.orthographic = true;
            _camera.orthographicSize = 1f; // 2 units high: 160 pixels per unit
            _camera.clearFlags = CameraClearFlags.SolidColor;
            _camera.backgroundColor = Color.black;
            _camera.allowHDR = false;
            // An sRGB target in a linear project, so the bytes read back are display values in both color spaces.
            _camera.targetTexture = Keep(new RenderTexture(Width, Height, 24));
            _readback = Keep(new Texture2D(Width, Height, TextureFormat.RGBA32, false));
        }

        [TearDown]
        public void DestroyCreatedObjects()
        {
            foreach (Object created in _created)
            {
                Object.DestroyImmediate(created);
            }
            _created.Clear();
        }

        [UnityTest]
        public IEnumerator RawImage_IsDrawnUpright()
        {
            // A 2 x 2 image, red over blue, stretched over the view.
            ShowOnCanvas("image.zur");
            yield return null;
            Color32[] screen = Capture();

            AssertColor(new Color32(255, 0, 0, 255), Seen(screen, 160, 20), "the top of the view");
            AssertColor(new Color32(0, 0, 255, 255), Seen(screen, 160, 140), "the bottom of the view");
        }

        [UnityTest]
        public IEnumerator RawImage_ShowsTheArtboardsColors()
        {
            ShowOnCanvas("button.zur");
            yield return null;
            Color32[] screen = Capture();

            AssertColor(Blue, Seen(screen, 100, 80), "the button");
            AssertColor(ArtboardFill, Seen(screen, 2, 2), "the artboard's fill");
            AssertColor(Color.black, Seen(screen, -4, 80), "left of the view");
            AssertColor(Color.black, Seen(screen, 160, -4), "above the view");
        }

        [UnityTest]
        public IEnumerator RawImage_HoversAndClicksThroughTheGraphicRaycaster()
        {
            ZuriPlayer player = ShowOnCanvas("button.zur");
            yield return null;

            yield return HoverClickAndSeeTheTick(player);
        }

        // Needs the Physics module in the project (collider, PhysicsRaycaster).
        [UnityTest]
        public IEnumerator Quad_IsDrawnUprightAndClickedThroughThePhysicsRaycaster()
        {
            // Unity's quad is 1 x 1: scaled and placed so that it covers the same pixels as the canvas view.
            GameObject quadObject = Keep(GameObject.CreatePrimitive(PrimitiveType.Quad));
            quadObject.transform.position = new Vector3(0.625f, 0.25f, 0f);
            quadObject.transform.localScale = new Vector3(2f, 1f, 1f);
            Shader unlit = Shader.Find("Unlit/Texture");
            Assert.IsNotNull(unlit, "the built-in Unlit/Texture shader");
            quadObject.GetComponent<Renderer>().sharedMaterial = Keep(new Material(unlit));
            var player = quadObject.AddComponent<ZuriPlayer>();
            quadObject.AddComponent<ZuriQuad>();
            player.Asset = Load("button.zur");
            _camera.gameObject.AddComponent<PhysicsRaycaster>();
            // The collider is in the physics scene after a physics step.
            yield return new WaitForFixedUpdate();
            Color32[] screen = Capture();

            AssertColor(Blue, Seen(screen, 100, 80), "the button");
            AssertColor(ArtboardFill, Seen(screen, 2, 2), "the artboard's fill");
            AssertColor(Color.black, Seen(screen, -4, 80), "left of the quad");

            yield return HoverClickAndSeeTheTick(player);
        }

        // scroll.zur: an 80 x 120 artboard with a Layout that scrolls, at (10, 10) to (70, 60), and
        // "scrolled" saying how far. The 320 x 160 view stretches its 80 x 120 texture: 0.75 of a
        // texture pixel to the screen pixel down the screen, 0.25 across.
        [UnityTest]
        public IEnumerator RawImage_ScrollsAfterTheDragThresholdAndFollowsOnePointer()
        {
            ZuriPlayer player = ShowOnCanvas("scroll.zur");
            yield return null;
            // 20 screen pixels are 15 of the texture down the screen: more than the 8 an instance starts with.
            Make<EventSystem>("EventSystem").pixelDragThreshold = 20;
            GameObject view = player.gameObject;

            // The pointer `id` with `button`, `up` screen pixels above (40, 40) of the artboard.
            PointerEventData At(int id, PointerEventData.InputButton button, float up)
            {
                PointerEventData pointer = Raycast(ScreenPoint(160f, 160f / 3f - up));
                pointer.pointerId = id;
                pointer.button = button;
                return pointer;
            }
            float Scrolled()
            {
                player.Tick(0f);
                return player.GetNumber("scrolled");
            }
            // A press that goes `up` screen pixels and is let go.
            float Stroke(int id, PointerEventData.InputButton button, float up)
            {
                ExecuteEvents.Execute(view, At(id, button, 0f), ExecuteEvents.pointerDownHandler);
                ExecuteEvents.Execute(view, At(id, button, up), ExecuteEvents.pointerMoveHandler);
                float scrolled = Scrolled();
                ExecuteEvents.Execute(view, At(id, button, up), ExecuteEvents.pointerUpHandler);
                return scrolled;
            }

            const PointerEventData.InputButton left = PointerEventData.InputButton.Left, right = PointerEventData.InputButton.Right;
            Assert.AreEqual(0f, Stroke(1, left, 16f), 0.01f, "16 screen pixels are 12 of the texture: within the EventSystem's threshold, a tap");
            Assert.AreEqual(6f, Stroke(1, left, 28f), 0.01f, "28 are 21: a scroll by the 6 beyond");
            Assert.AreEqual(6f, Stroke(1, right, 40f), 0.01f, "another button is not a press");

            // A second finger that comes down and lifts in the middle of a press is not the scene's:
            // the first goes on scrolling, from where it pressed.
            ExecuteEvents.Execute(view, At(1, left, 0f), ExecuteEvents.pointerDownHandler);
            ExecuteEvents.Execute(view, At(2, left, 30f), ExecuteEvents.pointerDownHandler);
            ExecuteEvents.Execute(view, At(1, left, 28f), ExecuteEvents.pointerMoveHandler);
            Assert.AreEqual(12f, Scrolled(), 0.01f, "the first finger's 28");
            ExecuteEvents.Execute(view, At(2, left, 30f), ExecuteEvents.pointerUpHandler);
            ExecuteEvents.Execute(view, At(1, left, 36f), ExecuteEvents.pointerMoveHandler);
            Assert.AreEqual(18f, Scrolled(), 0.01f, "and its 36, after the second lifted");
            ExecuteEvents.Execute(view, At(1, left, 36f), ExecuteEvents.pointerUpHandler);
            Assert.AreEqual(24f, Stroke(2, left, 28f), 0.01f, "the second finger can press on its own afterwards");

            // The wheel, as a ScrollRect takes it: a unit of it down moves the content up by the
            // sensitivity, 4 screen pixels, which are 3 of the texture. One that moves nothing (under
            // the list, at 100 of the artboard) goes on to what the view is in.
            view.GetComponent<ZuriPointerSurface>().ScrollSensitivity = 4f;
            Heard around = view.transform.parent.gameObject.AddComponent<Heard>();
            float Wheel(float up, float y)
            {
                PointerEventData wheel = Raycast(ScreenPoint(160f, y * 160f / 120f));
                wheel.scrollDelta = new Vector2(0f, up);
                ExecuteEvents.Execute(view, wheel, ExecuteEvents.scrollHandler);
                return Scrolled();
            }
            Assert.AreEqual(27f, Wheel(-1f, 40f), 0.01f, "a unit down");
            Assert.AreEqual(21f, Wheel(2f, 40f), 0.01f, "two up");
            Assert.AreEqual(0, around.Count, "the list took both");
            Assert.AreEqual(21f, Wheel(-1f, 100f), 0.01f, "beside the list nothing scrolls");
            Assert.AreEqual(1, around.Count, "and the wheel is the parent's");
        }

        // Counts the wheel events that reach it.
        sealed class Heard : MonoBehaviour, IScrollHandler
        {
            public int Count;

            public void OnScroll(PointerEventData eventData)
            {
                Count++;
            }
        }

        // examples/sound.json: firing "tap" reports an event that plays the file's one sound, a
        // 60 ms click (1,323 samples of 16-bit mono at 22,050 Hz), and pulses for 0.3 s.
        [UnityTest]
        public IEnumerator Player_PlaysTheSoundOfAnEvent()
        {
            var player = Make<ZuriPlayer>("Sound");
            player.Asset = Load("sound.zur");
            var heard = new List<string>();
            player.OnEvent.AddListener(heard.Add);
            yield return null;

            AudioClip clip = player.Asset.GetSound(player.Asset.File.SoundId(0));
            Assert.IsNotNull(clip, "the WAV was decoded");
            Assert.AreEqual((1323, 1, 22050), (clip.samples, clip.channels, clip.frequency));
            Assert.AreSame(clip, player.Asset.GetSound(player.Asset.File.SoundId(0)), "decoded once");
            Assert.IsNull(player.AudioSource, "nothing has played yet");

            // Muted: the event is reported, and nothing is set up to play it.
            player.Muted = true;
            player.Fire("tap");
            yield return new WaitForSeconds(0.5f);
            CollectionAssert.AreEqual(new[] { "tapped" }, heard);
            Assert.IsNull(player.AudioSource, "muted");

            player.Muted = false;
            player.Fire("tap");
            yield return null;
            yield return null;
            CollectionAssert.AreEqual(new[] { "tapped", "tapped" }, heard);
            Assert.IsNotNull(player.AudioSource, "the event's sound was given to an AudioSource");
            Assert.IsFalse(player.AudioSource.playOnAwake);
        }

        // Finds the view with the scene's raycasters and sends it what an input module sends
        // to the object a raycast returns. Then checks what the click drew.
        // ponytail: no input module and no device, the test plays that part; to cover them,
        // add a StandaloneInputModule with a BaseInput override that reports these positions.
        IEnumerator HoverClickAndSeeTheTick(ZuriPlayer player)
        {
            Make<EventSystem>("EventSystem");
            var events = new List<string>();
            player.OnEvent.AddListener(events.Add);

            // On the view, beside the button.
            PointerEventData pointer = Raycast(ScreenPoint(10f, 10f));
            Assert.AreSame(player.gameObject, pointer.pointerCurrentRaycast.gameObject, "the raycast should find the view");
            Assert.IsTrue(ExecuteEvents.Execute(player.gameObject, pointer, ExecuteEvents.pointerEnterHandler));
            Assert.IsFalse(player.GetBool("hover"), "the pointer is beside the button");
            // The camera the raycast hands to the events maps the pointer to the artboard, y down.
            Assert.IsTrue(player.GetComponent<ZuriPointerSurface>().ScreenToPixel(pointer.position, pointer.enterEventCamera, out Vector2 pixel));
            Assert.AreEqual(10f, pixel.x, 0.01f);
            Assert.AreEqual(10f, pixel.y, 0.01f);

            // On the button.
            pointer = Raycast(ScreenPoint(160f, 80f));
            Assert.AreSame(player.gameObject, pointer.pointerCurrentRaycast.gameObject);
            ExecuteEvents.Execute(player.gameObject, pointer, ExecuteEvents.pointerMoveHandler);
            Assert.IsTrue(player.GetBool("hover"));
            pointer.pointerPressRaycast = pointer.pointerCurrentRaycast;
            ExecuteEvents.Execute(player.gameObject, pointer, ExecuteEvents.pointerDownHandler);
            Assert.IsTrue(player.GetBool("pressed"));
            ExecuteEvents.Execute(player.gameObject, pointer, ExecuteEvents.pointerUpHandler);
            Assert.IsFalse(player.GetBool("pressed"));
            Assert.AreEqual(1f, player.GetNumber("clicks"));

            // The player's own Update dispatches the event.
            yield return null;
            CollectionAssert.AreEqual(new[] { "clicked" }, events);

            // The click draws a tick. Its long arm rises to the right through artboard pixel
            // (172, 74); in a picture that is upside down it would pass through (172, 86).
            player.Tick(1f);
            Color32[] screen = Capture();
            AssertColor(Color.white, Seen(screen, 172, 74), "the tick's long arm");
            AssertColor(Blue, Seen(screen, 172, 86), "below the tick's long arm");

            // Off the view: nothing is hit, and the exit an input module then sends ends the hover.
            pointer = Raycast(ScreenPoint(-20f, 80f));
            Assert.IsNull(pointer.pointerCurrentRaycast.gameObject, "the raycast should miss the view");
            ExecuteEvents.Execute(player.gameObject, pointer, ExecuteEvents.pointerExitHandler);
            Assert.IsFalse(player.GetBool("hover"));
        }

        // A 320 x 160 view on a canvas the camera draws, showing `file`.
        ZuriPlayer ShowOnCanvas(string file)
        {
            var canvas = Make<Canvas>("Canvas");
            canvas.renderMode = RenderMode.ScreenSpaceCamera;
            canvas.worldCamera = _camera;
            canvas.planeDistance = 1f;
            canvas.gameObject.AddComponent<GraphicRaycaster>();

            var view = new GameObject("View", typeof(RawImage), typeof(ZuriPlayer), typeof(ZuriRawImage));
            view.transform.SetParent(canvas.transform, false);
            var rect = (RectTransform)view.transform;
            rect.sizeDelta = new Vector2(320f, 160f);
            rect.anchoredPosition = new Vector2(Left + 160 - Width / 2, Top - 80 - Height / 2);
            var player = view.GetComponent<ZuriPlayer>();
            player.Asset = Load(file);
            return player;
        }

        // The pointer at a screen position, with what the scene's raycasters find there.
        static PointerEventData Raycast(Vector2 screenPosition)
        {
            var pointer = new PointerEventData(EventSystem.current) { position = screenPosition };
            var hits = new List<RaycastResult>();
            EventSystem.current.RaycastAll(pointer, hits);
            pointer.pointerCurrentRaycast = hits.Count > 0 ? hits[0] : default;
            return pointer;
        }

        // Renders the camera and reads its target back. Row 0 is the bottom of the screen.
        Color32[] Capture()
        {
            _camera.Render();
            RenderTexture previous = RenderTexture.active;
            RenderTexture.active = _camera.targetTexture;
            _readback.ReadPixels(new Rect(0f, 0f, Width, Height), 0, 0);
            RenderTexture.active = previous;
            return _readback.GetPixels32();
        }

        // The screen pixel showing pixel (x, y) of a 320 x 160 artboard in the view. Artboard y points down.
        static Color32 Seen(Color32[] screen, int x, int y)
        {
            return screen[(Top - 1 - y) * Width + Left + x];
        }

        // The screen position of point (x, y) of a 320 x 160 artboard in the view.
        static Vector2 ScreenPoint(float x, float y)
        {
            return new Vector2(Left + x, Top - y);
        }

        // Within 2 of 255 per channel: a linear project converts to linear and back.
        static void AssertColor(Color32 expected, Color32 drawn, string what)
        {
            int error = Mathf.Max(Mathf.Abs(expected.r - drawn.r), Mathf.Abs(expected.g - drawn.g), Mathf.Abs(expected.b - drawn.b));
            Assert.LessOrEqual(error, 2, what + ": expected " + expected + ", drawn " + drawn + " (" + QualitySettings.activeColorSpace + " color space)");
        }

        static ZuriAsset Load(string file)
        {
            // ponytail: editor play mode only (the asmdef requires UNITY_EDITOR); to run in a
            // built player, ship the fixtures in a Resources folder and drop that constraint.
            var asset = UnityEditor.AssetDatabase.LoadAssetAtPath<ZuriAsset>(Fixtures + file);
            Assert.IsNotNull(asset, Fixtures + file + " was not imported as a ZuriAsset");
            return asset;
        }

        T Make<T>(string name) where T : Component
        {
            return Keep(new GameObject(name)).AddComponent<T>();
        }

        T Keep<T>(T created) where T : Object
        {
            _created.Add(created);
            return created;
        }
    }
}
