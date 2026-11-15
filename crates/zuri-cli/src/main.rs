//! `zuri` command line. Every command reads `.zur` or JSON source, prints plain text by
//! default and one JSON object with `--json`, and exits non-zero on failure, so people,
//! scripts and agents can all drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use zuri_core::file::File;
use zuri_core::format::{self, RawObject};
use zuri_core::schema::{self, p, t};
use zuri_core::{json, raster, Error, Fit, Instance, Raster, Result};

const HELP: &str = "\
zuri - build, inspect and render Zuri (.zur) files

USAGE
  zuri <command> [arguments] [--json]

COMMANDS
  new <out.json>                 Write a starter scene to edit
  build <src.json> [-o out.zur]  Compile JSON source into a .zur file
  dump <file.zur> [-o out.json]  Turn a .zur file back into JSON source
  import <file> [-o out.json]    Convert SVG, OBJ, glTF/GLB or FBX into JSON source
  validate <file>                Check a .zur or JSON file; exit 1 if it is broken
  info <file>                    List artboards, animations, state machines, properties
  render <file> -o out.png       Render a frame (or --frames N for a numbered sequence)
  bench <file>                   Time advance + render over many frames
  schema [--markdown]            Print every object type and property (JSON, or a Markdown reference)
  help                           Show this text

RENDER / BENCH OPTIONS
  --artboard <name|index>        Artboard to use (default: the first)
  --animation <name>             Play this animation
  --state-machine <name>         Play this state machine (default: first machine, else first animation)
  --time <seconds>               Advance this long before rendering (default 0)
  --frames <n> --fps <n>         Render n frames, fps apart, to out-0001.png ...
  --width <px> --height <px>     Output size (default: the artboard size x --scale)
  --scale <factor>               Multiply the artboard size (default 1)
  --fit <mode>                   contain (default), cover, fill, none, fitWidth, fitHeight, scaleDown
  --align <where>                center (default), topLeft, topCenter, topRight, centerLeft, centerRight,
                                 bottomLeft, bottomCenter, bottomRight
  --background <#rrggbb[aa]>     Fill behind the artboard (default transparent)
  --set <property=value>         Set a view-model property (number, true/false, #color or text); repeatable
  --fire <trigger>               Fire a view-model trigger; repeatable

JSON source format: run `zuri schema`, or see docs/user/format.md.
";

struct Args {
    pos: Vec<String>,
    opts: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Args> {
        let mut a = Args { pos: Vec::new(), opts: HashMap::new() };
        let mut it = raw.iter();
        while let Some(tok) = it.next() {
            let name = match tok.as_str() {
                "-o" => "output",
                "-h" => "help",
                s if s.starts_with("--") => &s[2..],
                _ => {
                    a.pos.push(tok.clone());
                    continue;
                }
            };
            let value = if matches!(name, "json" | "help" | "markdown") {
                String::new()
            } else {
                it.next().cloned().ok_or_else(|| Error(format!("--{name} needs a value")))?
            };
            a.opts.entry(name.to_string()).or_default().push(value);
        }
        Ok(a)
    }
    fn get(&self, name: &str) -> Option<&str> {
        self.opts.get(name).and_then(|v| v.last()).map(String::as_str)
    }
    fn all(&self, name: &str) -> &[String] {
        self.opts.get(name).map_or(&[], Vec::as_slice)
    }
    fn num(&self, name: &str) -> Result<Option<f32>> {
        match self.get(name) {
            None => Ok(None),
            Some(s) => s.parse::<f32>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| Error(format!("--{name}: \"{s}\" is not a number"))),
        }
    }
    fn file(&self) -> Result<&str> {
        self.pos.get(1).map(String::as_str).ok_or_else(|| Error("missing file argument (see `zuri help`)".into()))
    }
}

fn read(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error(format!("cannot read {path}: {e}")))
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error(format!("cannot write {}: {e}", path.display())))
}

/// Load `.zur` bytes or JSON source into objects. `@path` assets resolve next to the source.
fn load(path: &str) -> Result<Vec<RawObject>> {
    let bytes = read(path)?;
    if bytes.starts_with(format::MAGIC) {
        return format::read(&bytes);
    }
    let dir = Path::new(path).parent().map(Path::to_path_buf).unwrap_or_default();
    let text = String::from_utf8(bytes).map_err(|_| Error(format!("{path} is neither a .zur file nor UTF-8 JSON")))?;
    // The full compile: resolves @path assets, converts 3D files, translates shaders.
    zuri_import::compile(&text, &dir)
}

