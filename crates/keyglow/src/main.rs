//! `keyglow`: the settings window. It edits `~/.config/keyglow/active.json`; the background
//! program (`keyglowd`) watches that file and lights the keyboard.

mod app;
mod editors;
mod model;

use eframe::egui;
use keyglow_core::store::{self, Store};

fn main() -> eframe::Result {
    let store = Store::new(Store::default_dir());
    let status_path = store::status_path();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Keyglow")
            .with_app_id("keyglow")
            .with_icon(icon())
            .with_inner_size([900.0, 580.0])
            .with_min_inner_size([660.0, 420.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Keyglow",
        options,
        Box::new(move |cc| {
            app::configure(&cc.egui_ctx);
            Ok(Box::new(app::App::open(store, status_path)))
        }),
    )
}

/// The window icon on X11. Wayland desktops take it from the launcher entry instead.
fn icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../packaging/keyglow.png"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_window_icon_loads() {
        let icon = super::icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
