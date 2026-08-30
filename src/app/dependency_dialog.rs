//! State for the startup "missing dependencies" dialog.
//!
//! `MusicPlayer` opens this dialog when [`crate::deps::detect_missing`] reports
//! something absent. The user checks which auto-installable deps to fetch
//! (default: all) and either installs them or discards. `Python3` is listed
//! but never auto-installable (it's an OS package), so it has no checkbox.

use std::collections::{HashMap, HashSet};

use crate::{deps::DepKind, i18n::Strings};

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
        }
    }

    /// Auto-installable deps the user selected that haven't been attempted
    /// yet, read from the shared `ops` map (the single source of truth for
    /// per-dependency install state).
    pub fn pending(&self, ops: &HashMap<DepKind, DepOpState>) -> Vec<DepKind> {
        self.selected
            .iter()
            .copied()
            .filter(|d| {
                let op = ops.get(d);
                !(op.is_some_and(|o| o.installing)
                    || op.is_some_and(|o| o.install_result.is_some()))
            })
            .collect()
    }

    /// Whether every selected dep has finished (success or failure), so the
    /// install button can disable and the dialog can show completion.
    pub fn all_resolved(&self, ops: &HashMap<DepKind, DepOpState>) -> bool {
        self.selected
            .iter()
            .all(|d| ops.get(d).is_some_and(|o| o.install_result.is_some()))
    }
}

/// Description line for a dependency kind, shown in both the startup dialog and
/// the Settings Dependencies section. Single source for the `DepKind` → text
/// mapping so the two views can't drift apart.
pub fn dep_desc(tr: &Strings, kind: DepKind) -> &'static str {
    match kind {
        DepKind::YtDlp => tr.deps_yt_dlp_desc,
        DepKind::YtMusicApi => tr.deps_ytmusicapi_desc,
        DepKind::Python3 => tr.deps_python3_desc,
    }
}
