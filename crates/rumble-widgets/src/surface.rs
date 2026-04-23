use eframe::egui::{InnerResponse, Margin, Rect, Sense, Shape, Ui, UiBuilder, vec2};

use crate::{theme::UiExt, tokens::SurfaceKind};

/// Draws a themed surface behind a block of content. Pattern mirrors
/// egui's `Frame::begin` / `end`:
/// 1. Reserve a shape slot.
/// 2. Run content in a child ui.
/// 3. Compute the framed rect from the content's used rect + margin.
/// 4. Ask the theme for a backdrop shape and install it in the reserved slot.
pub struct SurfaceFrame {
    kind: SurfaceKind,
    inner_margin: Option<Margin>,
}

impl SurfaceFrame {
    pub fn new(kind: SurfaceKind) -> Self {
        Self {
            kind,
            inner_margin: None,
        }
    }

    pub fn inner_margin(mut self, margin: impl Into<Margin>) -> Self {
        self.inner_margin = Some(margin.into());
        self
    }

    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let theme = ui.theme();
        let margin: Margin = self.inner_margin.unwrap_or_else(|| Margin::from(theme.tokens().pad_md));
        let m_left = margin.left as f32;
        let m_right = margin.right as f32;
        let m_top = margin.top as f32;
        let m_bottom = margin.bottom as f32;

        let where_to_paint = ui.painter().add(Shape::Noop);

        let outer = ui.available_rect_before_wrap();
        let content_rect = Rect::from_min_max(outer.min + vec2(m_left, m_top), outer.max - vec2(m_right, m_bottom));

        let mut child_ui = ui.new_child(UiBuilder::new().max_rect(content_rect).layout(*ui.layout()));
        let inner = content(&mut child_ui);
        let used = child_ui.min_rect();

        let framed = Rect::from_min_max(used.min - vec2(m_left, m_top), used.max + vec2(m_right, m_bottom));

        ui.painter().set(where_to_paint, theme.surface(framed, self.kind));

        ui.advance_cursor_after_rect(framed);
        let response = ui.interact(framed, ui.id().with("surface_frame"), Sense::hover());
        InnerResponse::new(inner, response)
    }
}
