//! Keeps the C header honest: (1) it declares exactly the functions the library exports,
//! and (2) a real C program built against it and the static library runs correctly.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every identifier in `text` that starts with `zuri_` and is followed by `(`.
fn functions(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("zuri_") {
        let name: String = rest[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let after = &rest[i + name.len()..];
        let boundary = i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric();
        if boundary && after.starts_with('(') {
            out.insert(name.clone());
        }
        rest = after;
    }
    out
}

#[test]
fn header_declares_exactly_the_exported_functions() {
    let source = std::fs::read_to_string(here().join("src/lib.rs")).unwrap();
    let exported: BTreeSet<String> =
        source.split("extern \"C\" fn ").skip(1).map(|s| s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()).collect();
    // Drop the comment block: it mentions functions in prose.
    let header = std::fs::read_to_string(here().join("include/zuri.h")).unwrap();
    let declared = functions(header.split("#ifndef ZURI_H").nth(1).unwrap());
    assert!(exported.len() > 40, "export scan found only {}", exported.len());
    assert_eq!(exported, declared, "src/lib.rs and include/zuri.h disagree");
}

#[test]
fn a_c_program_can_drive_the_runtime() {
    // target/<profile>/deps/abi-xxxx -> target/<profile>
    let target = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf();
    // `cargo test` leaves the static library in deps/; `cargo build` also copies it up.
    let name = if cfg!(windows) { "zuri.lib" } else { "libzuri.a" };
    let lib = [target.join("deps").join(name), target.join(name)].into_iter().find(|p| p.exists()).expect("static library not built");

    let examples = here().join("../../examples");
    let src = std::fs::read_to_string(examples.join("button.json")).unwrap();
    // The button, with a sound on its event ("RIFF" is not a sound: nothing here decodes it).
    let event = r#"{ "type": "Event", "name": "clicked""#;
    assert!(src.contains(event));
    let src = src.replacen(r#""children": ["#, r#""children": [ { "type": "AudioAsset", "name": "click", "bytes": "UklGRg==" },"#, 1);
    let src = src.replace(event, &format!(r#"{event}, "asset": "click", "volume": 0.5"#));
    let objects = zuri_core::json::compile(&src, &|_| unreachable!()).unwrap();
    let zur = target.join("abi-smoke.zur");
    std::fs::write(&zur, zuri_core::format::write(&objects, 0).unwrap()).unwrap();

    // The system's C compiler and what Rust's standard library links against there. On
    // Windows that is clang (it targets MSVC, like the library); CC overrides.
    let (compiler, libs): (&str, &[&str]) = if cfg!(windows) {
        // _CRT_SECURE_NO_WARNINGS: the Windows headers mark fopen deprecated, and warnings are errors here.
        ("clang", &["-D_CRT_SECURE_NO_WARNINGS", "-lkernel32", "-lntdll", "-luserenv", "-lws2_32", "-ladvapi32", "-lbcrypt", "-ldbghelp"])
    } else if cfg!(target_os = "macos") {
        ("cc", &["-lSystem"])
    } else {
        ("cc", &["-lm", "-lpthread", "-ldl"])
    };
    let exe = target.join(if cfg!(windows) { "abi-smoke.exe" } else { "abi-smoke" });
    let cc = Command::new(std::env::var("CC").unwrap_or(compiler.into()))
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(here().join("include"))
        .arg(here().join("tests/smoke.c"))
        .arg(&lib)
        .arg("-o")
        .arg(&exe)
        .args(libs)
        .output()
        .expect("a C compiler is required for this test (cc, or clang on Windows; set CC to choose)");
    assert!(cc.status.success(), "cc failed:\n{}", String::from_utf8_lossy(&cc.stderr));

    let run = Command::new(&exe).arg(&zur).output().unwrap();
    assert!(run.status.success(), "smoke test failed:\n{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "OK");
}

#[test]
fn an_event_without_a_name_is_not_reported() {
    // The compiler does not write one; a file made by hand can hold it. Core's
    // `take_events` leaves it out, and so does the C ABI: there is nothing to call it.
    let src = r##"{ "zuri": 1, "children": [ { "type": "Artboard", "width": 10, "height": 10, "children": [
      { "type": "Event", "name": "tapped" },
      { "type": "Shape", "name": "pad", "x": 5, "y": 5, "children": [ { "type": "Rectangle", "width": 10, "height": 10 } ] },
      { "type": "StateMachine", "children": [ { "type": "Listener", "target": "pad", "event": "down", "emit": "tapped" } ] } ] } ] }"##;
    let mut objects = zuri_core::json::compile(src, &|_| unreachable!()).unwrap();
    let polled = |objects: &[zuri_core::format::RawObject]| unsafe {
        let bytes = zuri_core::format::write(objects, 0).unwrap();
        let file = zuri::zuri_file_load(bytes.as_ptr(), bytes.len());
        let inst = zuri::zuri_instance_new(file, 0);
        zuri::zuri_instance_autoplay(inst);
        let mut px = [0u8; 10 * 10 * 4];
        assert!(zuri::zuri_instance_render(inst, px.as_mut_ptr(), 10, 10, 0, 0, false));
        zuri::zuri_instance_pointer(inst, 1, 5.0, 5.0);
        zuri::zuri_instance_advance(inst, 0.0);
        let (count, mut name) = (zuri::zuri_instance_poll_events(inst), [0u8; 16]);
        let len = zuri::zuri_instance_event_name(inst, 0, name.as_mut_ptr(), name.len());
        zuri::zuri_instance_free(inst);
        zuri::zuri_file_free(file);
        (count, String::from_utf8_lossy(&name[..len]).into_owned())
    };
    assert_eq!(polled(&objects), (1, "tapped".to_string()));
    let event = objects.iter().position(|o| o.type_key == zuri_core::schema::t::EVENT_NODE).unwrap();
    objects[event].props.retain(|(key, _)| *key != zuri_core::schema::p::NAME);
    assert_eq!(polled(&objects), (0, String::new()));
}
