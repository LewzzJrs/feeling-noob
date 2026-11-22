use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use fnoob_core::edit::{add_audio_file, add_audio_region, add_track};
use fnoob_core::model::{PluginSlot, Project, TrackKind};
use fnoob_engine::audio_data::{AudioData, AudioPool};
use fnoob_engine::plan::{MAX_BLOCK, PluginMap};
use fnoob_engine::{Command, RenderPlan, create};
use fnoob_platform::plugin::{Module, cid_to_hex, instantiate, scan};
use std::path::Path;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const EFFECT: &str = r"C:\Program Files\Common Files\VST3\ValhallaDSP\ValhallaSupermassive.vst3";

#[test]
#[ignore = "needs ValhallaSupermassive installed"]
fn insert_plugin_adds_a_reverb_tail() {
    let path = Path::new(EFFECT);
    let info = scan(path).unwrap().remove(0);
    let module = Module::load(path).unwrap();
    let mut project = Project::new("t", 48_000);
    project.metronome.enabled = false;
    let track = add_track(&mut project, TrackKind::Audio);
    let file = add_audio_file(&mut project, "audio/click.wav".into(), 1, 4800);
    add_audio_region(&mut project, track, 0, file, 0);
    let slot_id = project.alloc_id();
    project
        .track_mut(track)
        .unwrap()
        .strip
        .inserts
        .push(PluginSlot {
            id: slot_id,
            class_id: cid_to_hex(&info.cid),
            name: info.name.clone(),
            path_hint: EFFECT.into(),
            bypass: false,
            component_state: String::new(),
            controller_state: None,
            builtin: None,
        });
    let mut samples = vec![0.0; 4800];
    samples[0] = 1.0;
    let mut pool = AudioPool::new();
    pool.insert(file, Arc::new(AudioData::from_samples(1, samples)));
    let mut plugins = PluginMap::new();
    plugins.insert(slot_id, 7);
    let plan = Box::new(RenderPlan::build(&project, 48_000, &pool, &plugins));
    let (mut processor, mut handle) = create(48_000, 2, plan);
    let (_plugin, instance) = instantiate(&module, &info, 48_000.0, MAX_BLOCK, None).unwrap();
    handle
        .send(Command::InstallPlugin {
            slot: 7,
            plugin: instance,
        })
        .unwrap();
    handle.send(Command::Play).unwrap();
    let mut out = vec![0.0f32; 2 * 512];
    let mut tail = 0.0;
    assert_no_alloc(|| {
        for block in 0..100 {
            processor.process(&mut out);
            if block > 20 {
                tail += out.iter().map(|s| s * s).sum::<f32>();
            }
        }
    });
    assert!(tail > 0.0, "no reverb tail after the click region ended");
    handle.send(Command::RemovePlugin(7)).unwrap();
    processor.process(&mut out);
    handle.collect_garbage();
}
