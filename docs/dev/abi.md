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
| Versions | `zuri_abi_version()` is 1 and the header defines `ZURI_ABI_VERSION`; compare them at startup. It changes only for incompatible changes. |

## The frame loop

```c
ZuriFile *file = zuri_file_load(bytes, len);
ZuriInstance *inst = zuri_instance_new(file, 0);
zuri_instance_autoplay(inst);

/* every frame */
if (zuri_instance_advance(inst, dt))                  /* false: nothing changed */
    zuri_instance_render(inst, pixels, width, height, ZURI_FIT_CONTAIN, 0, false);
uint32_t n = zuri_instance_poll_events(inst);          /* then zuri_instance_event_name(inst, i, ...) */
/* Also: zuri_instance_seek (scrub a directly played animation), zuri_instance_declared_event_*
 * (list the events a scene can report), zuri_instance_layer_state (the state each layer is in),
 * zuri_instance_get_property[_color] (read back what an object draws with). */
```

Input goes in through the view model (`zuri_instance_set_number`, `_set_color`,
`_set_string`, `_fire`) and `zuri_instance_pointer` (in the pixel space of the last render).

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
