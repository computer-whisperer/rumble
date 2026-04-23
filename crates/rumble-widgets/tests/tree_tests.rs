//! Functional tests for `Tree`.
//!
//! Pattern: state lives inside the kittest harness (`new_ui_state`) so we
//! can read it between frames. Each frame the closure runs the tree,
//! captures the outer rect, and applies emitted events to the local
//! tree state — exactly what a real caller would do.

use std::sync::Arc;

use eframe::egui::{self, Key, Modifiers, PointerButton, Pos2, Rect, Vec2};
use egui_kittest::{Harness, kittest::Queryable};
use rumble_widgets::{
    DropEvent, DropPosition, ModernTheme, Tree, TreeNode, TreeNodeId, TreeResponse, UserState, install_theme,
};

fn install(ctx: &egui::Context) {
    install_theme(ctx, Arc::new(ModernTheme::default()));
}

const ROW_H: f32 = 28.0;

fn sample_tree() -> Vec<TreeNode> {
    vec![TreeNode::channel(1, "Lobby").with_children(vec![
        TreeNode::channel(2, "Music").with_children(vec![TreeNode::user(10, "alice", UserState::default())]),
        TreeNode::channel(3, "Gaming").with_children(vec![TreeNode::user(11, "bob", UserState::default())]),
    ])]
}

/// Aggregated events seen across all frames since reset. We accumulate
/// because `harness.run()` may step past the frame an event fires on,
/// and the final frame's response is empty.
#[derive(Default)]
struct EventLog {
    clicked: Vec<TreeNodeId>,
    double_clicked: Vec<TreeNodeId>,
    toggled: Vec<TreeNodeId>,
    activated: Vec<TreeNodeId>,
    dropped: Vec<DropEvent>,
    context: Vec<(TreeNodeId, Pos2)>,
    selection_changed: Vec<Option<TreeNodeId>>,
}

struct TestState {
    tree: Vec<TreeNode>,
    selected: Option<TreeNodeId>,
    log: EventLog,
    outer_rect: Rect,
    drag_drop: bool,
}

impl TestState {
    fn new() -> Self {
        Self {
            tree: sample_tree(),
            selected: None,
            log: EventLog::default(),
            outer_rect: Rect::ZERO,
            drag_drop: false,
        }
    }

    fn apply(&mut self, resp: TreeResponse) {
        if let Some(id) = resp.toggled {
            toggle_expanded(&mut self.tree, id);
            self.log.toggled.push(id);
        }
        if let Some(new_sel) = resp.selection_changed {
            self.selected = new_sel;
            self.log.selection_changed.push(new_sel);
        }
        if let Some(id) = resp.clicked {
            self.selected = Some(id);
            self.log.clicked.push(id);
        }
        if let Some(id) = resp.double_clicked {
            self.log.double_clicked.push(id);
        }
        if let Some(id) = resp.activated {
            self.log.activated.push(id);
        }
        if let Some(d) = resp.dropped {
            self.log.dropped.push(d);
        }
        if let Some(c) = resp.context {
            self.log.context.push(c);
        }
    }
}

fn toggle_expanded(nodes: &mut [TreeNode], id: TreeNodeId) -> bool {
    for n in nodes {
        if n.id == id {
            n.expanded = !n.expanded;
            return true;
        }
        if toggle_expanded(&mut n.children, id) {
            return true;
        }
    }
    false
}

/// One frame of the test app. Caller passes the closure to
/// `Harness::new_ui_state(_, TestState::new())`.
fn app(ui: &mut egui::Ui, state: &mut TestState) {
    install(ui.ctx());
    let resp = Tree::new("test_tree", &state.tree)
        .selected(state.selected)
        .drag_drop(state.drag_drop)
        .show(ui);
    if let Some(r) = &resp.response {
        state.outer_rect = r.rect;
    }
    state.apply(resp);
}

/// Vertical center of row `idx` within the outer tree rect, given a
/// fixed row height. The flattened sample tree (when fully expanded) is:
/// 0=Lobby, 1=Music, 2=alice, 3=Gaming, 4=bob.
fn row_center_y(outer: Rect, idx: usize) -> f32 {
    outer.top() + ROW_H * idx as f32 + ROW_H * 0.5
}

fn make_harness(state: TestState) -> Harness<'static, TestState> {
    // Tiny step_dt so harness.run()'s simulated time doesn't exceed
    // egui's default 0.3s double-click window across the multiple frames
    // it takes to process queued events.
    let mut h = egui_kittest::HarnessBuilder::default()
        .with_size(Vec2::new(400.0, 400.0))
        .with_step_dt(0.01)
        .build_ui_state(app, state);
    h.set_size(Vec2::new(400.0, 400.0));
    h
}

fn make_harness_dnd(mut state: TestState) -> Harness<'static, TestState> {
    state.drag_drop = true;
    make_harness(state)
}

