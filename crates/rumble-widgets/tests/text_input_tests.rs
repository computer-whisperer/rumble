//! Functional tests for `TextInput`.
//!
//! Pattern: state inside the harness via `new_ui_state`. Focus is taken by
//! clicking on the field with raw pointer events (kittest's `.click()` on
//! the wrapping rect can race with the inner `TextEdit`).

use std::sync::Arc;

use eframe::egui::{self, Key, Modifiers, PointerButton, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use rumble_widgets::{ModernTheme, TextInput, install_theme};

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

struct State {
    buf: String,
    multiline: bool,
    submit_on_enter: bool,
    placeholder: &'static str,
    submitted: Vec<String>,
    rect: Rect,
    focused: bool,
}

impl State {
    fn new() -> Self {
        Self {
            buf: String::new(),
            multiline: false,
            submit_on_enter: false,
            placeholder: "",
            submitted: Vec::new(),
            rect: Rect::ZERO,
            focused: false,
        }
    }
}

fn app(ui: &mut egui::Ui, s: &mut State) {
    install(ui.ctx());
    let resp = TextInput::new(&mut s.buf)
        .placeholder(s.placeholder)
        .multiline(s.multiline)
        .submit_on_enter(s.submit_on_enter)
        .desired_width(200.0)
        .show(ui);
    s.rect = resp.response.rect;
    s.focused = resp.response.has_focus();
    if let Some(text) = resp.submitted {
        s.submitted.push(text);
    }
}

fn make_harness(state: State) -> Harness<'static, State> {
    let mut h = egui_kittest::HarnessBuilder::default()
        .with_size(Vec2::new(400.0, 200.0))
        .with_step_dt(0.01)
        .build_ui_state(app, state);
    h.set_size(Vec2::new(400.0, 200.0));
    h
}

fn focus_field(h: &mut Harness<State>) {
    let pos = h.state().rect.center();
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
fn typing_updates_buffer() {
    let mut harness = make_harness(State::new());
    harness.run();
    focus_field(&mut harness);
    assert!(harness.state().focused, "click should focus the field");

    harness.event(egui::Event::Text("hello".into()));
    harness.run();

    assert_eq!(harness.state().buf, "hello");
}

#[test]
fn unfocused_text_event_is_ignored() {
    let mut harness = make_harness(State::new());
    harness.run();

    // Without focusing, sending text should not modify the buffer.
    harness.event(egui::Event::Text("nope".into()));
    harness.run();

    assert_eq!(harness.state().buf, "");
}

#[test]
fn enter_submits_and_clears_when_enabled() {
    let mut state = State::new();
    state.submit_on_enter = true;
    state.buf = String::from("hi");
    let mut harness = make_harness(state);
    harness.run();
    focus_field(&mut harness);

    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert_eq!(harness.state().submitted, vec![String::from("hi")]);
    assert_eq!(harness.state().buf, "", "buffer cleared on submit");
}

#[test]
fn enter_without_submit_on_enter_is_ignored_singleline() {
    // Single-line + submit_on_enter=false: Enter should not modify the
    // buffer (single-line TextEdit does not insert newlines).
    let mut state = State::new();
    state.buf = String::from("hi");
    let mut harness = make_harness(state);
    harness.run();
    focus_field(&mut harness);

    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert!(harness.state().submitted.is_empty());
    assert_eq!(harness.state().buf, "hi");
}

#[test]
fn shift_enter_inserts_newline_in_multiline() {
    let mut state = State::new();
    state.multiline = true;
    state.submit_on_enter = true;
    state.buf = String::from("a");
    let mut harness = make_harness(state);
    harness.run();
    focus_field(&mut harness);

    // Shift+Enter — should NOT submit; should insert a newline.
    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::SHIFT,
    });
    harness.run();

    assert!(harness.state().submitted.is_empty(), "Shift+Enter should not submit",);
    assert!(
        harness.state().buf.contains('\n'),
        "Shift+Enter should insert a newline; buf = {:?}",
        harness.state().buf,
    );
}

#[test]
fn multiline_submit_strips_trailing_newline() {
    // In multiline mode TextEdit inserts '\n' on every Enter (we only
    // *detect* unmodified Enter to submit). The widget must strip that
    // trailing newline before capturing.
    let mut state = State::new();
    state.multiline = true;
    state.submit_on_enter = true;
    state.buf = String::from("hello");
    let mut harness = make_harness(state);
    harness.run();
    focus_field(&mut harness);

    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert_eq!(harness.state().submitted, vec![String::from("hello")]);
    assert_eq!(harness.state().buf, "");
}

#[test]
fn typing_appends_to_existing_buffer() {
    let mut state = State::new();
    state.buf = String::from("foo");
    let mut harness = make_harness(state);
    harness.run();
    focus_field(&mut harness);

    // Singleline TextEdit places cursor at end on first focus
    // (cursor_at_end default). Typing should append.
    harness.event(egui::Event::Text("bar".into()));
    harness.run();

    assert_eq!(harness.state().buf, "foobar");
}

#[test]
fn allocates_visible_size() {
    let mut harness = make_harness(State::new());
    harness.run();
    let r = harness.state().rect;
    assert!(r.width() >= 200.0, "width >= desired_width; got {}", r.width());
    assert!(r.height() >= 18.0, "height > 0; got {}", r.height());
}

#[test]
fn placeholder_does_not_appear_in_buffer() {
    let mut state = State::new();
    state.placeholder = "Type a message…";
    let mut harness = make_harness(state);
    harness.run();
    assert_eq!(
        harness.state().buf,
        "",
        "placeholder text must not leak into the buffer",
    );
}

#[test]
fn multiline_allocates_more_height_than_singleline() {
    let single = make_harness(State::new());
    let mut multi_state = State::new();
    multi_state.multiline = true;
    let multi = make_harness(multi_state);

    let mut s = single;
    let mut m = multi;
    s.run();
    m.run();
    let _ = Pos2::new(0.0, 0.0);

    assert!(
        m.state().rect.height() > s.state().rect.height(),
        "multiline (default 3 rows) should allocate more height than single-line; singleline = {}, multiline = {}",
        s.state().rect.height(),
        m.state().rect.height(),
    );
}
