//! Password-masked variant of aetna's `text_input`.
//!
//! aetna's stock `text_input` is plaintext-only. Here we render a
//! same-char-count mask string ('•' per real char) and let aetna's stock
//! event handler mutate the real string. Selection and caret positions
//! stay in real-value byte coordinates, so the bullet rendering may be
//! visually off by a fraction of a pixel for non-ASCII passwords — fine
//! for ASCII, which is what passwords overwhelmingly are.

use aetna_core::prelude::*;

/// Render a password field with `value` masked as bullets.
pub fn password_input(value: &str, selection: TextSelection) -> El {
    let masked: String = "•".repeat(value.chars().count());
    // aetna's text_input takes selection in byte coordinates over the
    // value it's given. We render the masked string but want the caret
    // to land where the user-typed character would be, so remap real
    // byte indices → masked byte indices via char index.
    let masked_sel = remap_to_masked(value, selection);
    text_input(&masked, masked_sel)
}

/// Apply a routed `UiEvent` to a password field. Mirrors
/// `text_input::apply_event` but on the *real* value, with selection in
/// real-value byte coordinates.
pub fn apply_event(value: &mut String, selection: &mut TextSelection, event: &UiEvent) -> bool {
    text_input::apply_event(value, selection, event)
}

fn remap_to_masked(value: &str, selection: TextSelection) -> TextSelection {
    let head_chars = byte_to_char(value, selection.head);
    let anchor_chars = byte_to_char(value, selection.anchor);
    // Each '•' is 3 bytes; char index N maps to byte offset 3*N.
    TextSelection {
        head: head_chars * 3,
        anchor: anchor_chars * 3,
    }
}

fn byte_to_char(value: &str, byte_idx: usize) -> usize {
    if byte_idx >= value.len() {
        return value.chars().count();
    }
    value
        .char_indices()
        .position(|(i, _)| i >= byte_idx)
        .unwrap_or(value.chars().count())
}
