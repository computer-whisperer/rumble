//! In-process test harness for rumble-next.
//!
//! Mirrors `rumble_egui::TestHarness` so `harness-cli` (and any new
//! integration tests) can drive rumble-next without rendering to a
//! real display. The egui implementation is a much fuller surface
//! (input injection, query-by-label, etc.); this is a starting point
//! that we'll grow as the agent-loop story for rumble-next matures.
//!
//! Built only when the `test-harness` feature is on so production
//! binaries don't carry `egui_kittest`.

#![cfg(feature = "test-harness")]

use eframe::egui;
use egui_kittest::Harness;
use image::RgbaImage;

use crate::App;

/// Wraps a kittest `Harness` around `App` so callers can step frames,
/// snapshot the framebuffer, and inspect backend state — all without
/// opening a window.
pub struct TestHarness {
    harness: Harness<'static, App>,
}

impl TestHarness {
    /// Construct a new harness at the standard 1280×820 viewport. The
    /// `App` is created once and reused across `step()` calls. Caller
    /// is responsible for sandboxing the config dir (see
    /// `RUMBLE_NEXT_CONFIG_DIR` in `examples/screenshot.rs`).
    pub fn new() -> Self {
        Self::with_size(1280.0, 820.0)
    }

    pub fn with_size(width: f32, height: f32) -> Self {
        let harness = Harness::builder()
            .with_size(egui::Vec2::new(width, height))
            .with_pixels_per_point(2.0)
            .wgpu()
            .build_eframe(|cc| App::new(cc).expect("App::new failed"));

        Self { harness }
    }

    pub fn app(&self) -> &App {
        self.harness.state()
    }

    pub fn app_mut(&mut self) -> &mut App {
        self.harness.state_mut()
    }

    /// Run one event-loop step (handle input, render, tessellate).
    pub fn step(&mut self) {
        self.harness.step();
    }

    /// Run `n` steps in a row. Useful for letting `ScrollArea` settle
    /// or for waiting on async backend work to bubble up to state.
    pub fn run_frames(&mut self, n: usize) {
        for _ in 0..n {
            self.harness.step();
        }
    }

    /// Render the current frame to RGBA pixels. Requires the wgpu
    /// backend (set up by `Harness::builder().wgpu()` in `with_size`).
    pub fn render(&mut self) -> Result<RgbaImage, String> {
        self.harness.render().map_err(|e| e.to_string())
    }

    pub fn ctx(&self) -> &egui::Context {
        &self.harness.ctx
    }

    /// Direct access to the underlying kittest harness — escape hatch
    /// for things this thin wrapper doesn't expose yet.
    pub fn kittest(&self) -> &Harness<'static, App> {
        &self.harness
    }

    pub fn kittest_mut(&mut self) -> &mut Harness<'static, App> {
        &mut self.harness
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

// `App` doesn't implement Send/Sync (it owns a tokio runtime + raw
// egui types), so we can't move it across threads in the harness.
// Kittest expects to own its state, but that ownership stays on the
// constructing thread — which is exactly how tests use it.

// The non-`test-harness` build path doesn't expose the harness at all;
// callers should `--features rumble-next/test-harness` to use it.

impl App {
    /// Convenience accessor used by integration tests — returns the
    /// active paradigm so tests don't need to reach into private state.
    pub fn current_paradigm(&self) -> crate::Paradigm {
        self.paradigm
    }
}