fn open(path: &str) -> Result<std::sync::Arc<File>> {
    let file = File::from_objects(load(path)?)?;
    // The CLI can decode PNG images itself; other hosts decode with their platform.
    for asset in file.roots_of(t::IMAGE_ASSET) {
        let Some(bytes) = file.objects[asset as usize].bytes(p::BYTES) else { continue };
        let Ok(mut reader) = png::Decoder::new(std::io::Cursor::new(bytes)).read_info() else { continue };
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let Ok(info) = reader.next_frame(&mut buf) else { continue };
        let rgba: Vec<u8> = match (info.color_type, info.bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight) => buf[..info.buffer_size()].to_vec(),
            (png::ColorType::Rgb, png::BitDepth::Eight) => buf[..info.buffer_size()].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            (png::ColorType::Grayscale, png::BitDepth::Eight) => buf[..info.buffer_size()].iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => buf[..info.buffer_size()].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            _ => continue,
        };
        file.set_image(asset, info.width, info.height, &rgba)?;
    }
    Ok(file)
}

fn save_png(path: &Path, w: u32, h: u32, premultiplied: &[u8]) -> Result<()> {
    let mut px = premultiplied.to_vec();
    raster::unpremultiply(&mut px);
    let file = std::fs::File::create(path).map_err(|e| Error(format!("cannot write {}: {e}", path.display())))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let err = |e: png::EncodingError| Error(format!("cannot write {}: {e}", path.display()));
    enc.write_header().map_err(err)?.write_image_data(&px).map_err(err)
}

/// A playing instance plus the surface settings shared by `render` and `bench`.
struct Stage {
    inst: Instance,
    w: u32,
    h: u32,
    fit: Fit,
    align: (f32, f32),
    background: u32,
}

