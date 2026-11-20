# SHORTCUTS

Shortcuts taken in the code (tech debt). This is not the keyboard shortcut list; that is in `SPEC.md` section 6.

Each entry: file, what was skipped, when it needs fixing.

## Open

| # | File | What was skipped | When to fix |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Only the `f32` sample format is supported. Devices with other formats are rejected with an error. WASAPI shared mode is almost always `f32`. | When a device gets rejected. |
| S2 | `crates/fnoob-engine/src/device.rs` | Always uses the device's default config (sample rate, channels, buffer size). Buffer size cannot be chosen yet. | Phase 5, when recording latency is measured. |
| S3 | `crates/fnoob-engine/src/device.rs` | Stream errors only keep the `ErrorKind`; the detailed message is lost. No automatic reconnect; the user clicks "Reopen device" in the settings window. | Phase 8, or earlier if it happens often. |
| S6 | `crates/fnoob-app/src/main.rs` | The window icon is still the default `eframe` icon. | Phase 8 (original icon). |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs`, `io.rs` (tests) | Tests use folders in `%TEMP%` named by PID instead of the `tempfile` crate. | If tests start colliding when run in parallel. |
| S9 | `crates/fnoob-app/src/app.rs` (`sync_plan`) | The whole render plan is rebuilt and sent on every edit, including every frame of a drag. | When large projects feel slow (Phase 7 check). |
| S10 | `crates/fnoob-app/src/midi.rs` (`start_recording`) | Record captures MIDI only. Audio tracks are not recorded yet. | Phase 5. |
| S11 | `crates/fnoob-app/src/app.rs` | Count-in length is fixed at the project value (default 1 bar); there is no UI to change it. | Phase 7 (Project Settings dialog). |
| S12 | `crates/fnoob-app/src/app.rs` (`open`) | Open uses a folder picker for the `.fnoob` folder. No recent projects list. | Phase 7. |
| S13 | `crates/fnoob-app/src/app.rs` (`check_sample_rate`) | A project/device sample rate mismatch only shows a warning; playback is not blocked. Harmless until audio files exist. | Phase 5. |
| S14 | `crates/fnoob-app/src/timeline.rs` (`update_region_drag`) | Dragging regions to another track only works for a single region; multi-region drags move horizontally only. | When multi-track moves are needed. |
| S15 | `crates/fnoob-app/src/timeline.rs` (`update_region_resize`) | Only the right edge of a region can be dragged. The left edge (trim the start) is not supported. | When trimming starts is needed. |
| S16 | `crates/fnoob-core/src/history.rs` | Undo stores a full copy of the project per step (planned in `SPEC.md` 4.6). | If memory use becomes a problem. |
| S17 | `crates/fnoob-app/src/app.rs` (`run`) | Mixer and Bounce commands only show "not available yet". | Phases 6 and 7. |
| S18 | `crates/fnoob-app/src/app.rs` (`control_bar`) | The control bar does not wrap; on narrow windows its right side is cut off. | When window layout is polished (Phase 8). |
| S19 | `crates/fnoob-engine/src/processor.rs` (`drain_midi`), `midi_in.rs` | MIDI input is applied at the start of each audio block; the `midir` timestamp is ignored. Timing can be off by up to one buffer (about 10 ms). | Phase 5, together with latency measurement. |
| S20 | `crates/fnoob-engine/src/midi_in.rs` (`parse_message`) | Only note on/off is read. Sustain pedal, pitch bend, and other controllers are ignored. | After the MVP. |
| S21 | `crates/fnoob-engine/src/processor.rs` (`process`) | Tracks are summed without a mixer and clamped to ±1.0. Many loud notes can clip. | Phase 6 (mixer). |
| S22 | `crates/fnoob-engine/src/synth.rs` (`free_voice`) | When all 16 voices are busy, the stolen voice is cut instantly, which can click. | If it is audible in normal use. |
| S23 | `crates/fnoob-engine/src/processor.rs` (`set_plan`) | A voice that is already sounding keeps its old end time when its note is moved or deleted during playback. | If it confuses users. |
| S24 | `crates/fnoob-app/src/piano_roll.rs` | No rubber-band selection and no separate pointer/pencil tools: clicking empty space always adds a note. Notes moved past the region end are kept but do not play. | When the editor gets more tools. |
| S25 | `crates/fnoob-engine/src/processor.rs` (`Recorder`) | While loop recording, a key held across the loop end is cut at the loop end and not restarted at the loop start. | If players need held notes across the loop. |

## Fixed

| # | Fixed in | Note |
|---|---|---|
| S4 | Phase 3 | Tempo, metronome, and meter now live in the `Project` model. |
| S5 | Phase 3 | Keymap added; the position display shows bar.beat.division.tick. |
| S8 | Phase 4 | The placeholder beep is gone; MIDI regions play their notes through the Basic Synth. |
