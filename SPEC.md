# SPEC — feeling-noob

Spec version: 2, 2026-11-20.

`feeling-noob` is a DAW (Digital Audio Workstation) for Windows, written entirely in Rust and licensed GPLv3-or-later.

The name, icon, colors, look, and assets of feeling-noob are original. It does not use names, assets, or material from other products.

---

## 1. MVP features

Every feature has a "done when" criterion that can be checked by hand.

### F1. Tracks area
- Two track types: **Audio** and **Instrument**.
- New Tracks dialog: type, count, input (audio), record enable, input monitoring.
- Track header: name, M (mute), S (solo), R (record enable), I (input monitoring, audio only).
- Regions: select, move (drag or nudge), split at playhead, repeat, cut/copy/paste, delete, mute.
- Snap to grid on/off. Grid: bar, beat, 1/8, 1/16.
- Ruler at the top: bars/beats and the loop area.
- Done when: regions can be moved, split at the playhead, repeated, and deleted, and playback sounds right.

### F2. Transport
- Play/Stop, Record, Go to Beginning, Rewind/Forward (one bar).
- Loop: an area in the ruler; dragging in the ruler creates the loop area and turns looping on.
- Metronome on/off, count-in on/off (bar count set in Project Settings, default 1 bar).
- One fixed tempo per project, 20–400 bpm.
- One fixed time signature per project.
- Position display: bar.beat.division.tick, tempo, time signature.
- Done when: the loop repeats without an audible click at its boundary, the metronome lands on the beat, and count-in plays before recording.

### F3. Piano Roll
- Opened by double-clicking a MIDI region or with P/E.
- Add notes (click with the Pencil), delete, move, resize, edit velocity (slider and vertical drag).
- New notes take the length and velocity of the last edited note. New project default: length = active grid, velocity 100.
- Quantize selected notes to the grid (Q). In the MVP quantize is **destructive** (it changes note positions) and undoable.
- Done when: added or moved notes play back correctly right away, and Q tightens timing.

### F4. Mixer
- One channel strip per track, up to 8 **return tracks** (A to H), one **Master (Main)**. New projects start with 2 MIDI tracks, 2 audio tracks, and the returns A Reverb and B Delay. A bus, in this project, means a group track; groups are on the roadmap.
- Track strip: volume (−∞ to +6 dB), pan, mute, solo, up to 4 VST3 insert slots, one send per return track (level + pre/post fader), monitor In/Auto/Off for audio tracks (Auto plays the input while armed), output Main or No Output, peak meter.
- Return track: volume, pan, mute, 4 insert slots, meter.
- Master: volume, meter.
- Pan law: −3 dB constant power.
- Solo: when any track is soloed, the other tracks are silenced. Return tracks are never silenced by solo, so send effects on a soloed track stay audible.
- Done when: fader, pan, mute, solo, and send sound right and meters move.

### F5. VST3 plugins
- Scan the standard VST3 folder (`C:\Program Files\Common Files\VST3`) plus extra folders from Settings.
- Scanning runs in a **separate process** so a broken plugin cannot crash the app.
- Plugin Manager: plugin list, OK/failed status, Rescan button.
- VST3 instruments on Instrument tracks, VST3 effects in insert slots.
- Plugin editors open in their own window.
- Plugin state is saved in the project.
- Done when: one free VST3 instrument and one free VST3 effect can be loaded, played, saved, and reopened with the same state.

### F6. Recording
- MIDI from a keyboard (port chosen in Settings). Without loop: one new region per take. With loop: all passes are merged into one region.
- Audio from the interface input to an armed Audio track. Result: a WAV file in `audio/` plus a new region.
- Loop is ignored while recording audio (straight recording).
- Input monitoring per audio track.
- Done when: MIDI and audio recordings appear as regions at the right position and play back.

### F7. Save, load, autosave
- Project folder format (section 4). Save, Save As, Open, New.
- Every Save copies the previous file into `backups/`, at most 10.
- Autosave every 60 seconds when there are changes, into `autosave/`.
- On Open, if the autosave is newer than the last manual save, ask the user which one to use.
- Project file validation on load (section 4.4).
- Recent projects list.
- Done when: after force-closing the app with unsaved changes and reopening, the autosave offer appears with the right content.

