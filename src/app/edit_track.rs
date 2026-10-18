//! Mutable working copy of a track being edited in the track-editing popup.
//!
//! Holds only the text-editable fields (`source` is changed via the provider
//! "select" buttons) plus the original position so the edit can be written
//! back to the correct list.

use crate::{
    app::interaction::TrackPos,
    load_state::LoadState,
    providers::{ProviderId, SearchScope},
    types::Track,
};

#[derive(Debug, Clone)]
pub struct EditTrackState {
    pub title: String,
    pub artist: String,
    pub source: ProviderId,
    pub original: Track,
    pub pos: TrackPos,
    /// The provider whose "Find" action is currently in flight, if any.
    pub finding: Option<ProviderId>,
    /// The "Manual search" step, while it is open: it replaces the popup's
    /// body until it is confirmed or cancelled, so the working copy stays put
    /// underneath it.
    pub manual: Option<EditManualSearch>,
}

impl EditTrackState {
    /// Seed query for a provider search: the edited title/artist in the same
    /// shape as [`Track::search_query`], so the fields the user can see drive
    /// what the search asks for.
    pub fn search_query(&self) -> String {
        if self.artist.trim().is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title.trim(), self.artist.trim())
        }
    }
}

/// One provider search driven from the Edit Track popup: what it searches for,
/// the scope it runs in, its results, and the single result picked. The list
/// itself is addressed by
/// [`MANUAL_SEARCH_PANE_ID`](crate::app::pane::MANUAL_SEARCH_PANE_ID), so its
/// rows are ordinary track rows and its selection is a normal one-entry
/// selection.
#[derive(Debug, Clone)]
pub struct EditManualSearch {
    pub provider: ProviderId,
    pub scope: SearchScope,
    pub query: String,
    pub results: LoadState<Vec<Track>>,
    /// Indices picked in the result list. At most one is ever held (see
    /// `MusicPlayer::toggle_selection`), so the popup picks one track.
    pub selection: Vec<usize>,
    /// Id of the in-flight query; a reply carrying any other id answered a
    /// query the user has since replaced.
    pub request_id: u64,
}

impl EditManualSearch {
    pub fn new(provider: ProviderId, scope: SearchScope, query: String) -> Self {
        Self {
            provider,
            scope,
            query,
            results: LoadState::Ready(Vec::new()),
            selection: Vec::new(),
            request_id: 0,
        }
    }

    /// A query can start when one is asked for and none is in flight.
    pub fn can_run(&self) -> bool {
        !self.results.is_loading() && !self.query.trim().is_empty()
    }

    /// The results, while the list holds them (empty while loading or failed).
    pub fn tracks(&self) -> &[Track] {
        match &self.results {
            LoadState::Ready(tracks) => tracks,
            _ => &[],
        }
    }

    /// The result at `index`, while the list holds one there.
    pub fn track_at(&self, index: usize) -> Option<&Track> {
        match &self.results {
            LoadState::Ready(tracks) => tracks.get(index),
            _ => None,
        }
    }

    /// The picked result, while it is still in the result list.
    pub fn selected_track(&self) -> Option<&Track> {
        self.selection
            .first()
            .copied()
            .and_then(|i| self.track_at(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_search_picks_at_most_one_result() {
        let mut manual =
            EditManualSearch::new(ProviderId::YouTube, SearchScope::Songs, "query".into());
        let results = vec![
            Track::from_provider(
                ProviderId::YouTube,
                "a".into(),
                String::new(),
                "First",
                "Artist",
                0,
                String::new(),
                None,
                None,
            ),
            Track::from_provider(
                ProviderId::YouTube,
                "b".into(),
                String::new(),
                "Second",
                "Artist",
                0,
                String::new(),
                None,
                None,
            ),
        ];
        manual.results = LoadState::Ready(results);

        manual.selection = vec![1];
        assert_eq!(
            manual.selected_track().map(|t| t.title.as_str()),
            Some("Second")
        );
        // Picking another result replaces the previous pick.
        manual.selection = vec![0];
        assert_eq!(
            manual.selected_track().map(|t| t.title.as_str()),
            Some("First")
        );

        // A pick that no longer addresses a result resolves to nothing.
        manual.selection = vec![9];
        assert!(manual.selected_track().is_none());
    }

    #[test]
    fn query_joins_the_edited_fields() {
        let mut edit = EditTrackState {
            title: "Title".into(),
            artist: "Artist".into(),
            source: ProviderId::YouTube,
            original: Track::default(),
            pos: TrackPos::new(0, 0),
            finding: None,
            manual: None,
        };
        assert_eq!(edit.search_query(), "Title Artist");
        edit.artist = String::new();
        assert_eq!(edit.search_query(), "Title");
    }
}
