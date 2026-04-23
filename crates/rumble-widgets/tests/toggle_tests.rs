//! Functional tests for `Toggle`. Both visual styles share the same
//! interaction surface, so most tests cover the default Switch.

use std::sync::Arc;

use eframe::egui::{self, Vec2};
use egui_kittest::{Harness, kittest::Queryable};
use rumble_widgets::{ModernTheme, Toggle, ToggleStyle, install_theme};

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

struct State {
    on: bool,
    style: ToggleStyle,
    disabled: bool,
    label: &'static str,
    changed_count: u32,
}

impl State {
    fn new(label: &'static str) -> Self {
        Self {
            on: false,
            style: ToggleStyle::Switch,
            disabled: false,
            label,
            changed_count: 0,
        }
    }
}

fn app(ui: &mut egui::Ui, s: &mut State) {
    install(ui.ctx());
    let resp = Toggle::new(&mut s.on, s.label)
        .style(s.style)
        .disabled(s.disabled)
        .show(ui);
    if resp.response.changed() {
        s.changed_count += 1;
    }
}

fn make_harness(state: State) -> Harness<'static, State> {
    let mut h = egui_kittest::HarnessBuilder::default()
        .with_size(Vec2::new(300.0, 80.0))
        .with_step_dt(0.01)
        .build_ui_state(app, state);
    h.set_size(Vec2::new(300.0, 80.0));
    h
}

#[test]
fn click_toggles_value_on() {
    let mut harness = make_harness(State::new("Enable VAD"));
    harness.run();
    assert!(!harness.state().on);

    harness.get_by_label("Enable VAD").click();
    harness.run();

    assert!(harness.state().on, "click should flip false→true");
    assert_eq!(harness.state().changed_count, 1);
}

#[test]
fn second_click_toggles_value_off() {
    let mut harness = make_harness(State::new("Mute on join"));
    harness.run();
    harness.get_by_label("Mute on join").click();
    harness.run();
    assert!(harness.state().on);

    harness.get_by_label("Mute on join").click();
    harness.run();

    assert!(!harness.state().on, "second click should flip true→false");
    assert_eq!(harness.state().changed_count, 2);
}

#[test]
fn disabled_toggle_does_not_flip() {
    let mut state = State::new("Disabled");
    state.disabled = true;
    let mut harness = make_harness(state);
    harness.run();

    // get_by_label still finds the label; clicking should be a no-op
    // because Sense::hover() doesn't accept clicks.
    harness.get_by_label("Disabled").click();
    harness.run();

    assert!(!harness.state().on);
    assert_eq!(harness.state().changed_count, 0);
}

#[test]
fn checkbox_style_also_toggles() {
    let mut state = State::new("Push to talk");
    state.style = ToggleStyle::Checkbox;
    let mut harness = make_harness(state);
    harness.run();

    harness.get_by_label("Push to talk").click();
    harness.run();

    assert!(harness.state().on);
}

#[test]
fn label_text_is_accessible() {
    let mut harness = make_harness(State::new("Auto-deafen"));
    harness.run();
    let _ = harness.get_by_label("Auto-deafen");
}

#[test]
fn starts_in_initial_value() {
    let mut state = State::new("Already on");
    state.on = true;
    let mut harness = make_harness(state);
    harness.run();
    assert!(
        harness.state().on,
        "initial true should be preserved across first frame"
    );
}