### F8. Bounce to WAV
- Range: the loop area when looping is on, otherwise from the start to the end of the last region plus a 2 second tail.
- Bit depth: 16-bit, 24-bit, 32-bit float. Sample rate = project sample rate. Interleaved stereo.
- Offline render (faster than realtime) with progress and cancel.
- Muted tracks are excluded.
- Done when: the WAV matches what is heard during playback (listening check plus automated test).

### F9. Keyboard shortcuts
See section 6.

### F10. Built-in minimum
Needed so Phase 4 makes sound before VST3 hosting arrives in Phase 6:
- **Basic Synth**: 16-voice polyphonic, sine/saw/square, ADSR, volume. Default instrument for new tracks.
- **Metronome click**: short synthesized click, accented on beat 1.

---

## 2. Outside the MVP

Everything is listed in `docs/ROADMAP.md`.

---

## 3. Folders and modules

```
feeling-noob\
├── Cargo.toml                 workspace
├── LICENSE                    GPLv3 text
├── SPEC.md
├── docs\
│   ├── ROADMAP.md
│   └── SHORTCUTS.md           code shortcuts (tech debt), not keyboard shortcuts
└── crates\
    ├── fnoob-core\            lib, #![forbid(unsafe_code)]
    ├── fnoob-engine\          lib, #![forbid(unsafe_code)]
    ├── fnoob-platform\        lib, the only crate allowed to use unsafe (Phase 6)
    ├── fnoob-app\             bin "feeling-noob.exe", #![forbid(unsafe_code)]
    └── fnoob-scan\            bin "fnoob-scan.exe", plugin scan process (Phase 6)
```

Crate dependency direction:

```
fnoob-app ──► fnoob-core
    │  └────► fnoob-engine ──► fnoob-core
    └───────► fnoob-platform
fnoob-engine ──► fnoob-platform   (VST3 processing only)
fnoob-scan ──► fnoob-platform
```

