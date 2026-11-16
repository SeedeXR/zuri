/*
 * Zuri runtime - C API (ABI version 1).
 *
 * One surface for every host: Unity (P/Invoke), Unreal (C++), the web (wasm exports).
 *
 * Conventions
 *  - Handles are opaque; release each with its _free function. NULL is always accepted.
 *  - Strings in:  UTF-8 pointer + byte length. No terminator needed.
 *  - Strings out: pass a buffer and its capacity. The full byte length is returned and at
 *                 most `cap` bytes are written, with no terminator. Pass cap = 0 to measure.
 *  - Failure is `false` / NULL; zuri_last_error() says why (per thread).
 *  - Use a handle from one thread at a time. Different handles may be used concurrently.
 *  - Colors are 0xAARRGGBB. Pixels are RGBA8, rows top to bottom.
 *
 * Frame loop
 *      if (zuri_instance_advance(inst, dt))            // false = nothing changed, skip the draw
 *          zuri_instance_render(inst, pixels, w, h, ZURI_FIT_CONTAIN, 0, false);
 *
 * This header is checked against the Rust exports by crates/zuri-ffi/tests/abi.rs.
 */
#ifndef ZURI_H
#define ZURI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ZuriFile ZuriFile;
typedef struct ZuriInstance ZuriInstance;

#define ZURI_ABI_VERSION 1 /* what zuri_abi_version() returns for this header */
#define ZURI_NONE UINT32_MAX

/* How the artboard is scaled into the surface. FIT_WIDTH / FIT_HEIGHT match that side and
 * keep the aspect ratio; SCALE_DOWN is CONTAIN when the artboard is larger than the
 * surface and its own size otherwise. */
enum { ZURI_FIT_CONTAIN = 0, ZURI_FIT_COVER = 1, ZURI_FIT_FILL = 2, ZURI_FIT_NONE = 3, ZURI_FIT_WIDTH = 4, ZURI_FIT_HEIGHT = 5, ZURI_FIT_SCALE_DOWN = 6 };
/* Where the artboard sits in the room left over (or which part shows when it overflows).
 * OR one into the `fit` argument: ZURI_FIT_COVER | ZURI_ALIGN_TOP_LEFT. Default: center.
 * A fit or alignment this runtime does not know is treated as CONTAIN / CENTER. */
enum {
    ZURI_ALIGN_CENTER = 0 << 8, ZURI_ALIGN_TOP_LEFT = 1 << 8, ZURI_ALIGN_TOP_CENTER = 2 << 8, ZURI_ALIGN_TOP_RIGHT = 3 << 8,
    ZURI_ALIGN_CENTER_LEFT = 4 << 8, ZURI_ALIGN_CENTER_RIGHT = 5 << 8,
    ZURI_ALIGN_BOTTOM_LEFT = 6 << 8, ZURI_ALIGN_BOTTOM_CENTER = 7 << 8, ZURI_ALIGN_BOTTOM_RIGHT = 8 << 8
};
enum { ZURI_POINTER_MOVE = 0, ZURI_POINTER_DOWN = 1, ZURI_POINTER_UP = 2, ZURI_POINTER_EXIT = 3 };
enum { ZURI_NUMBER = 0, ZURI_BOOL = 1, ZURI_TRIGGER = 2, ZURI_COLOR = 3, ZURI_STRING = 4, ZURI_ENUM = 5 };
enum { ZURI_SHADER_WGSL = 0, ZURI_SHADER_GLSL = 1, ZURI_SHADER_HLSL = 2 };

/* A shader paint the host should draw (see zuri_instance_shader_request). */
typedef struct ZuriShaderRequest {
    uint32_t paint;     /* pass back to zuri_instance_set_shader_texture */
    uint32_t asset;     /* shader asset id, for zuri_file_shader_source */
    uint32_t width;     /* pixels */
    uint32_t height;
    float params[4];    /* the paint's p0..p3 */
    float time;         /* seconds since the instance started */
} ZuriShaderRequest;

/* ---- memory, version, errors ---- */
uint32_t zuri_abi_version(void);
uint8_t *zuri_alloc(size_t len);              /* zeroed, 8-byte aligned */
void zuri_free(uint8_t *ptr, size_t len);     /* len must be the length it was allocated with */
size_t zuri_last_error(char *buf, size_t cap);

/* ---- files ---- */
ZuriFile *zuri_file_load(const uint8_t *bytes, size_t len);
void zuri_file_free(ZuriFile *file);
uint32_t zuri_file_artboard_count(const ZuriFile *file);
size_t zuri_file_artboard_name(const ZuriFile *file, uint32_t index, char *buf, size_t cap);

