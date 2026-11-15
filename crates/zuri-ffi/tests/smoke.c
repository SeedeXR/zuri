/* Drives the C API the way an engine plugin does. Built and run by tests/abi.rs. */
#include "zuri.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond) do { if (!(cond)) { fprintf(stderr, "FAILED line %d: %s\n", __LINE__, #cond); return 1; } } while (0)
#define S(lit) lit, (sizeof(lit) - 1)

int main(int argc, char **argv) {
    CHECK(argc == 2);
    FILE *f = fopen(argv[1], "rb");
    CHECK(f);
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *bytes = malloc((size_t)len);
    CHECK(fread(bytes, 1, (size_t)len, f) == (size_t)len);
    fclose(f);

    CHECK(zuri_abi_version() == 1);

    /* Errors are values, not crashes. */
    char err[256] = {0};
    CHECK(zuri_file_load((const uint8_t *)"nope", 4) == NULL);
    CHECK(zuri_last_error(err, sizeof err - 1) > 0 && strstr(err, "ZURI"));
    CHECK(zuri_instance_new(NULL, 0) == NULL);
    CHECK(!zuri_instance_advance(NULL, 0.1f));
    zuri_instance_free(NULL);
    zuri_file_free(NULL);

    ZuriFile *file = zuri_file_load(bytes, (size_t)len);
    CHECK(file);
    free(bytes);
    CHECK(zuri_file_artboard_count(file) == 1);
    char name[64] = {0};
    CHECK(zuri_file_artboard_name(file, 0, name, sizeof name) == 6 && !strcmp(name, "Button"));
    CHECK(zuri_file_artboard_name(file, 0, NULL, 0) == 6); /* measuring */
    CHECK(zuri_instance_new(file, 7) == NULL);
    size_t image_len = 1; /* an id that is not an image asset has no bytes: NULL, length 0 */
    CHECK(zuri_file_image_bytes(file, ZURI_NONE, &image_len) == NULL && image_len == 0);

    ZuriInstance *inst = zuri_instance_new(file, 0);
    CHECK(inst);
    zuri_file_free(file); /* an instance keeps its file alive */
    CHECK(zuri_instance_width(inst) == 320.0f && zuri_instance_height(inst) == 160.0f);
    CHECK(zuri_instance_animation_count(inst) == 5 && zuri_instance_state_machine_count(inst) == 1);
    CHECK(zuri_instance_property_count(inst) == 5 && zuri_instance_property_type(inst, 3) == ZURI_COLOR);
    /* Only an enum has Options (examples/button.json has none): the calls are safe on any property. */
    CHECK(zuri_instance_property_option_count(inst, 3) == 0 && zuri_instance_property_option_name(inst, 3, 0, NULL, 0) == 0);
    CHECK(zuri_instance_play_state_machine(inst, 0) && !zuri_instance_play_state_machine(inst, 3));

    enum { W = 320, H = 160 };
    uint8_t *px = malloc(W * H * 4);
    CHECK(zuri_instance_advance(inst, 0.0f));
    CHECK(zuri_instance_render(inst, px, W, H, ZURI_FIT_CONTAIN, 0, false));
    const uint8_t *centre = px + (80 * W + 100) * 4;
    CHECK(centre[0] == 0x4f && centre[1] == 0x7c && centre[2] == 0xff && centre[3] == 0xff);

    /* Settles when idle. */
    bool active = true;
    for (int i = 0; i < 30; i++) active = zuri_instance_advance(inst, 1.0f / 60.0f);
    CHECK(!active);

    /* Data binding in, pointer in, events out. */
    CHECK(zuri_instance_set_color(inst, S("accent"), 0xffff3366u));
    CHECK(!zuri_instance_set_color(inst, S("missing"), 0));
    /* The error describes this failure, not an earlier one. */
    memset(err, 0, sizeof err);
    CHECK(zuri_last_error(err, sizeof err - 1) > 0 && strstr(err, "missing") && !strstr(err, "ZURI"));
    CHECK(zuri_abi_version() == ZURI_ABI_VERSION);
    uint32_t color = 0;
    CHECK(zuri_instance_get_color(inst, S("accent"), &color) && color == 0xffff3366u);
    zuri_instance_pointer(inst, ZURI_POINTER_MOVE, 160, 80);
    zuri_instance_pointer(inst, ZURI_POINTER_DOWN, 160, 80);
    zuri_instance_pointer(inst, ZURI_POINTER_UP, 160, 80);
    float clicks = 0;
    CHECK(zuri_instance_get_number(inst, S("clicks"), &clicks) && clicks == 1.0f);
    CHECK(zuri_instance_advance(inst, 1.0f / 60.0f));
    CHECK(zuri_instance_poll_events(inst) == 1);
    memset(name, 0, sizeof name);
    CHECK(zuri_instance_event_name(inst, 0, name, sizeof name) == 7 && !strcmp(name, "clicked"));
    CHECK(zuri_instance_poll_events(inst) == 0);

    CHECK(zuri_instance_render(inst, px, W, H, ZURI_FIT_CONTAIN, 0xff000000u, true));
    CHECK(centre[0] == 0xff && centre[1] == 0x33 && centre[2] == 0x66);
    CHECK(px[3] == 0xff); /* the background was applied */
    /* A key no Listener of the scene names: reported, and nothing reacts. */
    CHECK(!zuri_instance_key(inst, "q", 1, true) && !zuri_instance_key(NULL, "Enter", 5, true));
    CHECK(!zuri_instance_key(inst, "\xff", 1, true) && !zuri_instance_key(inst, NULL, 0, true)); /* not text, no name */
    /* Fit and alignment travel in one argument; values this runtime does not know fall back. */
    CHECK(zuri_instance_render(inst, px, W, H, ZURI_FIT_SCALE_DOWN | ZURI_ALIGN_BOTTOM_RIGHT, 0, false));
    CHECK(zuri_instance_render(inst, px, W, H, 0xffffu, 0, false));
    CHECK(!zuri_instance_render(inst, NULL, W, H, 0, 0, false));
    CHECK(!zuri_instance_render(inst, px, 0, H, 0, 0, false));

    float view[6] = {2, 0, 0, 2, 0, 0}, box[4];
    CHECK(zuri_instance_render_view(inst, px, W, H, view, 0, false));
    uint32_t hit = zuri_instance_hit_test(inst, 320, 160); /* artboard (160, 80) at 2x */
    CHECK(hit != ZURI_NONE && zuri_instance_bounds(inst, hit, box) && box[0] < 160 && box[2] > 160);
    CHECK(zuri_instance_hit_test(inst, 2, 2) == ZURI_NONE);

    char declared[32] = {0};
    CHECK(zuri_instance_declared_event_count(inst) == 1 && zuri_instance_declared_event_name(inst, 0, declared, sizeof declared) == 7);
    CHECK(strcmp(declared, "clicked") == 0 && zuri_instance_declared_event_name(inst, 1, declared, sizeof declared) == 0);

    /* Read-back: property key 3 is "x"; the hit shape sits at the artboard's centre. */
    float x = 0;
    uint32_t argb = 0;
    CHECK(zuri_instance_get_property(inst, hit, 3, &x) && !zuri_instance_get_property(inst, ZURI_NONE, 3, &x));
    CHECK(!zuri_instance_get_property_color(inst, hit, 3, &argb)); /* x is not a color */
    CHECK(!zuri_instance_get_property(inst, hit, 65536 + 3, &x)); /* an out-of-range key is not key 3 */
    /* A state machine is playing: each layer reports its state, and there is nothing to seek. */
    CHECK(zuri_instance_layer_count(inst) > 0 && zuri_instance_layer_state(inst, 0) != ZURI_NONE);
    CHECK(zuri_instance_layer_state(inst, 99) == ZURI_NONE && !zuri_instance_seek(inst, 1.0f));
    CHECK(zuri_instance_play_animation(inst, 0) && zuri_instance_seek(inst, 0.5f) && zuri_instance_layer_count(inst) == 0);

    free(px);
    zuri_instance_free(inst);
    puts("OK");
    return 0;
}