### 3.1 fnoob-core
Knows nothing about audio devices or UI.
- `model` — `Project`, `Track`, `Region`, `Note`, `ChannelStrip`, etc. (section 4).
- `time` — tick ↔ sample ↔ seconds conversion, bar.beat.div.tick formatting, grid snapping.
- `edit` — every edit as a pure function `fn(&mut Project, args) -> Result<()>`: split, repeat, nudge, quantize, add note, etc.
- `history` — undo/redo.
- `io` — atomic save, load + validation, backups, autosave, recent projects.
- `keymap` — `Command` ↔ key combination table (section 6).
- `settings` — `settings.json` in `%APPDATA%\feeling-noob\`.

### 3.2 fnoob-engine
- `plan` — `RenderPlan`: a render-ready snapshot of the model (positions in samples, sorted notes, audio data in memory). Built on the UI thread from `Project`.
- `transport` — play/stop/record state, position, loop, count-in.
- `render` — process one block: track → instrument/clip → inserts → volume/pan → send → bus → master.
- `dsp` — Basic Synth, metronome click, gain/pan, meters.
- `device` — open `cpal` output and input streams, audio callback.
- `midi_in` — `midir`, forwards events to the audio thread.
- `record` — WAV writer thread for recordings.
- `bounce` — offline render to WAV on a worker thread, using the same `render` code.
- `bridge` — UI ↔ audio thread interface: `Command` in, `Status` out.

### 3.3 fnoob-platform
All `unsafe` lives here, wrapped in a safe API.
- `vst3` — load a VST3 module (DLL), create instances, process audio, save/restore state, open the editor (HWND).
- `win` — Win32 calls missing from std (when needed), e.g. thread priority.

### 3.4 fnoob-app
- `ui::control_bar` — transport buttons, position display, loop, metronome, count-in.
- `ui::tracks` — track headers, ruler, region area.
- `ui::piano_roll`
- `ui::mixer`
- `ui::dialogs` — New Tracks, Project Settings, Bounce, Plugin Manager, Settings (audio/MIDI).
- `actions` — receives `Command` from keymap/menu/buttons, calls `fnoob_core::edit`, records undo, asks the engine to rebuild the plan.

### 3.5 fnoob-scan
- Called by the app: `fnoob-scan.exe <path-to-plugin.vst3>`.
- Loads the plugin, reads its info (name, vendor, category, instrument/effect, I/O count), writes JSON to stdout, exits.
- A crash or a 10 second timeout marks the plugin as failed.

---

## 4. Data model and project format

### 4.1 Time units
- Musical positions use **ticks**, `PPQ = 960` ticks per quarter note.
- The MVP has one tempo and one time signature, so `sample = tick × 60 × sample_rate / (bpm × 960)`.
- The engine works in samples (`u64`).
- Offsets inside audio files are in frames (samples per channel).

### 4.2 Project folder

```
MyTrack.fnoob\
├── project.json
├── audio\         recordings and imported WAV files
├── backups\       project-YYYYMMDD-HHMMSS.json, at most 10
└── autosave\      project.json
```

### 4.3 `project.json`

```json
{
  "format_version": 1,
  "app_version": "0.1.0",
  "name": "MyTrack",
  "sample_rate": 48000,
  "tempo_bpm": 120.0,
  "time_signature": { "numerator": 4, "denominator": 4 },
  "loop": { "enabled": false, "start_tick": 0, "end_tick": 15360 },
  "metronome": { "enabled": true, "count_in_enabled": false, "count_in_bars": 1, "level_db": -6.0 },
  "snap": { "enabled": true, "grid": "Beat" },
  "next_id": 42,
  "audio_files": [
    { "id": 7, "path": "audio/Audio 1-01.wav", "channels": 1, "frames": 480000 }
  ],
  "tracks": [
    {
      "id": 1,
      "name": "Inst 1",
      "kind": "Instrument",
      "color": 3,
      "record_enabled": false,
      "input_monitoring": false,
      "audio_input": null,
      "instrument": { "BasicSynth": { "waveform": "Saw", "attack_ms": 5.0, "decay_ms": 100.0, "sustain": 0.7, "release_ms": 200.0, "level_db": -12.0 } },
      "strip": {
        "volume_db": 0.0, "pan": 0.0, "mute": false, "solo": false,
        "inserts": [],
        "send": { "level_db": -144.0, "pre_fader": false }
      },
      "regions": [
        {
          "id": 10, "name": "Inst 1", "start_tick": 0, "length_tick": 7680, "muted": false,
          "content": { "Midi": { "notes": [ { "pitch": 60, "velocity": 100, "start_tick": 0, "length_tick": 240 } ] } }
        }
      ]
    },
    {
      "id": 2,
      "name": "Audio 1",
      "kind": "Audio",
      "color": 5,
      "record_enabled": true,
      "input_monitoring": false,
      "audio_input": { "first_channel": 0, "channels": 1 },
      "instrument": null,
      "strip": { "volume_db": 0.0, "pan": 0.0, "mute": false, "solo": false, "inserts": [], "send": { "level_db": -144.0, "pre_fader": false } },
      "regions": [
        { "id": 11, "name": "Audio 1-01", "start_tick": 3840, "length_tick": 9600, "muted": false,
          "content": { "Audio": { "file_id": 7, "offset_frames": 0, "gain_db": 0.0 } } }
      ]
    }
  ],
  "returns": [{ "name": "A Reverb", "volume_db": 0.0, "pan": 0.0, "mute": false, "inserts": [] }],
  "master": { "volume_db": 0.0 }
}
```

VST3 insert / instrument slot:

```json
{ "Vst3": { "class_id": "32 hex chars", "name": "Plugin Name", "path_hint": "C:/Program Files/Common Files/VST3/X.vst3", "bypass": false, "component_state": "base64", "controller_state": "base64" } }
```

Design notes:
- IDs come from the `next_id` counter in the project, not UUIDs. Simpler and enough for one file.
- Note positions are relative to the start of their region.
- `send.level_db = -144.0` means the send is off.
- Unknown fields are ignored on load so files from newer versions still open partially. A `format_version` newer than supported is rejected with a clear message.

### 4.4 Validation on load
Loading is refused (with a message, never a crash) when:
- The JSON is broken or `format_version` is unsupported.
- `sample_rate` is outside 8000–384000, `tempo_bpm` outside 20–400, or the time signature is invalid.
- IDs are duplicated, or a `file_id` is missing from `audio_files`.
- `pitch` > 127, `velocity` outside 1–127, or a length ≤ 0.
- An audio file `path` is absolute, contains `..`, or points outside the project folder. This is also a security rule: a project file must never make the app read or write outside its own folder.

Warnings (the project still opens):
- Missing audio file → the region stays, is marked "missing file", and is silent.
- Missing VST3 plugin → the slot is marked missing and its state is kept as-is so it is not lost on save.

### 4.5 Atomic save
1. Serialize `Project` to JSON in memory.
2. Write `project.json.tmp`, then `File::sync_all()`.
3. If `project.json` exists, copy it to `backups\project-<time>.json`. Delete the oldest backup when there are more than 10.
4. `std::fs::rename("project.json.tmp", "project.json")` (replaces the old file).
5. If any step fails, the old file stays intact and the user gets an error message.

Autosave uses the same steps into `autosave\project.json`, without backups.

### 4.6 Undo
- Snapshots: before every edit a copy of `Project` goes on the undo stack. Limit 100 steps.
- Reason: simplest and certainly correct. MVP projects are small (notes and regions; audio is only referenced, not copied).
- Recorded in `docs/SHORTCUTS.md` when implemented.

---

## 5. Engine

### 5.1 Threads

| Thread | Job | Allocation / locks / I/O allowed? |
|---|---|---|
| UI (eframe) | edit the model, build `RenderPlan`, draw UI, save | Yes |
| Audio output (`cpal` callback) | render blocks, transport, metronome, meters | **No** |
| Audio input (`cpal` callback) | copy input samples into a ring buffer | **No** |
| MIDI input (`midir` callback) | forward events into a ring buffer | Avoid; only push to `rtrb` |
| Recorder | pull samples from the ring buffer, write WAV | Yes |
| Bounce worker | offline render to WAV | Yes (not real-time) |
| Autosave | write files | Yes |

### 5.2 UI ↔ audio communication
- `rtrb` UI → audio: `Command` (Play, Stop, Locate, SetPlan, SetParam, ArmRecord, ...).
- `rtrb` audio → UI: `Status` (old plans to drop, recorded MIDI events, overruns).
- Atomics: playhead position, peak meter per strip (f32 stored as bits in `AtomicU32`).
- Plan swap: the UI builds a new `Box<RenderPlan>` → sends it through `rtrb` → the audio thread swaps it at the start of a block → the old plan goes back to the UI to be dropped. No large `drop` happens on the audio thread.
- All working buffers are allocated when the plan is built, sized for the device's maximum buffer.

### 5.3 Rendering one block
1. Take pending `Command`s.
2. Compute the block's sample range. If looping is on and the range crosses the loop end, split the block: up to the loop end, then from the loop start. On the jump, send note-off to every instrument.
3. Per track: the instrument (Basic Synth / VST3) receives the MIDI events in range, or the audio clip is copied from its in-memory buffer.
4. Inserts → volume → pan → meter → add to Master unless the output is No Output, and send to each return track.
5. Return track: inserts → volume/pan → Master.
6. The metronome is added directly to the output (not through the Master fader).
7. Output: zero NaN/denormals and clamp to ±1.0 as a safety net.
8. Store the playhead position in the atomic.

### 5.4 Audio device
- `cpal` WASAPI host, shared mode. The `cpal` WASAPI backend only supports shared mode; exclusive mode needs custom WASAPI code → roadmap.
- Settings: output device, input device, buffer size (when the device allows it).
- The project sample rate follows the device when the project is created. Opening a project on a device with a different rate shows a warning and blocks playback until the device is changed. Resampling is on the roadmap.
- Input and output are two separate streams. Input reaches the output thread through `rtrb` with a fixed safety buffer. Recording latency compensation: the region is shifted back by the input + output latency reported by the streams. Accuracy is measured in Phase 5.

### 5.5 Audio files
- Import reads WAV, MP3, FLAC, OGG/Vorbis, AIFF and AAC/M4A (`symphonia`). Files in another format or at another sample rate are decoded, resampled to the project rate (`rubato`) and saved into the project folder as 32-bit float WAV; matching WAV files are copied as they are. Recording writes WAV only.
- Whole files are loaded into memory (f32) when the plan is built and shared between plans with `Arc`. Disk streaming is on the roadmap.

### 5.6 VST3 hosting
- Audio processing: **in-process** (plugins are loaded into `feeling-noob.exe`).
- Scanning: **out-of-process** (`fnoob-scan.exe`).
- Reason: out-of-process audio needs real-time IPC (shared memory + an event per block), which is large and full of `unsafe`. In the MVP the most common crash happens during scanning, and that case is covered.

### 5.7 Audio thread safety (checked automatically)
- The audio callback is wrapped in `assert_no_alloc` in debug builds and tests: an allocation in the callback aborts.
- No `Mutex`, `RwLock`, blocking channels, file I/O, or `println!` on the audio path.
- Engine tests render offline through the same `render` function the callback uses.

---

## 6. Keyboard shortcuts

Phase 15 replaced the first set of single-letter shortcuts with this set.

| Command | Key |
|---|---|
| Play / Stop | `Space` |
| Record | `F9` |
| Go to Beginning | `Home` |
| Back / Forward 1 bar | `Left` / `Right` |
| Loop on/off | `Ctrl+L` |
| Metronome | `O` |
| Switch Timeline / Clip Grid | `Tab` |
| Switch Clip / Devices in the detail area | `Shift+Tab` |
| Show/Hide Detail Area | `Ctrl+Alt+L` |
| Show/Hide Browser | `Ctrl+Alt+B` |
| Show/Hide Mixer Section (clip grid) | `Ctrl+Alt+M` |
| Insert Audio Track / MIDI Track / Return Track | `Ctrl+T` / `Ctrl+Shift+T` / `Ctrl+Alt+T` |
| Insert Scene | `Ctrl+I` |
| Rename | `Ctrl+R` |
| Duplicate (regions, clip, or track) | `Ctrl+D` |
| Split at Playhead | `Ctrl+E` |
| Quantize | `Ctrl+U` |
| Snap to Grid | `Ctrl+4` |
| Mute regions, or the track when no region is selected | `0` |
| Nudge Left / Right (1 grid step) | `Alt+Left` / `Alt+Right` |
| Draw Mode | `B` |
| Automation | `A` |
| Computer MIDI Keyboard | `M` |
| Follow | `Ctrl+Shift+F` |
| Capture MIDI | `Ctrl+Shift+C` |
| Bounce / Export | `Ctrl+Shift+R` |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| Cut / Copy / Paste | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` |
| Select All | `Ctrl+A` |
| Delete | `Delete`, `Backspace` |
| Zoom In / Out | `+` or `=` / `-` |
| Select Previous / Next Track | `Up` / `Down` |
| New / Open / Save / Save As | `Ctrl+N` / `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` |
| Add Marker, Previous / Next Marker | `Alt+M`, `Alt+,` / `Alt+.` |

