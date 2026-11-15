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
        public void Player_AlignsTheArtboardInALargerTexture()
        {
            ZuriPlayer player = Play("button.zur");
            var serialized = new SerializedObject(player);
            serialized.FindProperty("fit").intValue = (int)ZuriFit.None;
            serialized.FindProperty("alignment").intValue = (int)ZuriAlignment.BottomRight;
            serialized.FindProperty("resolution").vector2IntValue = new Vector2Int(640, 320);
            serialized.ApplyModifiedPropertiesWithoutUndo();
            player.Rebuild();

            Assert.AreEqual(0, Pixel(player, 100, 80).a, "nothing is drawn in the top-left quadrant");
            Assert.AreEqual(Blue, Pixel(player, 320 + 100, 160 + 80), "the artboard sits in the bottom-right quadrant");
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
        public void Instance_ReportsDeclaredEventsLayerStatesAndObjectProperties()
        {
            ZuriInstance instance = Play("button.zur").Instance;

            CollectionAssert.AreEqual(new[] { "clicked" }, instance.DeclaredEvents);
            // The player started the state machine: every layer is in a state and there is no timeline to seek.
            Assert.IsNotEmpty(instance.LayerStates);
            Assert.AreNotEqual(uint.MaxValue, instance.LayerStates[0]);
            Assert.IsFalse(instance.Seek(1f));

            // Object ids are file indices; schema property key 3 is "x".
            uint shape = 0;
            float x = 0f;
            while (shape < 1000 && !instance.TryGetProperty(shape, 3, out x))
            {
                shape++;
            }
            Assert.AreEqual(160f, x);
            Assert.IsFalse(instance.TryGetPropertyColor(shape, 3, out _), "x is not a color");
            Assert.IsFalse(instance.TryGetProperty(uint.MaxValue, 3, out _));

            Assert.IsTrue(instance.PlayAnimation("idle"));
            Assert.IsTrue(instance.Seek(0.5f));
            Assert.IsEmpty(instance.LayerStates);
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

        [Test]
        public void NativePlugins_AreEnabledOnlyForThePlatformOfTheirFolder()
        {
            // Plugins/<folder>/...: the editor OS and the one desktop player the library is built for.
            var platforms = new Dictionary<string, (string EditorOs, BuildTarget Player)>
            {
                { "macOS", ("OSX", BuildTarget.StandaloneOSX) },
                { "Linux", ("Linux", BuildTarget.StandaloneLinux64) },
                { "Windows", ("Windows", BuildTarget.StandaloneWindows64) },
            };
            const string plugins = "Packages/com.seedexr.zuri/Plugins/";
            int libraries = 0;
            foreach (PluginImporter importer in PluginImporter.GetAllImporters())
            {
                string path = importer.assetPath;
                if (!path.StartsWith(plugins) || !importer.isNativePlugin)
                {
                    continue;
                }
                // Only the desktop folders: a mobile library has targets of its own.
                if (!platforms.TryGetValue(path.Substring(plugins.Length).Split('/')[0], out (string EditorOs, BuildTarget Player) want))
                {
                    continue;
                }
                libraries++;
                (string editorOs, BuildTarget player) = want;
                Assert.IsFalse(importer.GetCompatibleWithAnyPlatform(), path);
                Assert.IsTrue(importer.GetCompatibleWithEditor(), path);
                Assert.AreEqual(editorOs, importer.GetEditorData("OS"), path);
                foreach (BuildTarget target in new[] { BuildTarget.StandaloneOSX, BuildTarget.StandaloneWindows, BuildTarget.StandaloneWindows64, BuildTarget.StandaloneLinux64 })
                {
                    Assert.AreEqual(target == player, importer.GetCompatibleWithPlatform(target), path + " for " + target);
                }
            }
            Assert.Greater(libraries, 0, "no native library under " + plugins);
        }

        [Test]
        public void Inspector_DrawsAndItsFireButtonFiresTheTrigger()
        {
            if (SystemInfo.graphicsDeviceType == UnityEngine.Rendering.GraphicsDeviceType.Null)
            {
                Assert.Ignore("Editor windows need a graphics device: run Unity without -nographics.");
            }
            ZuriPlayer player = Play("button.zur");
            // The state machine dropdown is only drawn when Playback is not Auto.
            var serialized = new SerializedObject(player);
            serialized.FindProperty("playback").intValue = (int)ZuriPlayback.StateMachine;
            serialized.ApplyModifiedPropertiesWithoutUndo();
            player.Rebuild();
            Texture2D texture = player.Texture;
            var events = new List<string>();
            player.OnEvent.AddListener(events.Add);

            UnityEditor.Editor inspector = UnityEditor.Editor.CreateEditor(player);
            _created.Add(inspector);
            Assert.AreEqual("Zuri.Editor.ZuriPlayerEditor", inspector.GetType().FullName);
            var window = ScriptableObject.CreateInstance<InspectorHost>();
            window.Inspector = inspector;
            window.wantsMouseMove = true;
            try
            {
                window.Show();
                window.position = new Rect(0f, 0f, 400f, 800f);
                window.SendEvent(new Event { type = EventType.Layout });
                window.SendEvent(new Event { type = EventType.Repaint });
                CollectionAssert.IsSubsetOf(new[] { EventType.Layout, EventType.Repaint }, window.Passes);
                Assert.AreSame(texture, player.Texture, "drawing the inspector should not rebuild the player");

                // The last control is the Fire button of the last view-model property, the trigger "pulse".
                // A sent event's position is relative to the window with its tab bar, OnGUI's is not:
                // a mouse move that hits nothing measures the difference.
                var onFire = new Vector2(window.LastControl.xMax - 8f, window.LastControl.center.y);
                window.SendEvent(new Event { type = EventType.MouseMove, mousePosition = onFire });
                CollectionAssert.Contains(window.Passes, EventType.MouseMove, "the probing mouse move was not delivered to OnGUI, so the click position cannot be corrected");
                onFire += onFire - window.Mouse;
                window.SendEvent(new Event { type = EventType.MouseDown, mousePosition = onFire });
                window.SendEvent(new Event { type = EventType.MouseUp, mousePosition = onFire });
                CollectionAssert.AreEqual(new[] { "clicked" }, events);
            }
            finally
            {
                window.Close();
            }
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

        // An IMGUI context for an inspector: a window whose OnGUI draws it.
        sealed class InspectorHost : EditorWindow
        {
            public UnityEditor.Editor Inspector;
            public Rect LastControl;
            public Vector2 Mouse;
            public readonly List<EventType> Passes = new List<EventType>();

            void OnGUI()
            {
                Passes.Add(Event.current.type);
                if (Event.current.isMouse)
                {
                    Mouse = Event.current.mousePosition;
                }
                Inspector.OnInspectorGUI();
                if (Event.current.type == EventType.Repaint)
                {
                    LastControl = GUILayoutUtility.GetLastRect();
                }
            }
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
