# Aiko IME

Aiko IME is a lightweight Windows voice input tool written in Rust. It listens for a global hotkey, records microphone audio, streams it to an ASR backend, and inserts recognized text into the currently focused application through the Win32 `SendInput` API.

The project is inspired by and derived from [EvanDbg/doubao-ime-win](https://github.com/EvanDbg/doubao-ime-win). It keeps the same practical idea: a small, portable dictation helper that can type anywhere on Windows without installing a full IME.

> Status: beta / experimental. The current ASR provider uses an unofficial Doubao voice-input protocol and may stop working if the upstream service or protocol changes.

## Features

- Global trigger: double-tap `Ctrl` by default, with support for configurable combo hotkeys.
- Floating control: a small topmost HUD with recording waveform, confirm, and cancel controls.
- System tray: start, stop, settings, and quit from the tray menu.
- Text insertion: recognized text is inserted into the active app using native Windows input events.
- Portable layout: release builds can be packaged as a folder with `aiko-ime.exe` and `config.toml`.
- Optional AI post-processing: configuration fields are present for OpenAI-compatible post-processing and translation.

## Download

Download a release archive from the GitHub Releases page, unzip it, and run:

```powershell
.\aiko-ime.exe
```

On first launch, Aiko IME creates local runtime files next to the executable, including `config.toml` and `credentials.json`. These files are intentionally not committed to the repository.

## Usage

1. Run `aiko-ime.exe`.
2. Double-tap `Ctrl` to start voice input.
3. Speak into the microphone.
4. Double-tap `Ctrl` again, or click the confirm button, to stop and keep the inserted text.
5. Click the cancel button to stop and remove text inserted during the current session.

The floating window can be dragged. Its position is saved in `config.toml`.

## Configuration

Copy `config.toml.example` to `config.toml` when running from source, or edit the generated `config.toml` next to the executable.

```toml
[hotkey]
mode = "double_tap"
combo_key = "Ctrl+Shift+V"
double_tap_key = "Ctrl"
double_tap_interval = 300

[floating_button]
enabled = true
position_x = 100
position_y = 100
```

To use a combo hotkey instead of double-tap Ctrl:

```toml
[hotkey]
mode = "combo"
combo_key = "Ctrl+Shift+V"
```

## Build From Source

Requirements:

- Windows 10/11 x64
- Rust stable
- Visual Studio Build Tools 2022 with Desktop development with C++
- CMake
- Protocol Buffers compiler (`protoc`)

Build:

```powershell
git clone <repo-url>
cd aiko-ime

$env:PROTOC = "C:\path\to\protoc.exe"
cargo build --release
```

Portable package:

```powershell
$env:PROTOC = "C:\path\to\protoc.exe"
.\scripts\build-portable.ps1 -Version "1.1.1"
```

The portable package is written to `dist\aiko-ime-portable` and `aiko-ime-v<version>-portable.zip`.

## Architecture

- `src/main.rs`: UI mode and CLI test mode entry points.
- `src/business/hotkey_manager.rs`: global hotkey and double-tap modifier detection.
- `src/business/voice_controller.rs`: recording session orchestration.
- `src/audio/capture.rs`: microphone capture and Opus audio frame generation.
- `src/asr/`: device registration, ASR protocol, and WebSocket client.
- `src/business/text_inserter.rs`: Win32 text insertion.
- `src/ui/`: system tray and floating window.

## Notes On The ASR Provider

The current backend implements an unofficial Doubao voice-input protocol. This is useful for experimentation, but it is not a stable public API.

Known implications:

- The service may change or reject requests at any time.
- Network access is required.
- The project is not affiliated with ByteDance, Doubao, or any official Doubao product.
- Do not rely on this provider for critical production workflows.

A future direction is to add local/offline providers such as `sherpa-onnx`, SenseVoice, Vosk, or whisper.cpp.

## Development Checks

```powershell
cargo fmt
cargo check
cargo test
```

For recent CMake versions, the repository sets `CMAKE_POLICY_VERSION_MINIMUM=3.5` in `.cargo/config.toml` to keep the bundled Opus dependency buildable.

## Credits

- [EvanDbg/doubao-ime-win](https://github.com/EvanDbg/doubao-ime-win): original Windows Doubao voice input project that inspired this fork.
- [doubaoime-asr](https://github.com/starccy/doubaoime-asr): protocol reference mentioned by the upstream project.
- `cpal`, `opus-rs`, `tokio-tungstenite`, `tray-icon`, and the Rust Windows bindings.

## License

MIT. See `LICENSE`.
