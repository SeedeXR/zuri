//! `zuri mcp` driven as a client drives it: messages in on stdin, replies out on stdout.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

/// Send the messages, close stdin, and return the replies in order.
fn session(messages: &[Value]) -> Vec<Value> {
    let lines: Vec<Vec<u8>> = messages.iter().map(|message| message.to_string().into_bytes()).collect();
    raw_session(&lines)
}

/// The same, with each line given as bytes. The server gets a temporary folder of its
/// own, which has to be empty again when it leaves.
fn raw_session(lines: &[Vec<u8>]) -> Vec<Value> {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let tmp = std::env::temp_dir().join(format!("zuri-mcp-test-{}-{}", std::process::id(), COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_zuri"));
    // Where std::env::temp_dir looks, on Unix and on Windows.
    server.arg("mcp").env("TMPDIR", &tmp).env("TMP", &tmp).env("TEMP", &tmp).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut server = server.spawn().unwrap();
    let mut stdin = server.stdin.take().unwrap();
    for line in lines {
        stdin.write_all(line).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    drop(stdin); // the server leaves when its input ends
    let out = server.wait_with_output().unwrap();
    assert!(out.status.success());
    let left: Vec<_> = std::fs::read_dir(&tmp).unwrap().map(|entry| entry.unwrap().file_name()).collect();
    assert!(left.is_empty(), "left in the temporary folder: {left:?}");
    std::fs::remove_dir(&tmp).unwrap();
    String::from_utf8(out.stdout).unwrap().lines().map(|line| serde_json::from_str(line).expect("every line of stdout is one JSON message")).collect()
}

const SCENE: &str = r##"{ "zuri": 1, "children": [ { "type": "Artboard", "name": "Main", "width": 40, "height": 20, "children": [
  { "type": "Fill", "color": "#102030" } ] } ] }"##;

