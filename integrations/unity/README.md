# Zuri for Unity

Play [Zuri](https://github.com/SeedeXR/zuri) interactive vector animations (`.zur` files) in Unity:
state machines, data binding, pointer input and events, drawn into a texture you can show in
uGUI or on a mesh.

The package is a thin layer over the Zuri runtime (Rust, exposed through a C API). The runtime
renders on the CPU into a `Texture2D`; a GPU-native renderer is on the roadmap.

- [Install](#install)
- [Quick start](#quick-start)
- [API overview](#api-overview)
- [The frame loop and settling](#the-frame-loop-and-settling)
- [The texture](#the-texture)
- [Pointer input](#pointer-input)
- [Key input](#key-input)
- [Events](#events)
- [View-model binding](#view-model-binding)
- [Supported platforms](#supported-platforms)
- [Limitations](#limitations)
- [Rebuilding the native plugin](#rebuilding-the-native-plugin)
- [Tests](#tests)

## Install

Requires Unity 2021.3 or newer. See [Supported platforms](#supported-platforms) for where the
native plugin is available.

**From git**: Window > Package Manager > **+** > *Add package from git URL*:

```
https://github.com/SeedeXR/zuri.git?path=integrations/unity
```

**From disk**: clone the repository, then **+** > *Add package from disk* and pick
`integrations/unity/package.json`.

## Quick start

1. Drop a `.zur` file into the project. It imports as a **Zuri Asset** with a thumbnail.
   (The *Basic* sample in the Package Manager has two to try.)
2. Add **GameObject > UI > Raw Image**, then add **Zuri > Zuri Player** and
   **Zuri > Zuri Raw Image** to it.
3. Assign the asset to the player's **Asset** field. The first frame appears in the editor.
   Give the Raw Image the artboard's aspect ratio (the inspector shows the artboard size).
4. Make sure the scene has an Event System (**GameObject > UI > Event System**) so the artboard
   receives the pointer.
5. Press Play.

The player's inspector lists what the asset contains: pick the artboard and the state machine
or animation from dropdowns. While an instance exists it also shows a control for every
view-model property, which is a quick way to try an artboard's inputs.

From code:

```csharp
using UnityEngine;
using Zuri;

public class Example : MonoBehaviour
{
    [SerializeField] ZuriPlayer player;

    void OnEnable() => player.OnEvent.AddListener(OnZuriEvent);
    void OnDisable() => player.OnEvent.RemoveListener(OnZuriEvent);

    void OnZuriEvent(string eventName)
    {
        if (eventName == "clicked")
        {
            player.SetColor("accent", new Color32(0xff, 0x33, 0x66, 0xff));
        }
    }
}
```

## API overview

| Type | What it is |
| --- | --- |
| `ZuriAsset` | A `.zur` file as a `ScriptableObject`: the bytes plus the names of its artboards, animations, state machines and view-model properties. `ZuriAsset.Create(bytes)` makes one at runtime. |
| `ZuriPlayer` | `MonoBehaviour` that plays one artboard into a `Texture2D`. Owns the frame loop, the view-model API, `Pointer(...)`, `Key(...)` and the `OnEvent` UnityEvent. |
| `ZuriRawImage` | Shows a player in a uGUI `RawImage` and forwards EventSystem pointer events. |
| `ZuriQuad` | The same for a world-space quad (`Renderer` + collider). |
| `ZuriFile`, `ZuriInstance` | The engine-independent wrappers the components are built on. Pure C#, no `UnityEngine` references, `IDisposable`. Use them to render outside the component model. |
| `ZuriException` | Thrown when the runtime reports an error; the message is the runtime's own. |

`ZuriPlayer` settings:

| Field | Meaning |
| --- | --- |
| Asset | The `.zur` to play. |
| Artboard | Which artboard. Empty plays the first. |
| Playback | `Auto` (first state machine, else the first animation that no Joystick scrubs), `StateMachine` or `Animation`. |
| Playback Name | Which state machine or animation. Empty plays the first. |
| Fit | `Contain`, `Cover`, `Fill`, `None`, `FitWidth`, `FitHeight` or `ScaleDown` (`Contain` when the artboard is larger than the texture, its own size otherwise): how the artboard is scaled into the texture when the aspect ratios differ. |
| Alignment | `Center` (default), `TopLeft`, `TopCenter`, `TopRight`, `CenterLeft`, `CenterRight`, `BottomLeft`, `BottomCenter` or `BottomRight`: where the artboard sits in the room the fit leaves over, or which part shows when it overflows. |
| Resolution | Texture size in pixels. Zero uses the artboard's size. |
| Resolution Scale | Multiplies the size, for example 2 for high-density displays. |
| Background | Clear color. Transparent by default. |
| Premultiplied Alpha | Output premultiplied alpha instead of straight alpha. |
| Unscaled Time | Advance with `Time.unscaledDeltaTime`. |
| On Event | Invoked with the name of each event the artboard emits. |
| Muted | Do not play the sounds of the artboard's events. |
| Audio Source | What plays those sounds. Left empty, one is added to the object when the first sound plays. |

Settings are read when the player starts. After changing them from code, call
`player.Rebuild()`; assigning `player.Asset` does it for you.

Using the wrappers directly:

```csharp
using (ZuriFile file = ZuriFile.Load(bytes))
using (ZuriInstance instance = file.CreateInstance("Button"))
{
    instance.PlayStateMachine("Button");
    var pixels = new byte[320 * 160 * 4];
    if (instance.Advance(deltaSeconds))
    {
        instance.Render(pixels, 320, 160, ZuriFit.Contain, background: 0, premultiplied: false, alignment: ZuriAlignment.Center);
    }
}
```

`ZuriInstance` also has, for tools and inspectors:

| Member | What it does |
| --- | --- |
| `Seek(seconds)` | Scrubs the animation started with `PlayAnimation`; the next `Advance(0)` shows it. Returns `false` while a state machine (or nothing) is playing. |
| `LayerStates` | The object id of the state each layer of the playing state machine is in (`uint.MaxValue` for none). Empty while no state machine is playing. |
| `DeclaredEvents` | Names of the events `PollEvents` can report: the artboard's own and those of artboards nested in it. |
| `TryGetPropertyType(name, out type)` | Whether there is a view-model property of that name, and its type. A name can be a path into a nested artboard, `card/title` (or `rows/2/title` for the third copy of a list, a NestedArtboard with a `count`), here and in every getter and setter; `Properties` lists the outer artboard's. |
| `PropertiesIn(artboard)` | The view-model properties of a nested artboard, by its path: `card`, or `rows/2` for the third copy of a list. Empty where nothing is nested by that path, which a list without copies is too. Read anew at every call. |
| `TryGetProperty(objectId, key, out value)`, `TryGetPropertyColor(objectId, key, out argb)` | What an object currently draws with, after animation and binds, for a schema property key. Object ids are file indices. |

## The frame loop and settling

Each frame, in play mode, the player does this:

```csharp
if (instance.Advance(deltaTime))   // false: nothing changed since the last render
{
    instance.Render(...);          // straight into the texture's memory
    texture.Apply(false);
}
// then dispatch the events the artboard emitted
```

`Advance` returns `false` once nothing is moving, so an idle artboard (a button nobody is
touching, a finished one-shot animation) costs one cheap native call per frame and no rendering
or texture upload. Setting a view-model property to a new value, firing a trigger or a pointer
event that hits a listener wakes it up again.

`ZuriPlayer.Tick(deltaSeconds)` is that loop as a public method. It returns whether the texture
was redrawn. Call it yourself to drive an artboard manually, for example from an edit-mode tool.

In edit mode the player shows the first frame and does not animate.

Rendering allocates no managed memory: the runtime writes into
`Texture2D.GetRawTextureData<byte>()`, and property names are encoded to UTF-8 once and cached.

## The texture

`ZuriPlayer.Texture` is an `RGBA32` texture without mipmaps, marked sRGB (the runtime writes
sRGB-encoded colors, so it looks right in both Linear and Gamma projects). It is recreated by
`Rebuild()`; subscribe to `ZuriPlayer.TextureChanged` if you hold on to it.

**Its rows run top to bottom**, which is upside down for Unity. Flipping the pixels every frame
would cost a copy, so the flip is done when sampling instead:

- `ZuriRawImage` sets `RawImage.uvRect` to `(0, 1, 1, -1)`.
- `ZuriQuad` sets the material's main texture scale to `(1, -1)` and offset to `(0, 1)`.
- In your own material or shader, do the same: `uv.y = 1 - uv.y`.

Alpha is straight by default, which is what Unity's UI and most transparent shaders expect. Scaled
straight-alpha textures can show a dark fringe on soft edges; if that matters, enable
**Premultiplied Alpha** and use a material that blends `One OneMinusSrcAlpha`.

The texture is stretched over whatever shows it. Keep the display rect at the artboard's aspect
ratio, or set **Resolution** to the rect's aspect and let **Fit** and **Alignment** place the artboard.

`ZuriAsset.RenderPreview(width, height)` returns a one-off, upright texture of the first frame,
which is what the importer uses for the asset's thumbnail.

## Pointer input

State machines react to the pointer through listeners (enter, exit, down, up, click, move). Feed
them with `ZuriPlayer.Pointer(kind, pixel)`, where `pixel` is in texture pixels with the origin at
the **top-left** of the picture and y pointing **down**, the same space the artboard was rendered
into.

A press that goes far enough along a Layout that scrolls is a scroll, not a tap. How far is
`ZuriInstance.SetDragThreshold(pixels)`, in texture pixels: 8 until set. `ZuriRawImage` and
`ZuriQuad` set it at every press, to the EventSystem's `pixelDragThreshold` in texture pixels
(where the view stretches the texture, along the way that has more of them to the screen
pixel). If you feed `Pointer` yourself, set it yourself; it belongs to the instance, so set
it again when `ZuriPlayer` makes a new one (on enable, when its asset or artboard changes,
after `Rebuild`).

The scene has one pointer and one button, and the runtime keeps to them: a press begins when
a left button goes down (a finger is one) and ends when it goes up or its pointer leaves;
meanwhile another pointer is not the scene's, nor is a finger that came down beside it until
it lifts, nor another button. The components pass every pointer's events on with its id and
button, and end a press that came through them when they are disabled. If you feed the
pointer yourself with several pointers, do the same: `Pointer(kind, pixel, pointer, button)`
returns whether the event was the scene's.

`ZuriRawImage` and `ZuriQuad` do this for you from the EventSystem:

| EventSystem callback | Zuri pointer event |
| --- | --- |
| `OnPointerEnter`, `OnPointerMove` | `Move` |
| `OnPointerDown` | `Down` |
| `OnPointerUp` | `Up` |
| `OnPointerExit` | `Exit` |

- `ZuriRawImage` needs an Event System in the scene and a Graphic Raycaster on the canvas, and
  the Raw Image must be a raycast target. It works in overlay, camera and world-space canvases.
- `ZuriQuad` needs a collider on the object, a Physics Raycaster on the camera and an Event
  System. It expects Unity's built-in Quad mesh (or any mesh spanning -0.5..0.5 in local X and Y).
  It works out the position by intersecting the pointer ray with the quad's plane.

Both expose `ScreenToPixel(screenPosition, camera, out pixel)` if you need the mapping for your
own input code.

Pointer events only do something while a state machine is playing.

## Key input

State machines can also listen for keys (keyDown, keyUp). `ZuriPlayer.Key(key, down)` tells the
artboard that a key went down or up and returns whether a listener reacted. `key` is the key's
name as the web's `KeyboardEvent.key` gives it: `"Enter"`, `"Escape"`, `"Tab"`, `"Backspace"`,
`"ArrowLeft"`, `"ArrowRight"`, `"ArrowUp"`, `"ArrowDown"`, `" "` for the space bar, or the
character typed (`"a"`, `"A"`, `"1"`). `ZuriPlayer.KeyName(KeyCode)` returns that name for the
keys that do not type a character (Return and keypad Enter, Escape, Tab, Space, Backspace, Delete,
the arrows, Home, End, Page Up, Page Down) and null for every other key.

**Keys are not forwarded for you**: `ZuriRawImage` and `ZuriQuad` forward the pointer only.
Unity has two input backends and the package references neither, and the one API both could
share, `OnGUI`, receives no input in play mode when only the Input System package is active.
Forward the keys your artboard listens for from your own script, for example in `Update`:

```csharp
// Input Manager (UnityEngine.Input)
if (Input.GetKeyDown(KeyCode.Return)) player.Key(ZuriPlayer.KeyName(KeyCode.Return), true);

// Input System package (UnityEngine.InputSystem)
if (Keyboard.current != null && Keyboard.current.enterKey.wasPressedThisFrame) player.Key("Enter", true);
```

## Events

Artboards emit named events, for example from a state-machine transition. The player collects
them after every advance and invokes `OnEvent` (a `UnityEvent<string>`, also assignable in the
inspector) once per event, in order. With the wrappers, call `ZuriInstance.PollEvents(list)`.
`ZuriInstance.DeclaredEvents` lists the events that can be reported (the artboard's own and
those of artboards nested in it); the player's inspector shows them while an instance exists.

An event can carry a sound (an `AudioAsset` in the file, see the
[format guide](../../docs/user/format.md)). In play mode the player plays it, before it invokes
`OnEvent`, with `PlayOneShot` on its `AudioSource` at the event's volume. Assign an
`AudioSource` of your own to send the sounds to a mixer group or to place them in the world;
set `Muted` to play none. The sound must be an uncompressed WAV file (PCM of 8, 16, 24 or 32
bits, or 32-bit float): that is what can be decoded from bytes at run time, and any other
format is skipped, with a warning when the file is imported and when the sound is first due. With the wrappers, `ZuriInstance.TryGetEventSound(index, out
sound, out volume)` gives the id of the sound of an event from the last `PollEvents`,
`ZuriFile.SoundId(index)` lists the ids, `ZuriFile.GetEncodedSound(id)` gives a sound's bytes
and `ZuriAsset.GetSound(id)` the decoded `AudioClip`.

## View-model binding

An artboard's view model is a set of named, typed properties that its state machines and bound
properties read. They are the artboard's inputs and outputs.

| Type | Set | Get |
| --- | --- | --- |
| Number | `SetNumber(name, float)` | `GetNumber(name)` |
| Bool | `SetBool(name, bool)` | `GetBool(name)` |
| Color | `SetColor(name, Color32)` | `GetColor(name)` |
| String | `SetString(name, string)` | `GetString(name)` |
| Enum | `SetNumber(name, position)` or `SetString(name, optionName)` | `GetNumber(name)` or `GetString(name)` |
| Trigger | `Fire(name)` | |

- Setters return `false` if the artboard has no property with that name, or nothing is playing.
  Getters throw in those cases (`ZuriException`, `InvalidOperationException`).
- The runtime changes properties too (a listener can count clicks), so read them back whenever
  you need them.
- Setting a property every frame is fine: it does not allocate, and setting the value it
  already has does not trigger a render.
- Values are not saved with the scene. They reset when the player is rebuilt or disabled and
  when the domain reloads (entering play mode, recompiling).
- An enum is one of its Options. As a number it is the Option's position, counted from 0; as
  a string it is the Option's name. `SetString` returns `false` for a name that is not an Option.
- `ZuriInstance.Properties` (or `ZuriAsset.Artboards[i].Properties`) lists names and types,
  and an enum's Options (`ZuriProperty.Options`, empty for the other types).
  On `ZuriInstance`, colors are `uint` `0xAARRGGBB`.

## Supported platforms

| Platform | Native plugin | Status |
| --- | --- | --- |
| macOS editor and standalone (arm64 + x86_64) | `Plugins/macOS/libzuri.dylib`, built by `scripts/build-unity-plugins.sh` (not stored in git; its `.meta` files are copied from `PluginMeta~/`) | Built. Tested with Unity 6000.4.8f1 on an Apple-silicon Mac: EditMode and PlayMode tests in the editor (arm64, Metal), the PlayMode ones in both Gamma and Linear color space, and a standalone Mono player run as arm64 and, under Rosetta, as x86_64. |
| Windows x86_64 | `Plugins/Windows/x86_64/zuri.dll`, built by the same script on Windows (Git Bash); its `.meta` files come from `PluginMeta~/` | The library builds in CI on Windows, and the C# bindings pass the .NET harness there against a library built from the same source. Never opened in Unity on Windows. |
| Linux x86_64 | `Plugins/Linux/x86_64/libzuri.so`, built by the same script on Linux | As for Windows: built and driven from C# in CI, never opened in Unity on Linux. |
| Android | not shipped | Build commands are in the script. Untested. |
| iOS | not shipped | Build commands are in the script (static library, `__Internal`). Untested. |
| WebGL | not shipped | Needs an Emscripten build of the runtime; not attempted. |

The C# code has been compiled and tested with Unity 6000.4.8f1 only. It is written against the
Unity 2021.3 API and C# 9, but has not been run on 2021.3 or 2022.3. Only the built-in render
pipeline has been drawn with. IL2CPP builds are untested: the test machine's editor has no
IL2CPP build support for macOS (a build stops with "Currently selected scripting backend
(IL2CPP) is not installed").

On a platform without the plugin, importing a `.zur` reports an import error and a player logs
an error instead of playing.

## Limitations

- **CPU rendering.** Every changed frame is rasterized on the CPU and uploaded. Large textures
  that animate continuously are expensive; settle-skipping only helps while nothing moves.
- **Shader paints are not supported in Unity yet.** They fall back to the paint's color.
- **Images**: Unity decodes PNG and JPEG. WebP image assets are skipped with a warning.
- **Pointer release outside the artboard**: listeners without a target only fire inside the
  artboard, so an artboard that clears its "pressed" state on `up` can stay pressed when the
  pointer is released outside it. This is runtime behavior, not specific to Unity.
- **Sounds** must be uncompressed WAV; MP3, OGG and the rest are not played (see [Events](#events)).
  A sound cannot be stopped, looped or pitched.
- **One pointer.** There is no multi-touch: the scene follows the pointer that pressed, and a second finger is not the scene's (see [Pointer input](#pointer-input)).
- **No keyboard focus.** Keys are not forwarded automatically; see [Key input](#key-input).
- **Edit mode** shows a still frame. Players restart from the beginning, with view-model values
  reset, on domain reload and when the component is disabled and re-enabled.
- **Reimporting a `.zur`** does not restart players that are already showing it; toggle the
  component or call `Rebuild()`. Until then a player may play the wrong sound for an event, or
  none: it still plays the old file, whose sounds can have other ids than the new one's.
- `ZuriFile` and `ZuriInstance` wrap only the playback API. The C API's editor helpers (hit
  testing, bounds, custom view transforms), shader hand-off and JSON authoring functions are
  bound in `ZuriNative` but have no wrapper yet.
- A handle must be used from one thread at a time. The components use the main thread.

## Building the native plugin

A fresh clone has no native library: build it once before opening the package in Unity.

The plugin is the `zuri-ffi` crate. On macOS, with Rust and the `aarch64-apple-darwin` and
`x86_64-apple-darwin` targets installed, run from the repository root:

```sh
bash scripts/build-unity-plugins.sh
```

It builds both architectures, merges them into `Plugins/macOS/libzuri.dylib`, and recompiles the
`.zur` files of the sample and the tests from `examples/*.json`. If the x86_64 slice does not
build, the script says so and produces an arm64-only library.

The same script lists the commands for Windows, Linux, Android and iOS as comments. Those have
not been run. Each new binary also needs a `.meta` with its platform settings.

The bindings check the library's ABI version (`zuri_abi_version() == 3`) when a file is loaded
and throw a `ZuriException` on a mismatch.

## Tests

- **EditMode tests** (`Tests/Editor`): import, rendering through `ZuriPlayer`, settling, pointer
  mapping for `ZuriRawImage` and `ZuriQuad`, events, image decoding, a round trip through
  play mode (domain reload included), and the `ZuriPlayer` inspector: it is drawn in an editor
  window and its *Fire* button is clicked with IMGUI mouse events.

- **PlayMode tests** (`Tests/Runtime`): what a camera really draws and what the EventSystem's
  raycasters really find. A `ZuriRawImage` on a *Screen Space - Camera* canvas and a `ZuriQuad`
  in front of an orthographic camera are rendered into a render texture and read back: the
  picture is upright, sits where the rect or quad is, and has the artboard's colors. A
  `GraphicRaycaster` (canvas) and a `PhysicsRaycaster` (quad) find the view at a screen
  position, and the pointer events sent to what they found hover and click the button, raise
  its event and draw its tick. An event with a sound gives the decoded clip to an
  `AudioSource`, and none while the player is muted (that the clip is heard is not checked).
  They run in the editor's play mode, not in a built player.
  The test assembly needs the Physics module (`com.unity.modules.physics`) in the project,
  for the quad's collider and the `PhysicsRaycaster`; without it the assembly does not compile.

  Add the package to the project's `testables` and run them from the Test Runner, or from the
  command line:

  ```sh
  Unity -batchmode -projectPath <project> -runTests -testPlatform EditMode -testResults results.xml
  Unity -batchmode -projectPath <project> -runTests -testPlatform PlayMode -testResults results.xml
  ```

  Batch mode still creates a graphics device. With `-nographics` (a machine without a display)
  the PlayMode tests and the inspector test are skipped, not failed.

  From this repository, `bash scripts/verify-engines.sh unity` does all of it: it builds the
  native plugin, runs the .NET harness, makes a throwaway project and runs the EditMode tests,
  then the PlayMode tests once in Gamma and once in Linear color space. Unity runs with a
  graphics device there, and the gate fails if a test that needs the GPU is skipped.

  Not covered by any test: *Screen Space - Overlay* canvases on screen (their pointer mapping
  is tested, their pixels are not), world-space canvases, an input module with a real mouse or touch
  (the tests raycast and send the pointer events themselves), how the inspector looks, its
  dropdowns and its controls in play mode, URP and HDRP, and IL2CPP.

- **Plain .NET harness** (`Tests~/DotnetHarness`): compiles `ZuriNative.cs`, `ZuriFile.cs`,
  `ZuriInstance.cs` and `ZuriWav.cs` as they are and drives the native library without Unity:

  ```sh
  integrations/unity/Tests~/DotnetHarness/run.sh
  ```

## License

MIT OR Apache-2.0, like the rest of Zuri. See [LICENSE.md](LICENSE.md).