Rules:
- Every shortcut goes through one table in `fnoob-core::keymap`. The UI never reads keys on its own.
- Single-letter shortcuts are disabled while a text field has focus.
- Risk: some keyboard layouts (e.g. German) treat `Ctrl+Alt` as `AltGr`. US layout is safe. User-editable shortcuts are on the roadmap.

---

## 7. UI

Layout:

```
┌──────────────────────────────────────────────────────────────────────┐
│ Control bar: tempo, tap, meter, click | follow, position, transport,  │
│ capture | punch, loop, loop start/length | snap, draw, keys, CPU, view│
├──────────┬───────────────────────────────────────┬───────────────────┤
│ Browser  │ Ruler + loop area                     │ Track headers:    │
│ category │ Regions (timeline) or clip grid       │ name, I/O, mixer  │
│ | items  ├───────────────────────────────────────┴───────────────────┤
│          │ Return track and Main rows                                │
│          ├───────────────────────────────────────────────────────────┤
│          │ Detail area: Clip (properties + piano roll) or Devices    │
├──────────┴───────────────────────────────────────────────────────────┤
│ Status bar: what the control under the mouse does, or the last message│
└──────────────────────────────────────────────────────────────────────┘
```

- Menus: File, Edit, Create, Track, Transport, View (egui widgets, not native menus).
- A tempo lane at the bottom of the ruler shows the tempo at bar 1 and every tempo change; right-click adds, edits or deletes changes.
- Snap and grid size sit at the right end of the status bar. The control bar has Key and Scale; with Key on, the piano roll highlights scale notes and the root.
- The Main column of the clip grid chooses the Main Out and Cue Out output pairs and the cue volume for browser previews.
- In the clip grid the mixer section sits under every column, with an empty drop area, then the return track columns, then Main, which holds the scene launch buttons. Each column has an I/O section (input, monitor, output) and a mixer strip (send knobs, fader with meter, pan knob, activator, solo, arm).
- Dark theme by default.
- VST3 plugin editors in separate windows.

