//! The track-editing popup: the working copy of the track being edited, its
//! automatic provider "Find", and the manual per-provider search the "Manual
//! search" buttons open.

use std::thread;

use iced::{widget::operation, Task};

use super::{BackendResult, Message, MusicPlayer};
use crate::{
    app::{
        dialog::Dialog, interaction::TrackPos, pane::MANUAL_SEARCH_PANE_ID,
        ui::edit_manual_search_input_id, EditManualSearch, EditTrackState,
    },
    data::JsonStore,
    load_state::LoadState,
    providers::{ProviderId, SearchScope},
    types::Track,
};

impl MusicPlayer {
    fn edit_dialog_mut(&mut self) -> Option<&mut EditTrackState> {
        match &mut self.dialog {
            Some(Dialog::Edit(edit)) => Some(edit),
            _ => None,
        }
    }

    /// Whether the Edit Track popup is currently showing its manual-search
    /// step (so Escape backs out of it instead of closing the popup).
    pub fn edit_manual_open(&self) -> bool {
        matches!(&self.dialog, Some(Dialog::Edit(edit)) if edit.manual.is_some())
    }

    /// Open the track-editing popup for the track at `pos`, seeding the
    /// working copy from the live track. Only one track is edited at a time
    /// (the right-clicked one), so multi-selection is ignored here.
    pub fn open_edit_track(&mut self, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        self.dialog = Some(Dialog::Edit(EditTrackState {
            title: track.title.clone(),
            artist: track.artist.clone(),
            source: track.source,
            original: track,
            pos,
            finding: None,
            manual: None,
        }));
    }