fn stage(a: &Args) -> Result<Stage> {
    let file = open(a.file()?)?;
    if file.artboards.is_empty() {
        bail("the file has no artboards")?;
    }
    let ab = match a.get("artboard") {
        None => 0,
        Some(s) => match file.artboards.iter().position(|x| x.name == s).or_else(|| s.parse().ok().filter(|i| *i < file.artboards.len())) {
            Some(i) => i,
            None => bail(&format!("no artboard \"{s}\". Artboards: {}", file.artboards.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")))?,
        },
    };
    let mut inst = Instance::new(&file, ab)?;
    let find = |what: &str, names: Vec<&str>, wanted: &str| -> Result<usize> {
        names.iter().position(|n| *n == wanted).ok_or_else(|| Error(format!("no {what} \"{wanted}\". Available: {}", names.join(", "))))
    };
    match (a.get("animation"), a.get("state-machine")) {
        (Some(name), _) => {
            let i = find("animation", inst.animation_names(), name)?;
            inst.play_animation(i);
        }
        (_, Some(name)) => {
            let i = find("state machine", inst.state_machine_names(), name)?;
            inst.play_state_machine(i);
        }
        _ => inst.autoplay(),
    }
    for kv in a.all("set") {
        let Some((name, value)) = kv.split_once('=') else { bail(&format!("--set expects property=value, got \"{kv}\""))? };
        // The property's type decides how the value is read, so a string can hold "42" or "#fff".
        let Some(ty) = inst.properties().iter().find(|p| p.0 == name).map(|p| p.1) else {
            bail(&format!("no view-model property \"{name}\". Properties: {}", inst.properties().iter().map(|p| p.0).collect::<Vec<_>>().join(", ")))?
        };
        let ok = match (ty, value) {
            (1, "true" | "false") => inst.set_bool(name, value == "true"),
            (0 | 1, _) => value.parse().is_ok_and(|n| inst.set_number(name, n)),
            (3, _) => json::parse_color(value).is_some_and(|c| inst.set_color(name, c)),
            (4 | 5, _) => inst.set_string(name, value), // an enum by its Option's name
            _ => false,
        };
        if !ok {
            let want = [
                "a number",
                "true, false or a number",
                "set with --fire, not --set",
                "a color like #rgb, #rrggbb or #rrggbbaa",
                "a string",
                "an enum: give one of its Options",
            ];
            bail(&format!("--set {name}=\"{value}\": this property is {}", want.get(ty as usize).unwrap_or(&"of an unknown type")))?;
        }
    }
    for name in a.all("fire") {
        if !inst.fire(name) {
            bail(&format!("no view-model trigger \"{name}\""))?;
        }
    }
    let scale = a.num("scale")?.unwrap_or(1.0);
    let w = a.num("width")?.unwrap_or(inst.width() * scale).round().clamp(1.0, 16384.0) as u32;
    let h = a.num("height")?.unwrap_or(inst.height() * scale).round().clamp(1.0, 16384.0) as u32;
    let fit = match a.get("fit").unwrap_or("contain") {
        "contain" => Fit::Contain,
        "cover" => Fit::Cover,
        "fill" => Fit::Fill,
        "none" => Fit::None,
        "fitWidth" => Fit::FitWidth,
        "fitHeight" => Fit::FitHeight,
        "scaleDown" => Fit::ScaleDown,
        other => bail(&format!("--fit: \"{other}\" is not one of contain, cover, fill, none, fitWidth, fitHeight, scaleDown"))?,
    };
    // In grid order, so the position gives the place: column = x, row = y.
    const ALIGNS: [&str; 9] = ["topLeft", "topCenter", "topRight", "centerLeft", "center", "centerRight", "bottomLeft", "bottomCenter", "bottomRight"];
    let name = a.get("align").unwrap_or("center");
    let Some(i) = ALIGNS.iter().position(|x| *x == name) else { bail(&format!("--align: \"{name}\" is not one of {}", ALIGNS.join(", ")))? };
    let align = ((i % 3) as f32 * 0.5, (i / 3) as f32 * 0.5);
    let background = match a.get("background") {
        None => 0,
        Some(s) => match json::parse_color(s) {
            Some(c) => c,
            None => bail(&format!("--background: \"{s}\" is not a color like #rgb, #rrggbb or #rrggbbaa"))?,
        },
    };
    Ok(Stage { inst, w, h, fit, align, background })
}

fn bail<T>(msg: &str) -> Result<T> {
    Err(Error(msg.to_string()))
}

impl Stage {
    fn frame(&mut self, raster: &mut Raster, px: &mut [u8]) {
        raster::clear(px, self.background);
        // Fitted every frame: a scene may animate its artboard's size.
        let view = self.inst.fit(self.fit, self.align.0, self.align.1, self.w as f32, self.h as f32);
        self.inst.render(raster, px, self.w, self.h, view);
    }
}

fn info(path: &str, as_json: bool) -> Result<String> {
    let bytes = read(path)?.len();
    let file = open(path)?;
    let enum_name = |key: u16, i: u32| match schema::prop(key).map(|d| d.kind) {
        Some(schema::Kind::Enum(v)) => v.get(i as usize).copied().unwrap_or("?"),
        _ => "?",
    };
    let mut artboards = Vec::new();
    let mut text = format!("{path}: {bytes} bytes, {} objects\n", file.objects.len());
    for ab in &file.artboards {
        let o = &file.objects[ab.obj as usize];
        let (w, h) = (o.float(p::WIDTH).unwrap_or(0.0), o.float(p::HEIGHT).unwrap_or(0.0));
        text += &format!("Artboard \"{}\" {w}x{h}, {} drawable objects\n", ab.name, ab.nodes.len());
        let anims: Vec<_> = ab.anims.iter().map(|a| serde_json::json!({ "name": a.name, "fps": a.fps, "duration": a.duration, "loop": enum_name(p::LOOP, a.looping), "tracks": a.tracks.len() })).collect();
        for a in &ab.anims {
            text +=
                &format!("  animation \"{}\": {} frames @ {} fps, {}, {} tracks\n", a.name, a.duration, a.fps, enum_name(p::LOOP, a.looping), a.tracks.len());
        }
        let machines: Vec<_> =
            ab.machines.iter().map(|m| serde_json::json!({ "name": m.name, "layers": m.layers.len(), "listeners": m.listeners.len() })).collect();
        for m in &ab.machines {
            text += &format!("  state machine \"{}\": {} layers, {} listeners\n", m.name, m.layers.len(), m.listeners.len());
        }
        let props: Vec<_> = ab.vm.iter().map(|v| serde_json::json!({ "name": v.name, "type": enum_name(p::VALUE_TYPE, v.ty) })).collect();
        for v in &ab.vm {
            text += &format!("  property \"{}\": {}\n", v.name, enum_name(p::VALUE_TYPE, v.ty));
        }
        artboards.push(serde_json::json!({ "name": ab.name, "width": w, "height": h, "animations": anims, "stateMachines": machines, "properties": props }));
    }
    Ok(if as_json { serde_json::json!({ "ok": true, "bytes": bytes, "objects": file.objects.len(), "artboards": artboards }).to_string() + "\n" } else { text })
}

/// The schema as a Markdown reference (docs/reference/schema.md is this output, verbatim).
fn schema_markdown() -> String {
    let schema: serde_json::Value = serde_json::from_str(&json::schema_json()).unwrap_or_default();
    let mut out = String::from("# Zuri schema reference\n\n<!-- Generated by `zuri schema --markdown`. Do not edit: change crates/zuri-core/src/schema.rs and run scripts/verify.sh. -->\n\nEvery object in a Zuri document is `{ \"type\", ...properties, \"children\": [...] }`. A reference property holds the `name` of its target.\n");
    for ty in schema["types"].as_array().into_iter().flatten() {
        let parents: Vec<&str> = ty["parents"].as_array().into_iter().flatten().filter_map(|p| p.as_str()).collect();
        let place = if parents.is_empty() { "at the top level of the document".to_string() } else { format!("inside {}", parents.join(", ")) };
        out += &format!("\n## {}\n\n{} Lives {place}.\n\n", ty["type"].as_str().unwrap_or(""), ty["doc"].as_str().unwrap_or(""));
        let props = ty["properties"].as_array().cloned().unwrap_or_default();
        if props.is_empty() {
            continue;
        }
        out += "| Property | Kind | Default | Description |\n|---|---|---|---|\n";
        for pr in &props {
            let kind = match pr["values"].as_array() {
                Some(values) => values.iter().filter_map(|v| v.as_str()).map(|v| format!("`{v}`")).collect::<Vec<_>>().join(" "),
                None => pr["kind"].as_str().unwrap_or("").to_string(),
            };
            let default = match &pr["default"] {
                serde_json::Value::Null => String::new(),
                serde_json::Value::String(s) => format!("`{s}`"),
                other => format!("`{other}`"),
            };
            out += &format!("| `{}` | {kind} | {default} | {} |\n", pr["name"].as_str().unwrap_or(""), pr["doc"].as_str().unwrap_or("").replace('|', "\\|"));
        }
    }
    out
}

const STARTER: &str = include_str!("../../../examples/button.json");

fn run(a: &Args) -> Result<String> {
    let as_json = a.opts.contains_key("json");
    let done = |msg: String, extra: serde_json::Value| -> String {
        if as_json {
            let mut j = serde_json::json!({ "ok": true });
            j.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
            j.to_string() + "\n"
        } else {
            msg + "\n"
        }
    };
    let out_path =
        |default_ext: &str| -> Result<PathBuf> { Ok(a.get("output").map(PathBuf::from).unwrap_or(Path::new(a.file()?).with_extension(default_ext))) };

    match a.pos.first().map(String::as_str) {
        None | Some("help") => Ok(HELP.to_string()),
        _ if a.opts.contains_key("help") => Ok(HELP.to_string()),
        Some("schema") if a.opts.contains_key("markdown") => Ok(schema_markdown()),
        Some("schema") => Ok(json::schema_json() + "\n"),
        Some("new") => {
            let path = PathBuf::from(a.file()?);
            if path.exists() {
                bail(&format!("{} already exists", path.display()))?;
            }
            write(&path, STARTER.as_bytes())?;
            Ok(done(format!("wrote {} - edit it, then: zuri build {0}", path.display()), serde_json::json!({ "output": path })))
        }
        Some("build") => {
            let objects = load(a.file()?)?;
            File::from_objects(objects.clone())?; // catch broken references before shipping
            let bytes = format::write(&objects, 0)?;
            let out = out_path("zur")?;
            write(&out, &bytes)?;
            Ok(done(
                format!("wrote {} ({} bytes, {} objects)", out.display(), bytes.len(), objects.len()),
                serde_json::json!({ "output": out, "bytes": bytes.len(), "objects": objects.len() }),
            ))
        }
        Some("import") => {
            let text = zuri_import::import_file(Path::new(a.file()?))?;
            let out = out_path("json")?;
            write(&out, text.as_bytes())?;
            Ok(done(format!("wrote {} - build it with: zuri build {0}", out.display()), serde_json::json!({ "output": out })))
        }
        Some("dump") => {
            let text = json::decompile(&load(a.file()?)?)?;
            match a.get("output") {
                None => Ok(text),
                Some(out) => {
                    write(Path::new(out), text.as_bytes())?;
                    Ok(done(format!("wrote {out}"), serde_json::json!({ "output": out })))
                }
            }
        }
        Some("validate") => {
            let file = open(a.file()?)?;
            for i in 0..file.artboards.len() {
                Instance::new(&file, i)?;
            }
            Ok(done(
                format!("ok: {} objects, {} artboards", file.objects.len(), file.artboards.len()),
                serde_json::json!({ "objects": file.objects.len(), "artboards": file.artboards.len() }),
            ))
        }
        Some("info") => info(a.file()?, as_json),
        Some("render") => {
            let mut s = stage(a)?;
            let out = out_path("png")?;
            let (mut raster, mut px) = (Raster::default(), vec![0u8; (s.w * s.h * 4) as usize]);
            let time = a.num("time")?.unwrap_or(0.0).max(0.0);
            // Step like a real host so state machines see every frame on the way.
            let steps = (time * 60.0).ceil().max(1.0) as u32;
            for _ in 0..steps {
                s.inst.advance(time / steps as f32);
            }
            let frames = a.num("frames")?.unwrap_or(1.0).max(1.0) as u32;
            let dt = 1.0 / a.num("fps")?.unwrap_or(60.0).max(1e-3);
            let mut written = Vec::new();
            for i in 0..frames {
                if i > 0 {
                    s.inst.advance(dt);
                }
                s.frame(&mut raster, &mut px);
                let path = if frames == 1 {
                    out.clone()
                } else {
                    out.with_file_name(format!("{}-{:04}.png", out.file_stem().and_then(|s| s.to_str()).unwrap_or("frame"), i + 1))
                };
                save_png(&path, s.w, s.h, &px)?;
                written.push(path);
            }
            let events = s.inst.take_events();
            Ok(done(
                format!(
                    "wrote {} ({}x{}){}",
                    if frames == 1 { out.display().to_string() } else { format!("{frames} frames next to {}", out.display()) },
                    s.w,
                    s.h,
                    if events.is_empty() { String::new() } else { format!(", events: {}", events.join(", ")) }
                ),
                serde_json::json!({ "output": written, "width": s.w, "height": s.h, "events": events }),
            ))
        }
        Some("bench") => {
            let mut s = stage(a)?;
            let frames = a.num("frames")?.unwrap_or(300.0).max(1.0) as u32;
            let dt = 1.0 / a.num("fps")?.unwrap_or(60.0).max(1e-3);
            let (mut raster, mut px) = (Raster::default(), vec![0u8; (s.w * s.h * 4) as usize]);
            let (mut advance, mut render) = (0.0f64, 0.0f64);
            for _ in 0..frames {
                let t0 = Instant::now();
                s.inst.advance(dt);
                let t1 = Instant::now();
                s.frame(&mut raster, &mut px);
                advance += (t1 - t0).as_secs_f64();
                render += t1.elapsed().as_secs_f64();
            }
            let (adv_ms, ren_ms) = (advance * 1000.0 / frames as f64, render * 1000.0 / frames as f64);
            Ok(done(
                format!(
                    "{}x{}, {frames} frames: advance {adv_ms:.4} ms/frame, render {ren_ms:.3} ms/frame ({:.0} fps on one core)",
                    s.w,
                    s.h,
                    1000.0 / (adv_ms + ren_ms)
                ),
                serde_json::json!({ "width": s.w, "height": s.h, "frames": frames, "advanceMs": adv_ms, "renderMs": ren_ms }),
            ))
        }
        Some(other) => bail(&format!("unknown command \"{other}\" (see `zuri help`)")),
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let as_json = raw.iter().any(|a| a == "--json");
    match Args::parse(&raw).and_then(|a| run(&a)) {
        Ok(out) => print!("{out}"),
        Err(e) => {
            if as_json {
                println!("{}", serde_json::json!({ "ok": false, "error": e.0 }));
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(1);
        }
    }
}
