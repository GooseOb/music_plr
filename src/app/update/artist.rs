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

        let fetch = move || -> anyhow::Result<(String, ArtistPage)> {
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
        let name;
        let cached_tracks;
        {
            let ViewKind::Artist { name: n, page, .. } = &mut self.view_data_mut().kind else {
                return;
            };

            // Serve from the per-provider cache when this section's data was
            // already fetched — no loading state, no request.
            cached_tracks = page.serve_cached_section(section_kind, provider);
            if cached_tracks.is_none() {
                page.start_section_load(section_kind, provider);
            }
            name = n.clone();
        }
        if let Some(tracks) = cached_tracks {
            let slot = self.view_data_mut();
            slot.set_tracks(tracks);
            slot.selection.clear();
            return;
        }
        let rid = self.slot_request_id();
        self.load_artist_page(rid, &name, provider, section_kind.data_kinds());
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
                page.merge_header(provider, fetched.header.as_ref());
                let new_tracks = page.apply_fetch(kinds, provider, &fetched);
                if let Some(tracks) = new_tracks {
                    let slot = &mut self.nav_history[idx];
                    let enriched = match slot.tracks_mut() {
                        Some(existing) if same_popular_ids(existing, &tracks, provider) => {
                            // Enrichment resend of the rows already on screen:
                            // backfill metadata in place so a selection made while
                            // the fetch ran survives.
                            for (existing, incoming) in existing.iter_mut().zip(&tracks) {
                                if let (Some(e), Some(i)) = (
                                    existing.providers.get_mut(&provider),
                                    incoming.providers.get(&provider),
                                ) {
                                    e.duration = i.duration;
                                    e.play_count = i.play_count;
                                }
                            }
                            true
                        }
                        _ => false,
                    };
                    if !enriched {
                        slot.set_tracks(tracks);
                        slot.selection.clear();
                    }
                }
                self.finalize_view(idx);
                let view = self.nav_history[idx].clone();
                self.seed_artist_thumbnails(&view);
            }
            Err(msg) => {
                tracing::warn!("artist page load failed: {msg}");
                page.fail_sections(provider, &msg);
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
        if let Some(header) = &page.header {
            let key = crate::app::ui::artist::header_thumb_key(
                id,
                page.header_provider.unwrap_or_default(),
            );
            if !header.image.is_empty() {
                self.thumbnail_index.ensure(&key, &header.image);
            }
        }
        for (id, thumbnail) in page.card_thumbs() {
            if !thumbnail.is_empty() {
                self.thumbnail_index.ensure(id, thumbnail);
            }
        }
    }
}
