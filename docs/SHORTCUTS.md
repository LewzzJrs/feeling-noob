# SHORTCUTS

Shortcuts taken in the code (tech debt). This is not the keyboard shortcut list; that is in `SPEC.md` section 6.

Each entry: file, what was skipped, when it needs fixing.

## Open

| # | File | What was skipped | When to fix |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Only the `f32` sample format is supported. Devices with other formats are rejected with an error. WASAPI shared mode is almost always `f32`. | When a device gets rejected. |
| S2 | `crates/fnoob-engine/src/device.rs` (`stream_config`) | The chosen buffer size is used only when the device reports a size range that contains it. In WASAPI shared mode Windows often fixes the size, so the setting may have no effect. | When WASAPI exclusive mode is added (roadmap). |
| S3 | `crates/fnoob-engine/src/device.rs` | Stream errors only keep the `ErrorKind`; the detailed message is lost. No automatic reconnect; the user clicks "Reopen device" in the settings window. | Phase 8, or earlier if it happens often. |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs`, `io.rs` (tests) | Tests use folders in `%TEMP%` named by PID instead of the `tempfile` crate. | If tests start colliding when run in parallel. |
| S9 | `crates/fnoob-app/src/app.rs` (`sync_plan`) | The whole render plan is rebuilt and sent on every edit, including every frame of a drag. | When large projects feel slow (Phase 7 check). |
| S12 | `crates/fnoob-app/src/app.rs` (`open`) | Open uses a folder picker for the `.fnoob` folder instead of picking `project.json`. | Phase 8 polish. |
| S14 | `crates/fnoob-app/src/timeline.rs` (`update_region_drag`) | Dragging regions to another track only works for a single region; multi-region drags move horizontally only. | When multi-track moves are needed. |
| S16 | `crates/fnoob-core/src/history.rs` | Undo stores a full copy of the project per step (planned in `SPEC.md` 4.6). | If memory use becomes a problem. |
| S18 | `crates/fnoob-app/src/app.rs` (`control_bar`) | The control bar does not wrap; on narrow windows its right side is cut off. | When window layout is polished (Phase 8). |
| S19 | `crates/fnoob-engine/src/processor.rs` (`drain_midi`), `midi_in.rs` | MIDI input is applied at the start of each audio block; the `midir` timestamp is ignored. Timing can be off by up to one buffer (about 10 ms). | Phase 5, together with latency measurement. |
| S20 | `crates/fnoob-engine/src/midi_in.rs` (`parse_message`) | Only note on/off is read. Sustain pedal, pitch bend, and other controllers are ignored. | After the MVP. |
| S22 | `crates/fnoob-engine/src/synth.rs` (`free_voice`) | When all 16 voices are busy, the stolen voice is cut instantly, which can click. | If it is audible in normal use. |
| S23 | `crates/fnoob-engine/src/processor.rs` (`set_plan`) | A voice that is already sounding keeps its old end time when its note is moved or deleted during playback. | If it confuses users. |
| S24 | `crates/fnoob-app/src/piano_roll.rs` | No rubber-band selection and no separate pointer/pencil tools: clicking empty space always adds a note. Notes moved past the region end are kept but do not play. | When the editor gets more tools. |
| S25 | `crates/fnoob-engine/src/processor.rs` (`Recorder`) | While loop recording, a key held across the loop end is cut at the loop end and not restarted at the loop start. | If players need held notes across the loop. |
| S26 | `crates/fnoob-engine/src/processor.rs` (`read_input_frame`, `trim_input_backlog`) | Input and output streams are not clock-synced. Missing input samples become silence and an input backlog over 8192 frames is dropped. Slow or separate devices can leave gaps in recordings. | If recordings with two different devices are needed. |
| S27 | `crates/fnoob-engine/src/processor.rs` (`clip_frame`) | Overlapping audio regions on one track are mixed together instead of the later one covering the earlier one. | If overlapping regions confuse users. |
| S29 | `crates/fnoob-engine/src/audio_data.rs` | Whole audio files are loaded into memory (planned in `SPEC.md` 5.5). Long projects use a lot of RAM. | Roadmap (disk streaming). |
| S30 | `crates/fnoob-app/src/audio.rs` (`finish_audio_take`) | Latency compensation uses the latency reported by the audio driver. It has not been checked with a loopback cable. | When a loopback measurement is possible. |
| S33 | `crates/fnoob-app/src/timeline.rs` (`input_options`) | The input channel list follows the currently open input device; with no input open it shows 2 channels. | When devices with many inputs are tested. |
| S35 | `crates/fnoob-app/src/mixer.rs` | Meters show a decaying peak only: no peak hold, no clip indicator, no dB scale. | Phase 8 polish. |
| S36 | `crates/fnoob-core/src/model.rs` (`Track`) | Track mute and solo stay on `Track`, not inside `strip` as the `SPEC.md` 4.3 example shows. | When the project format is next changed. |
| S37 | `crates/fnoob-engine/src/processor.rs` | Plugin latency is ignored: there is no delay compensation between tracks. | When latency-heavy plugins are used. |
| S38 | `crates/fnoob-platform/src/plugin/instance.rs` | Only the main stereo bus of a plugin is used. Side-chain and extra output buses get silence. Parameter changes and events sent back by plugins are ignored. | After the MVP. |
| S40 | `crates/fnoob-platform/src/plugin/editor.rs` | Editor windows cannot be resized by the user and ignore display scaling requests. | Phase 8 polish. |
| S41 | `crates/fnoob-engine/src/processor.rs` (`run_plugin`) | Plugins may allocate inside their own processing; the no-allocation check is switched off around plugin calls. | Cannot be fixed for third-party code. |
| S42 | `crates/fnoob-app/src/plugins.rs` (`start_scan`) | Rescan always scans every plugin again. There is no way to rescan or block a single plugin. Error dialogs from broken plugins are moved to a hidden desktop on the scanner's main thread only; a dialog from another thread could still show. | When the plugin list grows. |
| S44 | `crates/fnoob-app/src/bounce.rs` | Bounce uses fresh plugin copies in real-time mode, not the VST3 offline mode. The whole-song tail is a fixed 2 seconds, so longer reverb tails are cut. | If plugins sound different in bounces. |
| S45 | `crates/fnoob-app/src/files.rs` (`tick_autosave`) | Autosave writes on the UI thread every 60 seconds; the interval cannot be changed. Untitled projects share one recovery slot in `%APPDATA%`. | If autosave stutters large projects. |
| S46 | `crates/fnoob-engine/src/record.rs` | A WAV file being recorded only gets its final header when recording stops; if the app crashes mid-take, that file cannot be opened. | When crash-safe recording is needed. |
| S47 | `crates/fnoob-platform/src/plugin/com.rs` (`ComponentHandler`) | Parameter edits sent by a plugin go through a mutex; a plugin that sends them from the audio thread would lock there. | If a plugin is seen doing this. |
| S48 | `installer/` | The installer is a ZIP with PowerShell scripts, not a signed setup program. Windows SmartScreen may warn on first start because the exe is not code-signed. | When a code-signing certificate is available. |
| S49 | `Cargo.lock` | No automated dependency vulnerability check (`cargo audit`) has been run; the tool is not installed. | Before the first public release. |
| S50 | `crates/fnoob-app/src/timeline.rs` (`region_menu`) | Region gain and fade lengths are picked from fixed steps in the right-click menu; there are no drag handles. | When finer control is needed. |
| S51 | `crates/fnoob-engine/src/plan.rs` (`StripPlan::at`) | Automation only covers track volume and pan, is drawn by hand (not recorded from fader moves), and has no curve shapes. | With plugin automation. |
| S52 | `crates/fnoob-app/src/musical.rs` | Musical typing uses a fixed US key layout and takes over those letter keys while it is on. | If other keyboard layouts are needed. |
| S53 | `crates/fnoob-engine/src/processor.rs` (`session_command`) | Clips cannot be recorded into grid slots, and clip playback is not recorded back to the timeline. | With punch and capture recording. |
| S54 | `crates/fnoob-app/src/session.rs` | Clips launch only with the mouse. There is no keyboard or MIDI mapping for slots or scenes. | With key/MIDI mapping. |
| S55 | `crates/fnoob-engine/src/processor.rs` (`tick_playing`) | When the timeline loops or the playhead jumps, notes still held by playing clips are released too, so a long clip note can be cut once. | If it is audible in normal use. |
| S56 | `crates/fnoob-app/src/piano_roll.rs` | Grid clips open in the piano roll at bar 1, and the piano roll playhead follows the timeline, not the clip. | When the clip editor gets its own view. |
| S57 | `crates/fnoob-engine/src/session.rs` (`launch`) | A scene with an empty slot always stops that track; slots cannot hide their stop button. Clips have no legato option and no per-clip launch quantize. | If users need those launch options. |
| S58 | `crates/fnoob-app/src/bounce.rs` | Bounce renders the timeline only; playing grid clips are not included. | With recording clips to the timeline. |
| S59 | `crates/fnoob-app/src/chain.rs` | The device chain holds at most 4 effects per track (`MAX_INSERTS`). Devices move with buttons, not by dragging. Compressor and limiter show no gain-reduction meter. There are no device presets or racks. | When chains need more devices or presets. |

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
| S11 | Phase 7 | Count-in length is set in File > Project Settings. |
| S17 | Phase 7 | Bounce to WAV works (Ctrl+B). |
| S28 | Phase 7 | File > Move Unused Audio Files moves files no snapshot uses into `audio/unused/`. |
| S31 | Phase 7 | Recording or importing audio in an untitled project asks for Save As first. |
| S39 | Phase 7 | Plugin states are written into the project before an insert or instrument changes, so undo restores them. |
| S6 | Phase 8 | Original app icon in the window and the exe, plus version info. |
| S43 | Phase 8 | The release package puts `fnoob-scan.exe` next to the app. |
| S15 | Phase 9 | Both region edges can be dragged; the left edge trims the start. |
| S32 | Phase 9 | Audio regions have fades and a gain setting in the right-click menu. |
