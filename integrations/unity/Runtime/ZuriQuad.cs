using UnityEngine;

namespace Zuri
{
    /// <summary>
    /// Shows a <see cref="ZuriPlayer"/> on a world-space quad and forwards pointer events to it.
    /// Put it on Unity's built-in Quad (or any mesh spanning -0.5..0.5 in local X and Y with
    /// matching UVs). Pointer input needs a Collider on the object, a PhysicsRaycaster on the
    /// camera and an EventSystem in the scene.
    /// </summary>
    /// <remarks>
    /// The component instantiates the renderer's material and sets its main texture with scale
    /// (1, -1) and offset (0, 1), because the player's texture has its rows top to bottom.
    /// Unlike <see cref="ZuriRawImage"/> it does not run in edit mode.
    /// </remarks>
    [RequireComponent(typeof(Renderer))]
    [AddComponentMenu("Zuri/Zuri Quad")]
    public sealed class ZuriQuad : ZuriPointerSurface
    {
        Material _material;

        void OnDestroy()
        {
            if (_material != null)
            {
                Destroy(_material);
            }
        }

        /// <inheritdoc/>
        protected override void ShowTexture(Texture2D texture)
        {
            if (_material == null)
            {
                _material = GetComponent<Renderer>().material;
            }
            _material.mainTexture = texture;
            _material.mainTextureScale = new Vector2(1f, -1f);
            _material.mainTextureOffset = new Vector2(0f, 1f);
        }

        /// <inheritdoc/>
        protected override bool TryGetNormalizedPoint(Vector2 screenPosition, Camera eventCamera, out Vector2 point)
        {
            point = default;
            if (eventCamera == null)
            {
                return false;
            }
            // Intersect with the quad's plane rather than its collider, so a release outside still maps.
            Ray ray = eventCamera.ScreenPointToRay(screenPosition);
            if (!new Plane(transform.forward, transform.position).Raycast(ray, out float distance))
            {
                return false;
            }
            Vector3 local = transform.InverseTransformPoint(ray.GetPoint(distance));
            point = new Vector2(local.x + 0.5f, local.y + 0.5f);
            return true;
        }
    }
}
