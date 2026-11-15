using UnityEngine;
using UnityEngine.EventSystems;

namespace Zuri
{
    /// <summary>
    /// Base of the components that show a <see cref="ZuriPlayer"/>'s texture and forward
    /// EventSystem pointer events (enter, exit, move, down, up) to it, mapped into the
    /// player's pixel space. See <see cref="ZuriRawImage"/> and <see cref="ZuriQuad"/>.
    /// </summary>
    public abstract class ZuriPointerSurface : MonoBehaviour, IPointerEnterHandler, IPointerExitHandler, IPointerMoveHandler, IPointerDownHandler, IPointerUpHandler
    {
        [SerializeField, Tooltip("The player to show. Empty uses the ZuriPlayer on this GameObject.")]
        ZuriPlayer player;

        /// <summary>The player whose texture is shown and which receives the pointer events.</summary>
        public ZuriPlayer Player
        {
            get => player;
            set
            {
                // Unsubscribing a handler that was never added does nothing.
                Unbind();
                player = value;
                if (isActiveAndEnabled)
                {
                    Bind();
                }
            }
        }

        /// <summary>Unity message. Overrides must call the base method.</summary>
        protected virtual void OnEnable()
        {
            if (player == null)
            {
                player = GetComponent<ZuriPlayer>();
            }
            Bind();
        }

        /// <summary>Unity message. Overrides must call the base method.</summary>
        protected virtual void OnDisable()
        {
            Unbind();
        }

        /// <summary>
        /// Maps a screen position to a pixel of the player's texture (origin top-left, y down),
        /// the space <see cref="ZuriPlayer.Pointer"/> expects. The pixel may lie outside the texture.
        /// Returns false if the position cannot be mapped, for example while nothing is playing.
        /// </summary>
        /// <param name="screenPosition">Position in screen pixels, as in <see cref="PointerEventData.position"/>.</param>
        /// <param name="eventCamera">The camera the position is seen through; null for a screen-space overlay canvas.</param>
        /// <param name="pixel">The texture pixel under the position.</param>
        public bool ScreenToPixel(Vector2 screenPosition, Camera eventCamera, out Vector2 pixel)
        {
            Texture2D texture = player != null ? player.Texture : null;
            if (texture == null || !TryGetNormalizedPoint(screenPosition, eventCamera, out Vector2 point))
            {
                pixel = default;
                return false;
            }
            // Unity's v points up; Zuri's rows run down.
            pixel = new Vector2(point.x * texture.width, (1f - point.y) * texture.height);
            return true;
        }

        /// <summary>Shows <paramref name="texture"/> (null when nothing is playing) with a flipped V.</summary>
        protected abstract void ShowTexture(Texture2D texture);

        /// <summary>
        /// Maps a screen position to this surface: (0, 0) is its bottom-left corner and (1, 1) its
        /// top-right, with values outside that range for positions off the surface.
        /// </summary>
        protected abstract bool TryGetNormalizedPoint(Vector2 screenPosition, Camera eventCamera, out Vector2 point);

        /// <inheritdoc/>
        public void OnPointerEnter(PointerEventData eventData)
        {
            Forward(ZuriPointerEvent.Move, eventData);
        }

        /// <inheritdoc/>
        public void OnPointerMove(PointerEventData eventData)
        {
            Forward(ZuriPointerEvent.Move, eventData);
        }

        /// <inheritdoc/>
        public void OnPointerDown(PointerEventData eventData)
        {
            Forward(ZuriPointerEvent.Down, eventData);
        }

        /// <inheritdoc/>
        public void OnPointerUp(PointerEventData eventData)
        {
            Forward(ZuriPointerEvent.Up, eventData);
        }

        /// <inheritdoc/>
        public void OnPointerExit(PointerEventData eventData)
        {
            if (player != null)
            {
                player.Pointer(ZuriPointerEvent.Exit, default);
            }
        }

        void Forward(ZuriPointerEvent kind, PointerEventData eventData)
        {
            // While a button is held the press camera stays valid even if the pointer has left.
            Camera eventCamera = eventData.pressEventCamera != null ? eventData.pressEventCamera : eventData.enterEventCamera;
            if (ScreenToPixel(eventData.position, eventCamera, out Vector2 pixel))
            {
                player.Pointer(kind, pixel);
            }
        }

        void Bind()
        {
            if (player != null)
            {
                player.TextureChanged += Refresh;
            }
            Refresh();
        }

        void Unbind()
        {
            if (player != null)
            {
                player.TextureChanged -= Refresh;
            }
        }

        void Refresh()
        {
            ShowTexture(player != null ? player.Texture : null);
        }
    }
}
