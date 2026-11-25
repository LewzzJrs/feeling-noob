# feeling-noob

A digital audio workstation for Windows, written in Rust.

> **Status: pre-alpha.** The app has a timeline with audio and instrument tracks, regions you can move, resize, split, repeat, copy, and delete, a transport with loop, metronome, count-in, tempo changes and ramps, and time signature changes, a built-in synth, a piano roll with quantize, MIDI keyboard input and MIDI recording, audio recording with punch in/out, loop recording into take lanes with comping, MIDI capture, audio import (WAV, MP3, FLAC, OGG, AIFF, M4A, resampled to the project rate) with waveforms and input monitoring, a mixer with volume, pan, sends to up to 8 return tracks, monitor and output choices, Main, and meters, built-in effects (EQ, compressor, delay, reverb, limiter, utility), a sampler and a drum sampler, VST3 effects and instruments with their editors, volume and pan automation, region fades and gain, markers, computer-keyboard note input, a clip-launch grid with scenes and follow actions, a device chain view, a browser with categories, track headers with built-in mixer controls, a detail area for clips and devices, an info line for the control under the mouse, undo/redo, keyboard shortcuts, project save/load with backups, autosave and recovery, and bounce to WAV.

## Goals

The first usable version aims to cover the basics of recording and arranging:

- Audio and instrument tracks with regions you can move, split, and repeat
- Transport with loop, metronome, count-in, tempo, and time signature
- Piano roll with note editing, velocity, and quantize
- Mixer with volume, pan, mute, solo, inserts, and a send bus
- VST3 instruments and effects
- MIDI and audio recording
- Project save/load with backups and autosave
- Export to WAV
- Keyboard-driven workflow

The full plan is in [SPEC.md](SPEC.md). Features planned for later are in [docs/ROADMAP.md](docs/ROADMAP.md).

## Requirements

- Windows 10 or later
- Rust stable with the MSVC toolchain (`stable-x86_64-pc-windows-msvc`), installed via [rustup](https://rustup.rs)
- Visual Studio Build Tools with the "Desktop development with C++" workload (needed for the MSVC linker)

## Build and run

Build everything once, so the plugin scanner `fnoob-scan.exe` sits next to the app:

```powershell
cargo build --workspace
```

```powershell
cargo run -p fnoob-app
```

To open a project folder directly:

```powershell
cargo run -p fnoob-app -- "C:\path\to\MySong.fnoob"
```

Release build:

```powershell
cargo build --release --workspace
```

The executables are written to `target\release\`. The C runtime is linked statically, so no Visual C++ redistributable is needed.

## Install

Build the release package:

```powershell
powershell -ExecutionPolicy Bypass -File installer\make-package.ps1
```

This writes `dist\feeling-noob-<version>-windows-x64.zip`. Unzip it and run `Install.cmd`. The app is installed for the current user in `%LOCALAPPDATA%\Programs\feeling-noob`, gets a Start menu entry, and shows up in Windows "Installed apps". `Uninstall.cmd` (or Installed apps) removes it and keeps your projects and settings.

## Tests

```powershell
cargo test --workspace
```

```powershell
cargo clippy --workspace --all-targets -- -D warnings
```

Some tests use real devices or installed plugins (ValhallaSupermassive, free from Valhalla DSP). They are skipped by default; run them with:

```powershell
cargo test --workspace -- --ignored --nocapture
```

## Project layout

| Crate | Purpose |
|---|---|
| `crates/fnoob-core` | Project model, editing, undo, project files, keymap, settings, time math. No audio or UI code. |
| `crates/fnoob-engine` | Audio engine: render plan, transport, loop, count-in, metronome, Basic Synth, MIDI input, audio clips, audio input and recording to WAV, audio devices. |
| `crates/fnoob-platform` | VST3 hosting: loading plugin files, processing, state, editor windows. The only crate with `unsafe` code. |
| `crates/fnoob-scan` | `fnoob-scan.exe`: reads one plugin file in its own process, so a broken plugin cannot crash the app. |
| `crates/fnoob-app` | The desktop app (egui). |

The audio callback never allocates, locks, or does file I/O. Debug builds and tests enforce the no-allocation rule with `assert_no_alloc`.

Settings are stored in `%APPDATA%\feeling-noob\settings.json`, and the scanned plugin list in `%APPDATA%\feeling-noob\plugins.json`.

## License

Copyright (C) 2026 LewzzJrs

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. See [LICENSE](LICENSE).
