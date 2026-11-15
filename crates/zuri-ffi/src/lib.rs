//! Zuri's C ABI. One surface for every host: Unity (P/Invoke), Unreal (C++), the web
//! (raw wasm exports) and anything else that can call C.
//!
//! Conventions (also documented in `include/zuri.h`, which a test keeps in sync):
//! - Handles are opaque pointers; free each with its `_free`. NULL is always accepted.
//! - Strings in are UTF-8 pointer + byte length (no terminator needed).
//! - Strings out: pass a buffer and its capacity; the full byte length is returned and at
//!   most `cap` bytes are written (no terminator). Call with `cap = 0` to measure.
//! - Functions return `false` / NULL on failure; `zuri_last_error` explains why.
//! - A handle must be used from one thread at a time. Panics never cross the boundary.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::sync::Arc;
use zuri_core::geom::Mat;
use zuri_core::schema::{p, t};
use zuri_core::{raster, File, Fit, Instance, Pointer, Raster};

pub struct ZuriFile(Arc<File>);

pub struct ZuriInstance {
    inst: Instance,
    raster: Raster,
    events: Vec<String>,
}

/// `ZuriShaderRequest` in zuri.h; the core struct is `#[repr(C)]` with that layout.
pub use zuri_core::instance::ShaderRequest as ZuriShaderRequest;

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn fail<T>(msg: impl ToString, value: T) -> T {
    LAST_ERROR.with(|e| *e.borrow_mut() = msg.to_string());
    value
}

/// Pass `ok` through, recording `why` when it is false, so `zuri_last_error` always
/// describes the call that just failed.
fn or_fail(ok: bool, why: impl FnOnce() -> String) -> bool {
    if !ok {
        fail(why(), false);
    }
    ok
}

/// Run `f`, turning a panic into `default` so unwinding never reaches foreign frames.
fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    #[cfg(panic = "unwind")]
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(v) => v,
            Err(_) => fail("internal error: the runtime panicked", default),
        }
    }
    #[cfg(not(panic = "unwind"))]
    {
        let _ = &default;
        f()
    }
}

