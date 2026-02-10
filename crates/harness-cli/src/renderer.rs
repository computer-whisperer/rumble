//! Headless egui rendering to PNG images.
//!
//! Uses egui_kittest's wgpu renderer for GPU-accelerated rasterization.

use eframe::egui;
use egui_kittest::{TestRenderer, wgpu::WgpuTestRenderer};
use image::RgbaImage;

/// Wrapper around WgpuTestRenderer for headless egui rendering.
pub struct Renderer {
    inner: WgpuTestRenderer,
}

impl Renderer {
    /// Create a new renderer with default wgpu setup.
    pub fn new() -> Self {
        Self {
            inner: WgpuTestRenderer::new(),
        }
    }

    /// Handle texture updates from egui.
    /// Must be called after each frame with the textures_delta from FullOutput.
    pub fn handle_delta(&mut self, delta: &egui::TexturesDelta) {
        self.inner.handle_delta(delta);
    }

    /// Render the egui output to an RGBA image.
    pub fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<RgbaImage, String> {
        self.inner.render(ctx, output)
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
