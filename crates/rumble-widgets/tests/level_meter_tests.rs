//! Functional tests for `LevelMeter` via egui_kittest.
//!
//! These exercise the public API only: allocation, drag interaction, and
//! the no-panic contract at edge values. The internal coordinate math is
//! covered by unit tests inside `level_meter.rs`.

use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use rumble_widgets::{Axis, LevelMeter, ModernTheme, Theme, install_theme};

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

#[test]
fn allocates_min_size() {
    let mut harness = Harness::new_ui_state(
        |ui, rect: &mut Option<Rect>| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.5).min_size(Vec2::new(200.0, 16.0)).show(ui);
            *rect = Some(resp.response.rect);
        },
        None,
    );
    harness.run();
    let r = harness.state().expect("rect must be allocated");
    assert_eq!(r.width(), 200.0);
    assert_eq!(r.height(), 16.0);
}

#[test]
fn vertical_orientation_swaps_default_size() {
    let mut harness = Harness::new_ui_state(
        |ui, rect: &mut Option<Rect>| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.5).orientation(Axis::Vertical).show(ui);
            *rect = Some(resp.response.rect);
        },
        None,
    );
    harness.run();
    let r = harness.state().unwrap();
    assert!(
        r.height() > r.width(),
        "vertical default should be tall+thin, got {}x{}",
        r.width(),
        r.height(),
    );
}

#[test]
fn explicit_min_size_overrides_orientation_default() {
    // .min_size after .orientation must win.
    let mut harness = Harness::new_ui_state(
        |ui, rect: &mut Option<Rect>| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.5)
                .orientation(Axis::Vertical)
                .min_size(Vec2::new(40.0, 200.0))
                .show(ui);
            *rect = Some(resp.response.rect);
        },
        None,
    );
    harness.run();
    let r = harness.state().unwrap();
    assert_eq!(r.width(), 40.0);
    assert_eq!(r.height(), 200.0);
}

/// Edge values must not panic. We render levels at, below, and above the
/// `[0.0, 1.0]` clamp range, plus NaN, in independent harnesses. If any
/// of these panic the test fails.
#[test]
fn renders_edge_levels_without_panic() {
    for level in [0.0_f32, 1.0, -0.5, 1.5, f32::NAN] {
        let mut harness = Harness::new_ui_state(
            |ui, _: &mut ()| {
                install(ui.ctx());
                LevelMeter::new(level).peak(level).threshold(level).show(ui);
            },
            (),
        );
        harness.run();
    }
}

#[test]
fn non_interactive_drag_yields_no_threshold() {
    #[derive(Default)]
    struct State {
        rect: Option<Rect>,
        drag_count: u32,
    }
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.5).min_size(Vec2::new(200.0, 16.0)).show(ui);
            state.rect = Some(resp.response.rect);
            if resp.threshold_drag.is_some() {
                state.drag_count += 1;
            }
        },
        State::default(),
    );
    harness.run();
    let rect = harness.state().rect.unwrap();
    harness.hover_at(rect.center());
    harness.drag_at(rect.center());
    harness.run();
    assert_eq!(
        harness.state().drag_count,
        0,
        "non-interactive meter must never report threshold drag",
    );
}

#[test]
fn interactive_drag_returns_normalized_position() {
    #[derive(Default)]
    struct State {
        rect: Option<Rect>,
        last_drag: Option<f32>,
    }
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.0)
                .min_size(Vec2::new(200.0, 16.0))
                .interactive(true)
                .show(ui);
            state.rect = Some(resp.response.rect);
            if let Some(t) = resp.threshold_drag {
                state.last_drag = Some(t);
            }
        },
        State::default(),
    );
    harness.run();
    let rect = harness.state().rect.expect("rect must be allocated");

    // Inner area = rect shrunk by the theme's bevel_inset (matches the impl).
    let inner = rect.shrink(ModernTheme::default().tokens().bevel_inset);
    let target_x = inner.left() + inner.width() * 0.25;
    let target_y = inner.center().y;

    harness.hover_at(Pos2::new(target_x, target_y));
    harness.drag_at(Pos2::new(target_x, target_y));
    harness.run();

    let drag = harness
        .state()
        .last_drag
        .expect("interactive drag must report a threshold value");
    assert!(
        (drag - 0.25).abs() < 0.01,
        "expected ~0.25 from a drag at 25% of the inner width, got {drag}",
    );
}

/// Drag past the right edge should clamp to 1.0, past the left edge to 0.0.
#[test]
fn interactive_drag_clamps_outside_bar() {
    #[derive(Default)]
    struct State {
        rect: Option<Rect>,
        last_drag: Option<f32>,
    }
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            install(ui.ctx());
            let resp = LevelMeter::new(0.0)
                .min_size(Vec2::new(200.0, 16.0))
                .interactive(true)
                .show(ui);
            state.rect = Some(resp.response.rect);
            if let Some(t) = resp.threshold_drag {
                state.last_drag = Some(t);
            }
        },
        State::default(),
    );
    harness.run();
    let rect = harness.state().rect.unwrap();

    // Press inside the bar (so the drag target is the meter), then drag
    // far outside its right edge.
    harness.hover_at(rect.center());
    harness.drag_at(rect.center());
    harness.run();
    harness.hover_at(Pos2::new(rect.right() + 500.0, rect.center().y));
    harness.run();

    let drag = harness.state().last_drag.unwrap();
    assert!(
        (drag - 1.0).abs() < 1e-3,
        "drag past right edge must clamp to 1.0, got {drag}",
    );
}
