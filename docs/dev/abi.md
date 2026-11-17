# The C ABI

`crates/zuri-ffi/include/zuri.h` is the contract between the runtime and every host. The
web wrapper, the Unity bindings and the Unreal plugin are all thin layers over it.

## Conventions

| Topic | Rule |
|---|---|
| Handles | Opaque pointers (`ZuriFile*`, `ZuriInstance*`). Free each with its `_free`. NULL is always accepted. An instance keeps its file alive. |
| Strings in | UTF-8 pointer + byte length. No terminator needed. |
| Strings out | Pass a buffer and its capacity. The full byte length is returned; at most `cap` bytes are written; no terminator. Call with `cap = 0` to measure. |
| Failure | `false` or NULL. `zuri_last_error` gives the reason (per thread). |
| Memory | `zuri_alloc(len)` is zeroed and 8-byte aligned; `zuri_free(ptr, len)` needs the same `len`. Buffers returned through an `out_len` are freed the same way. |
| Colors | `0xAARRGGBB`. |
| Pixels | RGBA8, rows top to bottom, straight or premultiplied alpha as requested. |
| Threads | One thread at a time per handle. Different handles may be used concurrently. |
| Panics | Never cross the boundary: every entry point catches them and reports an error. |
| Versions | `zuri_abi_version()` is 2 (1 until `zuri_instance_pointer` took the pointer and the button, 2026-11-17) and the header defines `ZURI_ABI_VERSION`; compare them at startup. It changes only for incompatible changes. Nothing has been released yet, so 1 means "this repository": wrappers and the runtime are built together, and additions made before the first release (new calls, new `fit` values, the alignment bits) do not bump it. From the first release on, a value an older runtime would misread is an incompatible change. |

## The frame loop

```c
ZuriFile *file = zuri_file_load(bytes, len);
ZuriInstance *inst = zuri_instance_new(file, 0);
zuri_instance_autoplay(inst);

/* every frame */
if (zuri_instance_advance(inst, dt))                  /* false: nothing changed */
    zuri_instance_render(inst, pixels, width, height, ZURI_FIT_CONTAIN, 0, false);
uint32_t n = zuri_instance_poll_events(inst);          /* then zuri_instance_event_name(inst, i, ...) */
/* An event may have a sound for the host to play: zuri_instance_event_sound(inst, i, &volume)
 * gives the audio asset, zuri_file_sound_bytes its encoded bytes (decode them once, at load). */
/* The fit argument takes an alignment too: ZURI_FIT_COVER | ZURI_ALIGN_TOP_LEFT. */
/* zuri_instance_key reports a key (named as the web's KeyboardEvent.key) to the scene. */
/* An enum property: zuri_instance_property_option_count / _name list its Options. */
/* Also: zuri_instance_seek (scrub a directly played animation), zuri_instance_declared_event_*
 * (list the events a scene can report), zuri_instance_layer_state (the state each layer is in),
 * zuri_instance_get_property[_color] (read back what an object draws with). */
```

Input goes in through the view model (`zuri_instance_set_number`, `_set_color`,
`_set_string`, `_fire`) and `zuri_instance_pointer` (in the pixel space of the last render).
Its last two arguments are the host's number for the pointer and the button that went
down or up (0 is the primary one): a host with several pointers or buttons passes every
event on, and the runtime keeps the scene to one pointer and one button (the press's)
and returns whether the event was the scene's. That rule is the runtime's, not the
hosts': do not filter pointers or buttons in a wrapper. What a host does keep is what
only it knows: that a press it passed on can no longer be let go through it (a surface
disabled, a capture taken away), which it reports as that pointer leaving; and, in a
browser, that `pointerup` is of the last button let go, which the player reports as
the release of the button that went down.
`zuri_instance_set_drag_threshold` says how many of those pixels a press may go along a
Layout that scrolls before it is a scroll and no longer a tap (8 until set): the host
knows the pointer and the screen, the runtime does not.
A property name can be a path: `card/title` is the property `title` of the artboard
nested under the name `card`, in the setters, the getters and `_fire` alike. Where that
NestedArtboard is a list (it has a `count`), a number says which copy, from 0:
`rows/2/title`; a list whose `count` is bound to a Property has its new length when
`zuri_instance_set_number` returns. The
functions that list properties list the outer artboard's only;
`zuri_instance_property_type_of` says what a name or a path refers to, or that it
refers to nothing (which also tells an empty string from a missing one). A nested
property that the artboard around it binds `into` is written from there on every advance:
a setter by path returns true and the value does not last. The compiler refuses a `/` in
a Property's or NestedArtboard's name and two NestedArtboards of one name; the loader does
not, and in a file made some other way a path finds the first NestedArtboard of that name.

## Host responsibilities
- **Images:** for each `zuri_file_image_id`, decode `zuri_file_image_bytes` (NULL with length 0
  when the id has no bytes) and call
  `zuri_file_set_image` with RGBA8 pixels, before or after creating instances.
- **Shader paints (optional):** after a render, answer each `zuri_instance_shader_request`
  with `zuri_instance_set_shader_texture`. Hosts that do not get the paint's fallback color.

## Builds
| Artifact | Command |
|---|---|
| Shared / static library | `cargo build -p zuri-ffi --release` -> `target/release/libzuri.{dylib,so,a}`, `zuri.dll` |
| WebAssembly | `cargo build -p zuri-ffi --profile wasm --target wasm32-unknown-unknown` |
| Authoring entry points | add `--features json` (`zuri_json_to_zur`, `zuri_zur_to_json`, `zuri_schema_json`); define `ZURI_JSON` before including `zuri.h` to declare them |
| Without text | `--no-default-features` |

## How it is kept honest
- `tests/abi.rs` fails if the header and the Rust exports list different functions.
- `tests/smoke.c` is compiled with `-Wall -Wextra -Werror` against the header and the
  static library and runs a full scenario: errors, enumeration, playback, settling, data
  binding, pointer input, events, rendering, editor helpers.
- The same scenario runs through wasm (`packages/web/test`) and C# (`integrations/unity/Tests~/DotnetHarness`).

## Changing it
Follow "Add a C ABI function" in `memory/instruction.md`. A change to an existing
signature is a breaking change: bump `zuri_abi_version` and update every wrapper in the
same change.