    /// Resolve an unresolved provider for the track being edited: the "Find"
    /// button in the Edit Track popup. Searches the provider for the track's
    /// title/artist and, on success, merges the resolved identity into the
    /// working copy so it can later be selected as the source.
    pub fn handle_edit_track_find_provider(&mut self, provider: ProviderId) {
        let Some(Dialog::Edit(edit)) = &mut self.dialog else {
            return;
        };
        if edit.finding.is_some() {
            return;
        }
        edit.finding = Some(provider);
        let original = edit.original.clone();
        let tx = self.result_tx.clone();
        thread::spawn(move || {
            let msg = match crate::providers::resolve_id(provider, &original) {
                Ok(resolved) => {
                    crate::app::BackendResult::EditTrackProviderResolved(provider, resolved)
                }
                Err(e) => {
                    crate::app::BackendResult::EditTrackProviderError(provider, e.to_string())
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// Merge a resolved provider identity (from the Edit Track "Find" action)
    /// into the working copy. `None` means no match was found; `Some(track)`
    /// carries that provider's identity (and its rich metadata).
    pub fn apply_edit_track_provider_resolution(
        &mut self,
        provider: ProviderId,
        resolved: Option<Track>,
    ) {
        let found_on = self.strings.found_on;
        let could_not_find_on = self.strings.could_not_find_on;
        {
            let Some(Dialog::Edit(edit)) = &mut self.dialog else {
                return;
            };
            edit.finding = None;
            if let Some(track) = resolved {
                if let Some(pt) = track.providers.get(&provider) {
                    edit.original.set_provider(provider, pt.clone());
                }
            } else {
                let msg = could_not_find_on(&edit.title, provider.label());
                self.notify_error(msg);
                return;
            }
        }
        self.notify(found_on(provider.label()));
    }

    /// Open the manual-search step for `provider`, seeded with the track's
    /// edited title/artist and the provider's first track scope, and run that
    /// seed query straight away so the list has something to pick.
    pub fn open_edit_manual_search(&mut self, provider: ProviderId) -> Task<Message> {
        let Some(edit) = self.edit_dialog_mut() else {
            return Task::none();
        };
        let scope = provider
            .track_scopes()
            .first()
            .copied()
            .unwrap_or(SearchScope::Songs);
        edit.manual = Some(EditManualSearch::new(provider, scope, edit.search_query()));
        self.drag.clear_hovered_track_in(MANUAL_SEARCH_PANE_ID);
        self.handle_edit_manual_run();
        operation::focus::<Message>(edit_manual_search_input_id())
    }

    /// The manual-search query was edited.
    pub fn handle_edit_manual_query(&mut self, value: String) {
        if let Some(manual) = self.edit_manual_mut() {
            manual.query = value;
        }
    }

    /// Switch the manual search's scope. Only scopes the provider returns tracks
    /// for are accepted; the previous scope's results no longer describe the
    /// search, so they go. Returns whether the switch happened, which is the
    /// caller's cue to run the query again in the new scope (as the search
    /// bar's scope chips re-run their search).
    pub fn handle_edit_manual_scope(&mut self, scope: SearchScope) -> bool {
        {
            let Some(manual) = self.edit_manual_mut() else {
                return false;
            };
            if scope == manual.scope || !manual.provider.track_scopes().contains(&scope) {
                return false;
            }
            manual.scope = scope;
            manual.results = LoadState::Ready(Vec::new());
            manual.selection.clear();
        }
        self.drag.clear_hovered_track_in(MANUAL_SEARCH_PANE_ID);
        true
    }

    /// Run the manual search. The request id rides along with the result so a
    /// query the user has since replaced cannot land in the list.
    pub fn handle_edit_manual_run(&mut self) {
        if !self
            .edit_manual_mut()
            .as_deref()
            .is_some_and(EditManualSearch::can_run)
        {
            return;
        }
        let rid = self.request_ids.next();
        let Some(manual) = self.edit_manual_mut() else {
            return;
        };
        manual.request_id = rid;
        manual.results = LoadState::Loading;
        manual.selection.clear();
        let (provider, scope, query) = (manual.provider, manual.scope, manual.query.clone());
        let tx = self.result_tx.clone();
        thread::spawn(move || {
            let results = match crate::providers::search(provider, &query, scope, 0) {
                Ok((tracks, _tab)) => Ok(tracks),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(BackendResult::ManualSearchResults { rid, results });
        });
    }

    /// Leave the manual-search step without changing the working copy.
    pub fn handle_edit_manual_cancel(&mut self) -> Task<Message> {
        if let Some(edit) = self.edit_dialog_mut() {
            edit.manual = None;
        }
        self.forget_manual_search_list();
        Task::none()
    }

    /// Store a finished manual-search query. Replies to a query that has since
    /// been replaced (or whose popup closed) are dropped.
    pub fn apply_manual_search_results(
        &mut self,
        rid: u64,
        results: Result<Vec<Track>, String>,
    ) -> Task<Message> {
        let Some(manual) = self.edit_manual_mut() else {
            return Task::none();
        };
        if manual.request_id != rid {
            return Task::none();
        }
        manual.results = match results {
            Ok(tracks) => LoadState::Ready(tracks),
            Err(e) => LoadState::Failed(e),
        };
        manual.selection.clear();
        // Queue artwork for the results; the tick drains the queue.
        let thumbs: Vec<(ProviderId, String, String)> = match &manual.results {
            LoadState::Ready(tracks) => tracks
                .iter()
                .map(|track| {
                    (
                        manual.provider,
                        track.primary_id().to_string(),
                        track.thumbnail().to_string(),
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        for (provider, id, url) in thumbs {
            self.thumbnail_index.ensure(provider, &id, &url);
        }
        // The rows under the cursor are different ones now.
        self.drag.clear_hovered_track_in(MANUAL_SEARCH_PANE_ID);
        Task::none()
    }

    /// Assign the picked result to the track being edited and return to the
    /// popup: the result's identity for the searched provider replaces the
    /// working copy's (and becomes its source). Title/artist are only filled
    /// in while still blank, so text the user has typed survives.
    pub fn apply_edit_manual_selection(&mut self) {
        let Some(edit) = self.edit_dialog_mut() else {
            return;
        };
        let Some(manual) = edit.manual.as_ref() else {
            return;
        };
        let Some(track) = manual.selected_track() else {
            return;
        };
        let (provider, title, artist) = (manual.provider, &track.title, &track.artist);
        let Some(pt) = track.providers.get(&provider).cloned() else {
            return;
        };
        if edit.title.trim().is_empty() {
            edit.title = title.clone();
        }
        if edit.artist.trim().is_empty() {
            edit.artist = artist.clone();
        }
        edit.original.set_provider(provider, pt);
        edit.source = provider;
        edit.manual = None;
        self.forget_manual_search_list();
        self.notify((self.strings.found_on)(provider.label()));
    }

    /// Drop everything the popup's result list left behind — its keyboard
    /// focus, its hover and its captured geometry — now that the step is
    /// closing. Without this a later list would inherit a stale row index.
    fn forget_manual_search_list(&mut self) {
        self.drag.clear_hovered_track_in(MANUAL_SEARCH_PANE_ID);
        self.drag.clear_hovered_track();
        self.bounds.manual_search = None;
    }

    /// Apply the edited fields back to the track's source list and close the
    /// popup. `source` follows the working copy (changed via the provider
    /// "select" buttons); `title`/`artist` are overwritten from the inputs.
    pub fn apply_edit_track(&mut self) {
        let dialog = self.dialog.take();
        let Some(Dialog::Edit(edit)) = dialog else {
            self.dialog = dialog;
            return;
        };
        let mut track = edit.original;
        track.title = edit.title;
        track.artist = edit.artist;
        track.source = edit.source;
        self.set_track_at(edit.pos, track);
        self.playlists.save();
        self.save_session();
    }
}

#[cfg(test)]
mod tests {
    use iced::Point;

    use super::*;
    use crate::{
        app::{
            interaction::{CtxAction, Pressed},
            pane::QUEUE_PANE_ID,
            ViewData,
        },
        data::config,
    };

    fn result_track(provider: ProviderId, id: &str, title: &str, artist: &str) -> Track {
        Track::from_provider(
            provider,
            id.into(),
            format!("https://example.com/{id}"),
            title,
            artist,
            0,
            String::new(),
            None,
            None,
        )
    }

    /// A player with a single pane, the queue holding one editable track and
    /// the Edit popup open on it.
    fn editing_player() -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::default()]);
        p.queue.tracks = vec![result_track(
            ProviderId::YouTube,
            "old",
            "Old Title",
            "Old Artist",
        )];
        p.open_edit_track(TrackPos::new(0, QUEUE_PANE_ID));
        p
    }

    fn edit_of(p: &MusicPlayer) -> &EditTrackState {
        match &p.dialog {
            Some(Dialog::Edit(edit)) => edit,
            other => panic!("expected the edit dialog, got {other:?}"),
        }
    }

    /// Put the manual-search step in place without letting it fire a provider
    /// request. Only the seeded-run test goes through
    /// [`MusicPlayer::open_edit_manual_search`].
    fn open_manual_step(p: &mut MusicPlayer, provider: ProviderId) {
        let edit = p.edit_dialog_mut().unwrap();
        let scope = provider
            .track_scopes()
            .first()
            .copied()
            .unwrap_or(SearchScope::Songs);
        let query = edit.search_query();
        edit.manual = Some(EditManualSearch::new(provider, scope, query));
    }

    #[test]
    fn manual_search_opens_seeded_with_the_edited_fields_and_runs_it() {
        let mut p = editing_player();
        p.edit_dialog_mut().unwrap().artist = "New Artist".into();
        // `Local` answers a search locally, so opening the step here costs no
        // provider request while still exercising the seeded run.
        let _ = p.open_edit_manual_search(ProviderId::Local);

        let manual = edit_of(&p).manual.as_ref().expect("manual search opens");
        assert_eq!(manual.provider, ProviderId::Local);
        assert_eq!(manual.scope, SearchScope::Songs);
        assert_eq!(manual.query, "Old Title New Artist");
        // The seeded query is already in flight, so the search button waits.
        assert!(manual.results.is_loading());
        assert!(!manual.can_run());
    }

    #[test]
    fn scope_switch_keeps_only_scopes_the_provider_returns_tracks_for() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.selection = vec![0];
            manual.results =
                LoadState::Ready(vec![result_track(ProviderId::YouTube, "a", "A", "B")]);
        }

        // A switch asks for the query to run again in the new scope, so the
        // list never sits empty after it.
        assert!(p.handle_edit_manual_scope(SearchScope::Videos));
        let manual = p.edit_manual_mut().unwrap();
        assert_eq!(manual.scope, SearchScope::Videos);
        assert!(
            manual.selection.is_empty(),
            "a new scope drops the old pick"
        );
        assert!(matches!(
            manual.results,
            LoadState::Ready(ref tracks) if tracks.is_empty()
        ));

        // Artists return cards, not tracks, so a track-only picker refuses it.
        assert!(!p.handle_edit_manual_scope(SearchScope::Artists));
        assert_eq!(p.edit_manual_mut().unwrap().scope, SearchScope::Videos);

        // A provider without video results clamps the same way.
        let _ = p.handle_edit_manual_scope(SearchScope::Songs);
        open_manual_step(&mut p, ProviderId::SoundCloud);
        assert!(!p.handle_edit_manual_scope(SearchScope::Videos));
        assert_eq!(p.edit_manual_mut().unwrap().scope, SearchScope::Songs);
    }

    #[test]
    fn results_for_a_replaced_query_are_dropped() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        let stale = p.request_ids.next();
        let fresh = p.request_ids.next();
        {
            // A query is in flight for `fresh`.
            let manual = p.edit_manual_mut().unwrap();
            manual.request_id = fresh;
            manual.results = LoadState::Loading;
        }
        let _ = p.apply_manual_search_results(
            stale,
            Ok(vec![result_track(ProviderId::YouTube, "a", "A", "B")]),
        );
        assert!(p.edit_manual_mut().unwrap().results.is_loading());

        let _ = p.apply_manual_search_results(fresh, Err("no results".into()));
        assert!(matches!(
            p.edit_manual_mut().unwrap().results,
            LoadState::Failed(_)
        ));
    }

    #[test]
    fn a_closed_popup_ignores_late_results() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        let rid = p.edit_manual_mut().unwrap().request_id;
        let _ = p.handle_edit_manual_cancel();
        let _ = p.apply_manual_search_results(rid, Ok(Vec::new()));
        assert!(p.edit_dialog_mut().unwrap().manual.is_none());
    }

    #[test]
    fn confirming_assigns_the_picked_track_to_the_track_being_edited() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![
                result_track(ProviderId::YouTube, "a", "First", "Found Artist"),
                result_track(ProviderId::YouTube, "b", "Second", "Found Artist"),
            ]);
        }
        // The list picks at most one result: the last pick wins.
        let pick = |p: &mut MusicPlayer, i: usize| {
            p.toggle_selection(TrackPos::new(i, MANUAL_SEARCH_PANE_ID));
        };
        pick(&mut p, 0);
        pick(&mut p, 1);
        assert_eq!(p.selection_in(MANUAL_SEARCH_PANE_ID), &[1]);
        p.apply_edit_manual_selection();

        let edit = p.edit_dialog_mut().unwrap();
        assert!(edit.manual.is_none(), "confirming returns to the popup");
        assert_eq!(edit.source, ProviderId::YouTube);
        assert_eq!(edit.original.provider_id(ProviderId::YouTube), Some("b"));
        // Non-empty fields the user already has are left alone.
        assert_eq!(edit.title, "Old Title");
        assert_eq!(edit.artist, "Old Artist");
    }

