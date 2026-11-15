//! Zuri editor shell. The UI is the web app in `../src`; this side owns only what a
//! browser cannot do: reading and writing files at real paths, and the authoring crate
//! (`zuri-import`: importers, shader translation, the full build with `@path` assets).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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

/// A font installed on this machine: (family, style, path, its number in that file).
type Installed = (String, String, String, u32);

/// Where this system keeps its fonts.
// ponytail: the usual folders. On Linux, fontconfig can be told of others (NixOS and
// Flatpak do): ask `fc-list` if someone's fonts are missing.
fn font_folders() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from).unwrap_or_default();
    let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let folders = if cfg!(target_os = "macos") {
        vec!["/System/Library/Fonts".into(), "/Library/Fonts".into(), home.join("Library/Fonts")]
    } else if cfg!(windows) {
        vec![var("WINDIR").join("Fonts"), var("LOCALAPPDATA").join("Microsoft/Windows/Fonts")]
    } else {
        vec![
            "/usr/share/fonts".into(),
            "/usr/local/share/fonts".into(),
            home.join(".fonts"),
            home.join(".local/share/fonts"),
            var("XDG_DATA_HOME").join("fonts"),
        ]
    };
    // A variable that is not set leaves a path that starts nowhere: not a folder to scan.
    folders.into_iter().filter(|folder| folder.is_absolute()).collect()
}

/// Family and style of each font in a file (a collection holds several): None for one
/// whose names cannot be read. Only the few bytes that say so are read: system fonts
/// are many, and some are tens of megabytes.
fn names_in(path: &Path) -> Option<Vec<Option<(String, String)>>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut read = |at: u64, len: usize| {
        let mut bytes = vec![0; len];
        file.seek(SeekFrom::Start(at)).and_then(|_| file.read_exact(&mut bytes)).ok().map(|()| bytes)
    };
    let be32 = |bytes: &[u8], at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let head = read(0, 12)?;
    let starts: Vec<u64> = match &head[..4] {
        b"ttcf" => read(12, 4 * be32(&head, 8).min(256) as usize)?.chunks(4).map(|at| be32(at, 0).into()).collect(),
        _ => vec![0],
    };
    let mut names = |start: u64| {
        // The font's list of tables: 16 bytes each, after a header that says how many.
        let count = read(start + 4, 2)?;
        let tables = read(start + 12, 16 * u16::from_be_bytes([count[0], count[1]]) as usize)?;
        let name = tables.chunks(16).find(|table| &table[..4] == b"name")?;
        zuri_import::font::names(&read(be32(name, 8).into(), be32(name, 12).min(1 << 20) as usize)?)
    };
    Some(starts.into_iter().map(&mut names).collect())
}

/// Every font in the system's font folders, by family and style. Read once.
fn installed() -> &'static [Installed] {
    static FONTS: OnceLock<Vec<Installed>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let (mut fonts, mut folders) = (Vec::new(), font_folders().into_iter().map(|folder| (folder, 0)).collect::<Vec<_>>());
        while let Some((folder, depth)) = folders.pop() {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                let path = entry.path();
                let kind = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                if path.is_dir() && depth < 4 {
                    folders.push((path, depth + 1));
                } else if ["ttf", "otf", "ttc", "otc"].contains(&kind.as_str()) {
                    for (index, names) in names_in(&path).into_iter().flatten().enumerate() {
                        // A family that starts with a dot is the system's own, not for documents.
                        if let Some((family, style)) = names.filter(|(family, _)| !family.starts_with('.')) {
                            fonts.push((family, style, path.to_string_lossy().into_owned(), index as u32));
                        }
                    }
                }
            }
        }
        fonts.sort();
        fonts.dedup_by(|a, b| (&a.0, &a.1) == (&b.0, &b.1)); // the same font in two folders
        fonts
    })
}

/// The fonts installed on this machine, for the font picker.
#[tauri::command(async)]
fn system_fonts() -> Vec<Installed> {
    installed().to_vec()
}

/// The bytes of one installed font, as a file of its own. Only what `system_fonts`
/// listed can be asked for: the path comes from the webview.
#[tauri::command(async)]
fn read_font(path: String, index: u32) -> Result<Response, String> {
    if !installed().iter().any(|font| font.2 == path && font.3 == index) {
        return Err(format!("{path} is not an installed font"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {path}: {e}"))?;
    zuri_import::font::single_face(&bytes, index).map(Response::new).ok_or(format!("{path} is not a font file this can read"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_file, write_text, import_file, translate_shader, export_zur, system_fonts, read_font])
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

    #[test]
    fn installed_fonts_are_listed_by_name_and_only_they_can_be_read() {
        let outfit = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples/assets/Outfit.ttf"));
        assert_eq!(names_in(outfit), Some(vec![Some(("Outfit".to_string(), "Thin".to_string()))]));
        assert_eq!(names_in(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))), Some(vec![None]), "not a font: no names");
        // Whatever this machine has: every entry is a font file that reads back as one font.
        let fonts = installed();
        println!("{} installed fonts, e.g. {:?}", fonts.len(), fonts.iter().map(|f| format!("{} {}", f.0, f.1)).take(5).collect::<Vec<_>>());
        assert!(fonts.windows(2).all(|pair| pair[0] < pair[1]), "sorted, each family and style once");
        assert!(font_folders().iter().all(|folder| folder.is_absolute()));
        // macOS and Windows always come with fonts; a Linux build machine may have none.
        assert!(!fonts.is_empty() || cfg!(target_os = "linux"), "no fonts in {:?}", font_folders());
        // They read back as fonts (a machine may hold an odd file or two that does not).
        let tried: Vec<bool> = fonts.iter().take(40).map(|(_, _, path, index)| read_font(path.clone(), *index).is_ok()).collect();
        assert!(tried.iter().filter(|read| **read).count() * 10 >= tried.len() * 9, "{tried:?}");
        assert!(read_font(outfit.to_string_lossy().into_owned(), 0).is_err(), "not in a font folder");
    }
}
