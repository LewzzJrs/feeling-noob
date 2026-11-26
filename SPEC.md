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

Phase 22 aligned the shortcuts with the common clip-based workstation layout on Windows.

| Command | Key |
|---|---|
| Play from insert marker / Stop (back to insert marker) | `Space` |
| Continue from stop point | `Shift+Space` |
| Play from insert marker (also while playing) | `Ctrl+Space` |
| Extend the time selection by grid / by track (timeline), extend the track title selection (titles), transpose by an octave / lengthen or shorten notes (piano roll) | `Shift+Left` / `Shift+Right`, `Shift+Up` / `Shift+Down` |
| Move insert marker to the playhead | `Ctrl+Shift+Space` |
| Zoom to time selection or selected clips / zoom back | `Z` / `X` |
| Fit the whole song in view | `W` |
| Record | `F9` |
| Back to Timeline (stop grid clips) | `F10` |
| Insert marker to start | `Home` |
| Move insert marker / nudge selected regions by grid | `Left` / `Right` (`Alt` + arrows also nudge) |
| Loop on/off, or loop the time selection | `Ctrl+L` |
| Loop start / end at insert marker | `Ctrl+F10` / `Ctrl+F11` |
| Loop shorter / longer by a bar | `Ctrl+Left` / `Ctrl+Right` |
| Loop halve / double | `Ctrl+Down` / `Ctrl+Up` |
| Metronome | `O` |
| Switch Timeline / Clip Grid | `Tab` |
| Switch Clip / Devices | `Shift+Tab`, `F12` |
| Show Clip view / Device view | `Ctrl+Alt+3` / `Ctrl+Alt+4` |
| Show/Hide Detail Area | `Ctrl+Alt+L` |
| Show/Hide Browser | `Ctrl+Alt+B`, `Ctrl+Alt+5` |
| Show/Hide Mixer / In-Out / Sends / Returns | `Ctrl+Alt+M` / `Ctrl+Alt+I` / `Ctrl+Alt+S` / `Ctrl+Alt+R` |
| Show take lanes of the selected track | `Ctrl+Alt+U` |
| Settings | `Ctrl+,` |
| Insert Audio / MIDI / Return track | `Ctrl+T` / `Ctrl+Shift+T` / `Ctrl+Alt+T` |
| Insert MIDI clip | `Ctrl+Shift+M` |
| Insert scene (clip grid) / insert silence over the time selection (timeline) | `Ctrl+I` |
| Capture and insert scene (copies the playing clips) | `Ctrl+Shift+I` |
| Record MIDI from the armed track into an empty clip grid slot (press again to stop) | `Ctrl+Shift+F9` |
| Cut / paste / duplicate / delete time (all tracks) | `Ctrl+Shift+X` / `Ctrl+Shift+V` / `Ctrl+Shift+D` / `Ctrl+Shift+Delete` |
| Launch selected clip / stop the selected track's clips | `Enter` / `Ctrl+Enter` |
| Rename | `Ctrl+R` |
| Duplicate | `Ctrl+D` |
| Split | `Ctrl+E` |
| Consolidate MIDI clips or the time selection into one clip | `Ctrl+J` |
| Crop clips to the time selection | `Ctrl+Shift+J` |
| Reverse selected clips | `R` |
| Create fades from the time selection over audio clip edges | `Ctrl+Alt+F` |
| Group / ungroup tracks | `Ctrl+G` / `Ctrl+Shift+G` |
| Freeze or unfreeze the selected track | `Ctrl+Alt+Shift+F` |
| Quantize | `Ctrl+U` |
| Narrow / widen grid | `Ctrl+1` / `Ctrl+2` |
| Triplet grid on/off | `Ctrl+3` |
| Snap to grid | `Ctrl+4` |
| Launch quantize 1/16, 1/8, 1/4, 1 bar, off | `Ctrl+6`, `Ctrl+7`, `Ctrl+8`, `Ctrl+9`, `Ctrl+0` |
| Deactivate regions, or the selected track | `0` |
| Solo / arm selected track | `S` / `C` |
| Track activators 1 to 8 | `F1` to `F8` |
| Draw mode / Automation / Computer MIDI keyboard | `B` / `A` / `M` |
| Follow | `Ctrl+Shift+F`, `Alt+Shift+F` |
| Capture MIDI | `Ctrl+Shift+C` |
| Export audio | `Ctrl+Shift+R` |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| Cut / Copy / Paste / Select All | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` / `Ctrl+A` |
| Delete | `Delete`, `Backspace` |
| Zoom In / Out | `+` or `=` / `-` |
| Select previous / next track | `Up` / `Down` |
| New / Open / Save / Save As / Quit | `Ctrl+N` / `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+Q` |
| Add marker, previous / next marker | `Alt+M`, `Alt+,` / `Alt+.` |

Clicking a track title selects the track itself: `Shift`+click selects a range of titles and `Ctrl`+click adds or removes one. With titles selected, Cut, Copy, Paste, Delete and Duplicate act on whole tracks; pasted tracks go after the last selected track.

While the computer MIDI keyboard is on, its letter keys play notes and do not reach these shortcuts.

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

- Menus: File, Edit, Create, Playback, View, Navigate, Options (egui widgets, not native menus). Switches in the menus show whether they are on. The track title menu also inserts audio, MIDI and return tracks, selects the track's clips, shows take lanes and offers 24 colours in three rows.
- The ruler starts with an overview strip of the whole song; it shows every clip and a frame around the visible part, and a click or drag moves the view. A minutes:seconds ruler sits under the tracks, with W, Z and X zoom buttons next to it. Locator buttons (Set or Delete, previous, next) sit above the track headers.
- Clicking a lane, a clip or the empty space sets the insert marker: a flashing line on the selected tracks (or all tracks) with triangles on both rulers. Dragging in empty lanes makes a time selection over one or more tracks; the status bar then shows its start, end, length and duration, and otherwise the insert marker position and time. `Ctrl+Shift+M` with a time selection makes a MIDI clip of that range on every selected MIDI track. With a time selection, cut, copy, duplicate and delete act on the parts of clips inside it on the selected tracks; paste puts them back at the insert marker and overwrites what is there. Groups can hold other groups; mute, solo and fold reach every level, and inner groups play into the group around them. Group tracks collect their member tracks: members play into the group's own channel (devices, volume, pan, sends) and the group plays to Main; muting or soloing the group mutes or solos its members. A group folds with the triangle in its header, and its lane shows its members' clips. Freezing renders a track with its instrument and devices into a WAV in the project folder and plays that file instead, with the devices off; unfreezing goes back, and Freeze and Flatten turns the track into an audio track holding the frozen file. Reversing an audio clip writes a reversed copy of its audio into the project. The control bar has tempo - and + buttons (1 BPM, or 0.1 with Shift), a count-in menu (off, 1, 2 or 4 bars), the MIDI Arrangement Overdub switch (+: new MIDI recordings merge with the clips under them instead of replacing them), Automation Arm (Auto: while playing, moving a track's volume or pan records automation) and Re-enable, which shows up after a track's volume or pan was moved by hand and lets it follow its automation again. The sample rate sits in the status bar next to the output device. Built-in audio effects: EQ, Compressor, Delay, Reverb, Limiter, Utility, Auto Filter (low-, high- or band-pass with resonance and an LFO), Chorus (two modulated delay lines with feedback), Saturator (soft, hard or warm curve with drive, output and mix) and Gate (threshold, attack, hold, release, floor). A chain holds up to 8 effects. Instruments: Basic Synth, FM Synth (two operators: ratio, amount and envelope), Sampler, Drum Sampler and VST3 plugins. MIDI tracks can hold up to 4 MIDI effects before the instrument: Chord (up to five semitone shifts), Scale (moves notes down into a key) and Arpeggiator (up, down, up/down or random at a grid rate, 1 to 4 octaves, gate); Chord and Scale shape clip and live notes, and the arpeggiator plays held notes while the song plays. The browser has Instruments, Drums (the Drum Sampler and samples named like drums), Audio Effects, MIDI Effects, Plug-ins, Samples and Templates. Playing a sample shows its waveform, length and channels under the list. File > Save as Template keeps the project's tracks, devices and MIDI (not audio) in the templates folder, and a template from the browser starts a new untitled project. In the Devices view every device has a title bar in the track colour; effects have a round on/off switch there, and a switched-off effect turns dim. Clicking an effect's title selects it (white frame); then Delete, Ctrl+X, Ctrl+C, Ctrl+V and Ctrl+D delete, cut, copy, paste (after the selected device, or at the end) and duplicate it, and right-clicking the title offers the same. The instrument can be selected the same way and deleted, cut, copied or pasted; a MIDI track without an instrument makes no sound and shows Drop an Instrument Here with an instrument menu. The free space after the chain says Drop Audio Effects Here. Loading an instrument on a track that still has its default name renames it after the instrument (1 MIDI becomes 1 Sampler). In the Clip Grid, Session Record records MIDI from the armed MIDI track into an empty slot (shown with a red dot) and plays the new clip as soon as recording stops; Capture Scene inserts a scene with copies of the clips that are playing; a green bar under a track's stop button shows that it is playing its timeline clips; an empty slot's menu removes or adds its stop button (a slot without one leaves the track playing when its scene launches); and each clip can have Legato (start where the previous clip was) and its own launch quantize. The piano roll has a MIDI Tools menu that works on the selected notes (or all notes): transpose by a semitone or octave, humanize timing and velocity, legato, strum up or down, arpeggiate up, down or up and down at the grid size, add a chord (major, minor, sus4, seventh, power, octave, or the third and fifth of the song key) and set a chance; a note's chance (0 to 100%) is rolled each time it would play, and notes below 100% are drawn dimmer with their percentage. Grids have triplet forms (1/2T to 1/32T). Audio clips have fade handles on their top corners: dragging one sets the fade-in or fade-out length (one undo step). Audio clips can loop like MIDI clips, and Consolidate (Ctrl+J) on audio renders the selected clips, with gain, fades and loops, into one new WAV in the project. Automation lanes can show Volume, Pan or a send (Send A to H). Right-clicking an automation point sets the shape of the line to the next point (straight, slow start, fast start) or deletes the point. The Clip view shows the clip position and length with Set buttons that take the insert marker, the time signature, and for MIDI clips a Loop switch with its length and Duplicate Loop; a looped MIDI clip repeats its first part over its whole length. The piano roll toolbar has Fit (zoom to the clip), Fold (only rows with notes), Scale (only rows in the song key), Highlight Scale, the grid size, and velocity tools: Randomize by a chosen amount and Ramp from one value to another, on the selected notes or all notes. Right-clicking a lane opens the selection menu: cut, copy, duplicate, delete, zoom to and back, deactivate clips, loop selection, snap on/off, fixed grid (8 bars to 1/32), narrow and widen grid.
- The ruler ends with two lanes. The time signature lane shows the signature at bar 1 and every change; right-click inserts a change at the nearest bar, and a marker's menu edits or deletes it. The song tempo lane shows tempo points joined by ramps, like an automation envelope: right-click adds a point, dragging a point moves it in time or changes its tempo, and a point's menu sets an exact tempo, turns its ramp off for a step, or deletes it.
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
