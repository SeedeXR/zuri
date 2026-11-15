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
| `ZuriPlayer` | `MonoBehaviour` that plays one artboard into a `Texture2D`. Owns the frame loop, the view-model API, `Pointer(...)` and the `OnEvent` UnityEvent. |
| `ZuriRawImage` | Shows a player in a uGUI `RawImage` and forwards EventSystem pointer events. |
| `ZuriQuad` | The same for a world-space quad (`Renderer` + collider). |
| `ZuriFile`, `ZuriInstance` | The engine-independent wrappers the components are built on. Pure C#, no `UnityEngine` references, `IDisposable`. Use them to render outside the component model. |
| `ZuriException` | Thrown when the runtime reports an error; the message is the runtime's own. |

`ZuriPlayer` settings:

| Field | Meaning |
| --- | --- |
| Asset | The `.zur` to play. |
| Artboard | Which artboard. Empty plays the first. |
| Playback | `Auto` (first state machine, else first animation), `StateMachine` or `Animation`. |
| Playback Name | Which state machine or animation. Empty plays the first. |
| Fit | `Contain`, `Cover`, `Fill` or `None`: how the artboard sits in the texture when the aspect ratios differ. |
| Resolution | Texture size in pixels. Zero uses the artboard's size. |
| Resolution Scale | Multiplies the size, for example 2 for high-density displays. |
| Background | Clear color. Transparent by default. |
| Premultiplied Alpha | Output premultiplied alpha instead of straight alpha. |
| Unscaled Time | Advance with `Time.unscaledDeltaTime`. |
| On Event | Invoked with the name of each event the artboard emits. |

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
        instance.Render(pixels, 320, 160, ZuriFit.Contain, background: 0, premultiplied: false);
    }
}
```

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
ratio, or set **Resolution** to the rect's aspect and let **Fit** place the artboard.

`ZuriAsset.RenderPreview(width, height)` returns a one-off, upright texture of the first frame,
which is what the importer uses for the asset's thumbnail.

## Pointer input

State machines react to the pointer through listeners (enter, exit, down, up, click, move). Feed
them with `ZuriPlayer.Pointer(kind, pixel)`, where `pixel` is in texture pixels with the origin at
the **top-left** of the picture and y pointing **down**, the same space the artboard was rendered
into.

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

## Events

Artboards emit named events, for example from a state-machine transition. The player collects
them after every advance and invokes `OnEvent` (a `UnityEvent<string>`, also assignable in the
inspector) once per event, in order. With the wrappers, call `ZuriInstance.PollEvents(list)`.

## View-model binding

An artboard's view model is a set of named, typed properties that its state machines and bound
properties read. They are the artboard's inputs and outputs.

| Type | Set | Get |
| --- | --- | --- |
| Number | `SetNumber(name, float)` | `GetNumber(name)` |
| Bool | `SetBool(name, bool)` | `GetBool(name)` |
| Color | `SetColor(name, Color32)` | `GetColor(name)` |
| String | `SetString(name, string)` | `GetString(name)` |
| Trigger | `Fire(name)` | |

- Setters return `false` if the artboard has no property with that name, or nothing is playing.
  Getters throw in those cases (`ZuriException`, `InvalidOperationException`).
- The runtime changes properties too (a listener can count clicks), so read them back whenever
  you need them.
- Setting a property every frame is fine: it does not allocate, and setting the value it
  already has does not trigger a render.
- Values are not saved with the scene. They reset when the player is rebuilt or disabled and
  when the domain reloads (entering play mode, recompiling).
- `ZuriInstance.Properties` (or `ZuriAsset.Artboards[i].Properties`) lists names and types.
  On `ZuriInstance`, colors are `uint` `0xAARRGGBB`.

## Supported platforms

| Platform | Native plugin | Status |
| --- | --- | --- |
| macOS editor and standalone (arm64 + x86_64) | `Plugins/macOS/libzuri.dylib`, built by `scripts/build-unity-plugins.sh` (not stored in git; its `.meta` is) | Built. Tested with Unity 6000.4.8f1 on an Apple-silicon Mac: EditMode tests in the editor (arm64) and a standalone Mono player run as arm64 and, under Rosetta, as x86_64. |
| Windows x86_64 | not shipped | Build commands are in `scripts/build-unity-plugins.sh`. Untested. |
| Linux x86_64 | not shipped | Build commands are in the script. Untested. |
| Android | not shipped | Build commands are in the script. Untested. |
| iOS | not shipped | Build commands are in the script (static library, `__Internal`). Untested. |
| WebGL | not shipped | Needs an Emscripten build of the runtime; not attempted. |

The C# code has been compiled and tested with Unity 6000.4.8f1 only. It is written against the
Unity 2021.3 API and C# 9, but has not been run on 2021.3 or 2022.3. IL2CPP builds are untested.

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
- **One pointer.** There is no multi-touch: every pointer is fed to the same state machine.
- **Edit mode** shows a still frame. Players restart from the beginning, with view-model values
  reset, on domain reload and when the component is disabled and re-enabled.
- **Reimporting a `.zur`** does not restart players that are already showing it; toggle the
  component or call `Rebuild()`.
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

The bindings check the library's ABI version (`zuri_abi_version() == 1`) when a file is loaded
and throw a `ZuriException` on a mismatch.

## Tests

- **EditMode tests** (`Tests/Editor`): import, rendering through `ZuriPlayer`, settling, pointer
  mapping for `ZuriRawImage` and `ZuriQuad`, events, image decoding, and a round trip through
  play mode (domain reload included). Add the package to the
  project's `testables` and run them from the Test Runner, or headless:

  ```sh
  Unity -batchmode -nographics -projectPath <project> -runTests -testPlatform EditMode -testResults results.xml
  ```

- **Plain .NET harness** (`Tests~/DotnetHarness`): compiles `ZuriNative.cs`, `ZuriFile.cs` and
  `ZuriInstance.cs` as they are and drives the native library without Unity:

  ```sh
  integrations/unity/Tests~/DotnetHarness/run.sh
  ```

## License

MIT OR Apache-2.0, like the rest of Zuri. See [LICENSE.md](LICENSE.md).
