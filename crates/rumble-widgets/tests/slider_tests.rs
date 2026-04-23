//! Functional tests for `Slider`.
//!
//! Pattern mirrors `tree_tests.rs`: state lives inside the kittest harness
//! via `new_ui_state`, so we can read it back between frames.
//! `Sense::click_and_drag` interferes with kittest's accesskit-driven
//! `.click()`, so we fire raw `PointerButton` / `PointerMoved` events.

use std::sync::Arc;

use eframe::egui::{self, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Vec2};
use egui_kittest::Harness;
use rumble_widgets::{ModernTheme, Slider, install_theme};

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

struct TestState {
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: Option<f32>,
    show_value: bool,
    width: f32,
    outer_rect: Rect,
    changed_count: u32,
}

impl TestState {
    fn new(value: f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: None,
            show_value: true,
            width: 200.0,
            outer_rect: Rect::ZERO,
            changed_count: 0,
        }
    }
}

fn app(ui: &mut egui::Ui, state: &mut TestState) {
    install(ui.ctx());
    let mut s = Slider::new(&mut state.value, state.range.clone())
        .width(state.width)
        .show_value(state.show_value);
    if let Some(step) = state.step {
        s = s.step(step);
    }
    let resp = s.show(ui);
    state.outer_rect = resp.response.rect;
    if resp.response.changed() {
        state.changed_count += 1;
    }
}

fn make_harness(state: TestState) -> Harness<'static, TestState> {
    let mut h = egui_kittest::HarnessBuilder::default()
        .with_size(Vec2::new(400.0, 100.0))
        .with_step_dt(0.01)
        .build_ui_state(app, state);
    h.set_size(Vec2::new(400.0, 100.0));
    h
}

/// The slider widget allocates `[width + 8 + value_box_width] x 18`. The
/// active track (where the thumb can sit) is inset by `THUMB_DIAMETER/2 = 7`
/// on each side, occupying the leftmost `width` portion. This converts a
/// normalized `[0,1]` position into a screen X inside the active track.
fn track_x(outer: Rect, width: f32, t: f32) -> f32 {
    let track_left = outer.left() + 7.0;
    let track_right = outer.left() + width - 7.0;
    track_left + (track_right - track_left) * t
}

fn track_y(outer: Rect) -> f32 {
    outer.center().y
}

/// Press + release the primary pointer at `pos`. Mirrors the `click_at`
/// helper in `tree_tests.rs` — avoids kittest's PointerGone tail which
/// can race click detection.
fn click_at(h: &mut Harness<TestState>, pos: Pos2) {
    h.hover_at(pos);
    h.run();
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[test]
fn click_jumps_value_to_position() {
    let mut harness = make_harness(TestState::new(0.0, 0.0..=10.0));
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    // Click at the midpoint of the active track → value 5.0.
    let pos = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    click_at(&mut harness, pos);

    let v = harness.state().value;
    assert!(
        (v - 5.0).abs() < 0.2,
        "click at midpoint should set value ≈ 5.0, got {v}",
    );
    assert!(
        harness.state().changed_count > 0,
        "Response.changed() should have fired"
    );
}

#[test]
fn click_at_left_end_sets_min_value() {
    let mut harness = make_harness(TestState::new(5.0, 0.0..=10.0));
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.0), track_y(outer));
    click_at(&mut harness, pos);

    assert_eq!(harness.state().value, 0.0);
}

#[test]
fn click_at_right_end_sets_max_value() {
    let mut harness = make_harness(TestState::new(5.0, 0.0..=10.0));
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 1.0), track_y(outer));
    click_at(&mut harness, pos);

    assert_eq!(harness.state().value, 10.0);
}

#[test]
fn drag_updates_value_continuously() {
    let mut harness = make_harness(TestState::new(0.0, 0.0..=10.0));
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let start = Pos2::new(track_x(outer, width, 0.0), track_y(outer));
    let mid = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    let end = Pos2::new(track_x(outer, width, 0.8), track_y(outer));

    harness.hover_at(start);
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    harness.hover_at(mid);
    harness.run();
    let mid_v = harness.state().value;
    harness.hover_at(end);
    harness.run();
    let end_v = harness.state().value;
    harness.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert!((mid_v - 5.0).abs() < 0.3, "mid drag ≈ 5.0, got {mid_v}");
    assert!((end_v - 8.0).abs() < 0.3, "end drag ≈ 8.0, got {end_v}");
}

