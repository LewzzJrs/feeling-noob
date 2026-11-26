use crate::app::App;
use eframe::egui;
use fnoob_core::keymap::Command;
use fnoob_core::model::Grid;

impl App {
    pub(crate) fn quantize_window(&mut self, ctx: &egui::Context) {
        if !self.quantize_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Quantize Settings")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                let mut grid = self.project.snap.grid;
                egui::ComboBox::from_label("Grid")
                    .selected_text(grid.label())
                    .show_ui(ui, |ui| {
                        for option in Grid::ALL.into_iter().chain(Grid::TRIPLETS) {
                            ui.selectable_value(&mut grid, option, option.label());
                        }
                    });
                if grid != self.project.snap.grid {
                    self.edit(|p| p.snap.grid = grid);
                }
                let mut settings = self.project.quantize;
                let strength = ui
                    .add(
                        egui::Slider::new(&mut settings.strength, 0..=100)
                            .suffix(" %")
                            .text("Amount"),
                    )
                    .on_hover_text(
                        "How far notes move toward the grid. 100% puts them right on it.",
                    );
                self.live_edit(&strength, |p| p.quantize.strength = settings.strength);
                let swing = ui
                    .add(
                        egui::Slider::new(&mut settings.swing, 0..=100)
                            .suffix(" %")
                            .text("Swing"),
                    )
                    .on_hover_text("Delay every second grid line by up to half a grid step.");
                self.live_edit(&swing, |p| p.quantize.swing = settings.swing);
                if ui
                    .checkbox(&mut settings.ends, "Quantize note ends")
                    .on_hover_text("Also move the end of each note to the grid.")
                    .changed()
                {
                    let ends = settings.ends;
                    self.edit(|p| p.quantize.ends = ends);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Quantize")
                        .on_hover_text("Quantize the selected clips or notes (Ctrl+U)")
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button("Reset").clicked() {
                        self.edit(|p| p.quantize = Default::default());
                    }
                });
            });
        if apply {
            self.run(Command::Quantize);
        }
        if !open {
            self.quantize_open = false;
        }
    }
}
