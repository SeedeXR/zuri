//! `zuri mcp`: the commands as Model Context Protocol tools over stdin / stdout, so that
//! an AI agent without a shell can read the format, check a scene and look at a render.
//!
//! Speaks the revision of 2026-07-28 (every request names its version in `_meta`, there
//! is no handshake) and, for a client that opens with `initialize`, the ones before it.
//! Read on 2026-11-15: modelcontextprotocol.io/specification/2026-07-28 (basic, lifecycle,
//! transports/stdio, server/discover, server/tools, changelog) and /2025-06-18.
//
// ponytail: tools only (no resources, prompts, logging, progress or input requests), and
// one request at a time: a call cannot be cancelled, so what a call may cost is bounded
// (one picture a call, `time` up to an hour). A scene given inline (`source`) cannot
// refer to `@path` assets, which are looked up next to a file: give a `file` for those.

use crate::{run, Args};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

const MODERN: &str = "2026-07-28";
const LEGACY: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const META: &str = "io.modelcontextprotocol/";
const INSTRUCTIONS: &str = "Zuri scenes are JSON: an object is a `type`, its properties and its `children`. \
Call `schema` to learn the types and properties, write a scene, `validate` it (an error says where it is and how to fix it), \
then `render` it to see it. `build` writes the .zur file that the web, Unity and Unreal runtimes play.";

/// One tool: its name, what it does, the arguments it cannot do without, and all of its
/// arguments as (name, type, description), where the type is `string`, `number`,
/// `boolean` or `strings` (a list of strings).
fn tool(name: &str, description: &str, required: &[&str], arguments: &[(&str, &str, &str)]) -> Value {
    let mut properties = Map::new();
    for (argument, ty, about) in arguments {
        let schema = match *ty {
            "strings" => json!({ "type": "array", "items": { "type": "string" }, "description": about }),
            ty => json!({ "type": ty, "description": about }),
        };
        properties.insert(argument.to_string(), schema);
    }
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false } })
}

fn tools() -> Value {
    const FILE: (&str, &str, &str) =
        ("file", "string", "Path of a .zur file or of JSON source. A relative path is taken from the folder the server was started in.");
    const SOURCE: (&str, &str, &str) = ("source", "string", "The JSON source itself, in place of `file`. It cannot use \"@path\" assets.");
    const OUTPUT: (&str, &str, &str) = ("output", "string", "Path to write to.");
    json!([
        tool("schema", "Every object type and property of the Zuri format, with what each means. Read it before writing a scene.", &[], &[("markdown", "boolean", "A Markdown reference in place of JSON.")]),
        tool("validate", "Check a scene, given as `file` or as `source`. An error names the object and says how to fix it.", &[], &[FILE, SOURCE]),
        tool("info", "List the artboards of a scene (`file` or `source`) with their animations, state machines, view-model properties and events.", &[], &[FILE, SOURCE]),
        tool(
            "render",
            "Draw a scene (`file` or `source`) and return the picture (PNG). Play an animation or a state machine, move time on, set view-model properties, to see any moment of it.",
            &[],
            &[
                FILE,
                SOURCE,
                ("output", "string", "Path of the PNG to write. Without it the picture is only returned."),
                ("artboard", "string", "Artboard name or index (default: the first)."),
                ("animation", "string", "Play this animation."),
                ("state-machine", "string", "Play this state machine (default: the first state machine, else the first animation)."),
                ("time", "number", "Seconds to advance before drawing (default 0, at most 3600)."),
                ("width", "number", "Width in pixels (default: the artboard's, times `scale`)."),
                ("height", "number", "Height in pixels."),
                ("scale", "number", "Multiply the artboard's size (default 1)."),
                ("fit", "string", "contain (default), cover, fill, none, fitWidth, fitHeight, scaleDown."),
                ("align", "string", "center (default), topLeft, topCenter, topRight, centerLeft, centerRight, bottomLeft, bottomCenter, bottomRight."),
                ("background", "string", "Color behind the artboard, #rrggbb or #rrggbbaa (default transparent)."),
                ("set", "strings", "View-model properties to set, each as \"name=value\" (a number, true / false, a #color or text)."),
                ("fire", "strings", "View-model triggers to fire."),
            ]
        ),
        tool("build", "Compile JSON source into the .zur file the runtimes play.", &["file"], &[FILE, OUTPUT]),
        tool("dump", "Turn a .zur file back into JSON source (returned, or written to `output`).", &["file"], &[FILE, OUTPUT]),
        tool("import", "Convert an SVG, OBJ, glTF / GLB or FBX file into JSON source.", &["file"], &[FILE, OUTPUT]),
    ])
}

