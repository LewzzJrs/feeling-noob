use assert_no_alloc::AllocDisabler;
use fnoob_core::model::Project;
use fnoob_engine::device::{list_output_devices, open_output};
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
        open_output(None, |rate| RenderPlan::from_project(&project, rate)).unwrap();
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
