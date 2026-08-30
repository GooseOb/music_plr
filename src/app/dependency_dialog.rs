//! State for the startup "missing dependencies" dialog.
//!
//! `MusicPlayer` opens this dialog when [`crate::deps::detect_missing`] reports
//! something absent. The user checks which auto-installable deps to fetch
//! (default: all) and either installs them or discards. `Python3` is listed
//! but never auto-installable (it's an OS package), so it has no checkbox.

use std::collections::{HashMap, HashSet};

use crate::deps::DepKind;

/// Live status of a single dependency's install/delete operation, shared by the
/// startup dialog and the Settings view so progress survives navigation.
#[derive(Debug, Default)]
pub struct DepOpState {
    pub installing: bool,
    pub install_result: Option<Result<(), String>>,
    pub progress: (u64, u64),
    pub deleting: bool,
    pub delete_result: Option<Result<(), String>>,
}

#[derive(Debug, Default)]
pub struct DependencyDialog {
    /// Dependencies detected as missing at startup.
    pub missing: Vec<DepKind>,
    /// Dependencies available on the system but not managed by the app, shown in
    /// a separate "found on system" section; the user may opt to install a managed
    /// copy, unchecked by default.
    pub found: Vec<DepKind>,
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
    pub fn new(missing: Vec<DepKind>, found: Vec<DepKind>) -> Self {
        // Default-select every auto-installable missing dep. ytmusicapi is only
        // selectable when Python 3 is present (otherwise its install would
        // fail immediately). Found-on-system deps start unchecked: the user must
        // explicitly opt to install a managed copy.
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
            found,
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
