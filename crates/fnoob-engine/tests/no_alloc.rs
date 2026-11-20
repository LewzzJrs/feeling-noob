use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_core::edit::{add_midi_region, add_note, add_track};
use fnoob_core::model::{Note, Project, TrackKind};
use fnoob_engine::processor::{MidiEvent, midi_channel};
use fnoob_engine::{Command, RenderPlan, create};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn process_does_not_allocate() {
    let mut project = Project::new("t", 48_000);
    let track = add_track(&mut project, TrackKind::Instrument);
    for bar in 0..8 {
        let region = add_midi_region(&mut project, track, bar * 3840, 3840).unwrap();
        for step in 0..20 {
            add_note(
                &mut project,
                region,
                Note {
                    pitch: 40 + step as u8,
                    velocity: 100,
                    start_tick: step * 120,
                    length_tick: 600,
                },
            );
        }
    }
    project.loop_range.enabled = true;
    project.metronome.count_in_enabled = true;
    let (mut processor, mut handle) = create(
        48_000,
        2,
        Box::new(RenderPlan::from_project(&project, 48_000)),
    );
    let (mut midi_tx, midi_rx) = midi_channel();
    let mut out = vec![0.0f32; 1024];
    project.tempo_bpm = 97.0;
    let commands = [
        Command::SetMidiInput(midi_rx),
        Command::SetLiveTrack(Some(track)),
        Command::SetPlan(Box::new(RenderPlan::from_project(&project, 48_000))),
        Command::Preview {
            track,
            pitch: 70,
            velocity: 90,
        },
        Command::Record,
        Command::Locate(1234),
        Command::SetPlan(Box::new(RenderPlan::from_project(&project, 48_000))),
    ];
    for command in commands {
        handle.send(command).unwrap();
    }
    assert_no_alloc(|| {
        for i in 0..2000u32 {
            if i % 7 == 0 {
                let _ = midi_tx.push(MidiEvent {
                    pitch: (i % 128) as u8,
                    velocity: if i % 14 == 0 { 0 } else { 100 },
                });
            }
            processor.process(&mut out);
        }
    });
    handle.send(Command::Stop).unwrap();
    assert_no_alloc(|| processor.process(&mut out));
    assert!(!handle.is_running());
    assert!(handle.pop_recorded().is_some());
    handle.collect_garbage();
}
