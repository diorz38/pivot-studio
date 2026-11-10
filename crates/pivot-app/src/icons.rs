//! SVG Icon assets and helper functions for Pivot Studio (Deriva aesthetic).

use egui::{Color32, Image, Response, Sense, Ui, Vec2};

pub const ICON_APP_LOGO: &[u8] = include_bytes!("../assets/icons/app-logo.svg");
pub const ICON_DATABASE: &[u8] = include_bytes!("../assets/icons/database.svg");
pub const ICON_GIT_MERGE: &[u8] = include_bytes!("../assets/icons/git-merge.svg");
pub const ICON_TABLE: &[u8] = include_bytes!("../assets/icons/table.svg");
pub const ICON_PALETTE: &[u8] = include_bytes!("../assets/icons/palette.svg");
pub const ICON_DOWNLOAD: &[u8] = include_bytes!("../assets/icons/download.svg");
pub const ICON_FILES: &[u8] = include_bytes!("../assets/icons/files.svg");
pub const ICON_PLAY: &[u8] = include_bytes!("../assets/icons/play.svg");
pub const ICON_ALERT_CIRCLE: &[u8] = include_bytes!("../assets/icons/alert-circle.svg");
pub const ICON_ALERT_TRIANGLE: &[u8] = include_bytes!("../assets/icons/alert-triangle.svg");
pub const ICON_LIGHTBULB: &[u8] = include_bytes!("../assets/icons/lightbulb.svg");
pub const ICON_COPY: &[u8] = include_bytes!("../assets/icons/copy.svg");
pub const ICON_GRIP_VERTICAL: &[u8] = include_bytes!("../assets/icons/grip-vertical.svg");
pub const ICON_SETTINGS: &[u8] = include_bytes!("../assets/icons/settings.svg");

/// Draw an SVG image tinted with a specific theme color.
pub fn draw_svg(ui: &mut Ui, uri: &'static str, bytes: &'static [u8], size: Vec2, tint: Color32) -> Response {
    let img = Image::from_bytes(uri, bytes)
        .fit_to_exact_size(size)
        .tint(tint);
    ui.add(img)
}

/// An interactive button with an SVG icon and theme tinting.
pub fn svg_icon_button(
    ui: &mut Ui,
    uri: &'static str,
    bytes: &'static [u8],
    size: Vec2,
    tint: Color32,
    tooltip: &str,
) -> Response {
    let padding = Vec2::new(6.0, 6.0);
    let total_size = size + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());
    let response = response.on_hover_text(tooltip);

    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
    }

    let img_rect = egui::Rect::from_center_size(rect.center(), size);
    let img = Image::from_bytes(uri, bytes)
        .fit_to_exact_size(size)
        .tint(if response.hovered() { Color32::WHITE } else { tint });
    img.paint_at(ui, img_rect);

    response
}

/// Render a badge/pill tag containing a small icon and text label.
pub fn icon_pill(
    ui: &mut Ui,
    uri: &'static str,
    bytes: &'static [u8],
    text: &str,
    bg: Color32,
    fg: Color32,
) {
    egui::Frame::NONE
        .fill(bg)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                draw_svg(ui, uri, bytes, Vec2::new(12.0, 12.0), fg);
                ui.colored_label(fg, egui::RichText::new(text).size(11.0).strong());
            });
        });
}