    #[test]
    fn confirming_fills_fields_that_are_still_blank() {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.queue.tracks = vec![Track::default()];
        p.open_edit_track(TrackPos::new(0, QUEUE_PANE_ID));
        p.edit_dialog_mut().unwrap().title = String::new();
        p.edit_dialog_mut().unwrap().artist = "Kept".into();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![result_track(
                ProviderId::YouTube,
                "b",
                "Second",
                "Found Artist",
            )]);
            manual.selection = vec![0];
        }
        p.apply_edit_manual_selection();

        let edit = p.edit_dialog_mut().unwrap();
        assert_eq!(edit.title, "Second");
        assert_eq!(edit.artist, "Kept");
    }

    #[test]
    fn confirming_without_a_pick_keeps_the_working_copy() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        p.edit_manual_mut().unwrap().results = LoadState::Ready(Vec::new());
        p.apply_edit_manual_selection();
        // Nothing was picked, so the step stays open and nothing changed.
        assert!(p.edit_dialog_mut().unwrap().manual.is_some());
        assert_eq!(
            p.edit_dialog_mut()
                .unwrap()
                .original
                .provider_id(ProviderId::YouTube),
            Some("old")
        );
    }

    #[test]
    fn escape_backs_out_of_the_step_but_leaves_the_popup_open() {
        use crate::app::Shortcut;
        let mut p = editing_player();
        assert!(!p.edit_manual_open());
        open_manual_step(&mut p, ProviderId::YouTube);
        assert!(p.edit_manual_open());
        // Other shortcuts still act normally while the step is open.
        let _ = p.update(Message::Shortcut(Shortcut::ClosePane));
        assert!(p.edit_manual_open());

        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(!p.edit_manual_open());
        assert!(matches!(p.dialog, Some(Dialog::Edit(_))));
    }

    #[test]
    fn a_result_plays_from_its_row_without_being_picked() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![
                result_track(ProviderId::YouTube, "a", "First", "Found Artist"),
                result_track(ProviderId::YouTube, "b", "Second", "Found Artist"),
                result_track(ProviderId::YouTube, "c", "Third", "Found Artist"),
            ]);
        }
        // The same "play from here" the queue and every list share.
        let _ = p.update(Message::PlayTrackAt(TrackPos::new(
            1,
            MANUAL_SEARCH_PANE_ID,
        )));

        assert_eq!(p.queue.tracks.len(), 2);
        assert_eq!(p.queue.current().map(|t| t.title.as_str()), Some("Second"));
        // The step is untouched: the result can still be picked afterwards.
        assert!(p.edit_manual_open());
        assert!(p.selection_in(MANUAL_SEARCH_PANE_ID).is_empty());
    }

    #[test]
    fn keyboard_navigation_moves_through_the_results() {
        use crate::app::Shortcut;
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![
                result_track(ProviderId::YouTube, "a", "First", "Found Artist"),
                result_track(ProviderId::YouTube, "b", "Second", "Found Artist"),
                result_track(ProviderId::YouTube, "c", "Third", "Found Artist"),
            ]);
        }

        // Arrow keys reach the popup's list, not the panes behind the modal.
        let _ = p.update(Message::Shortcut(Shortcut::HoverMove {
            dir: 1,
            extend: false,
        }));
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(0, MANUAL_SEARCH_PANE_ID))
        );
        let _ = p.update(Message::Shortcut(Shortcut::HoverMove {
            dir: 1,
            extend: false,
        }));
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(1, MANUAL_SEARCH_PANE_ID))
        );

        // Ctrl+Space picks it, and only it.
        let _ = p.update(Message::Shortcut(Shortcut::ToggleSelection));
        assert_eq!(p.selection_in(MANUAL_SEARCH_PANE_ID), &[1]);
    }

    #[test]
    fn the_result_list_is_an_ordinary_track_list() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![
                result_track(ProviderId::YouTube, "a", "First", "Found Artist"),
                result_track(ProviderId::YouTube, "b", "Second", "Found Artist"),
            ]);
        }
        // Resolved through the virtual pane like any other list...
        let pos = TrackPos::new(1, MANUAL_SEARCH_PANE_ID);
        assert_eq!(p.get_track_at(pos).map(|t| t.title), Some("Second".into()));
        assert_eq!(p.track_count_in(MANUAL_SEARCH_PANE_ID), 2);
        // ...with one selection, and a right-click that keeps the popup open.
        let _ = p.update(Message::DragPress(Pressed::Track(pos)));
        let _ = p.update(Message::LeftButtonReleased);
        assert_eq!(p.selection_in(MANUAL_SEARCH_PANE_ID), &[1]);
        assert!(p.is_selected_in(MANUAL_SEARCH_PANE_ID, 1));

        let _ = p.update(Message::TrackRightClicked(pos, Point::new(10.0, 20.0)));
        let Some(Dialog::ContextMenu(menu)) = &p.dialog else {
            panic!("a right-click on a result opens the track menu");
        };
        assert_eq!(menu.pos, pos);
        assert!(
            !menu
                .actions(&p.stream_cache)
                .iter()
                .any(|a| matches!(a, CtxAction::Edit)),
            "editing a transient result from inside the editor is not offered"
        );

        let _ = p.update(Message::CloseContextMenu);
        assert!(
            matches!(p.dialog, Some(Dialog::Edit(_))),
            "closing the menu returns to the popup it came from"
        );
    }

    #[test]
    fn a_menu_over_the_popup_still_draws_the_popup() {
        let mut p = editing_player();
        open_manual_step(&mut p, ProviderId::YouTube);
        {
            let manual = p.edit_manual_mut().unwrap();
            manual.results = LoadState::Ready(vec![result_track(
                ProviderId::YouTube,
                "a",
                "First",
                "Found Artist",
            )]);
        }
        let pos = TrackPos::new(0, MANUAL_SEARCH_PANE_ID);
        let _ = p.update(Message::TrackRightClicked(pos, Point::new(10.0, 20.0)));

        // The menu is the dialog slot, so the popup it covers is drawn from
        // the menu's `return_to` — building the view must cover both.
        assert!(matches!(p.dialog, Some(Dialog::ContextMenu(_))));
        let _ = crate::app::ui::view(&p);
    }

    #[test]
    fn the_step_is_driven_by_its_messages() {
        let mut p = editing_player();
        let _ = p.update(Message::EditTrackManualSearch(ProviderId::Local));
        let _ = p.update(Message::EditTrackManualScope(SearchScope::Songs));
        let _ = p.update(Message::EditTrackManualQuery("other".into()));
        {
            let manual = p.edit_manual_mut().unwrap();
            assert_eq!(manual.scope, SearchScope::Songs);
            assert_eq!(manual.query, "other");
            manual.results = LoadState::Ready(vec![result_track(
                ProviderId::Local,
                "b",
                "Second",
                "Found Artist",
            )]);
        }
        let _ = p.update(Message::DragPress(Pressed::Track(TrackPos::new(
            0,
            MANUAL_SEARCH_PANE_ID,
        ))));
        let _ = p.update(Message::LeftButtonReleased);
        let _ = p.update(Message::EditTrackManualConfirm);
        let edit = p.edit_dialog_mut().unwrap();
        assert!(edit.manual.is_none());
        assert_eq!(edit.source, ProviderId::Local);
        assert_eq!(edit.original.provider_id(ProviderId::Local), Some("b"));
    }
}
