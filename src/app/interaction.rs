//! Mouse, drag, and context-menu interaction state.

use iced::Point;

#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
    pub track_index: usize,
    pub position: (f32, f32),
    pub is_youtube: bool,
    pub is_downloaded: bool,
    pub in_playlist: bool,
    pub is_queue: bool,
    /// Resolved target indices for selection-aware operations: all selected
    /// indices if the right-clicked track is selected, otherwise just
    /// `[track_index]`.
    pub target_indices: Vec<usize>,
}

/// Mouse and drag interaction state, grouped for clarity.
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed_track: Option<usize>,
    pub pressed_track_is_queue: bool,
    pub hovered_track: Option<(usize, bool)>,
    pub drag_origin: Option<Point>,
    pub drag_active: bool,
    pub drag_drop_target: Option<usize>,
    /// Which list the cursor is currently hovering over during a drag.
    /// `None` means no list is targeted (e.g. hovering over the sidebar).
    /// `Some(DragTargetList::Queue)` when over the queue's up-next list.
    /// `Some(DragTargetList::TrackList)` when over the main track list.
    pub drag_target_list: Option<DragTargetList>,
    pub sidebar_hover_playlist: Option<usize>,
}

impl DragState {
    pub(crate) const fn cleanup(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed_track = None;
        self.drag_drop_target = None;
        self.drag_target_list = None;
        self.sidebar_hover_playlist = None;
    }
}

/// Identifies which track list a drag is currently hovering over.
/// Used to distinguish same-list reordering from cross-list copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTargetList {
    TrackList,
    Queue,
}
