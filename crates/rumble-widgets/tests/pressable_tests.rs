//! Functional tests for `Pressable` / `button` via egui_kittest.
//!
//! These are headless — no wgpu, no window. They drive the UI through
//! kittest's accesskit-backed queries and assert on app state.

use std::sync::Arc;

use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use rumble_widgets::{ButtonArgs, ModernTheme, Pressable, PressableRole, install_theme};

#[derive(Default)]
struct ClickState {
    clicked_default: u32,
    clicked_danger: u32,
}

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

/// Isolation: verify allocate_exact_size + Sense::click alone registers clicks.
#[test]
fn raw_allocate_exact_size_with_click_sense_fires() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut u32| {
            let (_rect, resp) = ui.allocate_exact_size(egui::Vec2::new(100.0, 30.0), egui::Sense::click());
            // Paint something so kittest has a non-empty widget to find, and
            // label it for accesskit lookup.
            ui.painter().rect_filled(resp.rect, 0.0, egui::Color32::GRAY);
            let label_resp = ui.put(resp.rect, egui::Label::new("raw-btn").selectable(false));
            let _ = label_resp;
            if resp.clicked() {
                *state += 1;
            }
        },
        0u32,
    );

    harness.run();
    harness.get_by_label("raw-btn").click();
    harness.run();

    assert_eq!(
        *harness.state(),
        1,
        "allocate_exact_size+click should register a click when label is placed at the same rect via ui.put",
    );
}

/// Control: sanity check that kittest itself works on egui's built-in button.
#[test]
fn builtin_egui_button_fires_on_click() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut u32| {
            if ui.button("Built-in").clicked() {
                *state += 1;
            }
        },
        0u32,
    );

    harness.run();
    harness.get_by_label("Built-in").click();
    harness.run();

    assert_eq!(
        *harness.state(),
        1,
        "sanity: built-in egui button should register a click"
    );
}

#[test]
fn button_fires_on_click() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut ClickState| {
            install(ui.ctx());
            if ButtonArgs::new("Click me").show(ui).clicked() {
                state.clicked_default += 1;
            }
        },
        ClickState::default(),
    );

    harness.run();
    harness.get_by_label("Click me").click();
    harness.run();

    assert_eq!(
        harness.state().clicked_default,
        1,
        "Pressable should fire .clicked() when clicked via kittest"
    );
}

#[test]
fn disabled_button_does_not_fire() {
    // Render BOTH an enabled and a disabled button, then click the disabled
    // one. This proves: (1) the disabled button is renderable / findable, so
    // the assertion isn't trivially satisfied by "label not found", and
    // (2) clicking only the disabled one leaves both counters correct.
    #[derive(Default)]
    struct State {
        enabled_clicks: u32,
        disabled_clicks: u32,
    }
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            if ButtonArgs::new("Enabled").show(ui).clicked() {
                state.enabled_clicks += 1;
            }
            if ButtonArgs::new("Disabled").disabled(true).show(ui).clicked() {
                state.disabled_clicks += 1;
            }
        },
        State::default(),
    );

    harness.run();
    // Sanity: both labels exist (panics if not).
    let _ = harness.get_by_label("Enabled");
    let _ = harness.get_by_label("Disabled");
    harness.get_by_label("Disabled").click();
    harness.run();

    assert_eq!(
        harness.state().disabled_clicks,
        0,
        "Disabled Pressable must not fire .clicked()"
    );
    assert_eq!(
        harness.state().enabled_clicks,
        0,
        "Control: enabled sibling should not have been clicked either"
    );
}

#[test]
fn danger_role_click_fires() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut ClickState| {
            install(ui.ctx());
            if ButtonArgs::new("Kick").role(PressableRole::Danger).show(ui).clicked() {
                state.clicked_danger += 1;
            }
        },
        ClickState::default(),
    );

    harness.run();
    harness.get_by_label("Kick").click();
    harness.run();

    assert_eq!(
        harness.state().clicked_danger,
        1,
        "Danger-role button should still report clicks"
    );
}

