#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("feeling-noob")
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "feeling-noob",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
