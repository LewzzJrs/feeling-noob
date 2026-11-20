# SHORTCUTS

Shortcuts taken in the code (tech debt). This is not the keyboard shortcut list; that is in `SPEC.md` section 6.

Each entry: file, what was skipped, when it needs fixing.

## Open

| # | File | What was skipped | When to fix |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Only the `f32` sample format is supported. Devices with other formats are rejected with an error. WASAPI shared mode is almost always `f32`. | When a device gets rejected. |
| S2 | `crates/fnoob-engine/src/device.rs` | Always uses the device's default config (sample rate, channels, buffer size). Buffer size cannot be chosen yet. | Phase 5, when recording latency is measured. |
| S3 | `crates/fnoob-engine/src/device.rs` | Stream errors only keep the `ErrorKind`; the detailed message is lost. No automatic reconnect; the user clicks "Reopen device" in Audio Settings. | Phase 8, or earlier if it happens often. |
| S6 | `crates/fnoob-app/src/main.rs` | The window icon is still the default `eframe` icon. | Phase 8 (original icon). |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs`, `io.rs` (tests) | Tests use folders in `%TEMP%` named by PID instead of the `tempfile` crate. | If tests start colliding when run in parallel. |
| S8 | `crates/fnoob-engine/src/plan.rs` | MIDI regions play a short beep at their start instead of their notes. | Phase 4 (Basic Synth). |
| S9 | `crates/fnoob-app/src/app.rs` (`sync_plan`) | The whole render plan is rebuilt and sent on every edit, including every frame of a drag. | When large projects feel slow (Phase 7 check). |
| S10 | `crates/fnoob-app/src/app.rs` (`Command::Record`) | Record only runs the transport (with count-in); nothing is captured. | Phase 4 (MIDI) and Phase 5 (audio). |
| S11 | `crates/fnoob-app/src/app.rs` | Count-in length is fixed at the project value (default 1 bar); there is no UI to change it. | Phase 7 (Project Settings dialog). |
| S12 | `crates/fnoob-app/src/app.rs` (`open`) | Open uses a folder picker for the `.fnoob` folder. No recent projects list. | Phase 7. |
| S13 | `crates/fnoob-app/src/app.rs` (`check_sample_rate`) | A project/device sample rate mismatch only shows a warning; playback is not blocked. Harmless until audio files exist. | Phase 5. |
| S14 | `crates/fnoob-app/src/timeline.rs` (`update_region_drag`) | Dragging regions to another track only works for a single region; multi-region drags move horizontally only. | When multi-track moves are needed. |
| S15 | `crates/fnoob-app/src/timeline.rs` | Regions cannot be resized by dragging their edges. | Phase 4, together with the Piano Roll. |
| S16 | `crates/fnoob-core/src/history.rs` | Undo stores a full copy of the project per step (planned in `SPEC.md` 4.6). | If memory use becomes a problem. |
| S17 | `crates/fnoob-app/src/app.rs` (`run`) | Mixer, Piano Roll, Quantize, and Bounce commands only show "not available yet". | Phases 4, 6, 7. |
| S18 | `crates/fnoob-app/src/app.rs` (`control_bar`) | The control bar does not wrap; on narrow windows its right side is cut off. | When window layout is polished (Phase 8). |

## Fixed

| # | Fixed in | Note |
|---|---|---|
| S4 | Phase 3 | Tempo, metronome, and meter now live in the `Project` model. |
| S5 | Phase 3 | Keymap added; the position display shows bar.beat.division.tick. |
