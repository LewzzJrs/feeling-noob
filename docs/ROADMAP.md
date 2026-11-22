# ROADMAP — feeling-noob

Features that are **not** in the MVP. Order within each group is a rough priority, not a promise.
"SPEC x" refers to a section of `SPEC.md`.

## 1. Out of scope for the MVP

- Clip launcher / loop grid
- Automatic drummer and other generated performers
- Tempo detection from recordings
- Audio time and pitch editing
- Step sequencer
- Score / notation editor
- Surround and spatial audio
- Built-in plugins beyond the basics (the MVP has only Basic Synth and the metronome, SPEC F10)

## 2. MVP simplifications to replace later

| In the MVP | Later | Reference |
|---|---|---|
| One tempo and one time signature per project | Tempo map and meter changes | SPEC 4.1 |
| Destructive quantize on selected notes | Non-destructive per-region quantize (strength, swing) | SPEC F3 |
| Audio recording ignores the loop | Take folders and comping | SPEC F6 |
| One new region per MIDI take without loop | Selectable merge/replace modes | SPEC F6 |
| WAV only, sample rate must match the project | Other formats (FLAC, MP3, OGG) + resampling | SPEC 5.5 |
| Whole audio files loaded into memory | Disk streaming | SPEC 5.5 |
| Single-threaded graph | Parallel track processing | SPEC 5.3 |
| Snapshot undo of the whole project | Change-based undo (less memory) | SPEC 4.6 |
| One bus (Bus 1), one send per track | Many buses/aux channels, many sends | SPEC F4 |
| At most 4 inserts per strip | Unlimited slots | SPEC F4 |
| VST3 processed in-process | Out-of-process plugin processing (full crash isolation) | SPEC 5.6 |
| No plugin delay compensation (PDC) | Automatic PDC | SPEC 5.3 |
| Bounce to stereo WAV only, no dither/normalize | Dither, normalize, mono, other formats, per-track export | SPEC F8 |
| Fixed shortcuts | User-editable shortcuts | SPEC 6 |
| Incoming MIDI timestamps rounded to the audio block | Precise timestamps from `midir` | SPEC 5.1 |
| WASAPI shared mode | WASAPI exclusive mode (needs custom WASAPI code; `cpal` does not support it), ASIO (SDK downloaded manually and pinned) | SPEC 5.4 |

## 3. Editing and mixing features

Done after the MVP (Phase 9): volume and pan automation, region gain and fades, trimming region starts, markers, computer-keyboard note input, duplicating and reordering tracks.

Done in Phase 10: browser panel (instruments, effects, sample folders with preview, drag onto tracks), inspector panel (region and track settings, markers), toolbar with visible buttons for every editing command, track context menu (rename, colour, duplicate, move, delete), follow-playhead switch.

Done in Phase 11: built-in effects (EQ, compressor, delay with tempo sync and ping-pong, reverb, limiter, utility) with a device window for each, and two sample instruments (Sampler, Drum Sampler with 16 pads from C1).

Known limits of Phase 11: samples must already be at the project sample rate (no resampling yet); the sampler has no loop points, start/end trim or filter; drum pads have no choke groups.

Done in Phase 12: clip-launch grid view (`Tab`) with scenes, launch quantize, launch modes (trigger, gate, toggle, repeat), follow actions with chance, per-track and global stop, back to timeline, moving and duplicating clips, copying between the grid and the timeline, and dropping samples onto slots.

Done in Phase 13: device chain view in the bottom area (`Shift+Tab`) for the selected track or Bus 1, with the instrument and each effect as a card: on/off, move left/right, replace, remove, inline controls for built-in devices, and drop from the browser.

Done in Phase 14: punch in and punch out (they use the loop area, shown as red lines on the ruler), and Capture MIDI (`Ctrl+Shift+C`), which turns the notes played last into a region, with or without the transport running.

Done in Phase 15: a new layout and way of working. Track headers with name, input and mixer controls sit right of the timeline, with Bus 1 and Main rows below. The clip grid has a mixer section under each column. A detail area at the bottom shows Clip (properties plus piano roll or waveform) or Devices. The browser has categories and places. The status bar explains the control under the mouse. The control bar adds tap tempo, loop start and length, draw mode and an overall CPU meter, and all keyboard shortcuts were replaced (see `SPEC.md` section 6).

Not started, in rough priority order:

- Take lanes and comping for audio
- Tempo and time signature changes (tempo map), tap tempo
- Audio warping and time stretch, clip tempo
- MIDI tools: transpose, humanize, legato, chop, strum, arpeggiate, rhythm and chord generators
- Scale mode (root and scale, fold and highlight in the piano roll)
- Automation of plugin parameters, automation recording, clip envelopes
- Crossfades between overlapping regions, fade handles on region corners
- Quantize strength and swing, groove templates
- Rubber-band selection, draw mode and chance lane in the piano roll
- Return tracks, more sends, groups and nested groups, sidechain inputs
- Crossfader and cue output
- Freeze / bounce in place, consolidate
- CPU meter per track
- Event list editor, project versions, MIDI output to hardware

## 4. Plugins and formats

- CLAP hosting via `clack-host`
- DAWproject import/export
- Standard MIDI File import/export

## 5. Platform and release

- Signed installer (code signing)
- Automatic updates
- UI languages other than English
- macOS / Linux (not planned)