#[test]
fn renders_visible_rows_only() {
    let mut state = TestState::new();
    toggle_expanded(&mut state.tree, 2); // collapse Music
    let mut harness = make_harness(state);
    harness.run();

    let _ = harness.get_by_label("Lobby");
    let _ = harness.get_by_label("Music");
    let _ = harness.get_by_label("Gaming");
    let _ = harness.get_by_label("bob");
    assert!(
        harness.query_by_label("alice").is_none(),
        "alice should be hidden when Music is collapsed",
    );
}

/// Press + release the primary pointer button at `pos` *without* firing
/// PointerGone (which kittest's `drop_at` does as a side effect). The
/// PointerGone tail can race with click detection on certain widgets.
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
fn click_emits_clicked_and_updates_selection() {
    let mut harness = make_harness(TestState::new());
    harness.run();

    let outer = harness.state().outer_rect;
    let y = row_center_y(outer, 1); // Music
    let x = outer.left() + 80.0;
    click_at(&mut harness, Pos2::new(x, y));

    assert_eq!(harness.state().log.clicked, vec![2]);
    assert_eq!(harness.state().selected, Some(2));
}

#[test]
fn caret_click_toggles_expanded_state() {
    let state = TestState::new();
    assert!(state.tree[0].expanded);
    let mut harness = make_harness(state);
    harness.run();

    let outer = harness.state().outer_rect;
    let y = row_center_y(outer, 0); // Lobby
    // Caret at depth 0: x = outer.left() + pad_sm (4) + caret_w/2 (~7).
    let caret_x = outer.left() + 4.0 + 7.0;
    click_at(&mut harness, Pos2::new(caret_x, y));

    assert_eq!(harness.state().log.toggled, vec![1], "caret on Lobby (id 1)");
    assert!(!harness.state().tree[0].expanded, "expanded flipped");
}

#[test]
fn arrow_down_moves_selection() {
    let mut state = TestState::new();
    state.selected = Some(1); // Lobby
    let mut harness = make_harness(state);
    harness.run();

    harness.key_press(Key::ArrowDown);
    harness.run();

    assert_eq!(
        harness.state().selected,
        Some(2),
        "ArrowDown from Lobby (1) → Music (2)",
    );
}

#[test]
fn enter_activates_selection() {
    let mut state = TestState::new();
    state.selected = Some(2); // Music
    let mut harness = make_harness(state);
    harness.run();

    harness.key_press(Key::Enter);
    harness.run();

    assert_eq!(harness.state().log.activated, vec![2]);
}

#[test]
fn double_click_emits_double_clicked() {
    let mut harness = make_harness(TestState::new());
    harness.run();

    let outer = harness.state().outer_rect;
    let y = row_center_y(outer, 4); // bob
    let x = outer.left() + 80.0;
    let pos = Pos2::new(x, y);

    // Fire press/release × 2 with a hover_at first, all queued before
    // the next frame, so egui sees them within one double-click window.
    harness.hover_at(pos);
    for _ in 0..2 {
        harness.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();

    assert_eq!(harness.state().log.double_clicked, vec![11]);
}

#[test]
fn drag_user_to_channel_emits_dropped_into() {
    // Drag-drop tests require drag_drop=true, but the default
    // TestState now uses false (set in `app`). Override per-test.
    let mut harness = make_harness_dnd(TestState::new());
    harness.run();

    let outer = harness.state().outer_rect;
    // alice (row 2) → Gaming (row 3). Middle band of Gaming = Into.
    let src = Pos2::new(outer.left() + 80.0, row_center_y(outer, 2));
    let dst = Pos2::new(outer.left() + 80.0, row_center_y(outer, 3));

    harness.hover_at(src);
    harness.event(egui::Event::PointerButton {
        pos: src,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    harness.hover_at(dst);
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos: dst,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert_eq!(
        harness.state().log.dropped,
        vec![DropEvent {
            source: 10,
            target: 3,
            position: DropPosition::Into,
        }],
    );
}

#[test]
fn drop_on_self_yields_no_event() {
    let mut harness = make_harness_dnd(TestState::new());
    harness.run();

    let outer = harness.state().outer_rect;
    let pos = Pos2::new(outer.left() + 80.0, row_center_y(outer, 4));
    harness.hover_at(pos);
    harness.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    assert!(harness.state().log.dropped.is_empty());
}

#[test]
fn empty_tree_yields_none_response() {
    let empty: Vec<TreeNode> = Vec::new();
    let mut harness = Harness::new_ui_state(
        |ui, captured: &mut bool| {
            install(ui.ctx());
            let resp = Tree::new("empty", &empty).show(ui);
            *captured = resp.response.is_some();
        },
        true,
    );
    harness.run();
    assert!(!*harness.state(), "empty tree → response None");
}
