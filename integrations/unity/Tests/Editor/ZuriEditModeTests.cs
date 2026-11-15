using System.Collections;
using System.Collections.Generic;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.TestTools;
using UnityEngine.UI;

namespace Zuri.Tests
{
    /// <summary>
    /// Drives the package end to end in the editor: .zur import, the native plugin, ZuriPlayer's
    /// texture, pointer mapping and events. The fixtures are built by scripts/build-unity-plugins.sh.
    /// </summary>
    public sealed class ZuriEditModeTests
    {
        const string Fixtures = "Packages/com.seedexr.zuri/Tests/Editor/";

        static readonly Color32 Blue = new Color32(0x4f, 0x7c, 0xff, 0xff);
        static readonly Color32 Pink = new Color32(0xff, 0x33, 0x66, 0xff);

        readonly List<Object> _created = new List<Object>();

        [TearDown]
        public void DestroyCreatedObjects()
        {
            foreach (Object created in _created)
            {
                Object.DestroyImmediate(created);
            }
            _created.Clear();
        }

        [Test]
        public void Importer_DescribesTheArtboards()
        {
            ZuriAsset asset = Load("button.zur");

            Assert.AreEqual(1, asset.Artboards.Count);
            ZuriArtboardInfo artboard = asset.Artboards[0];
            Assert.AreEqual("Button", artboard.Name);
            Assert.AreEqual(new Vector2(320f, 160f), new Vector2(artboard.Width, artboard.Height));
            CollectionAssert.AreEqual(new[] { "idle", "hover", "press", "tick", "untick" }, artboard.Animations);
            CollectionAssert.AreEqual(new[] { "Button" }, artboard.StateMachines);
            Assert.AreEqual(5, artboard.Properties.Length);
            Assert.AreEqual("accent", artboard.Properties[3].Name);
            Assert.AreEqual(ZuriPropertyType.Color, artboard.Properties[3].Type);
        }

        [Test]
        public void Create_RejectsGarbageWithTheRuntimesMessage()
        {
            var error = Assert.Throws<ZuriException>(() => ZuriAsset.Create(new byte[] { 1, 2, 3, 4 }));
            StringAssert.Contains("ZURI", error.Message);
        }

        [Test]
        public void Player_RendersTheFirstFrameIntoItsTexture()
        {
            ZuriPlayer player = Play("button.zur");

            Assert.AreEqual(320, player.Texture.width);
            Assert.AreEqual(160, player.Texture.height);
            Assert.AreEqual(TextureFormat.RGBA32, player.Texture.format);
            Assert.AreEqual(Blue, Pixel(player, 100, 80));
            Assert.AreEqual(new Color32(0x0f, 0x12, 0x20, 0xff), Pixel(player, 2, 2), "the artboard's own fill");
        }

        [Test]
        public void Player_ScalesItsResolution()
        {
            ZuriPlayer player = Play("button.zur", scale: 2f);

            Assert.AreEqual(640, player.Texture.width);
            Assert.AreEqual(320, player.Texture.height);
            Assert.AreEqual(Blue, Pixel(player, 200, 160));
        }

        [Test]
        public void Player_StopsRenderingOnceIdle()
        {
            ZuriPlayer player = Play("button.zur");

            bool redrawn = true;
            for (int i = 0; i < 30; i++)
            {
                redrawn = player.Tick(1f / 60f);
            }

            Assert.IsFalse(redrawn);
        }

        [Test]
        public void Player_ClickCountsAndRaisesTheEvent()
        {
            ZuriPlayer player = Play("button.zur");
            var events = new List<string>();
            player.OnEvent.AddListener(events.Add);

            player.Pointer(ZuriPointerEvent.Move, new Vector2(160f, 80f));
            player.Pointer(ZuriPointerEvent.Down, new Vector2(160f, 80f));
            player.Pointer(ZuriPointerEvent.Up, new Vector2(160f, 80f));

            Assert.IsTrue(player.Tick(1f / 60f), "the click should wake the player");
            Assert.AreEqual(1f, player.GetNumber("clicks"));
            CollectionAssert.AreEqual(new[] { "clicked" }, events);
        }

