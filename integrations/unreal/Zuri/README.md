# Zuri for Unreal Engine

Plays [Zuri](https://github.com/SeedeXR/zuri) scenes (`.zur`) in Unreal: interactive vector
animation with state machines, pointer input, events and data binding, in UMG and on meshes.

Written for Unreal Engine 5.8. Status per platform is in [Platforms](#platforms).

## Install

1. Build the runtime library. From the root of the Zuri repository:

   ```sh
   bash scripts/build-unreal-libs.sh
   ```

   It needs a Rust toolchain. It writes `zuri.h` and the static library into
   `Source/ThirdParty/ZuriLibrary/`; the plugin does not compile without them.

2. Copy or symlink `integrations/unreal/Zuri` into your project's `Plugins/` folder:

   ```sh
   ln -s /path/to/zuri/integrations/unreal/Zuri /path/to/MyProject/Plugins/Zuri
   ```

3. Open the project and let Unreal compile the plugin. It needs a C++ toolchain (Xcode on a
   Mac), also in a Blueprint-only project.

## Quick start

### UMG

1. Drag a `.zur` file into the Content Browser. It becomes a **Zuri File** asset.
   `Resources/button.zur` in this plugin is a small one to try: a button with hover and press
   states.
2. In a Widget Blueprint, add **Zuri** from the palette's Zuri section.
3. Set **File**. Leave **Artboard** and **Play Name** empty for the first artboard and its
   first state machine.

The widget draws at its own size, animates, and reacts to the mouse and to touch.

### Blueprint

Read and write the artboard's view model through the widget's instance, and listen for its
events:

```text
Event Construct
  -> ZuriWidget . Get Instance . Set Color  (Name "accent", Color)
  -> ZuriWidget . Get Instance . Set Number (Name "clicks", Value 0)

ZuriWidget . On Zuri Event (Event Name)
  -> Switch on Name: "clicked" -> ...
```

### C++

```cpp
#include "ZuriFile.h"
#include "ZuriInstance.h"

UZuriInstance* Instance = UZuriInstance::Create(this, File, TEXT("Button"));
Instance->SetColor(TEXT("accent"), FColor(0xff, 0x33, 0x66));

// Each frame:
Instance->Advance(DeltaSeconds);
if (Instance->Render(Width, Height))
{
	// Instance->GetTexture() has new pixels (and is a new texture if the size changed).
}
```

Add `"Zuri"` to your module's dependencies. Keep the instance in a `UPROPERTY`.

## API

| Class | What it is |
| --- | --- |
| `UZuriFile` | The asset: the `.zur` bytes, with `ArtboardNames` and `Sounds` (a Sound Wave for each sound in the file, made on import). |
| `UZuriInstance` | One playing copy of an artboard, drawn into a `UTexture2D`. Everything else is built on it. |
| `UZuriWidget` | UMG widget. Properties `File`, `Artboard`, `PlayName`, `Fit`, `Alignment`, `IsFocusable`, `bMuted`; `GetInstance()`; `SetFile()`, `SetFit()`, `SetAlignment()`; `OnZuriEvent`. `bMuted` is what each new instance starts with; while playing, set `GetInstance()->bMuted`. |
| `SZuriWidget` | The Slate widget behind `UZuriWidget`, for Slate-only UI: `SetInstance()`. |
| `UZuriComponent` | Actor component that plays an artboard into a texture parameter of a dynamic material. Properties include `Fit`, `Alignment` and `bMuted` (what the instance starts with). |

`UZuriInstance`, all of it callable from Blueprint:

| Group | Functions |
| --- | --- |
| Create | `Create(Outer, File, Artboard)`, `CreateByIndex(Outer, File, ArtboardIndex)` |
| Playback | `Play(Name)`, `Advance(DeltaSeconds)`, `Seek(Seconds)` (animations only), `GetLayerStates()`, `GetStateMachineNames()`, `GetAnimationNames()`, `GetArtboardSize()` |
| Rendering | `Render(Width, Height)`, `GetTexture()`, `SetFit()`, `GetFit()`, `SetAlignment()`, `GetAlignment()` |
| View model | `GetProperties()`, `GetPropertiesIn(Artboard)`, `HasProperty(Name)`, `SetNumber`/`GetNumber`, `SetBool`/`GetBool`, `SetColor`/`GetColor`, `SetString`/`GetString`, `FireTrigger`. A name can be a path into a nested artboard: `card/title`, or `rows/2/title` for the third copy of a list (a NestedArtboard with a `count`). |
| Input | `PointerMove`, `PointerDown`, `PointerUp`, `PointerExit`, `SetDragThreshold`, `Key` |
| Events | `OnZuriEvent(EventName)`, `GetDeclaredEvents()`, `OnZuriSound(Sound, Volume)`, `bMuted` |

`EZuriFit` is `Contain` (default), `Cover`, `Fill`, `None`, `FitWidth`, `FitHeight` or `ScaleDown`
(`Contain` when the artboard is larger than the target, its own size otherwise). `EZuriAlignment`
says where the artboard sits in the room the fit leaves over, or which part shows when it
overflows: `Center` (default), `TopLeft`, `TopCenter`, `TopRight`, `CenterLeft`, `CenterRight`,
`BottomLeft`, `BottomCenter` or `BottomRight`.

C++ also has `SetColor(Name, FColor)`, `GetPixels()` with `GetRenderSize()` for the last
frame's pixels, and `GetObjectProperty(Object, Key, OutValue)` / `GetObjectPropertyColor(...)`
for what an object currently draws with for a schema property key.

Names of artboards, state machines, animations and properties are case sensitive.

## Frame loop and render skipping

The runtime draws on the CPU into an RGBA8 buffer, and the plugin uploads that buffer to a
transient `UTexture2D`. Both steps are skipped whenever they can be:

- `Advance` returns whether the picture may have changed. An idle state machine settles and
  reports no change.
- `Render` draws and uploads only if something changed since the last draw: time advanced into
  a different picture, a property was set, the pointer did something, the fit or alignment changed, or the
  size is new. Otherwise it returns `false` and costs nothing.

`UZuriWidget` and `UZuriComponent` call both every frame, so a settled button costs one
`Advance` per frame and no drawing. A new size makes a new texture; the same size reuses it.

The texture is `PF_R8G8B8A8`, sRGB, with straight (not premultiplied) alpha, which is what Slate
and a material's color and opacity inputs expect.

Call into an instance from the game thread only.

## Input

`UZuriWidget` forwards mouse enter, move, leave, and left button down and up, converted to
pixels of its texture. A touch arrives the same way, followed by an exit when the finger lifts.
The widget takes the left button and captures the mouse while it is held. For decoration that
should not take clicks, set the widget's Visibility to **Not Hit-Testable**.

To drive an instance yourself, call `PointerMove`, `PointerDown`, `PointerUp` and `PointerExit`
with positions in pixels of the last `Render`, origin at the top left. `UZuriComponent` forwards
no input: trace against your mesh, turn the hit's UV into pixels (`UV * TextureSize`), and call
these. A press that goes far enough along a Layout that scrolls is a scroll, not a tap:
`SetDragThreshold` says how far in those pixels (8 until set). `UZuriWidget` sets it at every
press, to Slate's drag trigger distance (which is in pixels of the screen); if you feed the
pointer yourself, set it yourself, and again on a new instance. The scene has one pointer and
one button, and the runtime keeps to them: the pointer calls take the pointer's index and the
button (the widget passes Slate's) and return whether the event was the scene's; a press ends
when the left button is let go, its pointer leaves, or Slate takes the capture away.

### Keys

A scene can listen for keys. `UZuriWidget` forwards them while it has the keyboard focus, which
it takes only with **Is Focusable** set (off by default): on a click, by keyboard navigation, or
from Blueprint with Set Keyboard Focus. It keeps a key only if the scene reacted to it, so Tab,
the arrows and every other key the scene does not listen for still navigate the UI.

To send keys yourself, call `Key(KeyName, bDown)`; it returns whether a listener reacted. The
names are those of the web's `KeyboardEvent.key`, case sensitive: `Enter`, `Escape`, `Tab`,
`ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Backspace`, `Delete`, `Home`, `End`,
`PageUp`, `PageDown`, a space for the space bar, and otherwise the character typed (`a`, `A`,
`1`). The widget sends a character key when its character arrives, so Shift and the keyboard
layout count, and sends its release with the same character. A key held down is sent once: its
repeats are not sent, and stay the scene's if the first press was. A key still down when the
widget loses the focus is released then. The example button listens for
Enter, which clicks it.

## Events

A state machine emits named events (the example button emits `clicked`). `Advance` broadcasts
each one through `OnZuriEvent`, on the instance and on the `UZuriWidget` that owns it. The name
is an `FName`, so comparing it ignores case.

An event can carry a sound (an `AudioAsset` in the file, see the
[format guide](../../../docs/user/format.md)). Importing the `.zur` makes a Sound Wave of each
one with Unreal's own sound importer and keeps it inside the Zuri File asset (`Sounds`), so a
packaged game needs no decoder. `Advance` then plays the sound of each event as a 2D sound at
the event's volume, in a game world only: not in the UMG designer, and not for an instance
whose outer has no world. Just before `OnZuriEvent` it also broadcasts `OnZuriSound(Sound,
Volume)` on the instance, muted or not. To play the sounds your own way (at the place of a
surface in the world, say, or through a Sound Class), set `bMuted` and play them from
`OnZuriSound`. WAV is tested; other formats are handed to Unreal's importer as they are, and
a sound it cannot read is left out with an error in the import log.

## View-model binding

An artboard's view model is a list of typed properties that its animations and state machine
read. `GetProperties` lists them, each with its type: number, bool, trigger, color, string or
enum. The example button has `hover` and `pressed` (bool), `clicks`
(number), `accent` (color) and `pulse` (trigger).

- Setters return `false`, and log a warning, if there is no property of that name.
- Getters return zero, black or an empty string if there is none.
- A bool is the number 0 or 1, so `GetNumber` and `SetNumber` work on it too.
- `GetPropertiesIn` lists those of an artboard nested in it, by its path: `card`, or `rows/2`
  for the third copy of a list. It is empty where nothing is nested by that path, which a list
  without copies is too.
- An enum property is on one of its named options, which `GetProperties` lists in `Options`.
  `SetNumber` and `GetNumber` use the option's position, from 0; `SetString` and `GetString` use
  its name, and `SetString` refuses a name that is not an option.
- `SetColor` takes an `FLinearColor`, which Blueprint color pins are, and stores it as 8-bit sRGB.
  In C++, `SetColor(Name, FColor)` stores an authored sRGB value exactly.
- A change shows on the next `Advance` and `Render`.

## World-space surfaces

Add a **Zuri** component to an actor and set **File**. At Begin Play, create a dynamic material
instance for your mesh and assign it to the component's **Material**; the component sets the
material's `ZuriTexture` texture parameter (rename it with **Texture Parameter Name**) and
redraws at **Texture Size** when the picture changes. In the material, use the texture's RGB as
color and its alpha as opacity.

## Platforms

| Platform | Library | Status |
| --- | --- | --- |
| Mac (arm64, x86_64) | `lib/Mac/libzuri.a`, universal | Compiles and links with UE 5.8 for the editor and for game targets. The automation tests below pass in the editor on the Metal RHI (run on Apple silicon, off screen): they read back from the GPU the texture upload, `UZuriWidget` drawn by Slate at a layout scale of 2, and `UZuriComponent`'s dynamic material, and they drive the widget's pointer handlers. Not yet checked: what [Tests](#tests) lists as not covered, mainly a widget in a window on screen with a real mouse or finger, and a mesh drawn in a level. |
| Win64 | `lib/Win64/zuri.lib` | Not tested. The library cross-compiles (`build-unreal-libs.sh win64`), but the plugin has never been compiled, linked or run on Windows. |
| Linux (x86_64) | `lib/Linux/libzuri.a` | Not tested, as for Win64 (`build-unreal-libs.sh linux`). |
| iOS, Android, consoles | none | Not supported yet. |

## Limitations

- Rendering is on the CPU, with one texture upload per changed frame. Large, constantly
  animating surfaces cost accordingly. A GPU renderer on Unreal's render graph is on the roadmap.
- Shader paints are not supported in Unreal yet: they are drawn with the paint's color instead.
- Embedded images are decoded by Unreal, so the formats are the ones it reads (PNG, JPEG and
  others). WebP is not one of them; such an image is skipped with a warning.
- A sound is played 2D and cannot be stopped, looped or pitched.
- `UZuriComponent` forwards no pointer input (see [Input](#input)).
- Reimporting a file does not restart instances that are already playing; they keep the old
  version, with its sounds, until they are created again.
- A Zuri File asset that has sounds but no Sound Waves (one filled with `SetData`, or imported
  by a build of the plugin from before it made them) plays none; creating an instance of it
  logs a warning that says to reimport.
- Zuri File assets have no thumbnail or preview in the Content Browser yet.
- A render target is at most 16384 pixels on a side.

## Rebuilding the static library

```sh
bash scripts/build-unreal-libs.sh              # the host's platform
bash scripts/build-unreal-libs.sh mac win64    # a list of: mac, win64, linux
```

The script builds the `zuri-ffi` crate as a static library for each platform (on a Mac, arm64
and x86_64 joined with `lipo`), copies it to `Source/ThirdParty/ZuriLibrary/lib/<Platform>/`
with `zuri.h` in `include/`, and rebuilds the scenes in `Resources/` that the tests load.
Each target needs its Rust target installed, for example
`rustup target add x86_64-pc-windows-msvc`.

`ZuriLibrary.Build.cs` links the library and the system libraries rustc lists for it:

```sh
cargo rustc -p zuri-ffi --release --target <triple> --crate-type staticlib -- --print native-static-libs
```

Rebuild after pulling changes to the runtime. The plugin logs an error at startup if the
library's ABI version is not the one it was written for.

## Tests

The editor module has nine automation tests.

Six run anywhere, also without a GPU. `Zuri.Scroll` drags a Layout that scrolls
(`Resources/scroll.zur`, built from `integrations/unreal/fixtures/`): a press within the drag
threshold is a tap, and `SetDragThreshold` moves it. `Zuri.Button` imports `Resources/button.zur`, draws
it, and checks pixels, render skipping, data binding, pointer input, events and reimport. `Zuri.Image`
checks that an embedded PNG is decoded with its channels in the right order
(`Resources/image.zur`, built from `integrations/unreal/fixtures/`). `Zuri.Enum` checks an enum
property: its options are listed, and it is set and read by name and by position
(`Resources/enum.zur`, from the same folder). `Zuri.Keys` checks key input: Enter by name clicks
the button, and a `UZuriWidget` turns Slate's key and character events into key names for a scene
that counts them, without the repeats of a held key and with a release when the focus is lost
(`Resources/keys.zur`, from the same folder). `Zuri.Sound` imports `Resources/sound.zur`: a
Sound Wave of the file's WAV is inside the asset, `OnZuriSound` reports it with the event's
volume, a reimport makes it again while an instance from before keeps the wave it had, a
file filled with `SetData` warns that it has no Sound Waves, and the asset still has its
wave after being saved, unloaded and loaded from disk (the file is written to the project's `Saved` folder and deleted). If
the editor has an audio device (it has none with `-nosound`), a `UZuriComponent` in a
playing world then taps the loaded scene: the audio device starts the wave, and does not
while the component is muted. That plays a 60 ms click.

Three need a GPU and read pixels back from it:

- `Zuri.Texture`: the instance's texture on the GPU is, pixel for pixel, what the runtime drew,
  both for a new texture and for one updated in place. That covers the channel order and which
  row is the top.
- `Zuri.Widget`: a `UZuriWidget` drawn by Slate (`FWidgetRenderer` into a render target) at a
  layout scale of 2, away from the window's corner. The instance is drawn at the widget's size
  in pixels; the picture is in place and upright; a redrawn frame and a new size show. Then
  mouse enter, left button down and up, mouse move and a touch are given to the widget's
  handlers, with the geometry Slate's hit-test grid holds for the widget at that pixel: `hover`,
  `pressed` and `clicks` change, and `clicked` arrives at the widget's `OnZuriEvent`. The
  mouse-down reply asks for the keyboard focus, and an Enter key event clicks the button while
  a Tab key event is left unhandled.
- `Zuri.Component`: a `UZuriComponent` on an actor with a mesh, in a playing world. The mesh's
  dynamic material gets the instance's texture, and again after `TextureSize` changes; the
  material, drawn to a render target, shows the button and a redrawn frame.

Not covered by any test, and not checked by hand either:

- A widget in a window on screen, in the UMG designer, in Play In Editor or in a packaged game.
- Sounds in a cooked or packaged game, sounds from a `UZuriWidget` (the test uses the
  component), that the UMG designer stays silent, and any sound format other than WAV.
- Input that Slate routes itself from a real mouse, touch screen or keyboard: the tests call
  the widget's handlers, so mouse capture, focus and keyboard navigation are not exercised.
- A mesh drawn in a level: the material is drawn on its own, with `DrawMaterialToRenderTarget`.
- Translucent pixels. Both test scenes are opaque wherever they draw, so that Slate and a
  material blend the texture's straight alpha correctly is not checked.
- Any RHI other than Metal on Apple silicon.

```sh
UnrealEditor-Cmd MyProject.uproject -ExecCmds="Automation RunTests Zuri; Quit" -unattended -RenderOffscreen -log
```

With `-nullrhi` in place of `-RenderOffscreen` the editor needs no GPU and leaves the three
tests that need one out.

From this repository, `bash scripts/verify-engines.sh unreal` builds the library, packages
the plugin with `RunUAT BuildPlugin`, and runs all nine tests in a throwaway project, on the
GPU without a window and with the machine's audio output (the gate fails without one). The
throwaway project turns off the editor's crash-report and usage-data uploads to Epic. On a Mac that needs the Xcode Metal Toolchain
(`xcodebuild -downloadComponent MetalToolchain`); the script fails without it.

## License

Apache-2.0, like the rest of Zuri. See [LICENSE](LICENSE).
