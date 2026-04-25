//! "Luna" paradigm — Windows XP-flavoured chrome (bevelled toolbar,
//! green Connect, amber latched buttons, red danger). Uses LunaTheme.

use eframe::egui::{self, Align, CornerRadius, Layout, Margin, RichText, Stroke, Ui, epaint::RectShape};
use rumble_client::handle::BackendHandle;
use rumble_client_traits::Platform;
use rumble_protocol::{Command, ConnectionState, State};
use rumble_widgets::{ButtonArgs, PressableRole, SurfaceFrame, SurfaceKind, UiExt};

use crate::{
    adapters,
    shell::{Shell, room_header},
};

pub fn render<P: Platform + 'static>(ui: &mut Ui, shell: &mut Shell, state: &State, backend: &BackendHandle<P>) {
    menubar(ui);
    toolbar(ui, shell, state, backend);

    let rect = ui.available_rect_before_wrap();
    let status_h = 24.0;
    let body_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - status_h));
    let status_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, body_rect.max.y), rect.max);

    let sidebar_w = 340.0;
    let side_rect = egui::Rect::from_min_max(body_rect.min, egui::pos2(body_rect.min.x + sidebar_w, body_rect.max.y));
    let center_rect = egui::Rect::from_min_max(egui::pos2(side_rect.max.x, body_rect.min.y), body_rect.max);

    {
        let mut side_ui = ui.new_child(egui::UiBuilder::new().max_rect(side_rect));
        let tokens = side_ui.theme().tokens().clone();
        side_ui
            .painter()
            .add(RectShape::filled(side_rect, CornerRadius::ZERO, tokens.surface));
        side_ui.painter().line_segment(
            [side_rect.right_top(), side_rect.right_bottom()],
            Stroke::new(1.0, tokens.line_soft),
        );
        side_header(&mut side_ui);
        egui::Frame::NONE
            .inner_margin(Margin::symmetric(4, 0))
            .show(&mut side_ui, |ui| shell.tree_pane(ui, state, backend));
    }

    {
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(center_rect));
        center_column(&mut cui, shell, state, backend);
    }

    {
        let mut sui = ui.new_child(egui::UiBuilder::new().max_rect(status_rect));
        statusbar(&mut sui, state);
    }

    ui.advance_cursor_after_rect(rect);
}

fn menubar(ui: &mut Ui) {
    SurfaceFrame::new(SurfaceKind::Toolbar)
        .inner_margin(Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for item in ["File", "Edit", "Server", "View", "Audio", "Configure", "Help"] {
                    let _ = ButtonArgs::new(item).role(PressableRole::Ghost).show(ui);
                }
            });
        });
}

fn toolbar<P: Platform + 'static>(ui: &mut Ui, shell: &mut Shell, state: &State, backend: &BackendHandle<P>) {
    SurfaceFrame::new(SurfaceKind::Toolbar)
        .inner_margin(Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Luna gets Disconnect in the toolbar since we're already
                // connected by the time this paradigm renders. (Connect
                // still lives in the pre-connect view.)
                if ButtonArgs::new("↺ Reconnect")
                    .role(PressableRole::Default)
                    .show(ui)
                    .clicked()
                {
                    backend.send(Command::Disconnect);
                }
                if ButtonArgs::new("Disconnect")
                    .role(PressableRole::Default)
                    .show(ui)
                    .clicked()
                {
                    backend.send(Command::Disconnect);
                }
                sep(ui);

                shell.voice_row(ui, state, backend);
                sep(ui);

                let _ = ButtonArgs::new("+ Channel").role(PressableRole::Default).show(ui);
                let _ = ButtonArgs::new("Comment").role(PressableRole::Default).show(ui);
                sep(ui);

                let _ = ButtonArgs::new("Audio Wizard").role(PressableRole::Default).show(ui);
                if ButtonArgs::new("⚙ Settings")
                    .role(PressableRole::Default)
                    .active(shell.settings_open)
                    .show(ui)
                    .clicked()
                {
                    shell.settings_open = !shell.settings_open;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let tokens = ui.theme().tokens().clone();
                    let who = adapters::my_display_name(state).unwrap_or_else(|| "—".into());
                    ui.label(
                        RichText::new(format!("{who} · {}", adapters::connection_summary(state)))
                            .color(tokens.text_muted)
                            .font(tokens.font_body.clone()),
                    );
                });
            });
        });
}

fn sep(ui: &mut Ui) {
    ui.add_space(4.0);
    let tokens = ui.theme().tokens().clone();
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(1.0, 20.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, tokens.line_soft),
    );
    ui.add_space(4.0);
}

fn side_header(ui: &mut Ui) {
    SurfaceFrame::new(SurfaceKind::Titlebar)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            let tokens = ui.theme().tokens().clone();
            ui.label(
                RichText::new("Server tree")
                    .color(tokens.text_on_accent)
                    .strong()
                    .font(tokens.font_label.clone()),
            );
        });
}

fn center_column<P: Platform + 'static>(ui: &mut Ui, shell: &mut Shell, state: &State, backend: &BackendHandle<P>) {
    let rect = ui.available_rect_before_wrap();
    let composer_h = 56.0;
    let header_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + 44.0));
    let chat_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, header_rect.max.y),
        egui::pos2(rect.max.x, rect.max.y - composer_h),
    );
    let composer_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - composer_h), rect.max);

    {
        let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(header_rect));
        room_header(&mut hui, state);
    }
    {
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(chat_rect));
        shell.chat_stream(&mut cui, state);
    }
    {
        let mut kui = ui.new_child(egui::UiBuilder::new().max_rect(composer_rect));
        shell.composer(&mut kui, state, backend);
    }
    ui.advance_cursor_after_rect(rect);
}

fn statusbar(ui: &mut Ui, state: &State) {
    SurfaceFrame::new(SurfaceKind::Statusbar)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let tokens = ui.theme().tokens().clone();
                let user = adapters::my_display_name(state).unwrap_or_else(|| "—".into());
                let channel = state
                    .my_room_id
                    .and_then(|id| state.room_tree.get(id).map(|n| n.name.clone()))
                    .unwrap_or_else(|| "—".into());
                let peers = adapters::peers_in_current_room(state);
                let server = match &state.connection {
                    ConnectionState::Connected { server_name, .. } => server_name.clone(),
                    _ => "—".into(),
                };
                let cell = |ui: &mut Ui, label: &str, value: &str| {
                    ui.label(
                        RichText::new(label)
                            .color(tokens.text_muted)
                            .font(tokens.font_body.clone()),
                    );
                    ui.label(
                        RichText::new(value)
                            .color(tokens.text)
                            .strong()
                            .font(tokens.font_body.clone()),
                    );
                    ui.add_space(10.0);
                };
                cell(ui, "User:", &user);
                cell(ui, "Channel:", &channel);
                cell(ui, "Peers:", &peers.to_string());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("· {server}"))
                            .color(tokens.accent)
                            .font(tokens.font_mono.clone()),
                    );
                });
            });
        });
}
