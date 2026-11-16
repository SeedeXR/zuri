# Changelog

All notable changes to this package are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-15

### Added

- Bindings for the Zuri C API (ABI version 1) and the engine-independent wrappers `ZuriFile` and `ZuriInstance`.
- `.zur` importer producing `ZuriAsset`s with a thumbnail.
- `ZuriPlayer`: plays an artboard into a `Texture2D`, skipping the render while nothing changes.
- `ZuriRawImage` (uGUI) and `ZuriQuad` (world space): display and pointer input.
- Inspector for `ZuriPlayer` with artboard, state machine and animation dropdowns and live view-model controls.
- Key input: `ZuriInstance.Key` and `ZuriPlayer.Key` feed keyDown / keyUp listeners, and `ZuriPlayer.KeyName` names a `KeyCode` the way the runtime expects. Keys are not forwarded automatically.
- Enum view-model properties: `ZuriPropertyType.Enum` and `ZuriProperty.Options`; set and read by position with the number accessors or by name with the string accessors.
- Sounds: an event that names an `AudioAsset` plays it through the player's `AudioSource` (uncompressed WAV only); `ZuriPlayer.Muted`, `ZuriPlayer.AudioSource`, `ZuriAsset.GetSound`, `ZuriFile.SoundId` / `GetEncodedSound`, `ZuriInstance.TryGetEventSound`.
- View-model paths: every property name can be a path into a nested artboard (`card/title`); `ZuriInstance.TryGetPropertyType`.
- Native plugin for macOS (universal: arm64 and x86_64).
- "Basic" sample: an interactive button and a looping animation.
