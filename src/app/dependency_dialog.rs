//! State for the startup "missing dependencies" dialog.
//!
//! `MusicPlayer` opens this dialog when [`crate::deps::detect_missing`] reports
//! something absent. The user checks which auto-installable deps to fetch
//! (default: all) and either installs them or discards. `Python3` is listed
//! but never auto-installable (it's an OS package), so it has no checkbox.

use std::collections::{HashMap, HashSet};

use crate::deps::DepKind;

#[derive(Debug, Default)]
pub struct DependencyDialog {
    /// Dependencies detected as missing at startup.
    pub missing: Vec<DepKind>,
    /// Auto-installable missing deps the user has checked (all checked by
    /// default); the install action fetches exactly these.
    pub selected: HashSet<DepKind>,
    /// Deps whose install thread is currently running.
    pub installing: HashSet<DepKind>,
    /// Deps that installed successfully.
    pub done: HashSet<DepKind>,
    /// Deps whose install failed, with the error message.
    pub errors: HashMap<DepKind, String>,
    /// Download progress (bytes fetched, total bytes) for deps currently
    /// installing, so the dialog can render a live progress bar.
    pub progress: HashMap<DepKind, (u64, u64)>,
}

impl DependencyDialog {
    pub fn new(missing: Vec<DepKind>) -> Self {
        // Default-select every auto-installable dep. ytmusicapi is only
        // selectable when Python 3 is present (otherwise its install would
        // fail immediately).
        let selected = missing
            .iter()
            .copied()
            .filter(|d| {
                d.auto_installable()
                    && (d != &DepKind::YtMusicApi || crate::deps::availability().python3)
            })
            .collect();
        Self {
            missing,
            selected,
            ..Default::default()
        }
    }

    /// Auto-installable deps the user selected that haven't been attempted
    /// yet (not installing, done, or errored).
    pub fn pending(&self) -> Vec<DepKind> {
        self.selected
            .iter()
            .copied()
            .filter(|d| {
                !self.installing.contains(d)
                    && !self.done.contains(d)
                    && !self.errors.contains_key(d)
            })
            .collect()
    }

    /// Whether every selected dep has finished (success or failure), so the
    /// install button can disable and the dialog can show completion.
    pub fn all_resolved(&self) -> bool {
        self.selected
            .iter()
            .all(|d| self.done.contains(d) || self.errors.contains_key(d))
    }
}