/// A folder of our own for the files of one call, removed with what is in it when the
/// call is over, however it ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// The path of `file` in the folder, which is made on first use.
    fn path(scratch: &mut Option<Scratch>, file: &str) -> Result<String, String> {
        if scratch.is_none() {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_nanos());
            let dir = std::env::temp_dir().join(format!("zuri-mcp-{}-{nanos}", std::process::id()));
            // Ours alone: making it fails if anything is there already, and no one else
            // may put a file (or a link to somewhere else) into it.
            #[cfg_attr(not(unix), allow(unused_mut))] // only Unix has a mode to set
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder.create(&dir).map_err(|e| format!("cannot make the temporary folder {}: {e}", dir.display()))?;
            *scratch = Some(Scratch(dir));
        }
        Ok(scratch.as_ref().map(|dir| dir.0.join(file).to_string_lossy().into_owned()).unwrap_or_default())
    }
}

/// Run a tool as the command of the same name. What it prints is the text of the result.
/// A call that cannot work (a wrong argument, a broken scene) is a result too, with
/// `isError`, so that the agent reads why and tries again; only a tool that does not
/// exist is refused.
fn call(params: &Value) -> Result<Value, Value> {
    let (name, tools) = (params["name"].as_str().unwrap_or(""), tools());
    let Some(tool) = tools.as_array().and_then(|all| all.iter().find(|tool| tool["name"] == name)) else {
        return Err(json!({ "code": -32602, "message": format!("Unknown tool: {name}") }));
    };
    let result = |text: &str, failed: bool| json!({ "content": [{ "type": "text", "text": text }], "isError": failed });
    let text = |value: &Value| value.as_str().map_or_else(|| value.to_string(), String::from);
    let none = Map::new();
    let known = tool["inputSchema"]["properties"].as_object().unwrap_or(&none);
    let mut args = Args { pos: vec![name.to_string()], opts: HashMap::from([("json".to_string(), vec![String::new()])]) };
    let mut scratch = None;
    for (key, value) in params["arguments"].as_object().into_iter().flatten() {
        if !known.contains_key(key) {
            let takes: Vec<&str> = known.keys().map(String::as_str).collect();
            return Ok(result(&format!("`{name}` has no argument \"{key}\". It takes: {}", takes.join(", ")), true));
        }
        match (key.as_str(), value) {
            (_, Value::Null | Value::Bool(false)) => {}
            ("file", path) => args.pos.push(text(path)),
            ("source", source) => {
                let written = Scratch::path(&mut scratch, "scene.json").and_then(|path| match std::fs::write(&path, text(source)) {
                    Ok(()) => Ok(path),
                    Err(e) => Err(format!("cannot write {path}: {e}")),
                });
                match written {
                    Ok(path) => args.pos.push(path),
                    Err(why) => return Ok(result(&why, true)),
                }
            }
            (_, Value::Array(items)) => drop(args.opts.insert(key.clone(), items.iter().map(text).collect())),
            (_, single) => drop(args.opts.insert(key.clone(), vec![text(single)])),
        }
    }
    if name != "schema" && args.pos.len() != 2 {
        let either = if known.contains_key("source") { ", or the scene itself as `source` (one of the two)" } else { "" };
        return Ok(result(&format!("`{name}` takes a `file`{either}"), true));
    }
    if matches!(args.num("time"), Ok(Some(seconds)) if seconds > 3600.0) {
        return Ok(result("`time` is at most 3600 seconds", true));
    }
    // A picture nobody asked to keep is drawn to a file of ours and only returned.
    let keep = name != "render" || args.opts.contains_key("output");
    if !keep {
        match Scratch::path(&mut scratch, "frame.png") {
            Ok(path) => drop(args.opts.insert("output".into(), vec![path])),
            Err(why) => return Ok(result(&why, true)),
        }
    }

    // A panic in a command (a bug) fails this call, not the server and the calls after it.
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&args)));
    let ran = ran.unwrap_or_else(|_| Err(zuri_core::Error("the command stopped unexpectedly: a bug in zuri, worth reporting with this scene".into())));
    let failed = ran.is_err();
    let (mut printed, mut picture) = (ran.unwrap_or_else(|e| e.0), None);
    if let ("render", Ok(mut report)) = (name, serde_json::from_str::<Value>(&printed)) {
        picture = report["output"][0].as_str().and_then(|path| std::fs::read(path).ok());
        if !keep {
            // The file is ours and about to go: the report does not name it.
            report.as_object_mut().map(|report| report.remove("output"));
            printed = report.to_string();
        }
    }
    let mut reply = result(printed.trim_end(), failed);
    if let (Some(png), Some(content)) = (picture, reply["content"].as_array_mut()) {
        content.push(json!({ "type": "image", "data": zuri_core::json::base64_encode(&png), "mimeType": "image/png" }));
    }
    Ok(reply)
}

