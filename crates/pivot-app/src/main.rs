//! Pivot Studio desktop entry point (Phase 0 shell).

mod app;
mod chrome;
#[macro_use]
mod devlog;
mod dock;
pub mod icons;
mod menus;
mod pivot_grid;

use app::PivotApp;

fn main() -> eframe::Result<()> {
    eprintln!("[pivot-dev] starting Pivot Studio (debug build instrumentation active; set PIVOT_DEVLOG=0 to silence in debug builds)");
    let viewport = egui::ViewportBuilder::default()
        .with_title("Pivot Studio")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([900.0, 560.0])
        // No generic OS title bar (DesignCraft reference): custom chrome provides Home + menus.
        .with_decorations(false)
        .with_transparent(false);
    #[cfg(target_os = "macos")]
    let viewport = viewport
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Pivot Studio",
        options,
        Box::new(|cc| Ok(Box::new(PivotApp::new(cc)))),
    )
}
