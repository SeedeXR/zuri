//! Zuri editor shell. The UI is the web app in `../src`; this side owns only what a
//! browser cannot do: reading and writing files at real paths, and the authoring crate
//! (`zuri-import`: importers, shader translation, the full build with `@path` assets).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::Path;
use tauri::ipc::Response;

/// What the open dialogs offer (`src/io/files.ts`, `src/io/assets.ts`). SVG and 3D files go through `import_file`.
const READABLE: &[&str] = &["json", "zur", "png", "jpg", "jpeg", "webp", "ttf", "otf"];

/// Paths arrive from the webview, so they are only used when their extension is one the
/// editor's dialogs offer: a page that got compromised cannot read keys or write scripts.
/// ponytail: an extension list, not a path scope (any `.json` on disk is still reachable);
/// move the dialogs into Rust and remember the picked paths if that has to be closed.
fn allow(path: &str, extensions: &[&str]) -> Result<(), String> {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if extensions.contains(&ext.as_str()) {
        return Ok(());
    }
    Err(format!("{path}: the editor only handles {} files here", extensions.join(", ")))
}

/// Bytes of a file the user picked in an open dialog.
#[tauri::command]
fn read_file(path: String) -> Result<Response, String> {
    allow(&path, READABLE)?;
    std::fs::read(&path).map(Response::new).map_err(|e| format!("cannot read {path}: {e}"))
}

/// Write JSON source to the path the user picked in a save dialog.
#[tauri::command]
fn write_text(path: String, text: String) -> Result<(), String> {
    allow(&path, &["json"])?;
    std::fs::write(&path, text).map_err(|e| format!("cannot write {path}: {e}"))
}

/// Import an SVG / OBJ / glTF / GLB / FBX file as a JSON source document.
#[tauri::command]
fn import_file(path: String) -> Result<String, String> {
    zuri_import::import_file(Path::new(&path)).map_err(|e| e.to_string())
}

/// Translate a shader's WGSL into the languages the runtimes use: `(glsl, hlsl)`.
#[tauri::command]
fn translate_shader(wgsl: String) -> Result<(String, String), String> {
    zuri_import::translate_shader(&wgsl).map(|s| (s.glsl, s.hlsl)).map_err(|e| e.to_string())
}

/// Full build (`@path` assets resolved against `base_dir`, shaders translated) written to `path`.
#[tauri::command]
fn export_zur(json: String, base_dir: String, path: String) -> Result<(), String> {
    allow(&path, &["zur"])?;
    let bytes = zuri_import::build(&json, Path::new(&base_dir)).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("cannot write {path}: {e}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_file, write_text, import_file, translate_shader, export_zur])
        .run(tauri::generate_context!())
        .expect("error while running the Zuri editor");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_limited_to_the_extensions_the_editor_handles() {
        assert!(allow("/home/me/scene.JSON", READABLE).is_ok());
        assert!(allow("/home/me/logo.png", READABLE).is_ok());
        for path in ["/home/me/.ssh/id_rsa", "/etc/passwd", "/home/me/.env", "/home/me/scene.json.sh", "json"] {
            assert!(allow(path, READABLE).is_err(), "{path}");
        }
        assert!(allow("/home/me/scene.zur", &["json"]).is_err());
        assert!(allow("/home/me/.zshrc", &["json"]).unwrap_err().ends_with("only handles json files here"));
    }
}