---

## 8. Dependencies

Project rule: every new dependency is justified before it is added. Each one is added in the phase that needs it.

| Crate | Phase | Reason |
|---|---|---|
| `eframe` (+ `egui`) | 2 | UI. Immediate mode, easy custom drawing, MIT/Apache license. |
| `cpal` | 2 | WASAPI audio I/O. |
| `rtrb` | 2 | Lock-free ring buffer between UI and audio. |
| `serde`, `serde_json` | 2 | Settings and project files. |
| `assert_no_alloc` | 2 | Guarantees no allocation in the audio callback (active in debug and tests). |
| `rfd` | 3 | Native Windows Open/Save dialogs. |
| `midir` | 4 | MIDI input. |
| `hound` | 5 | WAV read/write. |
| `symphonia` | 20 | Decoding MP3, FLAC, OGG/Vorbis, AIFF and AAC/M4A on import (pure Rust, MPL-2.0). |
| `rubato` | 20 | High-quality resampling of imported audio to the project sample rate (pure Rust, MIT). |
| `base64` | 6 | Plugin state in JSON. |
| `vst3` | 6 | VST3 bindings. The host itself is our own code in `fnoob-platform`. |
| `windows-sys` | 6 | HWND for plugin editors, `LoadLibraryW`. |

Deliberately **not** used in the MVP: `tokio` (no async needed), `uuid` (replaced by a counter), `basedrop` (old plans are dropped on the UI thread via `rtrb`).

