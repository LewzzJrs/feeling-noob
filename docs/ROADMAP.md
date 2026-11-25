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
| Up to 8 return tracks, one send per return, output Main or none | Nested groups, routing to other tracks, returns sending to returns | SPEC F4 |
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

Done in Phase 13: device chain view in the bottom area (`Shift+Tab`) for the selected track or the return track, with the instrument and each effect as a card: on/off, move left/right, replace, remove, inline controls for built-in devices, and drop from the browser.

Done in Phase 14: punch in and punch out (they use the loop area, shown as red lines on the ruler), and Capture MIDI (`Ctrl+Shift+C`), which turns the notes played last into a region, with or without the transport running.

Done in Phase 15: a new layout and way of working. Track headers with name, input and mixer controls sit right of the timeline, with the return track and Main rows below. The clip grid has a mixer section under each column. A detail area at the bottom shows Clip (properties plus piano roll or waveform) or Devices. The browser has categories and places. The status bar explains the control under the mouse. The control bar adds tap tempo, loop start and length, draw mode and an overall CPU meter, and all keyboard shortcuts were replaced (see `SPEC.md` section 6).

Done in Phase 16: several return tracks (new projects start with A Reverb and B Delay and 2 MIDI plus 2 audio tracks), a send per return on every track, monitor In/Auto/Off, output Main or No Output, and a clip grid laid out as tracks, an empty drop area, return columns, and a Main column with the scene buttons, each with an I/O section and a fader strip with knobs.

Done in Phase 17: Main Out and Cue Out output pairs with a cue volume (browser previews play on the cue pair), a Key and Scale choice in the control bar that highlights scale notes in the piano roll, and browser category icons and tag filters for samples.

Done in Phase 18: loop recording for audio. Every loop pass becomes a take lane under the track, the last pass goes into the track, and dragging over a take lane (or clicking a take) copies that part into the track (comping). Take lanes open and close from the track title and can be deleted.

Done in Phase 19: tempo changes. A tempo lane under the ruler adds, edits and deletes tempo changes; regions, notes, the metronome, recording, bounce and plugin transport all follow the tempo map.

Done in Phase 21: time signature changes during the song and tempo ramps. The tempo lane adds time signature changes at bar starts and lets a tempo change ramp from the previous tempo; bar lines, positions, bar and beat snapping, bar jumps and the metronome (beat length and accents) follow them.

Done in Phase 22: shortcuts aligned with the common workstation layout on Windows (see `SPEC.md` section 6), a separate time signature lane, and song tempo points that ramp by default and can be dragged.

Done in Phase 23: time selection across tracks, a flashing insert marker, play from and move the insert marker, MIDI clips from a time selection, zoom to selection and back, fit the song, a minutes:seconds ruler, a song overview strip and locator buttons.

Done in Phase 24: time selection edits (cut, copy, paste, duplicate, delete, and the same over time for all tracks, insert silence), a selection menu, finer fixed grids, consolidate for MIDI, crop, reverse, fades from a selection, group tracks with their own channel, and freeze, unfreeze, freeze and flatten.

Done in Phase 25: clip view panel with position and length Set buttons, MIDI clip loops and Duplicate Loop, piano roll Fit, Fold and Scale views, a grid menu in the editor, and velocity Randomize and Ramp.

Done in Phase 26: device title bars in the track colour with an on/off switch, a drop area after the chain, and default track names that follow the loaded instrument.

Done in Phase 27: tempo step buttons, a count-in menu, MIDI arrangement overdub, automation arm that records volume and pan moves, re-enable automation after a manual override, and the sample rate in the status bar.

Not started, in rough priority order:

- Audio warping and time stretch, clip tempo
- MIDI tools: transpose, humanize, legato, chop, strum, arpeggiate, rhythm and chord generators
- Scale mode: fold the piano roll to scale notes, scale-aware MIDI tools
- Automation of plugin parameters, automation recording, clip envelopes
- Crossfades between overlapping regions, fade handles on region corners
- Quantize strength and swing, groove templates
- Rubber-band selection, draw mode and chance lane in the piano roll
- Nested groups, routing between tracks, sidechain inputs
- Crossfader and cue output
- Consolidating audio clips into one audio file
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
