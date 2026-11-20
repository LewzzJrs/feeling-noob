use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_core::edit::{add_midi_region, add_track};
use fnoob_core::model::{Project, TrackKind};
use fnoob_engine::{Command, RenderPlan, create};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn process_does_not_allocate() {
    let mut project = Project::new("t", 48_000);
    let track = add_track(&mut project, TrackKind::Instrument);
    for bar in 0..8 {
        add_midi_region(&mut project, track, bar * 3840, 960);
    }
    project.loop_range.enabled = true;
    project.metronome.count_in_enabled = true;
    let (mut processor, mut handle) = create(
        48_000,
        2,
        Box::new(RenderPlan::from_project(&project, 48_000)),
    );
    let mut out = vec![0.0f32; 1024];
    project.tempo_bpm = 97.0;
    let commands = [
        Command::SetPlan(Box::new(RenderPlan::from_project(&project, 48_000))),
        Command::Record,
        Command::Locate(1234),
        Command::SetPlan(Box::new(RenderPlan::from_project(&project, 48_000))),
    ];
    for command in commands {
        handle.send(command).unwrap();
    }
    assert_no_alloc(|| {
        for _ in 0..2000 {
            processor.process(&mut out);
        }
    });
    handle.send(Command::Stop).unwrap();
    assert_no_alloc(|| processor.process(&mut out));
    assert!(!handle.is_running());
    handle.collect_garbage();
}
