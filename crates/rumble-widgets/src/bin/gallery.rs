//! Gallery / smoke-test binary: shows every pressable role × state, plus
//! every SurfaceKind. Theme picker at top lets us swap themes at runtime.
//!
//! The scene itself lives in `rumble_widgets::gallery` so both this bin
//! and `examples/screenshot.rs` can mount it without duplicating code.

use eframe::{NativeOptions, egui};
use rumble_widgets::gallery::Gallery;

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 780.0])
            .with_title("rumble-widgets gallery"),
        ..Default::default()
    };
    eframe::run_native(
        "rumble-widgets gallery",
        options,
        Box::new(|_cc| Ok(Box::new(Gallery::default()))),
    )
}
