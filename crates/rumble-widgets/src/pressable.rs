use std::hash::Hash;

use eframe::egui::{Direction, Id, InnerResponse, Layout, Response, Sense, Ui, UiBuilder, Vec2, WidgetText};

use crate::{
    theme::UiExt,
    tokens::{PressableRole, PressableState, SurfaceKind, TextRole},
};

/// Interaction primitive: allocates a rect, senses click, computes a
/// `PressableState`, and asks the theme for a backdrop shape. The content
/// closure renders on top in a child `Ui`, with text color overridden to
/// match role + state.
pub struct Pressable {
    id_salt: Id,
    role: PressableRole,
    active: bool,
    disabled: bool,
    min_size: Vec2,
    text_role: TextRole,
}

impl Pressable {
    pub fn new(id_salt: impl Hash) -> Self {
        Self {
            id_salt: Id::new(id_salt),
            role: PressableRole::Default,
            active: false,
            disabled: false,
            min_size: Vec2::new(56.0, 26.0),
            text_role: TextRole::Body,
        }
    }

    pub fn role(mut self, role: PressableRole) -> Self {
        self.role = role;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }

    pub fn text_role(mut self, role: TextRole) -> Self {
        self.text_role = role;
        self
    }

    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let theme = ui.theme();

        let sense = if self.disabled { Sense::hover() } else { Sense::click() };
        let (rect, response) = ui.allocate_exact_size(self.min_size, sense);

        let state = PressableState {
            hovered: response.hovered(),
            pressed: response.is_pointer_button_down_on(),
            active: self.active,
            focused: response.has_focus(),
            disabled: self.disabled,
        };

        ui.painter().add(theme.pressable(rect, self.role, state));

        let text_color = theme.text_color(self.text_role, SurfaceKind::Panel, Some(self.role), state);

        let child_builder = UiBuilder::new()
            .id_salt(self.id_salt)
            .max_rect(rect)
            .layout(Layout::centered_and_justified(Direction::LeftToRight));

        let inner = ui
            .scope_builder(child_builder, |ui| {
                ui.style_mut().visuals.override_text_color = Some(text_color);
                // Labels default to selectable=true, which registers a
                // click-and-drag sense for text selection. Inside a
                // pressable that would eat the button's click.
                ui.style_mut().interaction.selectable_labels = false;
                content(ui)
            })
            .inner;

        InnerResponse::new(inner, response)
    }
}

/// Text-labelled `Pressable` convenience builder.
pub struct ButtonArgs {
    text: WidgetText,
    role: PressableRole,
    active: bool,
    disabled: bool,
    min_width: f32,
}

pub fn button(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    ButtonArgs::new(text).show(ui)
}

impl ButtonArgs {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            role: PressableRole::Default,
            active: false,
            disabled: false,
            min_width: 0.0,
        }
    }

    pub fn role(mut self, role: PressableRole) -> Self {
        self.role = role;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.theme();
        let pad_x = theme.tokens().pad_md;
        let pad_y = theme.tokens().pad_sm;
        let font = theme.font(TextRole::Body);

        let galley = self.text.clone().into_galley(
            ui,
            Some(eframe::egui::TextWrapMode::Extend),
            f32::INFINITY,
            font.clone(),
        );
        let size = Vec2::new(
            (galley.rect.width() + pad_x * 2.0).max(self.min_width),
            galley.rect.height() + pad_y * 2.0,
        );

        let text = galley.text().to_string();
        let id_salt = ("rumble_widgets::button", text.clone());

        Pressable::new(id_salt)
            .role(self.role)
            .active(self.active)
            .disabled(self.disabled)
            .min_size(size)
            .show(ui, |ui| {
                ui.label(eframe::egui::RichText::new(text).font(font.clone()));
            })
            .response
    }
}
