//! Titled group-box frame.
//!
//! `SurfaceKind::Group` draws a bordered rectangle. A group-box adds a
//! title chip that sits on the top border: visible in Qt QGroupBox,
//! Windows GroupBox, and the XP Luna "legend" pattern. Themes control the
//! title color via `Theme::group_title_color`.

use eframe::egui::{
    Align, CornerRadius, InnerResponse, Layout, Margin, Pos2, Rect, RichText, Sense, Shape, TextWrapMode, Ui,
    UiBuilder, Vec2, WidgetText, vec2,
};

use crate::{
    theme::UiExt,
    tokens::{SurfaceKind, TextRole},
};

const CHIP_PAD_X: f32 = 4.0;
const TITLE_INSET_X: f32 = 10.0;
const TITLE_BOTTOM_GAP: f32 = 4.0;

pub struct GroupBox {
    title: WidgetText,
    inner_margin: Option<Margin>,
}

impl GroupBox {
    pub fn new(title: impl Into<WidgetText>) -> Self {
        Self {
            title: title.into(),
            inner_margin: None,
        }
    }

    pub fn inner_margin(mut self, margin: impl Into<Margin>) -> Self {
        self.inner_margin = Some(margin.into());
        self
    }

    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let theme = ui.theme();
        let margin: Margin = self
            .inner_margin
            .unwrap_or_else(|| Margin::same(theme.tokens().pad_md as i8));

        let font = theme.font(TextRole::Label);
        let title_galley = self
            .title
            .clone()
            .into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, font.clone());
        let title_h = title_galley.rect.height();
        let top_reserve = (title_h * 0.5 + TITLE_BOTTOM_GAP).max(margin.top as f32);

        let where_to_paint_bg = ui.painter().add(Shape::Noop);
        let where_to_paint_chip = ui.painter().add(Shape::Noop);

        let outer = ui.available_rect_before_wrap();
        let content_rect = Rect::from_min_max(
            outer.min + vec2(margin.left as f32, top_reserve),
            outer.max - vec2(margin.right as f32, margin.bottom as f32),
        );

        let mut child_ui = ui.new_child(UiBuilder::new().max_rect(content_rect).layout(*ui.layout()));
        let inner = content(&mut child_ui);
        let used = child_ui.min_rect();

        // Group's top edge sits at the vertical midline of the title, so
        // the chip cleanly "breaks" the border.
        let frame_top = outer.min.y + title_h * 0.5;
        let framed = Rect::from_min_max(
            Pos2::new(outer.min.x, frame_top),
            used.max + vec2(margin.right as f32, margin.bottom as f32),
        );

        ui.painter()
            .set(where_to_paint_bg, theme.surface(framed, SurfaceKind::Group));

        // Chip: same color as the group fill, positioned over the top
        // border line. Painting after the border-shape replaces that
        // strip with solid fill — no per-primitive "cut" needed.
        let chip_w = title_galley.rect.width() + CHIP_PAD_X * 2.0;
        let chip_rect = Rect::from_min_size(
            Pos2::new(outer.min.x + TITLE_INSET_X, outer.min.y),
            Vec2::new(chip_w, title_h),
        );
        ui.painter().set(
            where_to_paint_chip,
            Shape::rect_filled(chip_rect, CornerRadius::ZERO, theme.tokens().surface_alt),
        );

        // Title text in an accessibility-aware child ui so screen
        // readers can find it (mirrors `paint_label_accessible` in
        // toggle.rs).
        let title_color = theme.group_title_color();
        let text = title_galley.text().to_string();
        let builder = UiBuilder::new()
            .id_salt("group_box_title")
            .max_rect(chip_rect)
            .layout(Layout::left_to_right(Align::Center));
        ui.scope_builder(builder, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.add_space(CHIP_PAD_X);
            ui.label(RichText::new(text).color(title_color).font(font));
        });

        ui.advance_cursor_after_rect(framed);
        let response = ui.interact(framed, ui.id().with("group_box"), Sense::hover());
        InnerResponse::new(inner, response)
    }
}
