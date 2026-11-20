use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_engine::{Command, TransportSettings, create};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn process_does_not_allocate() {
    let settings = TransportSettings {
        bpm: 120.0,
        beats_per_bar: 4,
        metronome: true,
    };
    let (mut processor, mut handle) = create(48_000, 2, settings);
    let mut out = vec![0.0f32; 1024];
    for command in [
        Command::Play,
        Command::SetTempo(97.0),
        Command::Locate(1234),
        Command::SetMetronome(false),
        Command::SetMetronome(true),
    ] {
        handle.send(command).unwrap();
    }
    assert_no_alloc(|| {
        for _ in 0..500 {
            processor.process(&mut out);
        }
    });
    handle.send(Command::Stop).unwrap();
    assert_no_alloc(|| processor.process(&mut out));
    assert!(!handle.is_playing());
}
