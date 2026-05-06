//! Regression test for the egui 0.33 `hit_test.rs:364` panic.
//!
//! Upstream egui filters NaN out of `WidgetRect.interact_rect` before
//! running `hit_test_on_close`, but it does NOT filter NaN out of the
//! `rect` field. Because `WidgetRect` derives `PartialEq` over every
//! field, a widget whose `rect` contains NaN compares unequal even to
//! itself, so `close.iter().position(|w| *w == hit_click).unwrap()` at
//! `hit_test.rs:364` panics. This test would panic against vanilla
//! egui 0.33.3; we depend on the patched copy in `vendor/egui/` which
//! adds a `rect.any_nan()` filter alongside the existing
//! `interact_rect` one.
//!
//! Upstream issue: https://github.com/emilk/egui/issues/7870
//!
//! When the upstream fix lands, drop the `[patch.crates-io]` block in
//! the workspace `Cargo.toml` and remove `vendor/egui/`. This test
//! should keep passing (the upstream fix is the same idea).

#![cfg(feature = "test-harness")]

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Sense};
use egui_kittest::Harness;

#[test]
fn nan_rect_widgets_do_not_panic_hit_test() {
    // Register a clickable widget whose `rect` contains NaN by calling
    // `ui.interact(...)` directly with a NaN rect. egui's `interact`
    // clips the `interact_rect` against the parent's (finite) clip
    // rect, and `Rect::intersect` propagates NaN-vs-finite as finite
    // (`f32::max(NaN, 1.0) == 1.0` per IEEE-754 / Rust semantics). So
    // the resulting WidgetRect has `rect` = NaN but `interact_rect` =
    // finite — exactly the state hit_test's NaN filter (which only
    // checks `interact_rect`) misses.
    //
    // The ScrollArea is here to guarantee a `Sense::drag()` widget
    // also lives under the click; the panic only fires from the
    // `(Some(hit_click), Some(hit_drag))` branch at hit_test.rs:362.
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(400.0, 400.0))
        .build(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for i in 0..50 {
                        ui.label(format!("filler {i}"));
                    }
                    // Plant a finite click target so we have something
                    // for the test to aim at; this is the rect we'll
                    // synthesize the click on.
                    let finite_rect = egui::Rect::from_min_size(egui::pos2(50.0, 200.0), egui::vec2(100.0, 30.0));
                    let id = egui::Id::new("nan_target");
                    let nan_rect = egui::Rect::from_min_max(
                        egui::pos2(f32::NAN, finite_rect.min.y),
                        egui::pos2(f32::NAN, finite_rect.max.y),
                    );
                    let _resp_a = ui.interact(finite_rect, id, Sense::click());
                    // Re-register the same id with a NaN rect: the
                    // `WidgetRects::insert` code path overwrites the
                    // stored rect, so this widget ends up with rect=NaN
                    // (and interact_rect partially clipped by clip_rect,
                    // which sanitises NaN into finite values via
                    // `f32::max`). Bypasses the layout-time debug_assert
                    // that catches NaN in `allocate_space`.
                    let _resp_b = ui.interact(nan_rect, id, Sense::click());
                });
            });
        });

    // Settle a frame so the widgets are in `prev_pass.widgets`.
    harness.step();

    // Click at the center of the (finite) interact_rect of the NaN
    // widget. This sets up the (Some(hit_click), Some(hit_drag)) case.
    let pos = Pos2::new(100.0, 215.0);
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}
