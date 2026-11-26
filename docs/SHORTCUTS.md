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
| S51 | `crates/fnoob-engine/src/plan.rs` (`StripPlan::at`) | Automation only covers track volume and pan and has no curve shapes. Automation Arm writes a point for every frame of a fader move, and a manual move overrides the whole track's automation, not just the moved control. | With plugin automation. |
| S52 | `crates/fnoob-app/src/musical.rs` | Musical typing uses a fixed US key layout and takes over those letter keys while it is on. | If other keyboard layouts are needed. |
| S53 | `crates/fnoob-engine/src/processor.rs` (`session_command`), `crates/fnoob-app/src/session.rs` (`toggle_session_record`) | Session Record takes MIDI only, from one armed MIDI track, and the new clip starts from its beginning at the next launch step rather than carrying on in time. Audio cannot be recorded into grid slots, and clip playback is not recorded back to the timeline. | With punch and capture recording. |
| S54 | `crates/fnoob-app/src/session.rs` | Clips launch only with the mouse. There is no keyboard or MIDI mapping for slots or scenes. | With key/MIDI mapping. |
| S55 | `crates/fnoob-engine/src/processor.rs` (`tick_playing`) | When the timeline loops or the playhead jumps, notes still held by playing clips are released too, so a long clip note can be cut once. | If it is audible in normal use. |
| S56 | `crates/fnoob-app/src/piano_roll.rs` | Grid clips open in the piano roll at bar 1, and the piano roll playhead follows the timeline, not the clip. | When the clip editor gets its own view. |
| S57 | `crates/fnoob-engine/src/session.rs` (`launch`) | Scenes cannot store their own tempo or time signature. | If users need those launch options. |
| S58 | `crates/fnoob-app/src/bounce.rs` | Bounce renders the timeline only; playing grid clips are not included. | With recording clips to the timeline. |
| S59 | `crates/fnoob-app/src/chain.rs` | The device chain holds at most 4 effects per track (`MAX_INSERTS`). Devices move with buttons, not by dragging. Compressor and limiter show no gain-reduction meter. There are no device presets or racks. | When chains need more devices or presets. |
| S60 | `crates/fnoob-app/src/midi.rs` (`capture_midi`) | Capture MIDI keeps the last 8192 played events and splits phrases at a fixed 4-second pause. Notes played while stopped keep the project tempo; the tempo is not guessed from the playing. Punch uses the loop area only, with no separate punch range and no pre-roll. | If players need these options. |
| S61 | `crates/fnoob-app/src/ui_kit.rs` (`hint`) | The status bar explains only controls that call `hint`; menus and some editor controls show nothing there. | When more controls get descriptions. |
| S62 | `crates/fnoob-app/src/timeline.rs` (`draw_headers`) | Track headers have a fixed width and height; they cannot be resized, only group tracks fold, and the output can only be Main or No Output. Return tracks cannot send to other returns. MIDI tracks have no monitor switch; live MIDI always plays on the armed or selected track. | With track groups and routing. |
| S63 | `crates/fnoob-engine/src/processor.rs` (`write_outputs`) | Cue Out carries only browser previews; the metronome and solo cueing still go to Main. Scale mode only highlights rows; the piano roll cannot fold to scale notes. Sample tags match words in file names, not real metadata. | With cue solo and MIDI tools. |
| S64 | `crates/fnoob-core/src/takes.rs`, `crates/fnoob-app/src/timeline.rs` (`draw_take_lanes`) | Take lanes are made only by loop recording of audio; MIDI loop recording still merges into one region. Take lanes show no waveform, and their regions cannot be moved or trimmed. Comping always snaps to the grid. | When take editing grows. |
| S65 | `crates/fnoob-core/src/time.rs` (`Timeline`, `MeterMap`) | At most 64 tempo changes and 64 time signature changes per song; ramps are linear in musical time. Clip launch quantize, count-in, Capture MIDI while stopped, piano roll grid lines and region default lengths use the tempo and time signature of bar 1. Audio is not stretched when the tempo changes. | With warping and meter changes. |
| S66 | `crates/fnoob-engine/src/decode.rs` | Imported files are decoded whole on the UI thread, so long MP3s pause the window for a moment. Files with more than two channels keep only their first two when played. Decoding was tested with generated WAV files only, not with real MP3, FLAC or OGG files. | If imports feel slow, or a format fails. |
| S68 | `crates/fnoob-app/src/timeline.rs` (`draw_overview`, `draw_insert_marker`), `crates/fnoob-app/src/arrange.rs` | The overview strip cannot be dragged to zoom, only to scroll. Zoom history keeps the last 32 views. The minutes:seconds ruler steps with the tempo at the left edge of the view. While playing, a click in a lane still moves the playhead as well as the insert marker. The time selection is not used yet by cut, copy, loop or consolidate. | With the time commands. |
| S69 | `crates/fnoob-core/src/range.rs`, `crates/fnoob-app/src/timeline.rs` (`selection_menu`) | Delete time and insert time move clips, take lanes, markers, tempo points and automation, but not time signature changes. Paste overwrites the target range and skips lanes whose track kind does not match. The selection menu has no info text, groove, MIDI export or freeze items. | With the matching features. |
| S70 | `crates/fnoob-core/src/range.rs` (`consolidate_midi`), `crates/fnoob-core/src/edit.rs` (`group_tracks`), `crates/fnoob-app/src/bounce.rs` (`freeze_track`) | Consolidate joins MIDI clips only; audio clips are left as they are. Groups cannot be nested, at most 16 groups reach the engine, group lanes only show their members' clips, and group columns in the clip grid have no clips to launch. A frozen track can still be edited, but the edits are not heard until it is frozen again. Freezing and reversing audio need a saved project; the frozen file starts at bar 1 and ends 2 seconds after the last clip. | When audio consolidate and group clips are added. |
| S71 | `crates/fnoob-core/src/model.rs` (`Region::loop_ticks`), `crates/fnoob-app/src/piano_roll.rs` (`editor_tools`) | Only MIDI clips loop, and the loop always starts at the clip start. Splitting, trimming or cropping a looped clip keeps the loop length but not its phase. Clips have no own colour, no clip envelopes or MPE tab, no groove and no velocity deviation. Randomize uses a new seed each click, so it cannot be repeated exactly. | With clip envelopes and audio clip loops. |
| S72 | `crates/fnoob-app/src/controlbar.rs` | The tempo buttons step the tempo instead of nudging playback for a moment. There is no session record button (recording into grid slots is S53). The control bar fits one row from about 1900 pixels wide and wraps on smaller windows. | When the bar gets icons. |
| S73 | `crates/fnoob-app/src/app.rs` (`menu_bar`) | There is no Help menu. Navigate switches views instead of moving keyboard focus between areas. The track menu has no info text, link tracks or assign track colour to clips, because clips have no colour of their own. | When help pages and clip colours exist. |
| S74 | `crates/fnoob-app/src/chain.rs` (`device_command`) | The instrument cannot be duplicated (a track holds one). New MIDI tracks still start with Basic Synth rather than empty. Devices cannot be dragged to reorder or between tracks. | With device drag and drop. |
| S67 | `crates/fnoob-core/src/keymap.rs`, `crates/fnoob-app/src/timeline.rs` | Some reference shortcuts have no command yet: triplet grid, adaptive grid, bounce to new track, optimize height (H), MIDI export, key/MIDI map. Song tempo points live in the ruler, not in a Main track automation lane, and a dragged point moves only when the drag ends. | With the matching features. |

## Fixed

| # | Fixed in | Note |
|---|---|---|
| S18 | Phase 15 | The control bar wraps onto a second row when the window is narrow. |
| S4 | Phase 3 | Tempo, metronome, and meter now live in the `Project` model. |
| S5 | Phase 3 | Keymap added; the position display shows bar.beat.division.tick. |
| S8 | Phase 4 | The placeholder beep is gone; MIDI regions play their notes through the Basic Synth. |
| S10 | Phase 5 | Record captures audio on armed audio tracks as well as MIDI. |
| S13 | Phase 5 | Playback and recording are blocked when the project and device sample rates differ. |
| S21 | Phase 6 | Tracks go through the mixer (volume, pan, send, return track, Master) before the output safety clamp. |
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
