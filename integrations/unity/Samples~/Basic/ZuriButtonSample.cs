using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.UI;
using Zuri;

/// <summary>
/// Shows the button example: it reacts to hover and press through its state machine, and every
/// click recolors it through the "accent" view-model property.
/// Usage: add this component to an empty GameObject, assign button.zur, add an Event System
/// (GameObject > UI > Event System) and press Play.
/// </summary>
public sealed class ZuriButtonSample : MonoBehaviour
{
    [SerializeField, Tooltip("The button.zur that ships with this sample.")]
    ZuriAsset button;

    [SerializeField, Tooltip("The accent color cycles through these, one step per click.")]
    Color32[] accents =
    {
        new Color32(0x4f, 0x7c, 0xff, 0xff),
        new Color32(0xff, 0x33, 0x66, 0xff),
        new Color32(0x1f, 0xc9, 0x8b, 0xff),
    };

    ZuriPlayer _player;

    void Start()
    {
        if (EventSystem.current == null)
        {
            Debug.LogWarning("ZuriButtonSample: add an Event System (GameObject > UI > Event System) to get pointer input.", this);
        }

        var canvas = new GameObject("Zuri Canvas", typeof(Canvas), typeof(GraphicRaycaster));
        canvas.GetComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;

        // A RawImage the size of the artboard, centred on screen.
        var view = new GameObject("Zuri Button", typeof(RawImage), typeof(ZuriPlayer), typeof(ZuriRawImage));
        view.transform.SetParent(canvas.transform, false);
        _player = view.GetComponent<ZuriPlayer>();
        _player.Asset = button;
        if (_player.Texture == null)
        {
            return;
        }
        view.GetComponent<RectTransform>().sizeDelta = new Vector2(_player.Texture.width, _player.Texture.height);
        _player.OnEvent.AddListener(OnZuriEvent);
    }

    // Hover and press need no code: ZuriRawImage forwards the pointer and the state machine reacts.
    void OnZuriEvent(string eventName)
    {
        if (eventName != "clicked")
        {
            return;
        }
        int clicks = (int)_player.GetNumber("clicks");
        _player.SetColor("accent", accents[clicks % accents.Length]);
        Debug.Log("Zuri button clicked " + clicks + " time(s).", this);
    }
}