/// The reply to one request, or None for a notification, which is never answered.
/// `legacy` is whether this client opened with `initialize`; such a client does not say
/// its version in each request.
fn answer(message: &Value, legacy: &mut bool) -> Option<Value> {
    let id = message.get("id").filter(|id| !id.is_null())?;
    let (method, params) = (message["method"].as_str().unwrap_or(""), &message["params"]);
    let meta = |key: &str| &params["_meta"][format!("{META}{key}")];
    let version = meta("protocolVersion").as_str();
    let server = json!({ "name": "zuri", "title": "Zuri", "version": env!("CARGO_PKG_VERSION") });
    let refuse = |code: i64, message: String| Err(json!({ "code": code, "message": message }));
    let result = match (method, version) {
        ("initialize", _) => {
            *legacy = true;
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = if LEGACY.contains(&asked) { asked } else { LEGACY[0] };
            Ok(json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": server, "instructions": INSTRUCTIONS }))
        }
        (_, Some(asked)) if asked != MODERN => {
            Err(json!({ "code": -32022, "message": "Unsupported protocol version", "data": { "supported": [MODERN], "requested": asked } }))
        }
        (_, Some(_)) if !meta("clientCapabilities").is_object() => refuse(-32602, format!("params._meta has no \"{META}clientCapabilities\"")),
        (_, None) if !*legacy => refuse(-32602, format!("the request does not name its protocol version (params._meta[\"{META}protocolVersion\"])")),
        ("server/discover", Some(_)) => Ok(json!({ "supportedVersions": [MODERN], "capabilities": { "tools": {} }, "instructions": INSTRUCTIONS })),
        ("ping", None) => Ok(json!({})),
        ("tools/list", _) => Ok(json!({ "tools": tools() })),
        ("tools/call", _) => call(params),
        _ => refuse(-32601, format!("Method not found: {method}")),
    };
    let body = match result {
        Ok(mut result) => {
            if version.is_some() {
                // What this revision asks of every result, and of the ones a client may keep.
                result["resultType"] = json!("complete");
                result["_meta"] = json!({ format!("{META}serverInfo"): server });
                if method != "tools/call" {
                    (result["ttlMs"], result["cacheScope"]) = (json!(3_600_000), json!("public"));
                }
            }
            ("result", result)
        }
        Err(error) => ("error", error),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, body.0: body.1 }))
}

/// Answer messages, one a line, until stdin closes. Nothing else goes to stdout.
pub fn serve() {
    let (mut legacy, mut input, mut line) = (false, std::io::stdin().lock(), Vec::new());
    // A reply to something that is not a request has no id to carry.
    let unreadable = |code: i64, message: String| Some(json!({ "jsonrpc": "2.0", "error": { "code": code, "message": message } }));
    // Lines as bytes: one that is not UTF-8 is a message to refuse, not the end of the input.
    while input.read_until(b'\n', &mut line).is_ok_and(|read| read > 0) {
        let reply = match serde_json::from_slice::<Value>(&line) {
            _ if line.iter().all(u8::is_ascii_whitespace) => None,
            Err(e) => unreadable(-32700, format!("Parse error: {e}")),
            Ok(message @ Value::Object(_)) => answer(&message, &mut legacy),
            // Several messages in one (only the revision of 2025-03-26 has these): the
            // replies come back in one, too.
            Ok(Value::Array(batch)) if !batch.is_empty() => {
                let replies: Vec<Value> = batch.iter().filter_map(|message| answer(message, &mut legacy)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(_) => unreadable(-32600, "Invalid Request: a message is a JSON object".into()),
        };
        line.clear();
        // One message a line (serde_json writes a newline inside a string as \n); stdout
        // passes each line on as it ends.
        if reply.is_some_and(|reply| writeln!(std::io::stdout(), "{reply}").is_err()) {
            break;
        }
    }
}
