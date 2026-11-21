use assert_no_alloc::AllocDisabler;
use fnoob_core::model::Project;
use fnoob_engine::device::{list_input_devices, list_output_devices, open_input, open_output};
use fnoob_engine::{Command, RenderPlan};
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
#[ignore = "needs a real audio output device and makes sound"]
fn default_device_plays_metronome() {
    let devices = list_output_devices().unwrap();
    assert!(!devices.is_empty());
    let mut project = Project::new("t", 48_000);
    project.tempo_bpm = 240.0;
    let (mut output, mut engine) =
        open_output(None, None, |rate| RenderPlan::from_project(&project, rate)).unwrap();
    engine.send(Command::Play).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    engine.send(Command::Stop).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let expected = u64::from(output.sample_rate) * 3 / 2;
    let position = engine.position();
    assert!(
        position > expected / 2,
        "position {position}, expected about {expected}"
    );
    assert!(!engine.is_running());
    assert_eq!(output.take_error(), None);
}

#[test]
#[ignore = "needs a real audio input and output device"]
fn default_input_delivers_samples_and_reports_latency() {
    assert!(!list_input_devices().unwrap().is_empty());
    let project = Project::new("t", 48_000);
    let (output, _engine) =
        open_output(None, None, |rate| RenderPlan::from_project(&project, rate)).unwrap();
    let (input, mut samples) = open_input(None, None, output.sample_rate).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    while samples.pop().is_ok() {}
    let mut count = 0usize;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        while samples.pop().is_ok() {
            count += 1;
        }
    }
    let expected = input.sample_rate as usize * usize::from(input.channels);
    assert!(count > 0, "no input samples arrived");
    println!(
        "{}: {count} of about {expected} samples per second, input latency {} frames, output latency {} frames",
        input.device_name,
        input.latency_frames(),
        output.latency_frames()
    );
}
