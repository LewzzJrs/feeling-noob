use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_core::edit::{add_audio_file, add_audio_region, add_midi_region, add_note, add_track};
use fnoob_core::model::{Note, Project, TrackKind};
use fnoob_engine::audio_data::{AudioData, AudioPool};
use fnoob_engine::processor::{MidiEvent, audio_channel, midi_channel};
use fnoob_engine::{Command, RenderPlan, create};
use std::sync::Arc;

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
    let audio = add_track(&mut project, TrackKind::Audio);
    let file = add_audio_file(&mut project, "audio/a.wav".into(), 2, 48_000);
    for bar in 0..4 {
        add_audio_region(&mut project, audio, bar * 1920, file, bar * 100);
    }
    project.tracks[1].input_monitoring = true;
    project.tracks[1].record_enabled = true;
    let mut pool = AudioPool::new();
    pool.insert(
        file,
        Arc::new(AudioData::from_samples(2, vec![0.1; 96_000])),
    );
    project.loop_range.enabled = true;
    project.metronome.count_in_enabled = true;
    let (mut processor, mut handle) = create(
        48_000,
        2,
        Box::new(RenderPlan::with_audio(&project, 48_000, &pool)),
    );
    let (mut input_tx, input_rx) = audio_channel(1 << 16);
    let (sink_tx, mut sink_rx) = audio_channel(1 << 20);
    let (mut midi_tx, midi_rx) = midi_channel();
    let mut out = vec![0.0f32; 1024];
    project.tempo_bpm = 97.0;
    let commands = [
        Command::SetMidiInput(midi_rx),
        Command::SetAudioInput {
            samples: input_rx,
            channels: 2,
        },
        Command::SetRecordSink(sink_tx),
        Command::SetLiveTrack(Some(track)),
        Command::SetPlan(Box::new(RenderPlan::with_audio(&project, 48_000, &pool))),
        Command::Preview {
            track,
            pitch: 70,
            velocity: 90,
        },
        Command::Record,
        Command::Locate(1234),
        Command::SetPlan(Box::new(RenderPlan::with_audio(&project, 48_000, &pool))),
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
            for _ in 0..1024 {
                let _ = input_tx.push(0.2);
            }
            processor.process(&mut out);
            while sink_rx.pop().is_ok() {}
        }
    });
    handle.send(Command::Stop).unwrap();
    assert_no_alloc(|| processor.process(&mut out));
    assert!(!handle.is_running());
    assert!(handle.pop_recorded().is_some());
    handle.collect_garbage();
}