#[test]
fn a_client_of_the_current_revision_checks_and_renders_a_scene() {
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
    let request = |id: u32, method: &str, mut params: Value| {
        params["_meta"] = meta.clone();
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    };
    let call = |id: u32, tool: &str, arguments: Value| request(id, "tools/call", json!({ "name": tool, "arguments": arguments }));
    let replies = session(&[
        request(1, "server/discover", json!({})),
        request(2, "tools/list", json!({})),
        call(3, "validate", json!({ "source": SCENE })),
        call(4, "validate", json!({ "source": SCENE.replace("Fill", "Fil") })),
        call(5, "render", json!({ "source": SCENE, "scale": 2 })),
        call(6, "nope", json!({})),
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled" }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/list", "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "1900-01-01" } } }),
        json!({ "jsonrpc": "2.0", "id": 8, "method": "tools/list" }),
        request(9, "resources/list", json!({})),
    ]);
    // One reply per request, in order; the notification got none.
    assert_eq!(replies.iter().map(|r| r["id"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let result = |i: usize| &replies[i]["result"];
    let error = |i: usize| replies[i]["error"]["code"].as_i64();

    assert_eq!((&result(0)["supportedVersions"], &result(0)["resultType"]), (&json!(["2026-07-28"]), &json!("complete")));
    assert_eq!(result(0)["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "zuri");
    let tools: Vec<&str> = result(1)["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools, ["schema", "validate", "info", "render", "build", "dump", "import"]);
    assert!(result(1)["tools"].as_array().unwrap().iter().all(|t| t["inputSchema"]["type"] == "object" && t["description"].is_string()));
    assert_eq!((&result(1)["cacheScope"], result(1)["ttlMs"].is_u64()), (&json!("public"), true));

    // A good scene, and a broken one: the failure is a result the agent can read, not a protocol error.
    assert_eq!(result(2)["isError"], false);
    let why = result(3)["content"][0]["text"].as_str().unwrap();
    assert!(result(3)["isError"] == true && why.contains("unknown type \"Fil\""), "{why}");
    // The picture comes back as a PNG of the size asked for, and no file is left named.
    let (report, picture) = (&result(4)["content"][0]["text"], &result(4)["content"][1]);
    let report: Value = serde_json::from_str(report.as_str().unwrap()).unwrap();
    assert_eq!((&report["width"], &report["height"], report.get("output")), (&json!(80), &json!(40), None));
    let png = zuri_core::json::base64_decode(picture["data"].as_str().unwrap()).unwrap();
    assert_eq!((&picture["mimeType"], &png[..8]), (&json!("image/png"), b"\x89PNG\r\n\x1a\n".as_slice()));
    let decoded = png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
    assert_eq!((decoded.info().width, decoded.info().height), (80, 40));

    // Protocol errors: an unknown tool, a version it does not speak, no version at all, an unknown method.
    assert_eq!((error(5), error(6), error(7), error(8)), (Some(-32602), Some(-32022), Some(-32602), Some(-32601)));
    assert_eq!(replies[6]["error"]["data"], json!({ "supported": ["2026-07-28"], "requested": "1900-01-01" }));
}

#[test]
fn calls_that_cannot_work_say_why_and_leave_nothing_behind() {
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
    let call = |id: u32, tool: &str, arguments: Value| {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": tool, "arguments": arguments, "_meta": meta } })
            .to_string()
            .into_bytes()
    };
    let replies = raw_session(&[
        call(1, "render", json!({ "source": SCENE, "stateMachine": "Nope" })), // the argument is `state-machine`
        call(2, "validate", json!({ "source": SCENE, "help": true })),
        call(3, "validate", json!({ "source": SCENE, "file": "also.json" })),
        call(4, "validate", json!({ "source": SCENE, "file": null })),
        call(5, "build", json!({})),
        call(6, "render", json!({ "source": SCENE, "time": 1e9 })),
        call(7, "render", json!({ "source": SCENE, "frames": 3000 })),
        b"{\"jsonrpc\": \"2.0\", \"id\": 8, \"method\": \"\xff\"}".to_vec(), // not UTF-8
        b"42".to_vec(),
        b"".to_vec(),
        call(9, "info", json!({ "source": SCENE })),
    ]);
    let said = |i: usize| (replies[i]["result"]["isError"].as_bool(), replies[i]["result"]["content"][0]["text"].as_str().unwrap_or(""));
    // A misspelt argument is named, with the ones the tool takes; nothing is quietly ignored.
    assert!(said(0).0 == Some(true) && said(0).1.contains("has no argument \"stateMachine\"") && said(0).1.contains("state-machine"), "{:?}", said(0));
    assert!(said(1).0 == Some(true) && said(1).1.contains("has no argument \"help\""), "{:?}", said(1));
    assert!(said(2).0 == Some(true) && said(2).1.contains("one of the two"), "{:?}", said(2));
    assert_eq!(said(3).0, Some(false), "an argument given as null is not given");
    assert!(said(4).0 == Some(true) && said(4).1.contains("takes a `file`"), "{:?}", said(4));
    assert!(said(5).0 == Some(true) && said(5).1.contains("3600"), "{:?}", said(5));
    assert!(said(6).0 == Some(true) && said(6).1.contains("has no argument \"frames\""), "{:?}", said(6));
    // Lines it cannot read are refused, without an id, and the server goes on.
    assert_eq!((replies[7]["error"]["code"].as_i64(), replies[7].get("id")), (Some(-32700), None));
    assert_eq!((replies[8]["error"]["code"].as_i64(), replies[8].get("id")), (Some(-32600), None));
    assert_eq!((replies.len(), replies[9]["id"].as_u64(), said(9).0), (10, Some(9), Some(false)), "the empty line got no reply; the call after it did");
}

#[test]
fn a_client_that_opens_with_initialize_is_served_as_before() {
    let replies = session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "old", "version": "1" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "info", "arguments": { "source": SCENE } } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
    ]);
    assert_eq!(replies.len(), 5);
    assert_eq!((&replies[0]["result"]["protocolVersion"], &replies[0]["result"]["capabilities"]), (&json!("2025-06-18"), &json!({ "tools": {} })));
    assert_eq!(replies[1]["result"], json!({}));
    // Results in the old shape: nothing this client's revision does not know.
    assert!(replies[2]["result"]["tools"].is_array() && replies[2]["result"].get("resultType").is_none());
    let info: Value = serde_json::from_str(replies[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(info["artboards"][0]["name"], "Main");
    // A version it does not know: it offers the newest one with a handshake.
    assert_eq!(replies[4]["result"]["protocolVersion"], "2025-11-25");

    // The revision of 2025-03-26 may send several messages in one line: one line comes back.
    let replies = session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26", "capabilities": {} } }),
        json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }, { "jsonrpc": "2.0", "id": 2, "method": "ping" }, { "jsonrpc": "2.0", "id": 3, "method": "tools/list" }]),
        json!([{ "jsonrpc": "2.0", "method": "notifications/cancelled" }]),
    ]);
    assert_eq!(replies.len(), 2, "a batch of notifications alone gets no reply");
    assert_eq!(replies[1].as_array().unwrap().iter().map(|reply| reply["id"].as_u64().unwrap()).collect::<Vec<_>>(), [2, 3]);
}

#[test]
fn only_the_bare_command_serves() {
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_zuri")).args(args).stdin(Stdio::null()).output().unwrap();
    let help = run(&["mcp", "--help"]);
    assert!(help.status.success() && String::from_utf8_lossy(&help.stdout).contains("COMMANDS"));
    let extra = run(&["mcp", "serve"]);
    assert!(!extra.status.success() && String::from_utf8_lossy(&extra.stderr).contains("takes no arguments"));
}