/* Images are decoded by the host: read the encoded bytes, hand back RGBA8 (straight alpha).
 * Files may embed any format; PNG and JPEG are the ones every host can decode. An image
 * that is never supplied simply does not draw. zuri_file_image_bytes returns a pointer owned
 * by the file, or NULL with *out_len = 0 when `id` is not an image asset or has no bytes. */
uint32_t zuri_file_image_count(const ZuriFile *file);
uint32_t zuri_file_image_id(const ZuriFile *file, uint32_t index);
const uint8_t *zuri_file_image_bytes(const ZuriFile *file, uint32_t id, size_t *out_len);
bool zuri_file_set_image(const ZuriFile *file, uint32_t id, uint32_t width, uint32_t height, const uint8_t *rgba);

/* Sounds: the host plays them, the runtime makes none. Encoded bytes, as for images. */
uint32_t zuri_file_sound_count(const ZuriFile *file);
uint32_t zuri_file_sound_id(const ZuriFile *file, uint32_t index);
const uint8_t *zuri_file_sound_bytes(const ZuriFile *file, uint32_t id, size_t *out_len);

size_t zuri_file_shader_source(const ZuriFile *file, uint32_t asset, uint32_t language, char *buf, size_t cap);

/* ---- instances ---- */
ZuriInstance *zuri_instance_new(const ZuriFile *file, uint32_t artboard);
void zuri_instance_free(ZuriInstance *inst);
float zuri_instance_width(const ZuriInstance *inst);
float zuri_instance_height(const ZuriInstance *inst);

uint32_t zuri_instance_animation_count(const ZuriInstance *inst);
size_t zuri_instance_animation_name(const ZuriInstance *inst, uint32_t index, char *buf, size_t cap);
uint32_t zuri_instance_state_machine_count(const ZuriInstance *inst);
size_t zuri_instance_state_machine_name(const ZuriInstance *inst, uint32_t index, char *buf, size_t cap);
bool zuri_instance_play_animation(ZuriInstance *inst, uint32_t index);
bool zuri_instance_play_state_machine(ZuriInstance *inst, uint32_t index);
void zuri_instance_autoplay(ZuriInstance *inst);
/* Jump the animation started with play_animation to `seconds` on its timeline; the next
 * advance (0 is fine) shows it. Nested artboards restart and run to the same time.
 * False when no animation is playing directly. */
bool zuri_instance_seek(ZuriInstance *inst, float seconds);
/* The State object each layer of the playing state machine is in (ZURI_NONE if none). */
uint32_t zuri_instance_layer_count(const ZuriInstance *inst);
uint32_t zuri_instance_layer_state(const ZuriInstance *inst, uint32_t layer);
bool zuri_instance_advance(ZuriInstance *inst, float dt);

/* ---- view model (data binding) ----
 * Booleans are numbers (0 = false, 1 = true): use set_number / get_number for them.
 * An enum is one of its Options: set_number / get_number work with the Option's position
 * (from 0), set_string / get_string with its name; zuri_instance_property_option_* list them.
 * Accessors are type-checked: set_number on a color property fails, and so on.
 * A name can be a path into a nested artboard: "card/title" is the property `title` of
 * the artboard nested under the name `card`, "card/badge/count" two levels down. Where
 * the nested artboard is a list (it has a `count`), a number says which copy, from 0:
 * "rows/2/title"; a list whose `count` is bound to a property has its new length when
 * zuri_instance_set_number returns. The
 * indexed functions list the outer artboard's properties only. A nested property that
 * the artboard around it binds `into` is written from there on every advance, so a value
 * set by path does not last: set the outer property.
 * zuri_instance_get_string returns 0 for an empty string and for a missing property;
 * zuri_instance_property_type_of tells them apart (ZURI_NONE: no such property). */