/// Click a toggle button twice: `active` flips on the first click and back on
/// the second. Covers the PTT/Mute/Deafen usage pattern from the gallery.
#[test]
fn toggle_round_trips_across_two_clicks() {
    #[derive(Default)]
    struct State {
        on: bool,
        clicks: u32,
    }

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            if ButtonArgs::new("PTT")
                .role(PressableRole::Accent)
                .active(state.on)
                .show(ui)
                .clicked()
            {
                state.on = !state.on;
                state.clicks += 1;
            }
        },
        State::default(),
    );

    harness.run();
    assert!(!harness.state().on, "initial state must be off");

    harness.get_by_label("PTT").click();
    harness.run();
    assert!(harness.state().on, "after 1 click, toggle must be on");

    harness.get_by_label("PTT").click();
    harness.run();
    assert!(!harness.state().on, "after 2 clicks, toggle must be back off");

    assert_eq!(harness.state().clicks, 2, "exactly two clicks registered");
}

/// Regression test: the Pressable's content ui must have selectable_labels
/// disabled, so `ui.label()` inside doesn't eat the button's click with its
/// text-selection drag sense. If this assertion ever flips, the
/// `button_fires_on_click` family of tests would silently fail with the
/// same "clicks don't register" bug we just fixed.
#[test]
fn pressable_disables_selectable_labels_in_content() {
    // Seed state to the opposite of what we expect, so the assertion can't
    // be satisfied by the state default.
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut bool| {
            install(ui.ctx());
            Pressable::new("reg")
                .min_size(egui::Vec2::new(80.0, 24.0))
                .show(ui, |inner| {
                    *state = inner.style().interaction.selectable_labels;
                    inner.label("inside");
                });
        },
        true,
    );

    harness.run();

    assert!(
        !*harness.state(),
        "Pressable must set interaction.selectable_labels=false so labels don't consume clicks for text selection"
    );
}

/// Tab cycles through Pressables in layout order, and Space on a focused
/// Pressable fires a click.
#[test]
fn tab_focus_then_space_fires_click() {
    #[derive(Default)]
    struct State {
        a: u32,
        b: u32,
        c: u32,
    }

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            if ButtonArgs::new("A").show(ui).clicked() {
                state.a += 1;
            }
            if ButtonArgs::new("B").show(ui).clicked() {
                state.b += 1;
            }
            if ButtonArgs::new("C").show(ui).clicked() {
                state.c += 1;
            }
        },
        State::default(),
    );

    // First frame: register widgets. No focus yet.
    harness.run();

    // Two Tabs should land focus on the second button.
    harness.key_press(egui::Key::Tab);
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();

    // Space on a focused Pressable should fire its click.
    harness.key_press(egui::Key::Space);
    harness.run();

    let s = harness.state();
    assert_eq!(s.a, 0, "A should not have fired");
    assert_eq!(s.b, 1, "Tab-Tab-Space should fire B exactly once");
    assert_eq!(s.c, 0, "C should not have fired");
}

#[test]
fn pressable_custom_content_fires_on_click() {
    // Pressable with a custom content closure (not the ButtonArgs convenience).
    // This exercises the primitive directly, bypassing galley measurement.
    #[derive(Default)]
    struct State {
        clicks: u32,
    }

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            let resp = Pressable::new("custom")
                .min_size(egui::Vec2::new(120.0, 32.0))
                .show(ui, |ui| {
                    ui.label("Custom PTT");
                })
                .response;
            if resp.clicked() {
                state.clicks += 1;
            }
        },
        State::default(),
    );

    harness.run();
    harness.get_by_label("Custom PTT").click();
    harness.run();

    assert_eq!(
        harness.state().clicks,
        1,
        "Pressable primitive should fire .clicked() on click"
    );
}
