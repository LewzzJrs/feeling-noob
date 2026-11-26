use crate::app::App;
use fnoob_core::edit;
use fnoob_core::midi_file::{self, MidiClip};
use fnoob_core::model::{Id, RegionContent, TrackKind};
use std::path::{Path, PathBuf};

pub const MIDI_EXTENSIONS: [&str; 2] = ["mid", "midi"];

pub fn is_midi_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MIDI_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

impl App {
    fn selected_midi_clip(&self) -> Option<MidiClip> {
        self.selection.regions.iter().find_map(|id| {
            self.project
                .tracks
                .iter()
                .flat_map(|t| &t.regions)
                .find(|r| r.id == *id)
                .and_then(|region| match &region.content {
                    RegionContent::Midi { notes } => Some(MidiClip {
                        name: region.name.clone(),
                        notes: notes.clone(),
                        length_tick: region.loop_ticks.unwrap_or(region.length_tick),
                        tempo_bpm: None,
                    }),
                    RegionContent::Audio { .. } => None,
                })
        })
    }

    pub(crate) fn export_midi_clip(&mut self) {
        let Some(clip) = self.selected_midi_clip() else {
            self.message = Some("Select a MIDI clip to export it.".into());
            return;
        };
        let name = crate::audio::file_stem_for(&clip.name);
        let mut picker = rfd::FileDialog::new()
            .set_title("Export MIDI Clip")
            .add_filter("MIDI file", &MIDI_EXTENSIONS)
            .set_file_name(format!("{name}.mid"));
        if let Some(dir) = self.project_dir.as_ref().and_then(|d| d.parent()) {
            picker = picker.set_directory(dir);
        }
        let Some(path) = picker.save_file() else {
            return;
        };
        let path = if is_midi_file(&path) {
            path
        } else {
            path.with_extension("mid")
        };
        let bytes = midi_file::write(&clip, self.project.tempo_bpm, self.project.time_signature);
        self.message = Some(match std::fs::write(&path, bytes) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Cannot write {}: {e}", path.display()),
        });
    }

    pub(crate) fn import_midi(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import MIDI files")
            .add_filter("MIDI files", &MIDI_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        let tick = self.playhead_tick();
        self.import_midi_files(&paths, self.selection.track, tick);
    }

    pub(crate) fn import_midi_files(&mut self, paths: &[PathBuf], track: Option<Id>, tick: u64) {
        let mut clips = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            match std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|data| midi_file::read(&data))
            {
                Ok(mut clip) => {
                    if clip.name.trim().is_empty() {
                        clip.name = path
                            .file_stem()
                            .map_or_else(|| "MIDI".to_owned(), |s| s.to_string_lossy().into());
                    }
                    clips.push(clip);
                }
                Err(e) => failed.push(format!(
                    "{}: {e}",
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned()
                    )
                )),
            }
        }
        if clips.is_empty() {
            self.message = Some(format!("Cannot import MIDI. {}", failed.join("; ")));
            return;
        }
        let target = track.filter(|id| {
            self.project
                .track(*id)
                .is_some_and(|t| t.kind == TrackKind::Instrument)
        });
        let mut added = Vec::new();
        let mut used = None;
        self.edit(|p| {
            let track = target.unwrap_or_else(|| edit::add_track(p, TrackKind::Instrument));
            used = Some(track);
            let mut at = tick;
            for clip in clips {
                let Some(region) = edit::add_midi_region(p, track, at, clip.length_tick) else {
                    continue;
                };
                if let Some(r) = p
                    .track_mut(track)
                    .and_then(|t| t.regions.iter_mut().find(|r| r.id == region))
                {
                    r.name = clip.name;
                    r.content = RegionContent::Midi { notes: clip.notes };
                }
                added.push(region);
                at += clip.length_tick;
            }
        });
        self.selection.track = used;
        self.selection.regions = added;
        self.message = Some(if failed.is_empty() {
            "MIDI imported.".to_owned()
        } else {
            format!("Some files were skipped. {}", failed.join("; "))
        });
    }
}
