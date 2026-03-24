//! Action event extraction and hit testing for UiDrawCmd::Action regions.

use crate::types::UiDrawCmd;

/// An action event fired when a user clicks an action region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEvent {
    /// Action name from the string table (e.g. "passkey_login").
    pub action: String,
    /// Bounding box of the action region in logical coordinates.
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Extract action regions from draw commands, resolving names from the string table.
pub fn collect_actions(draws: &[UiDrawCmd], string_table: &[String]) -> Vec<ActionEvent> {
    draws
        .iter()
        .filter_map(|cmd| {
            if let UiDrawCmd::Action { x, y, w, h, str_idx } = cmd {
                let action = string_table.get(*str_idx as usize)?.clone();
                Some(ActionEvent {
                    action,
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Hit-test a point against action regions, returning the first matching action.
pub fn hit_test_action(
    actions: &[ActionEvent],
    px: i32,
    py: i32,
) -> Option<&ActionEvent> {
    actions.iter().rev().find(|a| {
        px >= a.x && px < a.x + a.w as i32 && py >= a.y && py < a.y + a.h as i32
    })
}
