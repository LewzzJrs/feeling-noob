#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod bounce;
mod browser;
mod files;
mod inspector;
mod instrument;
mod keys;
mod midi;
mod mixer;
mod musical;
mod piano_roll;
mod plugins;
mod timeline;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("feeling-noob")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([640.0, 360.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "feeling-noob",
        options,
        Box::new(|_cc| {
            Ok(Box::new(app::App::new(
                std::env::args_os().nth(1).map(Into::into),
            )))
        }),
    )
}
