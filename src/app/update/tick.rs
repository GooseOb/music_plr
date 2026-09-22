use std::time::Duration;

use souvlaki::MediaPlayback;
use tracing::debug;

use super::{
    error, media_controls, mpsc, spawn_thumbnail_download, BackendResult, MediaControlEvent,
    MediaUpdate, Message, MusicPlayer, Task, ViewData, PREPEND,
};
use crate::{
    app::{interaction::TrackListKind, pane::PaneId, Dialog, ViewKind},
    data::{cache::StreamCache, JsonStore},
};

/// How long a toast notification stays visible before auto-dismissing.
pub(crate) const NOTIFICATION_DURATION: Duration = Duration::from_secs(4);

impl MusicPlayer {
    /// Start OS media controls (MPRIS/SMTC/Now Playing via souvlaki). `hwnd` is
    /// the window handle, required on Windows and ignored elsewhere. Idempotent:
    /// the first successful call wins, so the deferred Windows HWND resolution
    /// can't double-initialize.
    pub fn init_media_controls(&mut self, hwnd: Option<*mut std::ffi::c_void>) {
        if self.media_controls_started {
            return;
        }
        let (media_update_tx, media_update_rx) = mpsc::channel();
        media_controls::start(self.media_event_tx.clone(), media_update_rx, hwnd);
        self.media_update_tx = Some(media_update_tx);
        self.media_controls_started = true;
    }

    pub fn handle_tick(&mut self) -> Task<Message> {
        let mut task = Task::none();
        while let Ok(result) = self.result_rx.try_recv() {
            task = task.chain(self.process_result(result));
        }

        while let Ok(event) = self.media_event_rx.try_recv() {
            self.process_media_event(&event);
        }

        self.expire_notification();
        // Reconcile currently visible thumbnails: queue any missing ones for
        // download and flush the queue to a background thread.
        self.update_thumbnails();

        let s = self.audio.get_state();
        self.drain_audio_events();
        self.sync_playback_state(&s);
        self.register_pending_cache(&s);
        self.auto_advance(&s);

        self.update_media_controls_if_dirty();
        self.flush_session();

        for pane in self.pane_ids() {
            if self.pane(pane).lyrics.is_some() {
                self.ensure_lyrics_for_current(pane);
                task = task.chain(self.maybe_autoscroll_lyrics(pane));
            }
        }
        task
    }

    fn expire_notification(&mut self) {
        if let Some(toast) = &self.notification {
            if std::time::Instant::now() >= toast.until {
                self.notification = None;
            }
        }
    }

    fn drain_audio_events(&mut self) {
        if let Some(err) = self.audio.take_error() {
            self.notify_error(err);
        }
        if let Some(event) = self.audio.take_client_event() {
            self.notify_client_event(event);
        }
    }

