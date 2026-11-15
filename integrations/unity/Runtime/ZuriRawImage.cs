using UnityEngine;
using UnityEngine.UI;

namespace Zuri
{
    /// <summary>
    /// Shows a <see cref="ZuriPlayer"/> in a uGUI <see cref="RawImage"/> and forwards pointer
    /// events to it. Needs an EventSystem in the scene and a GraphicRaycaster on the canvas.
    /// </summary>
    /// <remarks>
    /// The component sets the RawImage's texture and its UV rect to (0, 1, 1, -1), because the
    /// player's texture has its rows top to bottom. The texture is stretched over the
    /// RectTransform: give the rect the artboard's aspect ratio (an AspectRatioFitter works).
    /// </remarks>
    [ExecuteAlways]
    [RequireComponent(typeof(RawImage))]
    [AddComponentMenu("Zuri/Zuri Raw Image")]
    public sealed class ZuriRawImage : ZuriPointerSurface
    {
        /// <inheritdoc/>
        protected override void ShowTexture(Texture2D texture)
        {
            RawImage image = GetComponent<RawImage>();
            image.texture = texture;
            image.uvRect = new Rect(0f, 1f, 1f, -1f);
        }

        /// <inheritdoc/>
        protected override bool TryGetNormalizedPoint(Vector2 screenPosition, Camera eventCamera, out Vector2 point)
        {
            var rectTransform = (RectTransform)transform;
            Rect rect = rectTransform.rect;
            if (rect.width <= 0f || rect.height <= 0f || !RectTransformUtility.ScreenPointToLocalPointInRectangle(rectTransform, screenPosition, eventCamera, out Vector2 local))
            {
                point = default;
                return false;
            }
            point = new Vector2((local.x - rect.xMin) / rect.width, (local.y - rect.yMin) / rect.height);
            return true;
        }
    }
}
