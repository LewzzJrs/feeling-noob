# SHORTCUTS

Shortcuts taken in the code (tech debt). This is not the keyboard shortcut list; that is in `SPEC.md` section 6.

Each entry: file, what was skipped, when it needs fixing.

## Open

| # | File | What was skipped | When to fix |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Only the `f32` sample format is supported. Devices with other formats are rejected with an error. WASAPI shared mode is almost always `f32`. | When a device gets rejected. |
| S2 | `crates/fnoob-engine/src/device.rs` (`stream_config`) | The chosen buffer size is used only when the device reports a size range that contains it. In WASAPI shared mode Windows often fixes the size, so the setting may have no effect. | When WASAPI exclusive mode is added (roadmap). |
| S3 | `crates/fnoob-engine/src/device.rs` | Stream errors only keep the `ErrorKind`; the detailed message is lost. No automatic reconnect; the user clicks "Reopen device" in the settings window. | Phase 8, or earlier if it happens often. |
| S6 | `crates/fnoob-app/src/main.rs` | The window icon is still the default `eframe` icon. | Phase 8 (original icon). |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs`, `io.rs` (tests) | Tests use folders in `%TEMP%` named by PID instead of the `tempfile` crate. | If tests start colliding when run in parallel. |
| S9 | `crates/fnoob-app/src/app.rs` (`sync_plan`) | The whole render plan is rebuilt and sent on every edit, including every frame of a drag. | When large projects feel slow (Phase 7 check). |
| S11 | `crates/fnoob-app/src/app.rs` | Count-in length is fixed at the project value (default 1 bar); there is no UI to change it. | Phase 7 (Project Settings dialog). |
| S12 | `crates/fnoob-app/src/app.rs` (`open`) | Open uses a folder picker for the `.fnoob` folder. No recent projects list. | Phase 7. |
| S14 | `crates/fnoob-app/src/timeline.rs` (`update_region_drag`) | Dragging regions to another track only works for a single region; multi-region drags move horizontally only. | When multi-track moves are needed. |
| S15 | `crates/fnoob-app/src/timeline.rs` (`update_region_resize`) | Only the right edge of a region can be dragged. The left edge (trim the start) is not supported. | When trimming starts is needed. |
| S16 | `crates/fnoob-core/src/history.rs` | Undo stores a full copy of the project per step (planned in `SPEC.md` 4.6). | If memory use becomes a problem. |
| S17 | `crates/fnoob-app/src/app.rs` (`run`) | The Bounce command only shows "not available yet". | Phase 7. |
| S18 | `crates/fnoob-app/src/app.rs` (`control_bar`) | The control bar does not wrap; on narrow windows its right side is cut off. | When window layout is polished (Phase 8). |
| S19 | `crates/fnoob-engine/src/processor.rs` (`drain_midi`), `midi_in.rs` | MIDI input is applied at the start of each audio block; the `midir` timestamp is ignored. Timing can be off by up to one buffer (about 10 ms). | Phase 5, together with latency measurement. |
| S20 | `crates/fnoob-engine/src/midi_in.rs` (`parse_message`) | Only note on/off is read. Sustain pedal, pitch bend, and other controllers are ignored. | After the MVP. |
| S22 | `crates/fnoob-engine/src/synth.rs` (`free_voice`) | When all 16 voices are busy, the stolen voice is cut instantly, which can click. | If it is audible in normal use. |
| S23 | `crates/fnoob-engine/src/processor.rs` (`set_plan`) | A voice that is already sounding keeps its old end time when its note is moved or deleted during playback. | If it confuses users. |
| S24 | `crates/fnoob-app/src/piano_roll.rs` | No rubber-band selection and no separate pointer/pencil tools: clicking empty space always adds a note. Notes moved past the region end are kept but do not play. | When the editor gets more tools. |
| S25 | `crates/fnoob-engine/src/processor.rs` (`Recorder`) | While loop recording, a key held across the loop end is cut at the loop end and not restarted at the loop start. | If players need held notes across the loop. |
| S26 | `crates/fnoob-engine/src/processor.rs` (`read_input_frame`, `trim_input_backlog`) | Input and output streams are not clock-synced. Missing input samples become silence and an input backlog over 8192 frames is dropped. Slow or separate devices can leave gaps in recordings. | If recordings with two different devices are needed. |
| S27 | `crates/fnoob-engine/src/processor.rs` (`clip_frame`) | Overlapping audio regions on one track are mixed together instead of the later one covering the earlier one. | If overlapping regions confuse users. |
| S28 | `crates/fnoob-app/src/audio.rs` | Recorded and imported WAV files stay in `audio/` when their regions are deleted or undone. | Phase 7 (clean up unused files on save, with a prompt). |
| S29 | `crates/fnoob-engine/src/audio_data.rs` | Whole audio files are loaded into memory (planned in `SPEC.md` 5.5). Long projects use a lot of RAM. | Roadmap (disk streaming). |
| S30 | `crates/fnoob-app/src/audio.rs` (`finish_audio_take`) | Latency compensation uses the latency reported by the audio driver. It has not been checked with a loopback cable. | When a loopback measurement is possible. |
| S31 | `crates/fnoob-app/src/audio.rs` (`prepare_audio_take`, `import_audio`) | Recording and importing audio need a saved project folder; there is no temporary folder for untitled projects. | Phase 7. |
| S32 | `crates/fnoob-app/src/timeline.rs` | Audio regions have no fades, and their gain can only be changed in `project.json`. | Phase 6 (mixer) or later. |
| S33 | `crates/fnoob-app/src/timeline.rs` (`input_options`) | The input channel list follows the currently open input device; with no input open it shows 2 channels. | When devices with many inputs are tested. |
| S35 | `crates/fnoob-app/src/mixer.rs` | Meters show a decaying peak only: no peak hold, no clip indicator, no dB scale. | Phase 8 polish. |
| S36 | `crates/fnoob-core/src/model.rs` (`Track`) | Track mute and solo stay on `Track`, not inside `strip` as the `SPEC.md` 4.3 example shows. | When the project format is next changed. |
| S37 | `crates/fnoob-engine/src/processor.rs` | Plugin latency is ignored: there is no delay compensation between tracks. | When latency-heavy plugins are used. |
| S38 | `crates/fnoob-platform/src/plugin/instance.rs` | Only the main stereo bus of a plugin is used. Side-chain and extra output buses get silence. Parameter changes and events sent back by plugins are ignored. | After the MVP. |
| S39 | `crates/fnoob-app/src/plugins.rs` | Removing an insert and undoing it reloads the plugin from the state of the last save; tweaks made since then are lost. | Phase 7, with autosave. |
| S40 | `crates/fnoob-platform/src/plugin/editor.rs` | Editor windows cannot be resized by the user and ignore display scaling requests. | Phase 8 polish. |
| S41 | `crates/fnoob-engine/src/processor.rs` (`run_plugin`) | Plugins may allocate inside their own processing; the no-allocation check is switched off around plugin calls. | Cannot be fixed for third-party code. |
| S42 | `crates/fnoob-app/src/plugins.rs` (`start_scan`) | Rescan always scans every plugin again. There is no way to rescan or block a single plugin. Some broken plugins show their own error dialogs during a scan. | When the plugin list grows. |
| S43 | `crates/fnoob-app/src/plugins.rs` (`scanner_path`) | `fnoob-scan.exe` must sit next to the app; during development it is built with `cargo build --workspace`. | Phase 8 (installer). |

## Fixed

| # | Fixed in | Note |
|---|---|---|
| S4 | Phase 3 | Tempo, metronome, and meter now live in the `Project` model. |
| S5 | Phase 3 | Keymap added; the position display shows bar.beat.division.tick. |
| S8 | Phase 4 | The placeholder beep is gone; MIDI regions play their notes through the Basic Synth. |
| S10 | Phase 5 | Record captures audio on armed audio tracks as well as MIDI. |
| S13 | Phase 5 | Playback and recording are blocked when the project and device sample rates differ. |
| S21 | Phase 6 | Tracks go through the mixer (volume, pan, send, Bus 1, Master) before the output safety clamp. |
| S34 | Phase 6 | Insert slots load VST3 effects; instrument tracks can use a VST3 instrument. |
