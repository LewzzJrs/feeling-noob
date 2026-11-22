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

Not started, in rough priority order:

- Built-in effects: EQ, compressor, delay, reverb
- Punch in/out recording
- Take folders and comping for audio
- Tempo and time signature changes (tempo map)
- Automation of plugin parameters, and automation recording
- Crossfades between overlapping regions
- Quantize strength and swing
- Rubber-band selection and tools in the piano roll
- Sidechain inputs and more buses and sends
- Track folders / grouped tracks and mixer groups
- Freeze / render in place
- Inspector panel and sound/patch browser
- On-screen keyboard
- Event list editor
- Region time stretch
- Project versions (alternative mixes in one project)
- MIDI output to hardware

## 4. Plugins and formats

- CLAP hosting via `clack-host`
- DAWproject import/export
- Standard MIDI File import/export

## 5. Platform and release

- Signed installer (code signing)
- Automatic updates
- UI languages other than English
- macOS / Linux (not planned)
