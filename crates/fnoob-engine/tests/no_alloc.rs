use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_core::edit::{add_audio_file, add_audio_region, add_midi_region, add_note, add_track};
use fnoob_core::model::{Instrument, Note, Project, TrackKind};
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
    let mut drums = fnoob_core::model::DrumParams::default();
    for pad in &mut drums.pads {
        pad.file = Some(file);
    }
    for instrument in [
        Instrument::Sampler(fnoob_core::model::SamplerParams {
            file: Some(file),
            ..Default::default()
        }),
        Instrument::DrumSampler(Box::new(drums)),
    ] {
        let sampled = add_track(&mut project, TrackKind::Instrument);
        let region = add_midi_region(&mut project, sampled, 0, 3840 * 4).unwrap();
        for step in 0..64 {
            add_note(
                &mut project,
                region,
                Note {
                    pitch: 30 + (step % 40) as u8,
                    velocity: 100,
                    start_tick: step * 240,
                    length_tick: 200,
                },
            );
        }
        project.track_mut(sampled).unwrap().instrument = Some(instrument);
    }
    let mut builtin_slots = Vec::new();
    for effect in fnoob_core::effects::BuiltinEffect::all() {
        let id = project.alloc_id();
        builtin_slots.push((id, effect));
    }
    for (id, effect) in &builtin_slots[..4] {
        project.tracks[1]
            .strip
            .inserts
            .push(fnoob_core::model::PluginSlot::builtin(*id, *effect));
    }
    for (id, effect) in &builtin_slots[4..] {
        project
            .bus
            .inserts
            .push(fnoob_core::model::PluginSlot::builtin(*id, *effect));
    }
    let mut map = fnoob_engine::plan::PluginMap::new();
    for (index, (id, _)) in builtin_slots.iter().enumerate() {
        map.insert(*id, index);
    }
    for scene in 0..3 {
        let clip = fnoob_core::session::add_midi_clip(&mut project, track, scene, 1920).unwrap();
        for step in 0..8 {
            add_note(
                &mut project,
                clip,
                Note {
                    pitch: 50 + step as u8,
                    velocity: 90,
                    start_tick: step * 240,
                    length_tick: 300,
                },
            );
        }
        fnoob_core::session::add_audio_clip(&mut project, audio, scene, file).unwrap();
        let settings = fnoob_core::session::clip_mut(&mut project, track, scene).unwrap();
        settings.follow.after_tick = 960;
        settings.follow.a = fnoob_core::session::FollowAction::Any;
        settings.follow.b = fnoob_core::session::FollowAction::Next;
        settings.follow.chance_a = 0.5;
        settings.launch = fnoob_core::session::LaunchMode::Repeat;
    }
    project.launch_quantize = fnoob_core::session::LaunchQuantize::Sixteenth;
    project.tracks[1].record_enabled = true;
    let mut pool = AudioPool::new();
    pool.insert(
        file,
        Arc::new(AudioData::from_samples(2, vec![0.1; 96_000])),
    );
    project.loop_range.enabled = true;
    project.punch.punch_in = true;
    project.punch.punch_out = true;
    project.metronome.count_in_enabled = true;
    let (mut processor, mut handle) = create(
        48_000,
        2,
        Box::new(RenderPlan::build(&project, 48_000, &pool, &map)),
    );
    for (index, (_, effect)) in builtin_slots.iter().enumerate() {
        handle
            .send(Command::InstallBuiltin {
                slot: index,
                effect: Box::new(fnoob_engine::effects::BuiltinProcessor::new(
                    *effect, 48_000,
                )),
            })
            .unwrap();
    }
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
        Command::SetPlan(Box::new(RenderPlan::build(&project, 48_000, &pool, &map))),
        Command::Preview {
            track,
            pitch: 70,
            velocity: 90,
        },
        Command::Record,
        Command::LaunchScene(0),
        Command::LaunchClip { track, slot: 1 },
        Command::Locate(1234),
        Command::SetPlan(Box::new(RenderPlan::build(&project, 48_000, &pool, &map))),
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
            if i == 900 {
                let _ = handle.send(Command::ReleaseClip { track, slot: 1 });
                let _ = handle.send(Command::StopClips(Some(audio)));
            }
            if i == 1200 {
                let _ = handle.send(Command::LaunchScene(2));
            }
            if i == 1600 {
                let _ = handle.send(Command::BackToArrangement);
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
