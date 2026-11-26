use crate::audio_data::wav_spec;
use crate::plan::{MAX_BLOCK, RenderPlan};
use crate::processor::{Command, EngineHandle, Processor, create};
use crate::record::write_sample;
use fnoob_core::settings::RecordFormat;
use fnoob_platform::plugin::PluginProcessor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

pub struct BounceOutcome {
    pub result: Result<PathBuf, String>,
    pub engine: (Processor, EngineHandle),
}

pub struct BounceJob {
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    total: u64,
    handle: JoinHandle<BounceOutcome>,
}

pub struct BounceRequest {
    pub plan: Box<RenderPlan>,
    pub sample_rate: u32,
    pub start: u64,
    pub frames: u64,
    pub path: PathBuf,
    pub format: RecordFormat,
    pub plugins: Vec<(usize, Box<PluginProcessor>)>,
    pub builtins: Vec<(usize, Box<crate::effects::BuiltinProcessor>)>,
}

fn part_path(path: &std::path::Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

fn render(
    processor: &mut Processor,
    request: &BounceRequest,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let part = part_path(&request.path);
    let spec = wav_spec(2, request.sample_rate, request.format);
    let mut writer = hound::WavWriter::create(&part, spec).map_err(|e| e.to_string())?;
    let mut buffer = vec![0.0f32; MAX_BLOCK * 2];
    let mut done = 0u64;
    while done < request.frames {
        if cancel.load(Ordering::Acquire) {
            drop(writer);
            let _ = std::fs::remove_file(&part);
            return Err("cancelled".into());
        }
        let block = (request.frames - done).min(MAX_BLOCK as u64) as usize;
        let out = &mut buffer[..block * 2];
        processor.process(out);
        for &sample in out.iter() {
            write_sample(&mut writer, request.format, sample).map_err(|e| e.to_string())?;
        }
        done += block as u64;
        progress.store(done, Ordering::Release);
    }
    writer.finalize().map_err(|e| e.to_string())?;
    std::fs::rename(&part, &request.path).map_err(|e| e.to_string())
}

pub fn start_bounce(mut request: BounceRequest) -> Result<BounceJob, String> {
    if request.frames == 0 {
        return Err("there is nothing to bounce".into());
    }
    if let Some(dir) = request.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let plan = std::mem::replace(
        &mut request.plan,
        Box::new(RenderPlan::idle(request.sample_rate)),
    );
    let (mut processor, mut handle) = create(request.sample_rate, 2, plan);
    for (slot, plugin) in request.plugins.drain(..) {
        handle
            .send(Command::InstallPlugin { slot, plugin })
            .map_err(|_| "too many plugins".to_owned())?;
    }
    for (slot, effect) in request.builtins.drain(..) {
        handle
            .send(Command::InstallBuiltin { slot, effect })
            .map_err(|_| "too many plugins".to_owned())?;
    }
    for command in [Command::Locate(request.start), Command::Play] {
        handle
            .send(command)
            .map_err(|_| "the bounce engine is busy".to_owned())?;
    }
    let progress = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (thread_progress, thread_cancel) = (Arc::clone(&progress), Arc::clone(&cancel));
    let total = request.frames;
    let handle = std::thread::Builder::new()
        .name("fnoob-bounce".into())
        .spawn(move || {
            let result = render(&mut processor, &request, &thread_progress, &thread_cancel)
                .map(|()| request.path.clone());
            BounceOutcome {
                result,
                engine: (processor, handle),
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(BounceJob {
        progress,
        cancel,
        total,
        handle,
    })
}

impl BounceJob {
    pub fn progress(&self) -> f32 {
        self.progress.load(Ordering::Acquire) as f32 / self.total.max(1) as f32
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub fn finish(self) -> BounceOutcome {
        match self.handle.join() {
            Ok(outcome) => outcome,
            Err(_) => {
                let (processor, handle) = create(48_000, 2, Box::new(RenderPlan::idle(48_000)));
                BounceOutcome {
                    result: Err("the bounce thread crashed".into()),
                    engine: (processor, handle),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_data::load_wav;
    use fnoob_core::edit::{add_midi_region, add_note, add_track};
    use fnoob_core::model::{Note, Project, TrackKind};

    fn project() -> Project {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, track, 0, 3840).unwrap();
        for i in 0..8 {
            add_note(
                &mut p,
                region,
                Note {
                    pitch: 60 + i,
                    velocity: 100,
                    start_tick: u64::from(i) * 480,
                    length_tick: 400,
                    chance: 100,
                },
            );
        }
        p
    }

    fn direct(project: &Project, block: usize, frames: usize) -> Vec<f32> {
        let mut plan = RenderPlan::from_project(project, 48_000);
        plan.metronome = false;
        let (mut p, mut h) = create(48_000, 2, Box::new(plan));
        h.send(Command::Play).unwrap();
        let mut out = vec![0.0; frames * 2];
        for chunk in out.chunks_mut(block * 2) {
            p.process(chunk);
        }
        out
    }

    #[test]
    fn output_does_not_depend_on_block_size() {
        let p = project();
        assert_eq!(direct(&p, 64, 30_000), direct(&p, 2048, 30_000));
        assert_eq!(direct(&p, 333, 30_000), direct(&p, 1024, 30_000));
    }

    #[test]
    fn bounce_file_matches_playback() {
        let p = project();
        let path = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("bounce-{}", std::process::id()))
            .join("mix.wav");
        let mut plan = RenderPlan::from_project(&p, 48_000);
        plan.metronome = false;
        let job = start_bounce(BounceRequest {
            plan: Box::new(plan),
            sample_rate: 48_000,
            start: 0,
            frames: 30_000,
            path: path.clone(),
            format: RecordFormat::Float32,
            plugins: Vec::new(),
            builtins: Vec::new(),
        })
        .unwrap();
        while !job.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(job.progress(), 1.0);
        let outcome = job.finish();
        assert_eq!(outcome.result.unwrap(), path);
        let data = load_wav(&path, 48_000).unwrap();
        assert_eq!(data.channels, 2);
        assert_eq!(data.samples, direct(&p, 512, 30_000));
        assert!(!part_path(&path).exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn cancel_removes_the_partial_file() {
        let p = project();
        let path = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("bounce-cancel-{}", std::process::id()))
            .join("mix.wav");
        let job = start_bounce(BounceRequest {
            plan: Box::new(RenderPlan::from_project(&p, 48_000)),
            sample_rate: 48_000,
            start: 0,
            frames: 48_000 * 600,
            path: path.clone(),
            format: RecordFormat::Int16,
            plugins: Vec::new(),
            builtins: Vec::new(),
        })
        .unwrap();
        job.cancel();
        let outcome = job.finish();
        assert_eq!(outcome.result, Err("cancelled".into()));
        assert!(!path.exists() && !part_path(&path).exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
