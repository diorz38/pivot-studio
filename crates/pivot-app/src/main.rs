//! Pivot Studio desktop entry point (Phase 0 shell).

mod app;
mod chrome;
mod dock;
mod menus;

use app::PivotApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pivot Studio")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pivot Studio",
        options,
        Box::new(|cc| Ok(Box::new(PivotApp::new(cc)))),
    )
}