---

## 9. Tests

- `fnoob-core`: unit tests for time conversion, snapping, split, repeat, quantize, undo, serialization round-trip, validation (including `..` and absolute paths), atomic save (simulated failure mid-way).
- `fnoob-engine`: deterministic offline rendering.
  - Metronome: clicks land on the exact sample at 120 bpm, 48 kHz.
  - Basic Synth: notes start on the exact sample, no NaN.
  - Loop: rendering across the loop boundary produces the right sequence.
  - Mixer: mute, solo, pan, send.
  - Bounce: WAV output equals the render buffer.
  - Every render test runs inside `assert_no_alloc`.
- Manual checks per phase: listed in each phase report.
- Commands: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`.

---

## 10. Phase plan (done criteria)

| Phase | Content | Done when |
|---|---|---|
| 2 Skeleton | Cargo workspace, `git init`, LICENSE, empty window, output device selection, metronome sound | `cargo run` opens a window; metronome is heard on the chosen device; `cargo test` passes |
| 3 Timeline & transport | project model, Tracks area, regions, transport, loop, position display, tempo/time signature, undo, shortcuts | F1 and F2 meet their criteria; dummy regions play a beep |
| 4 Instrument + Piano Roll + MIDI | Basic Synth, Piano Roll, quantize, MIDI input, MIDI recording | F3, MIDI part of F6, F10 |
| 5 Audio track + recording | input device, monitoring, WAV recording, WAV import, latency compensation | audio part of F6 |
| 6 Mixer + VST3 | channel strips, return track, Master, meters, scanning, Plugin Manager, VST3 inserts/instruments, editors | F4, F5 |
| 7 Save/load, autosave, export | full project format, backups, autosave, recovery, bounce | F7, F8 |
| 8 Release check | code audit, `.exe` check (imports, hardening flags, DLLs), installer | release checklist passes |

Ordering note: basic save/load is needed early for manual testing. Phase 3 already writes `project.json` (without backups/autosave). Phase 7 adds backups, autosave, recovery, and full validation.

---

## 11. Open decisions

1. WASAPI exclusive mode (lower latency). Decided after shared-mode latency is measured in Phase 5.
