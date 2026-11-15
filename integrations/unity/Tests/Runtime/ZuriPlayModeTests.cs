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
