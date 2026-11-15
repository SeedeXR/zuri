using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditorInternal;
using UnityEngine;

namespace Zuri.Editor
{
    /// <summary>
    /// Inspector for <see cref="ZuriPlayer"/>: picks the artboard, state machine and animation
    /// from what the asset contains, and edits the playing instance's view-model properties live.
    /// </summary>
    [CustomEditor(typeof(ZuriPlayer))]
    public sealed class ZuriPlayerEditor : UnityEditor.Editor
    {
        SerializedProperty _asset, _artboard, _playback, _playbackName;

        void OnEnable()
        {
            _asset = serializedObject.FindProperty("asset");
            _artboard = serializedObject.FindProperty("artboard");
            _playback = serializedObject.FindProperty("playback");
            _playbackName = serializedObject.FindProperty("playbackName");
        }

        /// <summary>Keeps the live property values fresh while the artboard changes them.</summary>
        public override bool RequiresConstantRepaint()
        {
            return Application.isPlaying && !serializedObject.isEditingMultipleObjects && ((ZuriPlayer)target).Instance != null;
        }

        /// <inheritdoc/>
        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            EditorGUI.BeginChangeCheck();

            EditorGUILayout.PropertyField(_asset);
            var asset = _asset.hasMultipleDifferentValues ? null : _asset.objectReferenceValue as ZuriAsset;
            if (asset != null)
            {
                ZuriArtboardInfo? selected = DrawContents(asset);
                if (selected.HasValue)
                {
                    EditorGUILayout.LabelField("Artboard Size", selected.Value.Width + " x " + selected.Value.Height);
                }
            }
            else
            {
                EditorGUILayout.PropertyField(_artboard);
                EditorGUILayout.PropertyField(_playback);
                EditorGUILayout.PropertyField(_playbackName);
            }

            // Everything else, in declaration order, with its default control.
            DrawPropertiesExcluding(serializedObject, "m_Script", "asset", "artboard", "playback", "playbackName");

            if (EditorGUI.EndChangeCheck())
            {
                serializedObject.ApplyModifiedProperties();
                foreach (UnityEngine.Object player in targets)
                {
                    ((ZuriPlayer)player).Rebuild();
                }
                InternalEditorUtility.RepaintAllViews();
            }

            if (!serializedObject.isEditingMultipleObjects)
            {
                var player = (ZuriPlayer)target;
                // What On Event can be invoked with.
                if (player.Instance != null && player.Instance.DeclaredEvents.Count > 0)
                {
                    EditorGUILayout.LabelField("Declared Events", string.Join(", ", player.Instance.DeclaredEvents));
                }
                DrawLiveProperties(player);
            }
        }

        // Artboard, playback mode and name as dropdowns. Returns the selected artboard, if it exists.
        ZuriArtboardInfo? DrawContents(ZuriAsset asset)
        {
            List<string> artboardNames = asset.Artboards.Select(info => info.Name).ToList();
            NamePopup(_artboard, artboardNames, "(first)");
            EditorGUILayout.PropertyField(_playback);

            int index = string.IsNullOrEmpty(_artboard.stringValue) ? 0 : artboardNames.IndexOf(_artboard.stringValue);
            if (_artboard.hasMultipleDifferentValues || index < 0 || index >= asset.Artboards.Count)
            {
                EditorGUILayout.PropertyField(_playbackName);
                return null;
            }
            ZuriArtboardInfo artboard = asset.Artboards[index];
            if (!_playback.hasMultipleDifferentValues && _playback.enumValueIndex != (int)ZuriPlayback.Auto)
            {
                bool animation = _playback.enumValueIndex == (int)ZuriPlayback.Animation;
                NamePopup(_playbackName, animation ? artboard.Animations : artboard.StateMachines, "(first)");
            }
            return artboard;
        }

        // A string property edited as a choice among `names`; the empty string shows as `emptyLabel`.
        static void NamePopup(SerializedProperty property, IList<string> names, string emptyLabel)
        {
            var values = new List<string> { "" };
            var labels = new List<GUIContent> { new GUIContent(emptyLabel) };
            foreach (string name in names)
            {
                values.Add(name);
                // A slash would open a submenu.
                labels.Add(new GUIContent(name.Replace('/', '∕')));
            }
            int selected = values.IndexOf(property.stringValue);
            if (selected < 0)
            {
                selected = values.Count;
                values.Add(property.stringValue);
                labels.Add(new GUIContent(property.stringValue + " (missing)"));
            }

            Rect rect = EditorGUILayout.GetControlRect();
            var label = EditorGUI.BeginProperty(rect, new GUIContent(property.displayName, property.tooltip), property);
            EditorGUI.showMixedValue = property.hasMultipleDifferentValues;
            EditorGUI.BeginChangeCheck();
            selected = EditorGUI.Popup(rect, label, selected, labels.ToArray());
            if (EditorGUI.EndChangeCheck())
            {
                property.stringValue = values[selected];
            }
            EditorGUI.showMixedValue = false;
            EditorGUI.EndProperty();
        }

        // One control per view-model property of the playing instance. Nothing here is serialized.
        static void DrawLiveProperties(ZuriPlayer player)
        {
            ZuriInstance instance = player.Instance;
            if (instance == null || instance.Properties.Count == 0)
            {
                return;
            }
            EditorGUILayout.Space();
            EditorGUILayout.LabelField("View Model (live, not saved)", EditorStyles.boldLabel);
            bool changed = false;
            foreach (ZuriProperty property in instance.Properties)
            {
                string name = property.Name;
                EditorGUI.BeginChangeCheck();
                switch (property.Type)
                {
                    case ZuriPropertyType.Number:
                        float number = EditorGUILayout.FloatField(name, player.GetNumber(name));
                        if (EditorGUI.EndChangeCheck())
                        {
                            changed |= player.SetNumber(name, number);
                        }
                        break;
                    case ZuriPropertyType.Bool:
                        bool flag = EditorGUILayout.Toggle(name, player.GetBool(name));
                        if (EditorGUI.EndChangeCheck())
                        {
                            changed |= player.SetBool(name, flag);
                        }
                        break;
                    case ZuriPropertyType.Color:
                        Color32 color = EditorGUILayout.ColorField(name, player.GetColor(name));
                        if (EditorGUI.EndChangeCheck())
                        {
                            changed |= player.SetColor(name, color);
                        }
                        break;
                    case ZuriPropertyType.String:
                        string text = EditorGUILayout.TextField(name, player.GetString(name));
                        if (EditorGUI.EndChangeCheck())
                        {
                            changed |= player.SetString(name, text);
                        }
                        break;
                    case ZuriPropertyType.Trigger:
                        EditorGUI.EndChangeCheck();
                        Rect rect = EditorGUI.PrefixLabel(EditorGUILayout.GetControlRect(), new GUIContent(name));
                        if (GUI.Button(rect, "Fire"))
                        {
                            changed |= player.Fire(name);
                        }
                        break;
                    default:
                        EditorGUI.EndChangeCheck();
                        break;
                }
            }
            // In play mode the player's Update picks the change up; in edit mode nothing ticks.
            if (changed && !Application.isPlaying)
            {
                player.Tick(0f);
                InternalEditorUtility.RepaintAllViews();
            }
        }
    }
}