unsafe fn bytes_in<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}
unsafe fn str_in<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(bytes_in(ptr, len)).unwrap_or("")
}
unsafe fn str_out(s: &str, buf: *mut u8, cap: usize) -> usize {
    if !buf.is_null() {
        let n = s.len().min(cap);
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf, n);
    }
    s.len()
}
/// Write `v` through `out`. False when there is no value or `out` is NULL.
unsafe fn put<T>(v: Option<T>, out: *mut T) -> bool {
    match v {
        Some(v) if !out.is_null() => {
            out.write(v);
            true
        }
        _ => false,
    }
}
/// Hand a buffer to the caller; it is released with `zuri_free(ptr, len)`.
#[cfg(feature = "json")]
unsafe fn give(bytes: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    let ptr = zuri_alloc(bytes.len());
    if ptr.is_null() {
        return fail("out of memory", ptr);
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    if !out_len.is_null() {
        *out_len = bytes.len();
    }
    ptr
}

/// Every buffer crossing the ABI is 8-byte aligned, so hosts can view it as floats or ints.
fn layout(len: usize) -> Option<std::alloc::Layout> {
    std::alloc::Layout::from_size_align(len.max(1), 8).ok()
}

// ---- memory, version, errors ---------------------------------------------------------

#[no_mangle]
pub extern "C" fn zuri_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn zuri_alloc(len: usize) -> *mut u8 {
    match layout(len) {
        Some(l) => unsafe { std::alloc::alloc_zeroed(l) },
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn zuri_free(ptr: *mut u8, len: usize) {
    if let (false, Some(l)) = (ptr.is_null(), layout(len)) {
        std::alloc::dealloc(ptr, l);
    }
}

#[no_mangle]
pub unsafe extern "C" fn zuri_last_error(buf: *mut u8, cap: usize) -> usize {
    LAST_ERROR.with(|e| str_out(&e.borrow(), buf, cap))
}

// ---- files -------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn zuri_file_load(bytes: *const u8, len: usize) -> *mut ZuriFile {
    guard(std::ptr::null_mut(), || match File::load(bytes_in(bytes, len)) {
        Ok(f) => Box::into_raw(Box::new(ZuriFile(f))),
        Err(e) => fail(e, std::ptr::null_mut()),
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_file_free(file: *mut ZuriFile) {
    if !file.is_null() {
        drop(Box::from_raw(file));
    }
}

#[no_mangle]
pub unsafe extern "C" fn zuri_file_artboard_count(file: *const ZuriFile) -> u32 {
    file.as_ref().map_or(0, |f| f.0.artboards.len() as u32)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_file_artboard_name(file: *const ZuriFile, index: u32, buf: *mut u8, cap: usize) -> usize {
    match file.as_ref().and_then(|f| f.0.artboards.get(index as usize)) {
        Some(a) => str_out(&a.name, buf, cap),
        None => 0,
    }
}

/// Number of image assets. The host decodes each and returns pixels with `zuri_file_set_image`.
#[no_mangle]
pub unsafe extern "C" fn zuri_file_image_count(file: *const ZuriFile) -> u32 {
    file.as_ref().map_or(0, |f| f.0.roots_of(t::IMAGE_ASSET).len() as u32)
}

/// Id of the `index`-th image asset, or `UINT32_MAX`.
#[no_mangle]
pub unsafe extern "C" fn zuri_file_image_id(file: *const ZuriFile, index: u32) -> u32 {
    file.as_ref().and_then(|f| f.0.roots_of(t::IMAGE_ASSET).get(index as usize).copied()).unwrap_or(u32::MAX)
}

/// Encoded bytes (PNG / JPEG / WebP) of an image asset. The pointer is owned by the file.
/// NULL with `*out_len = 0` when `id` is not an image asset or it carries no bytes.
#[no_mangle]
pub unsafe extern "C" fn zuri_file_image_bytes(file: *const ZuriFile, id: u32, out_len: *mut usize) -> *const u8 {
    let bytes =
        file.as_ref().and_then(|f| f.0.objects.get(id as usize)).filter(|o| o.type_key == t::IMAGE_ASSET).and_then(|o| o.bytes(p::BYTES)).unwrap_or(&[]);
    if !out_len.is_null() {
        *out_len = bytes.len();
    }
    if bytes.is_empty() {
        std::ptr::null()
    } else {
        bytes.as_ptr()
    }
}

/// Supply decoded pixels (straight-alpha RGBA8, `width * height * 4` bytes) for an image asset.
#[no_mangle]
pub unsafe extern "C" fn zuri_file_set_image(file: *const ZuriFile, id: u32, width: u32, height: u32, rgba: *const u8) -> bool {
    guard(false, || {
        let Some(f) = file.as_ref() else { return fail("file is NULL", false) };
        let len = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if rgba.is_null() {
            return fail("rgba is NULL", false);
        }
        match f.0.set_image(id, width, height, bytes_in(rgba, len)) {
            Ok(()) => true,
            Err(e) => fail(e, false),
        }
    })
}

/// Source of a shader asset. `language`: 0 = WGSL, 1 = GLSL ES 3.00, 2 = HLSL.
#[no_mangle]
pub unsafe extern "C" fn zuri_file_shader_source(file: *const ZuriFile, asset: u32, language: u32, buf: *mut u8, cap: usize) -> usize {
    let key = [p::WGSL, p::GLSL, p::HLSL].get(language as usize).copied().unwrap_or(p::WGSL);
    let src = file.as_ref().and_then(|f| f.0.objects.get(asset as usize)).filter(|o| o.type_key == t::SHADER_ASSET).and_then(|o| o.str(key)).unwrap_or("");
    str_out(src, buf, cap)
}

// ---- instances ---------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_new(file: *const ZuriFile, artboard: u32) -> *mut ZuriInstance {
    guard(std::ptr::null_mut(), || {
        let Some(f) = file.as_ref() else { return fail("file is NULL", std::ptr::null_mut()) };
        match Instance::new(&f.0, artboard as usize) {
            Ok(inst) => Box::into_raw(Box::new(ZuriInstance { inst, raster: Raster::default(), events: Vec::new() })),
            Err(e) => fail(e, std::ptr::null_mut()),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_free(inst: *mut ZuriInstance) {
    if !inst.is_null() {
        drop(Box::from_raw(inst));
    }
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_width(inst: *const ZuriInstance) -> f32 {
    inst.as_ref().map_or(0.0, |i| i.inst.width())
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_height(inst: *const ZuriInstance) -> f32 {
    inst.as_ref().map_or(0.0, |i| i.inst.height())
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_animation_count(inst: *const ZuriInstance) -> u32 {
    inst.as_ref().map_or(0, |i| i.inst.animation_names().len() as u32)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_animation_name(inst: *const ZuriInstance, index: u32, buf: *mut u8, cap: usize) -> usize {
    inst.as_ref().and_then(|i| i.inst.animation_names().get(index as usize).map(|n| str_out(n, buf, cap))).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_state_machine_count(inst: *const ZuriInstance) -> u32 {
    inst.as_ref().map_or(0, |i| i.inst.state_machine_names().len() as u32)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_state_machine_name(inst: *const ZuriInstance, index: u32, buf: *mut u8, cap: usize) -> usize {
    inst.as_ref().and_then(|i| i.inst.state_machine_names().get(index as usize).map(|n| str_out(n, buf, cap))).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_play_animation(inst: *mut ZuriInstance, index: u32) -> bool {
    guard(false, || or_fail(inst.as_mut().is_some_and(|i| i.inst.play_animation(index as usize)), || format!("no animation at index {index}")))
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_play_state_machine(inst: *mut ZuriInstance, index: u32) -> bool {
    guard(false, || or_fail(inst.as_mut().is_some_and(|i| i.inst.play_state_machine(index as usize)), || format!("no state machine at index {index}")))
}

/// Play the first state machine, or failing that the first animation.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_autoplay(inst: *mut ZuriInstance) {
    guard((), || {
        if let Some(i) = inst.as_mut() {
            i.inst.autoplay()
        }
    })
}

/// Jump the playing animation to `seconds` on its timeline; the next advance shows it.
/// False when no animation is playing directly (state machines have no single timeline).
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_seek(inst: *mut ZuriInstance, seconds: f32) -> bool {
    guard(false, || or_fail(inst.as_mut().is_some_and(|i| i.inst.seek(seconds)), || "seek needs an animation started with play_animation".into()))
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_layer_count(inst: *const ZuriInstance) -> u32 {
    guard(0, || inst.as_ref().map_or(0, |i| i.inst.states().len() as u32))
}

/// Object id of the State that layer `layer` of the playing state machine is in, or ZURI_NONE.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_layer_state(inst: *const ZuriInstance, layer: u32) -> u32 {
    guard(u32::MAX, || inst.as_ref().and_then(|i| i.inst.states().get(layer as usize).copied()).unwrap_or(u32::MAX))
}

/// Advance by `dt` seconds. Returns true if the picture may have changed (render again).
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_advance(inst: *mut ZuriInstance, dt: f32) -> bool {
    guard(false, || inst.as_mut().is_some_and(|i| i.inst.advance(dt)))
}

// ---- view model --------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_property_count(inst: *const ZuriInstance) -> u32 {
    inst.as_ref().map_or(0, |i| i.inst.properties().len() as u32)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_property_name(inst: *const ZuriInstance, index: u32, buf: *mut u8, cap: usize) -> usize {
    inst.as_ref().and_then(|i| i.inst.properties().get(index as usize).map(|(n, _)| str_out(n, buf, cap))).unwrap_or(0)
}

/// 0 = number, 1 = bool, 2 = trigger, 3 = color, 4 = string; `UINT32_MAX` if out of range.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_property_type(inst: *const ZuriInstance, index: u32) -> u32 {
    inst.as_ref().and_then(|i| i.inst.properties().get(index as usize).map(|(_, ty)| *ty)).unwrap_or(u32::MAX)
}

/// Set a number property. Bools are numbers: 0 = false, 1 = true.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_set_number(inst: *mut ZuriInstance, name: *const u8, name_len: usize, value: f32) -> bool {
    guard(false, || {
        or_fail(inst.as_mut().is_some_and(|i| i.inst.set_number(str_in(name, name_len), value)), || {
            format!("no number or bool view-model property \"{}\", or the value is not finite", str_in(name, name_len))
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_get_number(inst: *const ZuriInstance, name: *const u8, name_len: usize, out: *mut f32) -> bool {
    put(inst.as_ref().and_then(|i| i.inst.number(str_in(name, name_len))), out) || fail(format!("no view-model property \"{}\"", str_in(name, name_len)), false)
}

/// Colors are 0xAARRGGBB.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_set_color(inst: *mut ZuriInstance, name: *const u8, name_len: usize, argb: u32) -> bool {
    guard(false, || {
        or_fail(inst.as_mut().is_some_and(|i| i.inst.set_color(str_in(name, name_len), argb)), || {
            format!("no view-model property \"{}\"", str_in(name, name_len))
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_get_color(inst: *const ZuriInstance, name: *const u8, name_len: usize, out: *mut u32) -> bool {
    put(inst.as_ref().and_then(|i| i.inst.color(str_in(name, name_len))), out) || fail(format!("no view-model property \"{}\"", str_in(name, name_len)), false)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_set_string(inst: *mut ZuriInstance, name: *const u8, name_len: usize, value: *const u8, value_len: usize) -> bool {
    guard(false, || {
        or_fail(inst.as_mut().is_some_and(|i| i.inst.set_string(str_in(name, name_len), str_in(value, value_len))), || {
            format!("no view-model property \"{}\"", str_in(name, name_len))
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_get_string(inst: *const ZuriInstance, name: *const u8, name_len: usize, buf: *mut u8, cap: usize) -> usize {
    inst.as_ref().and_then(|i| i.inst.string(str_in(name, name_len)).map(|s| str_out(s, buf, cap))).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_fire(inst: *mut ZuriInstance, name: *const u8, name_len: usize) -> bool {
    guard(false, || {
        or_fail(inst.as_mut().is_some_and(|i| i.inst.fire(str_in(name, name_len))), || format!("no view-model property \"{}\"", str_in(name, name_len)))
    })
}

// ---- input and events --------------------------------------------------------------

/// Pointer event in surface pixels. `kind`: 0 = move, 1 = down, 2 = up, 3 = left the surface.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_pointer(inst: *mut ZuriInstance, kind: u32, x: f32, y: f32) {
    guard((), || {
        let kind = match kind {
            0 => Pointer::Move,
            1 => Pointer::Down,
            2 => Pointer::Up,
            _ => Pointer::Exit,
        };
        if let Some(i) = inst.as_mut() {
            i.inst.pointer(kind, x, y)
        }
    })
}

/// The events the artboard declares (for inspectors); `poll_events` reports the ones that fired.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_declared_event_count(inst: *const ZuriInstance) -> u32 {
    guard(0, || inst.as_ref().map_or(0, |i| i.inst.event_names().len() as u32))
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_declared_event_name(inst: *const ZuriInstance, index: u32, buf: *mut u8, cap: usize) -> usize {
    guard(0, || inst.as_ref().and_then(|i| i.inst.event_names().get(index as usize).map(|n| str_out(n, buf, cap))).unwrap_or(0))
}

/// Collect the events emitted since the last poll; returns how many. Read them with
/// `zuri_instance_event_name` before polling again.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_poll_events(inst: *mut ZuriInstance) -> u32 {
    guard(0, || {
        inst.as_mut().map_or(0, |i| {
            i.events = i.inst.take_events();
            i.events.len() as u32
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_event_name(inst: *const ZuriInstance, index: u32, buf: *mut u8, cap: usize) -> usize {
    inst.as_ref().and_then(|i| i.events.get(index as usize).map(|n| str_out(n, buf, cap))).unwrap_or(0)
}

// ---- rendering ---------------------------------------------------------------------

unsafe fn render(inst: *mut ZuriInstance, rgba: *mut u8, width: u32, height: u32, view: Mat, background: u32, premultiplied: bool) -> bool {
    guard(false, || {
        let Some(i) = inst.as_mut() else { return fail("instance is NULL", false) };
        let len = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if rgba.is_null() || len == 0 || width > 16384 || height > 16384 {
            return fail("render target must be non-NULL and between 1 and 16384 pixels per side", false);
        }
        let px = std::slice::from_raw_parts_mut(rgba, len);
        raster::clear(px, background);
        i.inst.render(&mut i.raster, px, width, height, view);
        if !premultiplied {
            raster::unpremultiply(px);
        }
        true
    })
}

/// Render into `rgba` (`width * height * 4` bytes, rows top to bottom). The buffer is first
/// cleared to `background` (0xAARRGGBB; 0 = transparent). `fit`: 0 = contain, 1 = cover,
/// 2 = fill, 3 = none. `premultiplied` selects the alpha convention of the output.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_render(
    inst: *mut ZuriInstance,
    rgba: *mut u8,
    width: u32,
    height: u32,
    fit: u32,
    background: u32,
    premultiplied: bool,
) -> bool {
    let fit = [Fit::Contain, Fit::Cover, Fit::Fill, Fit::None][(fit as usize).min(3)];
    // A NULL instance is reported by `render`; the view is then never used.
    let view = inst.as_ref().map_or(Mat::IDENTITY, |i| i.inst.fit(fit, width as f32, height as f32));
    render(inst, rgba, width, height, view, background, premultiplied)
}

/// Like `zuri_instance_render`, with an explicit artboard-to-pixel transform
/// `[a, b, c, d, tx, ty]` (x' = a*x + c*y + tx, y' = b*x + d*y + ty) for zoom and pan.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_render_view(
    inst: *mut ZuriInstance,
    rgba: *mut u8,
    width: u32,
    height: u32,
    view: *const f32,
    background: u32,
    premultiplied: bool,
) -> bool {
    if view.is_null() {
        return fail("view is NULL", false);
    }
    let m = std::slice::from_raw_parts(view, 6);
    if m.iter().any(|v| !v.is_finite()) {
        return fail("view contains a non-finite number", false);
    }
    render(inst, rgba, width, height, Mat { a: m[0], b: m[1], c: m[2], d: m[3], tx: m[4], ty: m[5] }, background, premultiplied)
}

/// Shader paints seen by the last render. The host draws each one and returns the pixels
/// with `zuri_instance_set_shader_texture`; they are used from the next render on.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_shader_request_count(inst: *const ZuriInstance) -> u32 {
    inst.as_ref().map_or(0, |i| i.inst.shader_requests().len() as u32)
}

#[no_mangle]
pub unsafe extern "C" fn zuri_instance_shader_request(inst: *const ZuriInstance, index: u32, out: *mut ZuriShaderRequest) -> bool {
    put(inst.as_ref().and_then(|i| i.inst.shader_requests().get(index as usize).copied()), out)
}

/// Pixels for a shader paint: straight-alpha RGBA8, `width * height * 4` bytes, rows top to bottom.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_set_shader_texture(inst: *mut ZuriInstance, paint: u32, width: u32, height: u32, rgba: *const u8) -> bool {
    guard(false, || {
        let len = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        match inst.as_mut() {
            Some(i) if !rgba.is_null() => i.inst.set_shader_texture(paint, width, height, bytes_in(rgba, len)),
            _ => false,
        }
    })
}

// ---- editor helpers ----------------------------------------------------------------

/// The value `object` currently draws with for schema property `key` (number, bool, enum).
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_get_property(inst: *const ZuriInstance, object: u32, key: u32, out: *mut f32) -> bool {
    // A key that does not fit a u16 is no property, not a different one.
    guard(false, || {
        put(inst.as_ref().zip(u16::try_from(key).ok()).and_then(|(i, key)| i.inst.property(object, key)), out)
            || fail(format!("object {object} has no number property with key {key} (or `out` is NULL)"), false)
    })
}

/// Like `zuri_instance_get_property`, for color properties (0xAARRGGBB).
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_get_property_color(inst: *const ZuriInstance, object: u32, key: u32, out: *mut u32) -> bool {
    guard(false, || {
        put(inst.as_ref().zip(u16::try_from(key).ok()).and_then(|(i, key)| i.inst.property_color(object, key)), out)
            || fail(format!("object {object} has no color property with key {key} (or `out` is NULL)"), false)
    })
}

/// Id of the top-most object under surface pixel `x, y` (as of the last render), or `UINT32_MAX`.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_hit_test(inst: *const ZuriInstance, x: f32, y: f32) -> u32 {
    guard(u32::MAX, || inst.as_ref().and_then(|i| i.inst.hit_test(x, y)).unwrap_or(u32::MAX))
}

/// Artboard-space bounding box `[x0, y0, x1, y1]` of an object and everything inside it.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_bounds(inst: *const ZuriInstance, object: u32, out: *mut f32) -> bool {
    guard(false, || put(inst.as_ref().and_then(|i| i.inst.bounds(object)), out.cast()))
}

/// Object-to-artboard transform `[a, b, c, d, tx, ty]`.
#[no_mangle]
pub unsafe extern "C" fn zuri_instance_world_matrix(inst: *const ZuriInstance, object: u32, out: *mut f32) -> bool {
    put(inst.as_ref().and_then(|i| i.inst.world_matrix(object)).map(|m| [m.a, m.b, m.c, m.d, m.tx, m.ty]), out.cast())
}

// ---- authoring (feature "json") ------------------------------------------------------

/// Compile JSON source to `.zur` bytes. Returns NULL on error. Free with `zuri_free(ptr, *out_len)`.
/// `"@path"` byte properties are not available here; embed assets as base64.
#[cfg(feature = "json")]
#[no_mangle]
pub unsafe extern "C" fn zuri_json_to_zur(json: *const u8, len: usize, out_len: *mut usize) -> *mut u8 {
    guard(std::ptr::null_mut(), || {
        let no_files = |path: &str| Err(zuri_core::Error(format!("cannot read \"@{path}\" here; embed the asset as base64")));
        let objects = match zuri_core::json::compile(str_in(json, len), &no_files) {
            Ok(o) => o,
            Err(e) => return fail(e, std::ptr::null_mut()),
        };
        // Reject scenes the runtime would refuse to load, with the same message.
        if let Err(e) = File::from_objects(objects.clone()) {
            return fail(e, std::ptr::null_mut());
        }
        match zuri_core::format::write(&objects, 0) {
            Ok(bytes) => give(bytes, out_len),
            Err(e) => fail(e, std::ptr::null_mut()),
        }
    })
}

/// Decompile `.zur` bytes to JSON source (UTF-8). Returns NULL on error. Free with `zuri_free`.
#[cfg(feature = "json")]
#[no_mangle]
pub unsafe extern "C" fn zuri_zur_to_json(bytes: *const u8, len: usize, out_len: *mut usize) -> *mut u8 {
    guard(std::ptr::null_mut(), || match zuri_core::format::read(bytes_in(bytes, len)).and_then(|o| zuri_core::json::decompile(&o)) {
        Ok(text) => give(text.into_bytes(), out_len),
        Err(e) => fail(e, std::ptr::null_mut()),
    })
}

/// The schema (every type and property) as JSON. Free with `zuri_free`.
#[cfg(feature = "json")]
#[no_mangle]
pub unsafe extern "C" fn zuri_schema_json(out_len: *mut usize) -> *mut u8 {
    guard(std::ptr::null_mut(), || give(zuri_core::json::schema_json().into_bytes(), out_len))
}