uint32_t zuri_instance_property_count(const ZuriInstance *inst);
size_t zuri_instance_property_name(const ZuriInstance *inst, uint32_t index, char *buf, size_t cap);
uint32_t zuri_instance_property_type(const ZuriInstance *inst, uint32_t index);
/* The type of the property a name or path refers to; ZURI_NONE if there is none. */
uint32_t zuri_instance_property_type_of(const ZuriInstance *inst, const char *name, size_t name_len);
uint32_t zuri_instance_property_option_count(const ZuriInstance *inst, uint32_t index);
size_t zuri_instance_property_option_name(const ZuriInstance *inst, uint32_t index, uint32_t option, char *buf, size_t cap);
bool zuri_instance_set_number(ZuriInstance *inst, const char *name, size_t name_len, float value);
bool zuri_instance_get_number(const ZuriInstance *inst, const char *name, size_t name_len, float *out);
bool zuri_instance_set_color(ZuriInstance *inst, const char *name, size_t name_len, uint32_t argb);
bool zuri_instance_get_color(const ZuriInstance *inst, const char *name, size_t name_len, uint32_t *out);
bool zuri_instance_set_string(ZuriInstance *inst, const char *name, size_t name_len, const char *value, size_t value_len);
size_t zuri_instance_get_string(const ZuriInstance *inst, const char *name, size_t name_len, char *buf, size_t cap);
bool zuri_instance_fire(ZuriInstance *inst, const char *name, size_t name_len);

/* ---- input and events ---- */
/* A pointer event at a pixel of the last render: kind 0 move, 1 down, 2 up, 3 the pointer
 * left the surface. One pointer: with several fingers, send the events of one. A press
 * that moves a Layout that scrolls is a scroll: until it is let go the scene's Listeners
 * hear only that the pointer has left. Report an up or a leaving for every down: until
 * then the press is open and the Layouts it holds follow every move. */
void zuri_instance_pointer(ZuriInstance *inst, uint32_t kind, float x, float y);
/* A key went down or up, for the scene's keyDown / keyUp Listeners. `key` is the key's name
 * as the web's KeyboardEvent.key gives it: "Enter", "Escape", "Tab", "ArrowLeft", " " (space),
 * or the character typed ("a", "A", "1"). Returns true when a Listener reacted. */
bool zuri_instance_key(ZuriInstance *inst, const char *key, size_t key_len, bool down);
/* The events poll_events can report: the artboard's own and those of artboards nested in it. */
uint32_t zuri_instance_declared_event_count(const ZuriInstance *inst);
size_t zuri_instance_declared_event_name(const ZuriInstance *inst, uint32_t index, char *buf, size_t cap);
/* poll_events moves the events fired since the last poll into a list read by event_name. */
uint32_t zuri_instance_poll_events(ZuriInstance *inst);
size_t zuri_instance_event_name(const ZuriInstance *inst, uint32_t index, char *buf, size_t cap);
/* The audio asset the index-th polled event plays, and how loud (0..1) in *out_volume
 * (which may be NULL). UINT32_MAX when the event has no sound. */
uint32_t zuri_instance_event_sound(const ZuriInstance *inst, uint32_t index, float *out_volume);

/* ---- rendering ---- */
bool zuri_instance_render(ZuriInstance *inst, uint8_t *rgba, uint32_t width, uint32_t height, uint32_t fit, uint32_t background, bool premultiplied);
bool zuri_instance_render_view(ZuriInstance *inst, uint8_t *rgba, uint32_t width, uint32_t height, const float *view, uint32_t background, bool premultiplied);

uint32_t zuri_instance_shader_request_count(const ZuriInstance *inst);
bool zuri_instance_shader_request(const ZuriInstance *inst, uint32_t index, ZuriShaderRequest *out);
/* A scene with shader paints keeps asking for frames only once the host has supplied a
 * shader texture; hosts that never do get the paint's fallback color and normal settling. */
bool zuri_instance_set_shader_texture(ZuriInstance *inst, uint32_t paint, uint32_t width, uint32_t height, const uint8_t *rgba);

/* ---- editor helpers ---- */
uint32_t zuri_instance_hit_test(const ZuriInstance *inst, float x, float y);
bool zuri_instance_bounds(const ZuriInstance *inst, uint32_t object, float *out);
bool zuri_instance_world_matrix(const ZuriInstance *inst, uint32_t object, float *out);
/* The value an object currently draws with (after animation and binds) for a schema
 * property key: numbers, booleans and enums as float; colors as 0xAARRGGBB. */
bool zuri_instance_get_property(const ZuriInstance *inst, uint32_t object, uint32_t key, float *out);
bool zuri_instance_get_property_color(const ZuriInstance *inst, uint32_t object, uint32_t key, uint32_t *out);

/* ---- authoring: exported only by builds with the "json" feature (the editor and CLI).
 * Define ZURI_JSON before including this header to declare them. ---- */
#ifdef ZURI_JSON
uint8_t *zuri_json_to_zur(const char *json, size_t len, size_t *out_len);
uint8_t *zuri_zur_to_json(const uint8_t *bytes, size_t len, size_t *out_len);
uint8_t *zuri_schema_json(size_t *out_len);
#endif

#ifdef __cplusplus
}
#endif

#endif /* ZURI_H */
