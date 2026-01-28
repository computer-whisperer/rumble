//! Rumble voice chat client - eframe runner.
//!
//! This is the native desktop runner for the Rumble application.
//! It uses eframe to create a window and run the egui-based UI.

use clap::Parser;
use eframe::egui;
use egui_test::{Args, RumbleApp};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Create the tokio runtime - this will be passed to RumbleApp
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rumble",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(EframeWrapper::new(cc.egui_ctx.clone(), runtime, args)))
        }),
    )
}

/// Wrapper that implements `eframe::App` for `RumbleApp`.
///
/// This keeps the `RumbleApp` independent of eframe, allowing it to be
/// used with other runners like the test harness.
struct EframeWrapper {
    app: RumbleApp,
    /// Keep the runtime alive for the lifetime of the application.
    _runtime: tokio::runtime::Runtime,
}

impl EframeWrapper {
    fn new(ctx: egui::Context, runtime: tokio::runtime::Runtime, args: Args) -> Self {
        let app = RumbleApp::new(ctx, runtime.handle().clone(), args);
        Self { app, _runtime: runtime }
    }
}

impl eframe::App for EframeWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.render(ctx);
    }
}