    fn sync_playback_state(&mut self, s: &crate::audio::PlayerState) {
        if self.is_playing != s.is_playing || (self.duration - s.duration).abs() > 0.001 {
            self.media_controls_dirty = true;
        }
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }
    }

    fn register_pending_cache(&mut self, s: &crate::audio::PlayerState) {
        let Some(pending) = self.pending_cache_id.clone() else {
            return;
        };
        if !s.cache_ready {
            return;
        }
        if self.stream_cache.insert(pending.provider_id, &pending.id) {
            debug!(
                "Registered cached track: {:?}:{:?}",
                pending.provider_id, pending.id
            );
        }
        self.pending_cache_id = None;
        if self.pending_normalization_id.as_deref() == Some(pending.id.as_str()) {
            let path = StreamCache::path_for(pending.provider_id, &pending.id);
            self.request_normalization_analysis(&pending.id, path);
            self.pending_normalization_id = None;
        }
    }

    fn auto_advance(&mut self, s: &crate::audio::PlayerState) {
        if !(s.stream_finished && !s.is_playing && !self.track_loading) {
            return;
        }
        if self.repeat {
            if let Some(track) = self.queue.current().cloned() {
                self.play_track_internal(&track, track.source);
            }
            self.audio.clear_stream_finished();
        } else if self.queue.current().is_some() {
            self.next_track();
        } else {
            self.audio.clear_stream_finished();
        }
    }

    pub(super) fn capture_bounds_task(&self) -> Task<Message> {
        super::operation::CaptureBounds::with_panes(self.pane_ids()).into()
    }

    fn update_media_controls_if_dirty(&mut self) {
        if self.media_controls_dirty {
            self.send_media_update();
            self.media_controls_dirty = false;
        }
    }

    pub(crate) fn seed_tracks_thumbnails(&mut self, tracks: &[crate::types::Track]) {
        for track in tracks {
            if !track.thumbnail().is_empty() {
                self.thumbnail_index
                    .ensure(track.source, track.primary_id(), track.thumbnail());
            }
        }
    }

    /// Seed a view's thumbnail ids into the index so the next tick drains any
    /// missing ones. Called wherever a view becomes active (navigation,
    /// results installed) — the tick only drains, it never re-scans visibility.
    pub(crate) fn seed_view_thumbnails(&mut self, view: &ViewData) {
        let tracks = view.tracks().to_vec();
        self.seed_tracks_thumbnails(&tracks);
        match &view.kind {
            // A local playlist backs its tracks from the store, not from
            // `ViewData`, so seed from the store or its artwork never drains.
            ViewKind::Playlist(entry) => {
                if let Some(playlist) = self.playlists.playlists.get(entry.index) {
                    let tracks = playlist.tracks.clone();
                    self.seed_tracks_thumbnails(&tracks);
                }
            }
            ViewKind::Album(r) => {
                self.thumbnail_index.ensure(r.provider, &r.id, &r.thumbnail);
            }
            ViewKind::PlaylistView(r) => {
                self.thumbnail_index.ensure(r.provider, &r.id, &r.thumbnail);
            }
            _ => {}
        }
        if let ViewKind::Search(s) = &view.kind {
            if let crate::providers::SearchTab::Artists(cards)
            | crate::providers::SearchTab::Albums(cards)
            | crate::providers::SearchTab::Playlists(cards) = &s.tab
            {
                for card in cards {
                    self.thumbnail_index
                        .ensure(s.provider, &card.id, &card.thumbnail);
                }
            }
        }
    }

    fn update_thumbnails(&mut self) {
        if let Some(entries) = self.thumbnail_index.drain_pending() {
            spawn_thumbnail_download(entries, &self.result_tx);
        }
    }

    pub fn process_media_event(&mut self, event: &MediaControlEvent) {
        use souvlaki::{MediaControlEvent as E, SeekDirection};
        match event {
            E::Toggle => self.toggle_play_pause(),
            E::Next => self.next_track(),
            E::Previous => self.previous_track(),
            E::Stop => {
                if self.is_playing {
                    self.toggle_play_pause();
                }
            }
            E::Play => {
                if !self.is_playing {
                    self.toggle_play_pause();
                }
            }
            E::Pause => {
                if self.is_playing {
                    self.audio.pause();
                    self.media_controls_dirty = true;
                }
            }
            E::SetVolume(vol) => self.set_volume(*vol as f32),
            E::SeekBy(direction, delta) => {
                let delta_us = delta.as_micros() as i64;
                let delta_us = match direction {
                    SeekDirection::Forward => delta_us,
                    SeekDirection::Backward => -delta_us,
                };
                let delta_frac = delta_us as f32 / 1_000_000.0 / self.duration.max(0.001);
                let new_frac = (self.progress + delta_frac).clamp(0.0, 1.0);
                self.seek(new_frac);
            }
            E::Seek(_) | E::SetPosition(_) | E::OpenUri(_) | E::Raise | E::Quit => {}
        }
    }

    /// Fill in missing badge/date/thumbnail on the browsed album view once
    /// metadata arrives from the backend.
    fn apply_album_meta(&mut self, pane: PaneId, idx: usize, meta: crate::providers::AlbumMeta) {
        let crate::providers::AlbumMeta {
            badge,
            date,
            thumbnail,
        } = meta;
        let seed = match &self.pane(pane).nav_history.get(idx).map(|v| &v.kind) {
            Some(ViewKind::Album(r)) if r.thumbnail.is_empty() && !thumbnail.is_empty() => {
                Some((r.provider, r.id.clone()))
            }
            _ => None,
        };
        if let Some((provider, id)) = seed {
            self.thumbnail_index.ensure(provider, &id, &thumbnail);
        }
        if let ViewKind::Album(r) = &mut self.pane_mut(pane).nav_history[idx].kind {
            if r.badge.is_empty() {
                r.badge = badge;
            }
            if r.date.is_empty() {
                r.date = date;
            }
            if r.thumbnail.is_empty() && !thumbnail.is_empty() {
                r.thumbnail = thumbnail;
            }
        }
    }

    fn install_results(&mut self, pane: PaneId, idx: usize, tracks: Vec<crate::types::Track>) {
        let slot = &mut self.pane_mut(pane).nav_history[idx];
        slot.set_tracks(tracks);
        slot.selection.clear();
        slot.request_id = 0;
        self.finalize_view(pane, idx);
    }

    pub(crate) fn finalize_view(&mut self, pane: PaneId, idx: usize) {
        self.save_session();
        self.seed_view_thumbnails(&self.pane(pane).nav_history[idx].clone());
    }

    fn process_search_results(
        &mut self,
        rid: u64,
        tracks: Vec<crate::types::Track>,
        tab: crate::providers::SearchTab,
    ) {
        // Apply to the slot that requested this search.
        if let Some((pane, idx)) = self.slot_for_request(rid) {
            if let ViewKind::Search(s) = &mut self.pane_mut(pane).nav_history[idx].kind {
                let count = if tab.is_track_tab() {
                    tracks.len()
                } else {
                    tab.card_count().unwrap_or(0)
                };
                // A blank query is a one-shot browse (charts/trending) with no
                // pagination, so it is always exhausted.
                s.exhausted = s.query.trim().is_empty() || count < crate::theme::SEARCH_PAGE_SIZE;
                s.tab = tab;
                self.install_results(pane, idx, tracks);
                // Snapshot the completed search for the sidebar "Search" item.
                self.last_search_view = Some(self.pane(pane).nav_history[idx].clone());
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn process_result(&mut self, result: BackendResult) -> Task<Message> {
        match result {
            BackendResult::DependencyProgress(kind, downloaded, total) => {
                if let Some(op) = self.dep_ops.get_mut(&kind) {
                    op.progress = (downloaded, total);
                }
                Task::none()
            }
            BackendResult::DependencyInstalled(kind, status) => {
                if let Some(op) = self.dep_ops.get_mut(&kind) {
                    op.installing = false;
                    op.install_result = Some(status);
                    op.progress = (0, 0);
                }
                // The startup dialog reads its completion/toast state from the
                // shared `dep_ops` map rather than keeping its own copies.
                if let Some(Dialog::Dependencies(dialog)) = &self.dialog {
                    let any_ok = dialog.selected.iter().any(|k| {
                        self.dep_ops
                            .get(k)
                            .and_then(|o| o.install_result.as_ref())
                            .is_some_and(Result::is_ok)
                    });
                    if dialog.all_resolved(&self.dep_ops) && any_ok {
                        self.notify(self.strings.deps_installed_toast);
                    }
                }
                Task::none()
            }
            BackendResult::DependencyDeleted(kind, status) => {
                if let Some(op) = self.dep_ops.get_mut(&kind) {
                    op.deleting = false;
                    op.delete_result = Some(status);
                }
                Task::none()
            }
            BackendResult::SearchResults(rid, tracks, tab) => {
                self.process_search_results(rid, tracks, tab);
                self.capture_bounds_task()
            }
            BackendResult::SearchResultsAppend(rid, tracks) => {
                let exhausted = tracks.len() < crate::theme::SEARCH_PAGE_SIZE;
                if let Some((pane, idx)) = self.slot_for_request(rid) {
                    {
                        let slot = &mut self.pane_mut(pane).nav_history[idx];
                        if let Some(existing) = slot.tracks_mut() {
                            existing.extend(tracks);
                        }
                        if let ViewKind::Search(s) = &mut slot.kind {
                            s.exhausted = exhausted;
                            s.append_in_flight = false;
                        }
                        slot.request_id = 0;
                    }
                    if self
                        .last_search_view
                        .as_ref()
                        .is_some_and(|l| rid == l.request_id)
                    {
                        self.last_search_view = Some(self.pane(pane).nav_history[idx].clone());
                    }
                    self.finalize_view(pane, idx);
                    self.capture_bounds_task()
                } else {
                    Task::none()
                }
            }
            BackendResult::BrowseResults(rid, tracks, meta) => {
                // Apply to the slot that issued the browse, matched by request id
                if let Some((pane, idx)) = self.slot_for_request(rid) {
                    self.install_results(pane, idx, tracks);
                    if let Some(meta) = meta {
                        self.apply_album_meta(pane, idx, meta);
                    }
                }
                self.capture_bounds_task()
            }
            BackendResult::ArtistIdResolved {
                rid,
                provider,
                resolved_id,
            } => {
                self.apply_artist_id_resolved(rid, provider, &resolved_id);
                Task::none()
            }
            BackendResult::ArtistSectionLoaded {
                rid,
                provider,
                kind,
                data,
            } => {
                self.apply_artist_section(rid, provider, kind, *data);
                self.capture_bounds_task()
            }
            BackendResult::CardPlaylistReady(idx, name, tracks) => {
                // A dragged card turned into a playlist; the browse result
                // fills it. The playlist view reads tracks from the store, so
                // they appear as soon as we insert them.
                if idx < self.playlists.playlists.len() {
                    let count = self.playlists.insert_tracks_at(idx, tracks.iter(), PREPEND);
                    self.seed_tracks_thumbnails(&tracks);
                    let msg = (self.strings.added_to)(count, &name);
                    self.notify(msg);
                }
                self.capture_bounds_task()
            }
            BackendResult::RadioResults(rid, label, tracks) => {
                if let Some((pane, idx)) = self.slot_for_request(rid) {
                    let kind = match self.pane(pane).nav_history[idx].kind {
                        ViewKind::ArtistRadio(_) => ViewKind::ArtistRadio(label),
                        _ => ViewKind::SongRadio(label),
                    };
                    self.pane_mut(pane).nav_history[idx].kind = kind;
                    self.install_results(pane, idx, tracks);
                }
                self.capture_bounds_task()
            }
            BackendResult::DownloadComplete(track, _provider) => {
                self.process_download_complete(&track);
                Task::none()
            }
            BackendResult::DownloadError(msg) => {
                error!("Download error: {}", msg);
                self.notify_error(msg);
                Task::none()
            }
            BackendResult::PlayerClientEvent(event) => {
                self.notify_client_event(event);
                Task::none()
            }
            BackendResult::SearchError(rid, msg) => {
                self.process_search_error(rid, msg);
                Task::none()
            }
            BackendResult::EditTrackProviderResolved(provider, resolved) => {
                self.apply_edit_track_provider_resolution(provider, resolved);
                Task::none()
            }
            BackendResult::EditTrackProviderError(_provider, message) => {
                if let Some(Dialog::Edit(edit)) = &mut self.dialog {
                    edit.finding = None;
                }
                self.notify_error(message);
                Task::none()
            }
            BackendResult::ProviderResolved {
                original,
                provider,
                resolved,
                rid,
                pos,
                play,
            } => {
                self.apply_provider_resolution(original, provider, resolved, rid, pos, play);
                Task::none()
            }
            BackendResult::ProviderResolveError {
                title,
                provider,
                message,
            } => {
                let msg = (self.strings.failed_resolve_on)(&title, provider.label(), &message);
                self.notify_error(msg);
                Task::none()
            }
            BackendResult::ThumbnailDownloaded(provider, id) => {
                self.thumbnail_index.mark_downloaded(provider, &id);
                Task::none()
            }
            BackendResult::NormalizationComputed(id, gain) => {
                self.normalization_cache.insert(id, gain);
                crate::audio::save_gains(&self.normalization_cache);
                Task::none()
            }
            BackendResult::LocalFilesPicked(paths) => {
                if !paths.is_empty() {
                    self.handle_add_local_music(&paths);
                }
                Task::none()
            }
            BackendResult::LyricsFetched(result, track_id, provider) => {
                self.process_lyrics_fetched(&result, &track_id, provider)
            }
            BackendResult::ImportPathsPicked { method, paths } => {
                if paths.is_empty() {
                    Task::none()
                } else {
                    self.handle_import_paths(method, &paths)
                }
            }
            BackendResult::VersionChecked {
                current,
                latest,
                release_url,
                asset_url,
                sha256,
                package_managed,
                error,
            } => self.process_version_checked(
                current,
                latest,
                release_url,
                asset_url,
                sha256,
                package_managed,
                error,
            ),
            BackendResult::UpdateProgress(downloaded, total) => {
                if let crate::app::update::UpdateStatus::Updating { progress } =
                    &mut self.update_status
                {
                    *progress = (downloaded, total);
                }
                Task::none()
            }
            BackendResult::UpdateComplete(result) => {
                self.process_update_complete(result);
                Task::none()
            }
        }
    }

    /// A failed search/browse/radio fetch: surface the error on the
    /// requesting pane's view only while it is still waiting (never wiping
    /// loaded results), plus a toast.
    fn process_search_error(&mut self, rid: u64, msg: String) {
        if let Some((pane, idx)) = self.slot_for_request(rid) {
            let slot = &mut self.pane_mut(pane).nav_history[idx];
            if matches!(
                slot.kind,
                ViewKind::Search(_)
                    | ViewKind::SongRadio(_)
                    | ViewKind::ArtistRadio(_)
                    | ViewKind::Album(_)
                    | ViewKind::PlaylistView(_)
            ) {
                slot.set_failed(msg.clone());
                if let ViewKind::Search(s) = &mut slot.kind {
                    s.append_in_flight = false;
                }
            }
        }
        self.notify_error(msg);
    }

    fn process_download_complete(&mut self, track: &crate::types::Track) {
        let path = track.local_path().unwrap_or_default();
        self.download_registry.register(track.clone());
        let msg = (self.strings.download_complete)(&path);
        self.notify(msg);
        self.thumbnail_index
            .mark_downloaded(track.source, track.primary_id());
        let panes: Vec<PaneId> = self.pane_ids();
        for pane in panes {
            if matches!(self.view_data_in(pane).kind, ViewKind::Downloads) {
                if let Some(tracks) = self.view_data_in_mut(pane).tracks_mut() {
                    tracks.push(track.clone());
                }
            }
        }
    }

    /// A lyrics fetch finished: apply it to every pane waiting on lyrics
    /// for that track from that provider. Panes are independent — a pane
    /// showing another provider keeps its own state and fetch.
    fn process_lyrics_fetched(
        &mut self,
        result: &Result<crate::lyrics::Lyrics, String>,
        track_id: &str,
        provider: crate::lyrics::LyricsProvider,
    ) -> Task<Message> {
        if track_id.is_empty() {
            return Task::none();
        }
        let mut tasks = Vec::new();
        let panes: Vec<PaneId> = self.pane_ids();
        let mut cache = None;
        for pane in panes {
            let Some(state) = self.pane_mut(pane).lyrics.as_mut() else {
                continue;
            };
            if state.track_id.as_deref() != Some(track_id) {
                continue;
            }
            if state.provider != provider {
                continue;
            }
            if state.editing {
                continue;
            }
            match &result {
                Ok(lyrics) => {
                    let cache =
                        cache.get_or_insert_with(crate::data::lyrics_cache::LyricsCache::load);
                    cache.insert(track_id, lyrics);
                    let mode = crate::app::LyricsViewMode::for_lyrics(lyrics);
                    state.lyrics = crate::load_state::LoadState::Ready(lyrics.clone());
                    state.mode = mode;
                    state.scrolled_to = None;
                    state.viewport = None;
                }
                Err(e) => state.lyrics = crate::load_state::LoadState::Failed(e.clone()),
            }
            state.track_id = Some(track_id.to_owned());
            self.sync_lyrics_editor(pane);
            tasks.push(self.scroll_lyrics_to_active(pane));
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            tasks.insert(0, self.capture_bounds_task());
            Task::batch(tasks)
        }
    }

    fn slot_tracks(&self, pane: PaneId, idx: usize) -> &[crate::types::Track] {
        match self.pane(pane).nav_history.get(idx).map(|v| &v.kind) {
            Some(ViewKind::Playlist(entry)) => self
                .playlists
                .playlists
                .get(entry.index)
                .map_or(&[], |p| &p.tracks),
            _ => self
                .pane(pane)
                .nav_history
                .get(idx)
                .map_or(&[], |v| v.tracks()),
        }
    }

    fn set_track_at_slot(
        &mut self,
        pane: PaneId,
        history_idx: usize,
        track_idx: usize,
        track: crate::types::Track,
    ) {
        let target = match self
            .pane(pane)
            .nav_history
            .get(history_idx)
            .map(|v| &v.kind)
        {
            Some(ViewKind::Playlist(entry)) => Some(entry.index),
            _ => None,
        };
        if let Some(sp) = target {
            if let Some(pl) = self.playlists.playlists.get_mut(sp) {
                if let Some(t) = pl.tracks.get_mut(track_idx) {
                    *t = track.clone();
                }
            }
            self.playlists.save();
        }
        if let Some(slot) = self.pane_mut(pane).nav_history.get_mut(history_idx) {
            if let Some(t) = slot.tracks_mut().and_then(|ts| ts.get_mut(track_idx)) {
                *t = track;
            }
        }
    }

    /// Apply a resolved provider track to `original`: write its full provider
    /// metadata back into the source list, then either play (replacing the
    /// queue) or download.
    fn apply_provider_resolution(
        &mut self,
        mut original: crate::types::Track,
        provider: crate::providers::ProviderId,
        resolved: Option<crate::types::Track>,
        rid: u64,
        pos: crate::app::interaction::TrackPos,
        play: bool,
    ) {
        if let Some(resolved_track) = resolved {
            if let Some(pt) = resolved_track.providers.get(&provider) {
                original.set_provider(provider, pt.clone());
            }
            original.source = provider;
            self.thumbnail_index
                .ensure(provider, original.primary_id(), original.thumbnail());

            let slot = if pos.list == TrackListKind::Active {
                self.slot_for_request(rid)
            } else {
                None
            };

            if let Some((pane, slot)) = slot {
                self.set_track_at_slot(pane, slot, pos.index, original.clone());
                if play {
                    let queue = self
                        .slot_tracks(pane, slot)
                        .get(pos.index..)
                        .unwrap_or(&[])
                        .to_vec();
                    self.set_queue(queue);
                    self.record_now_playing_origin();
                } else {
                    self.spawn_download_thread_for(provider, original);
                }
            } else {
                if pos.list != TrackListKind::Active {
                    self.set_track_at(pos, original.clone());
                }
                if play {
                    self.handle_play_track(pos);
                } else {
                    self.spawn_download_thread_for(provider, original);
                }
            }
        } else {
            let title = original.title.clone();
            let msg = (self.strings.could_not_find_on)(&title, provider.label());
            self.notify_error(msg);
        }
    }

    pub fn send_media_update(&self) {
        if let Some(ref tx) = self.media_update_tx {
            let track = self.queue.current();
            let update = MediaUpdate {
                playback: if self.is_playing {
                    MediaPlayback::Playing { progress: None }
                } else if track.is_some() {
                    MediaPlayback::Paused { progress: None }
                } else {
                    MediaPlayback::Stopped
                },
                title: track.map(|t| t.title.clone()).unwrap_or_default(),
                artist: track.map(|t| t.artist.clone()).unwrap_or_default(),
                duration_secs: self.duration,
                has_track: track.is_some(),
            };
            let _ = tx.send(update);
        }
    }

    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    fn process_version_checked(
        &mut self,
        current: String,
        latest: Option<String>,
        release_url: String,
        asset_url: Option<String>,
        sha256: Option<String>,
        package_managed: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let _ = current;

        if package_managed {
            self.update_status = crate::app::update::UpdateStatus::PackageManaged;
            self.notify_error(self.strings.package_managed.to_string());
            return Task::none();
        }

        if let Some(err) = error {
            self.update_status = crate::app::update::UpdateStatus::Error(err.clone());
            self.notify_error(err);
            return Task::none();
        }

        if let Some(latest) = latest {
            if let (Some(url), Some(sha)) = (asset_url, sha256) {
                self.update_status = crate::app::update::UpdateStatus::Available {
                    version: latest.clone(),
                    release_url,
                    asset_url: url,
                    sha256: sha,
                };
                self.notify_for(
                    (self.strings.update_available)(&latest),
                    std::time::Duration::from_secs(6),
                );
            } else {
                self.update_status = crate::app::update::UpdateStatus::Error(
                    "New version found but no matching binary for this platform".to_string(),
                );
            }
        } else {
            self.update_status = crate::app::update::UpdateStatus::UpToDate;
        }
        Task::none()
    }

    fn process_update_complete(&mut self, result: Result<String, String>) {
        match result {
            Ok(version) => {
                self.update_status = crate::app::update::UpdateStatus::UpdateApplied;
                self.notify((self.strings.update_applied)(&version));
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    std::process::exit(0);
                });
            }
            Err(e) => {
                self.update_status = crate::app::update::UpdateStatus::Error(e.clone());
                self.notify_error(e);
            }
        }
    }
}
