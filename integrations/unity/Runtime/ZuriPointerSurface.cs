using UnityEngine;
using UnityEngine.EventSystems;

namespace Zuri
{
    /// <summary>
    /// Base of the components that show a <see cref="ZuriPlayer"/>'s texture and forward
    /// EventSystem pointer events (enter, exit, move, down, up, scroll) to it, mapped into the
    /// player's pixel space. See <see cref="ZuriRawImage"/> and <see cref="ZuriQuad"/>.
    /// </summary>
    public abstract class ZuriPointerSurface : MonoBehaviour, IPointerEnterHandler, IPointerExitHandler, IPointerMoveHandler, IPointerDownHandler, IPointerUpHandler, IScrollHandler
    {
        [SerializeField, Tooltip("The player to show. Empty uses the ZuriPlayer on this GameObject.")]
        ZuriPlayer player;

        [SerializeField, Tooltip("Screen pixels a Layout scrolls for each unit of the event system's scroll delta, as a ScrollRect's Scroll Sensitivity.")]
        float scrollSensitivity = 1f;

        /// <summary>
        /// How many screen pixels a Layout that scrolls goes for each unit of
        /// <see cref="PointerEventData.scrollDelta"/>, as <c>ScrollRect.scrollSensitivity</c>
        /// (and 1 until set, as there). What a unit is depends on the input module.
        /// </summary>
        public float ScrollSensitivity
        {
            get => scrollSensitivity;
            set => scrollSensitivity = value;
        }

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
                player.Pointer(ZuriPointerEvent.Exit, default, unchecked((uint)eventData.pointerId));
                pressed = pressed == eventData.pointerId ? null : pressed;
            }
        }

        /// <inheritdoc/>
        public void OnScroll(PointerEventData eventData)
        {
            // As a ScrollRect: the content goes with the wheel (whose up is the screen's down),
            // by this much on the screen. In texture pixels, the scene scrolls the other way.
            Vector2 content = new Vector2(eventData.scrollDelta.x, -eventData.scrollDelta.y) * scrollSensitivity;
            Camera eventCamera = eventData.enterEventCamera;
            bool taken = player != null
                && ScreenToPixel(eventData.position, eventCamera, out Vector2 pixel)
                && ScreenToPixel(eventData.position + content, eventCamera, out Vector2 moved)
                && player.Wheel(pixel, pixel - moved);
            // A wheel that moved nothing here is for what is around: a ScrollRect this is in.
            if (!taken && transform.parent != null)
            {
                ExecuteEvents.ExecuteHierarchy(transform.parent.gameObject, eventData, ExecuteEvents.scrollHandler);
            }
        }

        // The pointer whose press this surface passed on, which it ends if it stops showing the player.
        int? pressed;

        void Forward(ZuriPointerEvent kind, PointerEventData eventData)
        {
            // While a button is held the press camera stays valid even if the pointer has left.
            Camera eventCamera = eventData.pressEventCamera != null ? eventData.pressEventCamera : eventData.enterEventCamera;
            if (!ScreenToPixel(eventData.position, eventCamera, out Vector2 pixel))
            {
                // A release that cannot be placed still ends the press: for the scene the pointer has left.
                if (kind == ZuriPointerEvent.Up && eventData.button == PointerEventData.InputButton.Left)
                {
                    OnPointerExit(eventData);
                }
                return;
            }
            // The runtime follows one pointer and one button, the press's: it is told which pointer
            // and which button each event is of. (InputButton.Left is 0, as its primary button is.)
            bool taken = player.Pointer(kind, pixel, unchecked((uint)eventData.pointerId), (uint)eventData.button);
            if (kind == ZuriPointerEvent.Up && taken)
            {
                pressed = null;
            }
            else if (kind == ZuriPointerEvent.Down && taken)
            {
                pressed = eventData.pointerId;
                // How far a press may go before it scrolls a Layout: the EventSystem's drag threshold in
                // texture pixels (where the view stretches the texture, on the axis with more per screen pixel).
                float threshold = EventSystem.current != null ? EventSystem.current.pixelDragThreshold : 10;
                if (ScreenToPixel(eventData.position + new Vector2(threshold, 0f), eventCamera, out Vector2 across)
                    && ScreenToPixel(eventData.position + new Vector2(0f, threshold), eventCamera, out Vector2 along))
                {
                    player.Instance.SetDragThreshold(Mathf.Max((across - pixel).magnitude, (along - pixel).magnitude));
                }
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
                // The press that came through this surface cannot be let go through it any more.
                if (pressed.HasValue)
                {
                    player.Pointer(ZuriPointerEvent.Exit, default, unchecked((uint)pressed.Value));
                    pressed = null;
                }
            }
        }

        void Refresh()
        {
            ShowTexture(player != null ? player.Texture : null);
        }
    }
}
