use super::{thread, BackendResult, MusicPlayer, ViewData};
use crate::{
    app::ViewKind,
    providers::{ArtistDataKind, ArtistPage, ArtistPageState, ArtistSectionKind, ProviderId},
};

/// True when both lists are the same popular-track rows (per `provider`
/// ids), i.e. the incoming list is a metadata refresh rather than new data.
fn same_popular_ids(
    a: &[crate::types::Track],
    b: &[crate::types::Track],
    provider: ProviderId,
) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.providers.get(&provider).map(|p| &p.id) == y.providers.get(&provider).map(|p| &p.id)
        })
}

impl MusicPlayer {
    /// Open an artist page for the artist `id` on `source`. The source
    /// provider serves everything it can in its own request(s); the other
    /// streamable provider is asked only for its header (subscribers /
    /// followers stats). Both run in parallel on their own threads. Anything
    /// else loads lazily when a section picker selects that provider.
    pub fn open_artist(&mut self, id: &str, name: &str, source: ProviderId) {
        let kind = ViewKind::Artist {
            id: id.to_string(),
            name: name.to_string(),
            source,
            page: Box::new(ArtistPageState::new(source, id)),
        };
        self.push_new_view(ViewData {
            kind,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Opening artist: {name}..."));

        // Source: everything it can serve. YouTube answers in one request;
        // SoundCloud runs all five endpoints concurrently.
        self.load_artist_page(rid, name, source, ArtistDataKind::ALL);
        // Companion header stats (e.g. SoundCloud Followers on a YouTube
        // artist page) — a single cheap profile request.
        if source != ProviderId::YouTube {
            self.load_artist_page(rid, name, ProviderId::YouTube, &[ArtistDataKind::Header]);
        }
        if source != ProviderId::SoundCloud {
            self.load_artist_page(rid, name, ProviderId::SoundCloud, &[ArtistDataKind::Header]);
        }
    }

    /// Fetch only `kinds` of `provider`'s artist data in the background,
    /// resolving the artist id by name when unknown.
    fn load_artist_page(
        &mut self,
        rid: u64,
        name: &str,
        provider: ProviderId,
        kinds: &'static [ArtistDataKind],
    ) {
        let known_id = {
            let ViewKind::Artist { page, .. } = &mut self.view_data_mut().kind else {
                return;
            };
            page.provider_ids.get(&provider).cloned()
        };
        let name = name.to_string();
        let tx = self.result_tx.clone();

        let fetch = move || -> anyhow::Result<(String, crate::providers::ArtistPage)> {
            let id = match known_id {
                Some(id) => Some(id),
                None => crate::providers::resolve_artist_id(provider, &name)?,
            };
            let resolved = id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Could not find {name} on {}", provider.label()))?;
            Ok((
                resolved.clone(),
                crate::providers::fetch_artist_page(provider, &resolved, kinds)?,
            ))
        };

        thread::spawn(move || {
            let (resolved_id, page) = match fetch() {
                Ok((id, page)) => (Some(id), Ok(page)),
                Err(e) => (None, Err(e.to_string())),
            };
            // Render the page immediately; for YouTube popular tracks a
            // second pass backfills durations/view counts once yt-dlp
            // finishes.
            let enrich = provider == ProviderId::YouTube && ArtistDataKind::Popular.wanted(kinds);
            if let (true, Ok(page)) = (enrich, &page) {
                let _ = tx.send(BackendResult::ArtistPageLoaded {
                    rid,
                    provider,
                    resolved_id: resolved_id.clone(),
                    kinds,
                    page: Box::new(Ok(page.clone())),
                });
            }
            let mut page = page;
            if enrich {
                if let Ok(page) = &mut page {
                    crate::providers::enrich_track_metadata(&mut page.popular);
                }
            }
            let _ = tx.send(BackendResult::ArtistPageLoaded {
                rid,
                provider,
                resolved_id,
                kinds,
                page: Box::new(page),
            });
        });
    }

    /// Reuse the slot's live request id so results keep matching this
    /// view even across several concurrent loads.
    fn slot_request_id(&mut self) -> u64 {
        let current = self.view_data().request_id;
        if current == 0 {
            let next = self.request_ids.next();
            self.view_data_mut().request_id = next;
            next
        } else {
            current
        }
    }

    pub fn handle_artist_section_provider_changed(
        &mut self,
        section_kind: ArtistSectionKind,
        provider: ProviderId,
    ) {
        fn set_section<T: Default>(
            section: &mut crate::providers::ArtistSection<T>,
            provider: ProviderId,
        ) {
            *section = crate::providers::ArtistSection {
                provider: Some(provider),
                loading: true,
                content: Default::default(),
            };
        }
        // Static slices so the fetch closure can own them.
        let needed: &'static [ArtistDataKind] = match section_kind {
            ArtistSectionKind::Popular => &[ArtistDataKind::Popular],
            ArtistSectionKind::Albums => &[ArtistDataKind::Albums],
            ArtistSectionKind::Playlists => &[ArtistDataKind::Playlists],
            ArtistSectionKind::Related => &[ArtistDataKind::Related],
        };
        let name;
        {
            let ViewKind::Artist { name: n, page, .. } = &mut self.view_data_mut().kind else {
                return;
            };
            name = n.clone();

            // Serve from the per-provider cache when this section's data was
            // already fetched — no loading state, no request.
            if let Some(cached) = page.pages.get(&provider) {
                if cached.covers(needed) {
                    let mut tracks: Option<Vec<crate::types::Track>> = None;
                    match section_kind {
                        ArtistSectionKind::Popular => {
                            page.popular.content.clone_from(&cached.page.popular);
                            page.popular.provider = Some(provider);
                            tracks = Some(cached.page.popular.clone());
                        }
                        ArtistSectionKind::Albums => {
                            page.albums.content.clone_from(&cached.page.albums);
                            page.albums.provider = Some(provider);
                        }
                        ArtistSectionKind::Playlists => {
                            page.playlists.content.clone_from(&cached.page.playlists);
                            page.playlists.provider = Some(provider);
                        }
                        ArtistSectionKind::Related => {
                            page.related.content.clone_from(&cached.page.related);
                            page.related.provider = Some(provider);
                        }
                    }
                    if let Some(tracks) = tracks {
                        let slot = self.view_data_mut();
                        slot.tracks = tracks;
                        slot.selection.clear();
                    }
                    return;
                }
            }

            match section_kind {
                ArtistSectionKind::Popular => set_section(&mut page.popular, provider),
                ArtistSectionKind::Albums => set_section(&mut page.albums, provider),
                ArtistSectionKind::Playlists => set_section(&mut page.playlists, provider),
                ArtistSectionKind::Related => set_section(&mut page.related, provider),
            }
        }
        let rid = self.slot_request_id();
        self.load_artist_page(rid, &name, provider, needed);
    }

    /// Merge a finished artist-data fetch into its view slot. Only the
    /// requested `kinds` are applied to the sections; the header picture and
    /// bio come exclusively from whichever provider owns the header
    /// (`header_provider`), while its stats merge additively (labels are
    /// provider-scoped, e.g. "Monthly listeners" / "Followers" counts).
    pub fn apply_artist_page(
        &mut self,
        rid: u64,
        provider: ProviderId,
        resolved_id: Option<String>,
        kinds: &'static [ArtistDataKind],
        result: Result<ArtistPage, String>,
    ) {
        let Some(idx) = self.slot_for_request(rid) else {
            return;
        };
        let ViewKind::Artist { page, .. } = &mut self.nav_history[idx].kind else {
            return;
        };
        match result {
            Ok(fetched) => {
                if let Some(id) = resolved_id {
                    page.provider_ids.insert(provider, id);
                }
                page.pages
                    .entry(provider)
                    .or_default()
                    .merge(kinds, fetched.clone());
                let explicit = page.header_provider == Some(provider);
                match (&mut page.header, &fetched.header) {
                    (None, Some(incoming)) => {
                        let mut header = incoming.clone();
                        if !explicit {
                            header.image.clear();
                            header.description.clear();
                        }
                        page.header = Some(header);
                    }
                    (Some(existing), Some(incoming)) => {
                        for stat in &incoming.stats {
                            if !existing.stats.iter().any(|(label, _)| label == &stat.0) {
                                existing.stats.push(stat.clone());
                            }
                        }
                        if explicit {
                            existing.image.clone_from(&incoming.image);
                            existing.description.clone_from(&incoming.description);
                        }
                    }
                    _ => {}
                }
                let mut new_tracks: Option<Vec<crate::types::Track>> = None;
                if ArtistDataKind::Popular.wanted(kinds) && page.popular.provider == Some(provider)
                {
                    page.popular.content.clone_from(&fetched.popular);
                    page.popular.loading = false;
                    new_tracks = Some(fetched.popular.clone());
                }
                if ArtistDataKind::Albums.wanted(kinds) && page.albums.provider == Some(provider) {
                    page.albums.content.clone_from(&fetched.albums);
                    page.albums.loading = false;
                }
                if ArtistDataKind::Playlists.wanted(kinds)
                    && page.playlists.provider == Some(provider)
                {
                    page.playlists.content.clone_from(&fetched.playlists);
                    page.playlists.loading = false;
                }
                if ArtistDataKind::Related.wanted(kinds) && page.related.provider == Some(provider)
                {
                    page.related.content.clone_from(&fetched.related);
                    page.related.loading = false;
                }
                let view = self.nav_history[idx].clone();
                if let Some(tracks) = new_tracks {
                    let slot = &mut self.nav_history[idx];
                    if same_popular_ids(&slot.tracks, &tracks, provider) {
                        // Enrichment resend of the rows already on screen:
                        // backfill metadata in place so a selection made while
                        // the fetch ran survives.
                        for (existing, incoming) in slot.tracks.iter_mut().zip(&tracks) {
                            if let (Some(e), Some(i)) = (
                                existing.providers.get_mut(&provider),
                                incoming.providers.get(&provider),
                            ) {
                                e.duration = i.duration;
                                e.play_count = i.play_count;
                            }
                        }
                    } else {
                        slot.tracks = tracks;
                        slot.selection.clear();
                    }
                }
                self.finalize_view(idx);
                self.seed_artist_thumbnails(&view);
            }
            Err(msg) => {
                tracing::warn!("artist page load failed: {msg}");
                for loading in [
                    (&mut page.popular.loading, page.popular.provider),
                    (&mut page.albums.loading, page.albums.provider),
                    (&mut page.playlists.loading, page.playlists.provider),
                    (&mut page.related.loading, page.related.provider),
                ] {
                    if loading.1 == Some(provider) {
                        *loading.0 = false;
                    }
                }
                self.notify_error(format!("{}: {msg}", provider.label()));
            }
        }
    }

    /// Switch which provider supplies the header block (picture, bio). If
    /// that provider's header data is already cached it is applied instantly;
    /// otherwise a header-only fetch is kicked off.
    pub fn handle_artist_header_provider_changed(&mut self, provider: ProviderId) {
        let (name, cache_hit) = {
            let ViewKind::Artist { name, page, .. } = &mut self.view_data_mut().kind else {
                return;
            };
            page.header_provider = Some(provider);
            let cached = page
                .pages
                .get(&provider)
                .filter(|c| c.covers(&[ArtistDataKind::Header]))
                .and_then(|c| c.page.header.clone());
            if let (Some(existing), Some(incoming)) = (&mut page.header, &cached) {
                existing.image.clone_from(&incoming.image);
                existing.description.clone_from(&incoming.description);
            }
            (name.clone(), cached.is_some())
        };
        if cache_hit {
            // The newly selected provider's picture may never have been
            // seeded before (only the previous owner's was), so queue it.
            let view = self.view_data().clone();
            self.seed_artist_thumbnails(&view);
            return;
        }
        let rid = self.slot_request_id();
        self.load_artist_page(rid, &name, provider, &[ArtistDataKind::Header]);
    }

    /// Seed thumbnail downloads for the artist header and all card rows of
    /// the given artist-page view.
    pub(crate) fn seed_artist_thumbnails(&mut self, view: &ViewData) {
        let ViewKind::Artist { id, page, .. } = &view.kind else {
            return;
        };
        let Some(header) = &page.header else {
            return;
        };
        let key =
            crate::app::ui::artist::header_thumb_key(id, page.header_provider.unwrap_or_default());
        if !header.image.is_empty() {
            self.thumbnail_index.ensure(&key, &header.image);
        }
        let cards = page
            .albums
            .content
            .iter()
            .map(|c| (&c.id, &c.thumbnail))
            .chain(page.playlists.content.iter().map(|c| (&c.id, &c.thumbnail)))
            .chain(page.related.content.iter().map(|c| (&c.id, &c.thumbnail)));
        for (id, thumbnail) in cards {
            if !thumbnail.is_empty() {
                self.thumbnail_index.ensure(id, thumbnail);
            }
        }
    }
}
