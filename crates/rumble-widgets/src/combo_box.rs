//! Dropdown selector — a flat Field-styled button plus a popup list.
//!
//! Built on our own `SurfaceFrame(Popup)` + `Pressable` primitives so it
//! picks up each theme's chrome without a per-theme wrapper. Open state
//! lives in egui's temporary memory keyed by the widget id.

use eframe::egui::{
    Area, Color32, Frame, Id, Order, Pos2, Rect, Response, RichText, Sense, Shape, TextWrapMode, Ui, Vec2, WidgetText,
    emath::GuiRounding,
};

use crate::{
    pressable::ButtonArgs,
    surface::SurfaceFrame,
    theme::UiExt,
    tokens::{PressableRole, SurfaceKind, TextRole},
};

const ARROW_W: f32 = 16.0;
const ROW_PAD_X: f32 = 8.0;
const DEFAULT_WIDTH: f32 = 180.0;

pub struct ComboBox<'a> {
    id_salt: String,
    selected: &'a mut usize,
    options: Vec<String>,
    width: f32,
    disabled: bool,
}

impl<'a> ComboBox<'a> {
    pub fn new(id_salt: impl Into<String>, selected: &'a mut usize, options: Vec<String>) -> Self {
        Self {
            id_salt: id_salt.into(),
            selected,
            options,
            width: DEFAULT_WIDTH,
            disabled: false,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.theme();
        let tokens = theme.tokens();

        let id = Id::new(&self.id_salt).with("combo_box");
        let mut open: bool = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));

        let height = tokens.font_body.size + tokens.pad_sm * 2.0 + 4.0;
        let size = Vec2::new(self.width, height);
        let sense = if self.disabled { Sense::hover() } else { Sense::click() };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);

        if response.clicked() && !self.disabled {
            open = !open;
            response.mark_changed();
        }

        // Paint Field surface for the button chrome.
        ui.painter().add(theme.surface(rect, SurfaceKind::Field));

        // Current selection text.
        let current_text = self.options.get(*self.selected).cloned().unwrap_or_default();
        let text_color = if self.disabled { tokens.text_muted } else { tokens.text };
        let text_rect = Rect::from_min_max(
            Pos2::new(rect.left() + ROW_PAD_X, rect.top()),
            Pos2::new(rect.right() - ARROW_W, rect.bottom()),
        );
        let font = theme.font(TextRole::Body);
        let galley =
            WidgetText::from(current_text).into_galley(ui, Some(TextWrapMode::Truncate), text_rect.width(), font);
        let text_pos = Pos2::new(text_rect.left(), text_rect.center().y - galley.rect.height() * 0.5);
        ui.painter().galley(text_pos, galley, text_color);

        // Dropdown arrow on the right.
        let arrow_rect = Rect::from_min_max(Pos2::new(rect.right() - ARROW_W, rect.top()), rect.max);
        paint_arrow(ui, arrow_rect, text_color, open);

        // Popup.
        if open && !self.disabled {
            let popup_pos = Pos2::new(rect.left(), rect.bottom() + 2.0);
            let area = Area::new(id.with("popup"))
                .order(Order::Foreground)
                .fixed_pos(popup_pos)
                .constrain(true);
            let area_resp = area.show(ui.ctx(), |ui| {
                // Neutralize any outer Frame so our SurfaceFrame is the
                // only chrome layer.
                Frame::NONE.show(ui, |ui| {
                    SurfaceFrame::new(SurfaceKind::Popup).inner_margin(4).show(ui, |ui| {
                        ui.set_min_width(self.width - 8.0);
                        ui.set_max_width(self.width - 8.0);
                        for (i, opt) in self.options.iter().enumerate() {
                            let active = i == *self.selected;
                            let r = ButtonArgs::new(RichText::new(opt))
                                .role(PressableRole::Ghost)
                                .active(active)
                                .min_width(self.width - 16.0)
                                .show(ui);
                            if r.clicked() {
                                *self.selected = i;
                                open = false;
                                response.mark_changed();
                            }
                        }
                    });
                });
            });

            // Click outside the popup closes it.
            if ui.input(|i| i.pointer.any_click())
                && !area_resp.response.contains_pointer()
                && !response.contains_pointer()
            {
                open = false;
            }
        }

        ui.memory_mut(|m| m.data.insert_temp(id, open));
        response
    }
}

fn paint_arrow(ui: &mut Ui, rect: Rect, color: Color32, open: bool) {
    // Snap: `convex_polygon` is not auto-snapped by the tessellator.
    let rect = rect.round_to_pixels(ui.ctx().pixels_per_point());
    let w = 8.0;
    let h = 4.0;
    let cx = rect.center().x;
    let cy = rect.center().y;
    let (p1, p2, p3) = if open {
        (
            Pos2::new(cx - w * 0.5, cy + h * 0.5),
            Pos2::new(cx + w * 0.5, cy + h * 0.5),
            Pos2::new(cx, cy - h * 0.5),
        )
    } else {
        (
            Pos2::new(cx - w * 0.5, cy - h * 0.5),
            Pos2::new(cx + w * 0.5, cy - h * 0.5),
            Pos2::new(cx, cy + h * 0.5),
        )
    };
    ui.painter().add(Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        eframe::egui::Stroke::NONE,
    ));
}
