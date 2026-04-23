//! Side-by-side companion to `reference/luna-comparison/reference.html`.
//!
//! Mirrors the XP.css layout so both can be screenshotted and diffed
//! visually: titlebar, a "Button states" fieldset with rest/hover/focus/
//! pressed rows, and a "Form controls" fieldset with text inputs and
//! checkboxes. Buttons here paint the theme directly with forced states —
//! the production `Pressable` derives hover/focus/pressed from Response
//! and can't be forced, which is the point we're working around for
//! static screenshots.

use std::sync::Arc;

use eframe::{
    NativeOptions,
    egui::{self, Align, CentralPanel, Color32, Context, FontId, Layout, Pos2, Rect, Ui, Vec2},
};
use rumble_widgets::{
    LevelMeter, LunaTheme, PressableRole, Slider, SurfaceFrame, SurfaceKind, TextInput, TextRole, Toggle, ToggleStyle,
    Tree, TreeNode, TreeNodeId, UserState, install_theme, theme::Theme, tokens::PressableState,
};

struct App {
    theme: Arc<dyn Theme>,
    installed: bool,
    server_addr: String,
    password: String,
    ptt: bool,
    auto_deafen: bool,
    volume: f32,
    sensitivity: f32,
    tree: Vec<TreeNode>,
    selected: Option<TreeNodeId>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            theme: Arc::new(LunaTheme::default()) as Arc<dyn Theme>,
            installed: false,
            server_addr: String::from("voice.example.org:64738"),
            password: String::from("secret"),
            ptt: true,
            auto_deafen: false,
            volume: 75.0,
            sensitivity: 30.0,
            tree: vec![
                TreeNode::channel(1, "Lobby").with_children(vec![
                    TreeNode::user(
                        10,
                        "alice",
                        UserState {
                            talking: true,
                            ..Default::default()
                        },
                    ),
                    TreeNode::user(
                        11,
                        "bob",
                        UserState {
                            muted: true,
                            ..Default::default()
                        },
                    ),
                ]),
                TreeNode::channel(2, "Music").with_children(vec![TreeNode::user(
                    12,
                    "carol",
                    UserState {
                        deafened: true,
                        ..Default::default()
                    },
                )]),
                TreeNode::channel(3, "AFK"),
            ],
            selected: Some(10),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if !self.installed {
            install_theme(ctx, self.theme.clone());
            self.installed = true;
        }

        // Outer "desktop" background — matches the HTML page so the window
        // chrome reads the same against a blue backdrop.
        ctx.style_mut(|s| {
            s.visuals.panel_fill = Color32::from_rgb(0x3a, 0x6e, 0xa5);
        });

        CentralPanel::default().show(ctx, |ui| {
            // Center a fixed-width "window" frame like the HTML does.
            let window_width = 560.0;
            let available = ui.available_rect_before_wrap();
            let left_inset = ((available.width() - window_width) * 0.5).max(0.0);
            ui.add_space(20.0);
            ui.allocate_ui_with_layout(
                Vec2::new(window_width + left_inset * 2.0, ui.available_height()),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(window_width, ui.available_height()),
                        Layout::top_down(Align::Min),
                        |ui| self.window_contents(ui),
                    );
                },
            );
        });
    }
}