        [Test]
        public void Player_SetColorRecolorsTheButton()
        {
            ZuriPlayer player = Play("button.zur");

            Assert.IsTrue(player.SetColor("accent", Pink));
            Assert.IsFalse(player.SetColor("missing", Pink));
            Assert.IsTrue(player.Tick(0f));

            Assert.AreEqual(Pink, player.GetColor("accent"));
            Assert.AreEqual(Pink, Pixel(player, 100, 80));
        }

        [Test]
        public void ImageAssets_AreDecodedTheRightWayUp()
        {
            // The fixture stretches a 2 x 2 image, red over blue, across a 64 x 64 artboard.
            ZuriPlayer player = Play("image.zur");

            Assert.AreEqual(new Color32(255, 0, 0, 255), Pixel(player, 32, 8));
            Assert.AreEqual(new Color32(0, 0, 255, 255), Pixel(player, 32, 56));
        }

        [Test]
        public void RenderPreview_IsUprightForUnity()
        {
            Texture2D preview = Load("image.zur").RenderPreview(64, 64);
            _created.Add(preview);

            // Unity's y points up: the red half is at the top.
            Assert.AreEqual(new Color32(255, 0, 0, 255), (Color32)preview.GetPixel(32, 56));
            Assert.AreEqual(new Color32(0, 0, 255, 255), (Color32)preview.GetPixel(32, 8));
        }

        [Test]
        public void RawImage_ShowsTheTextureFlippedAndForwardsPointerEvents()
        {
            var canvas = new GameObject("Canvas", typeof(Canvas));
            _created.Add(canvas);
            canvas.GetComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
            var view = new GameObject("View", typeof(RawImage), typeof(ZuriPlayer), typeof(ZuriRawImage));
            view.transform.SetParent(canvas.transform, false);
            var rect = (RectTransform)view.transform;
            rect.sizeDelta = new Vector2(320f, 160f);
            var player = view.GetComponent<ZuriPlayer>();
            var surface = view.GetComponent<ZuriRawImage>();
            player.Asset = Load("button.zur");

            Assert.AreSame(player.Texture, view.GetComponent<RawImage>().texture);
            Assert.AreEqual(new Rect(0f, 1f, 1f, -1f), view.GetComponent<RawImage>().uvRect);

            // 60 left of and 40 above the centre of a 320 x 160 rect is texture pixel (100, 40).
            Assert.IsTrue(surface.ScreenToPixel(ScreenPoint(rect, new Vector2(-60f, 40f)), null, out Vector2 pixel));
            Assert.AreEqual(100f, pixel.x, 0.01f);
            Assert.AreEqual(40f, pixel.y, 0.01f);

            var pointer = new PointerEventData(null) { position = ScreenPoint(rect, Vector2.zero) };
            surface.OnPointerEnter(pointer);
            Assert.IsTrue(player.GetBool("hover"));
            surface.OnPointerDown(pointer);
            surface.OnPointerUp(pointer);
            Assert.AreEqual(1f, player.GetNumber("clicks"));
            surface.OnPointerExit(pointer);
            Assert.IsFalse(player.GetBool("hover"));
        }