#[test]
fn step_snapping_rounds_value() {
    let mut state = TestState::new(0.0, 0.0..=10.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    // Click at ~0.53 (between 5 and 6) → should snap to 5.
    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.53), track_y(outer));
    click_at(&mut harness, pos);

    let v = harness.state().value;
    assert_eq!(v, 5.0, "step=1.0 should snap to integer; got {v}");
}

#[test]
fn arrow_right_increments_by_step_when_focused() {
    let mut state = TestState::new(5.0, 0.0..=10.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    // Click on the slider at the current value's position to give it focus
    // *without* changing the value.
    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    click_at(&mut harness, pos);
    assert_eq!(harness.state().value, 5.0, "click at midpoint preserves value");

    harness.key_press(Key::ArrowRight);
    harness.run();

    assert_eq!(harness.state().value, 6.0, "ArrowRight: 5 + 1 = 6");
}

#[test]
fn arrow_left_decrements_by_step() {
    let mut state = TestState::new(5.0, 0.0..=10.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    click_at(&mut harness, pos);

    harness.key_press(Key::ArrowLeft);
    harness.run();

    assert_eq!(harness.state().value, 4.0);
}

#[test]
fn home_jumps_to_min_end_jumps_to_max() {
    let mut state = TestState::new(5.0, 0.0..=10.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    click_at(&mut harness, pos);

    harness.key_press(Key::Home);
    harness.run();
    assert_eq!(harness.state().value, 0.0, "Home jumps to range start");

    harness.key_press(Key::End);
    harness.run();
    assert_eq!(harness.state().value, 10.0, "End jumps to range end");
}

#[test]
fn arrow_keys_clamp_at_range_bounds() {
    // Start at min; ArrowLeft should not go below.
    let mut state = TestState::new(0.0, 0.0..=10.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.0), track_y(outer));
    click_at(&mut harness, pos);

    harness.key_press(Key::ArrowLeft);
    harness.run();
    assert_eq!(harness.state().value, 0.0, "should not underflow min");
}

#[test]
fn negative_range_increments_correctly() {
    // -20..=20 with step 1 — common for dB gain.
    let mut state = TestState::new(-5.0, -20.0..=20.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    // Click at value's normalized position: (-5 - -20) / 40 = 0.375.
    let pos = Pos2::new(track_x(outer, width, 0.375), track_y(outer));
    click_at(&mut harness, pos);
    let v = harness.state().value;
    assert!((v - -5.0).abs() < 1.0, "click at 0.375 norm ≈ -5.0, got {v}");

    harness.key_press(Key::ArrowRight);
    harness.run();
    let after = harness.state().value;
    assert_eq!(after, v + 1.0, "ArrowRight steps by +1");
}

#[test]
fn page_up_steps_by_ten_steps() {
    let mut state = TestState::new(0.0, 0.0..=100.0);
    state.step = Some(1.0);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let width = harness.state().width;
    let pos = Pos2::new(track_x(outer, width, 0.5), track_y(outer));
    click_at(&mut harness, pos);
    let mid = harness.state().value;

    harness.key_press(Key::PageUp);
    harness.run();
    assert_eq!(harness.state().value, mid + 10.0);
}

#[test]
fn no_focus_means_keys_are_ignored() {
    // Allocate a focusable filler before the slider and grab focus there;
    // the slider must not respond to keys it doesn't own.
    struct State {
        value: f32,
    }
    let mut state = State { value: 5.0 };
    let mut harness = Harness::new_ui_state(
        |ui: &mut egui::Ui, s: &mut State| {
            install(ui.ctx());
            let (_, dummy) = ui.allocate_exact_size(Vec2::new(40.0, 18.0), Sense::click());
            dummy.request_focus();
            Slider::new(&mut s.value, 0.0..=10.0).step(1.0).show(ui);
        },
        state,
    );
    harness.run();
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(harness.state().value, 5.0, "unfocused slider ignores keys");
    state = State { value: 0.0 };
    let _ = state; // silence move warning
}