impl App {
    fn window_contents(&mut self, ui: &mut Ui) {
        // Titlebar.
        self.titlebar(ui, "Rumble — Luna Comparison");

        // Window body — a Panel surface with padding.
        SurfaceFrame::new(SurfaceKind::Panel)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                self.button_states(ui);
                ui.add_space(12.0);
                self.form_controls(ui);
                ui.add_space(12.0);
                self.channels(ui);
            });
    }

    fn channels(&mut self, ui: &mut Ui) {
        SurfaceFrame::new(SurfaceKind::Group)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                let legend_color = Color32::from_rgb(0x00, 0x46, 0xd5);
                ui.label(egui::RichText::new("Channels").color(legend_color));
                ui.add_space(6.0);

                let resp = egui::ScrollArea::vertical()
                    .max_height(80.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Tree::new("luna_tree", &self.tree)
                            .selected(self.selected)
                            .row_height(22.0)
                            .show(ui)
                    })
                    .inner;

                if let Some(id) = resp.clicked {
                    self.selected = Some(id);
                }
                if let Some(id) = resp.toggled {
                    toggle_expanded(&mut self.tree, id);
                }
                if let Some(Some(id)) = resp.selection_changed {
                    self.selected = Some(id);
                }
            });
    }

    fn titlebar(&self, ui: &mut Ui, title: &str) {
        // Allocate a fixed 28-px-tall strip and let the theme paint it.
        let (rect, _resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), egui::Sense::hover());
        ui.painter().add(self.theme.surface(rect, SurfaceKind::Titlebar));

        let text_color = self
            .theme
            .text_color(TextRole::Body, SurfaceKind::Titlebar, None, PressableState::default());
        let font = FontId::new(13.0, egui::FontFamily::Proportional);
        let text_pos = Pos2::new(rect.left() + 8.0, rect.center().y);
        // XP titlebar text has a 1px/1px drop shadow in `#0f1089`. Paint the
        // shadow first, then the main text on top.
        ui.painter().text(
            text_pos + Vec2::new(1.0, 1.0),
            egui::Align2::LEFT_CENTER,
            title,
            font.clone(),
            Color32::from_rgb(0x0f, 0x10, 0x89),
        );
        ui.painter()
            .text(text_pos, egui::Align2::LEFT_CENTER, title, font, text_color);

        // Titlebar buttons on the right — minimize / maximize / close. We
        // skip the SVG glyphs and just show placeholder boxes so layout
        // matches.
        let btn_size = Vec2::new(21.0, 21.0);
        let mut x = rect.right() - 5.0;
        for _ in 0..3 {
            let b = Rect::from_min_max(
                Pos2::new(x - btn_size.x, rect.center().y - btn_size.y * 0.5),
                Pos2::new(x, rect.center().y + btn_size.y * 0.5),
            );
            ui.painter().rect_filled(b, 0.0, Color32::from_rgb(0x00, 0x50, 0xee));
            x -= btn_size.x + 2.0;
        }
    }

    fn button_states(&self, ui: &mut Ui) {
        SurfaceFrame::new(SurfaceKind::Group)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                // "legend"
                let legend_color = Color32::from_rgb(0x00, 0x46, 0xd5);
                ui.label(egui::RichText::new("Button states").color(legend_color));
                ui.add_space(6.0);

                let row = |ui: &mut Ui, label: &str, state: PressableState, show_disabled: bool| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).small().weak());
                        ui.add_space(12.0);
                        fake_button(ui, self.theme.as_ref(), "OK", PressableRole::Default, state, 68.0);
                        fake_button(ui, self.theme.as_ref(), "Cancel", PressableRole::Default, state, 68.0);
                        if show_disabled {
                            let s = PressableState {
                                disabled: true,
                                ..Default::default()
                            };
                            fake_button(ui, self.theme.as_ref(), "Disabled", PressableRole::Default, s, 68.0);
                        }
                    });
                };

                row(ui, "rest   ", PressableState::default(), true);
                row(
                    ui,
                    "hover  ",
                    PressableState {
                        hovered: true,
                        ..Default::default()
                    },
                    false,
                );
                row(
                    ui,
                    "focus  ",
                    PressableState {
                        focused: true,
                        ..Default::default()
                    },
                    false,
                );
                row(
                    ui,
                    "pressed",
                    PressableState {
                        pressed: true,
                        ..Default::default()
                    },
                    false,
                );
            });
    }

    fn form_controls(&mut self, ui: &mut Ui) {
        SurfaceFrame::new(SurfaceKind::Group)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                let legend_color = Color32::from_rgb(0x00, 0x46, 0xd5);
                ui.label(egui::RichText::new("Form controls").color(legend_color));
                ui.add_space(6.0);

                egui::Grid::new("luna_form")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Server:");
                        TextInput::new(&mut self.server_addr)
                            .desired_width(260.0)
                            .id_salt("luna_server")
                            .show(ui);
                        ui.end_row();

                        ui.label("Password:");
                        TextInput::new(&mut self.password)
                            .password(true)
                            .desired_width(260.0)
                            .id_salt("luna_password")
                            .show(ui);
                        ui.end_row();
                    });

                ui.add_space(6.0);
                Toggle::new(&mut self.ptt, "Push to talk")
                    .style(ToggleStyle::Checkbox)
                    .show(ui);
                Toggle::new(&mut self.auto_deafen, "Auto-deafen on lock")
                    .style(ToggleStyle::Checkbox)
                    .show(ui);

                ui.add_space(8.0);
                ui.label("Mic level:");
                LevelMeter::new(0.55)
                    .vad(0.2, 0.6)
                    .peak(0.78)
                    .min_size(eframe::egui::Vec2::new(260.0, 14.0))
                    .show(ui);

                ui.add_space(8.0);
                egui::Grid::new("luna_sliders")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Volume:");
                        Slider::new(&mut self.volume, 0.0..=100.0)
                            .step(1.0)
                            .suffix("%")
                            .width(220.0)
                            .show(ui);
                        ui.end_row();

                        ui.label("Sensitivity:");
                        Slider::new(&mut self.sensitivity, 0.0..=100.0)
                            .step(1.0)
                            .suffix("%")
                            .width(220.0)
                            .show(ui);
                        ui.end_row();
                    });
            });
    }
}

/// Paint a button at a forced `PressableState` without registering any
/// interaction. Used for screenshot pages that want to show every visual
/// state on one view.
fn fake_button(ui: &mut Ui, theme: &dyn Theme, text: &str, role: PressableRole, state: PressableState, min_width: f32) {
    let tokens = theme.tokens();
    let pad_x = tokens.pad_md;
    let pad_y = tokens.pad_sm;
    let font = theme.font(TextRole::Body);

    let galley =
        egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font.clone());
    let size = Vec2::new(
        (galley.rect.width() + pad_x * 2.0).max(min_width),
        galley.rect.height() + pad_y * 2.0,
    );

    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().add(theme.pressable(rect, role, state));

    let text_color = theme.text_color(TextRole::Body, SurfaceKind::Panel, Some(role), state);
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.rect.width() * 0.5,
            rect.center().y - galley.rect.height() * 0.5,
        ),
        galley,
        text_color,
    );
}

fn toggle_expanded(nodes: &mut [TreeNode], id: TreeNodeId) {
    for n in nodes {
        if n.id == id {
            n.expanded = !n.expanded;
            return;
        }
        toggle_expanded(&mut n.children, id);
    }
}

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 760.0])
            .with_title("Luna compare"),
        ..Default::default()
    };
    eframe::run_native("luna compare", options, Box::new(|_cc| Ok(Box::new(App::default()))))
}