        [Test]
        public void Quad_MapsScreenPositionsThroughTheCamera()
        {
            var cameraObject = new GameObject("Camera", typeof(Camera));
            _created.Add(cameraObject);
            cameraObject.transform.position = new Vector3(0f, 0f, -5f);
            var camera = cameraObject.GetComponent<Camera>();

            ZuriPlayer player = Play("button.zur");
            player.gameObject.AddComponent<MeshRenderer>();
            player.transform.localScale = new Vector3(2f, 1f, 1f);
            var quad = player.gameObject.AddComponent<ZuriQuad>();
            // ZuriQuad does not run in edit mode, so wire the player the way the inspector would.
            var serialized = new SerializedObject(quad);
            serialized.FindProperty("player").objectReferenceValue = player;
            serialized.ApplyModifiedPropertiesWithoutUndo();

            // Local (0.25, 0.25) on a quad spanning -0.5..0.5 is 3/4 right and 3/4 up: pixel (240, 40).
            Vector2 screen = camera.WorldToScreenPoint(player.transform.TransformPoint(new Vector3(0.25f, 0.25f, 0f)));
            Assert.IsTrue(quad.ScreenToPixel(screen, camera, out Vector2 pixel));
            Assert.AreEqual(240f, pixel.x, 0.5f);
            Assert.AreEqual(40f, pixel.y, 0.5f);
        }

        [UnityTest]
        public IEnumerator Player_SurvivesEnteringAndLeavingPlayMode()
        {
            const string objectName = "Zuri Play Mode Test";
            new GameObject(objectName).AddComponent<ZuriPlayer>().Asset = Load("button.zur");
            Assert.AreEqual(Blue, Pixel(GameObject.Find(objectName).GetComponent<ZuriPlayer>(), 100, 80));

            // Reloads the domain: native handles are gone and this method's locals are reset.
            yield return new EnterPlayMode();
            ZuriPlayer player = GameObject.Find(objectName).GetComponent<ZuriPlayer>();
            Assert.IsNotNull(player.Instance, "the player did not restart in play mode");
            Assert.AreEqual(Blue, Pixel(player, 100, 80));

            // In play mode the player's own Update drives the frame loop.
            player.Pointer(ZuriPointerEvent.Move, new Vector2(160f, 80f));
            player.Pointer(ZuriPointerEvent.Down, new Vector2(160f, 80f));
            player.Pointer(ZuriPointerEvent.Up, new Vector2(160f, 80f));
            player.SetColor("accent", Pink);
            for (int i = 0; i < 5; i++)
            {
                yield return null;
            }
            player = GameObject.Find(objectName).GetComponent<ZuriPlayer>();
            Assert.AreEqual(1f, player.GetNumber("clicks"));
            Assert.AreEqual(Pink, Pixel(player, 100, 80));

            yield return new ExitPlayMode();
            player = GameObject.Find(objectName).GetComponent<ZuriPlayer>();
            Assert.IsNotNull(player.Instance, "the player did not restart in edit mode");
            Assert.AreEqual(Blue, Pixel(player, 100, 80), "view-model values reset with the scene");
            Object.DestroyImmediate(player.gameObject);
        }

        static ZuriAsset Load(string file)
        {
            var asset = AssetDatabase.LoadAssetAtPath<ZuriAsset>(Fixtures + file);
            Assert.IsNotNull(asset, Fixtures + file + " was not imported as a ZuriAsset");
            return asset;
        }

        ZuriPlayer Play(string file, float scale = 1f)
        {
            var gameObject = new GameObject("Zuri Test");
            _created.Add(gameObject);
            var player = gameObject.AddComponent<ZuriPlayer>();
            if (scale != 1f)
            {
                var serialized = new SerializedObject(player);
                serialized.FindProperty("resolutionScale").floatValue = scale;
                serialized.ApplyModifiedPropertiesWithoutUndo();
            }
            player.Asset = Load(file);
            Assert.IsNotNull(player.Texture, "the player did not start");
            return player;
        }

        // A pixel of the player's texture in Zuri's convention: origin top-left, y down.
        static Color32 Pixel(ZuriPlayer player, int x, int y)
        {
            return player.Texture.GetRawTextureData<Color32>()[y * player.Texture.width + x];
        }

        static Vector2 ScreenPoint(RectTransform rect, Vector2 local)
        {
            return RectTransformUtility.WorldToScreenPoint(null, rect.TransformPoint(local));
        }
    }
}
