use fnoob_platform::plugin::{Module, Transport, instantiate, scan};
use std::path::Path;

const EFFECT: &str = r"C:\Program Files\Common Files\VST3\ValhallaDSP\ValhallaSupermassive.vst3";

fn transport() -> Transport {
    Transport {
        playing: true,
        position: 0,
        tempo: 120.0,
        numerator: 4,
        denominator: 4,
        quarter_notes: 0.0,
    }
}

#[test]
#[ignore = "needs ValhallaSupermassive installed"]
fn effect_loads_processes_and_keeps_state() {
    let path = Path::new(EFFECT);
    let classes = scan(path).unwrap();
    assert!(!classes.is_empty());
    let module = Module::load(path).unwrap();
    let info = &classes[0];
    let (plugin, mut processor) = instantiate(&module, info, 48_000.0, 512, None).unwrap();
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    left[0] = 1.0;
    right[0] = 1.0;
    let mut energy = 0.0;
    for block in 0..200 {
        processor.process(&mut left, &mut right, &transport());
        energy += left.iter().chain(&right).map(|s| s * s).sum::<f32>();
        assert!(left.iter().all(|s| s.is_finite()), "block {block}");
        left.fill(0.0);
        right.fill(0.0);
    }
    assert!(energy > 0.0, "the reverb produced no tail");
    let (component, controller) = plugin.state().unwrap();
    assert!(!component.is_empty());
    drop(processor);
    drop(plugin);
    let (restored, _processor) = instantiate(
        &module,
        info,
        48_000.0,
        512,
        Some((&component, controller.as_deref())),
    )
    .unwrap();
    assert_eq!(restored.state().unwrap().0, component);
    println!(
        "{} by {}: {} bytes of state",
        info.name,
        info.vendor,
        component.len()
    );
}

#[test]
#[ignore = "needs ValhallaSupermassive installed and opens a window"]
fn editor_opens_and_closes() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };
    let path = Path::new(EFFECT);
    let info = scan(path).unwrap().remove(0);
    let module = Module::load(path).unwrap();
    let (plugin, _processor) = instantiate(&module, &info, 48_000.0, 512, None).unwrap();
    let editor = plugin.open_editor("editor test").unwrap();
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(1500) {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!editor.is_closed());
    drop(editor);
    let second = plugin.open_editor("editor test again").unwrap();
    drop(second);
}
