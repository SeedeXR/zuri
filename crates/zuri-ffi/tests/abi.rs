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
