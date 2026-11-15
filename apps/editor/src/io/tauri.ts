// The desktop shell's side of file IO: native dialogs and the Rust commands in
// src-tauri/src/main.rs. Everything here is only called when `isTauri` is true.

import { invoke } from '@tauri-apps/api/core';
import { type DialogFilter, open, save } from '@tauri-apps/plugin-dialog';

/** True inside the Tauri window; false in a plain browser tab. */
export const isTauri: boolean = '__TAURI_INTERNALS__' in window;

// The extensions here are also the ones `read_file` / `write_text` / `export_zur` accept (src-tauri/src/main.rs).
export const pickOpenPath = (filters: DialogFilter[]): Promise<string | null> => open({ multiple: false, directory: false, filters });
export const pickSavePath = (defaultPath: string, filters: DialogFilter[]): Promise<string | null> => save({ defaultPath, filters });

export const readFile = async (path: string): Promise<Uint8Array> => new Uint8Array(await invoke<ArrayBuffer>('read_file', { path }));
export const writeText = (path: string, text: string): Promise<void> => invoke('write_text', { path, text });
/** Import SVG / OBJ / glTF / GLB / FBX as a JSON source document. */
export const importFile = (path: string): Promise<string> => invoke('import_file', { path });
export const translateShader = (wgsl: string): Promise<[glsl: string, hlsl: string]> => invoke('translate_shader', { wgsl });
/** The full build: `@path` assets resolved against `baseDir`, shaders translated. */
export const exportZur = (json: string, baseDir: string, path: string): Promise<void> => invoke('export_zur', { json, baseDir, path });

export const baseName = (path: string): string => path.split(/[\\/]/).pop() ?? path;
export const dirName = (path: string): string => path.replace(/[\\/][^\\/]*$/, '') || '.';
