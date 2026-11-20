use assert_no_alloc::AllocDisabler;
use fnoob_engine::device::{list_output_devices, open_output};
use fnoob_engine::{Command, TransportSettings};
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
#[ignore = "needs a real audio output device and makes sound"]
fn default_device_plays_metronome() {
    let devices = list_output_devices().unwrap();
    assert!(!devices.is_empty());
    let settings = TransportSettings {
        bpm: 240.0,
        beats_per_bar: 4,
        metronome: true,
    };
    let (mut output, mut engine) = open_output(None, settings).unwrap();
    engine.send(Command::Play).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    engine.send(Command::Stop).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let expected = u64::from(output.sample_rate) * 3 / 2;
    let position = engine.position();
    assert!(position > expected / 2, "position {position}, expected about {expected}");
    assert!(!engine.is_playing());
    assert_eq!(output.take_error(), None);
}
