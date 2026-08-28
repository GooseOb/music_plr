use super::{thread, BackendResult, MusicPlayer, ViewData};
use crate::{
    app::{view_data::ArtistEntry, ViewKind},
    load_state::LoadState,
    providers::{
        spawn_artist_kinds_fetch, ArtistDataKind, ArtistKindData, ArtistKindResult,
        ArtistPageState, ArtistSectionKind, ProviderId, SectionContent,
    },
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
    pub fn open_artist(&mut self, id: Option<&str>, name: &str, source: ProviderId) {
        let kind = ViewKind::Artist(ArtistEntry {
            id: id.unwrap_or_default().to_string(),
            name: name.to_string(),
            source,
            page: Box::new(match id {
                Some(id) => ArtistPageState::new(source, id),
                None => ArtistPageState::loading_for(source),
            }),
        });
        // Popular tracks render from the view's track list; keep it in the
        // Loading state until they arrive instead of an empty "Nothing here".
        self.push_new_view(ViewData {
            kind,
            content: LoadState::Loading,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        let msg = (self.strings.opening_artist)(name);
        self.notify(msg);

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

    /// Fetch each kind of `provider`'s artist data in the background,
    /// resolving the artist id by name when unknown. Results arrive as one
    /// [`BackendResult::ArtistSectionLoaded`] per kind as soon as its data
    /// is available.
    fn load_artist_page(
        &mut self,
        rid: u64,
        name: &str,
        provider: ProviderId,
        kinds: &'static [ArtistDataKind],
    ) {
        let known_id = {
            let ViewKind::Artist(entry) = &mut self.view_data_mut().kind else {
                return;
            };
            entry.page.provider_ids.get(&provider).cloned()
        };
        let name = name.to_string();
        let not_found = self.strings.could_not_find_on;
        let tx = self.result_tx.clone();

        thread::spawn(move || {
            let fail_all = |msg: String| {
                for &kind in kinds {
                    let _ = tx.send(BackendResult::ArtistSectionLoaded {
                        rid,
                        provider,
                        kind,
                        data: Box::new(Err(msg.clone())),
                    });
                }
            };
            let resolved_id = match known_id {
                Some(id) => id,
                None => match crate::providers::resolve_artist_id(provider, &name) {
                    // The id is only reported back when freshly resolved;
                    // an already-known one needs no caching.
                    Ok(Some(id)) => {
                        let _ = tx.send(BackendResult::ArtistIdResolved {
                            rid,
                            provider,
                            resolved_id: id.clone(),
                        });
                        id
                    }
                    Ok(None) => {
                        fail_all((not_found)(&name, provider.label()));
                        return;
                    }
                    Err(e) => {
                        fail_all(e.to_string());
                        return;
                    }
                },
            };
            // YouTube's enrichment resend of Popular arrives later from the
            // fetch itself; everything here forwards as it lands.
            for ArtistKindResult(kind, data) in
                spawn_artist_kinds_fetch(provider, &resolved_id, kinds)
            {
                let _ = tx.send(BackendResult::ArtistSectionLoaded {
                    rid,
                    provider,
                    kind,
                    data: Box::new(data),
                });
            }
        });
    }

    /// Reuse the slot's live request id so results keep matching this
    /// view even across several concurrent loads.
    pub(super) fn slot_request_id(&mut self) -> u64 {
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
        let served;
        {
            let ViewKind::Artist(entry) = &mut self.view_data_mut().kind else {
                return;
            };

            // Serve from the per-provider cache when this section's data was
            // already fetched — no loading state, no request.
            served = entry.page.serve_cached_section(section_kind, provider);
            if served.is_none() {
                entry.page.start_section_load(section_kind, provider);
            }
            name = entry.name.clone();
        }
        if let Some(content) = served {
            if let SectionContent::Tracks(tracks) = content {
                // Popular tracks mirror into the view's track list so the
                // usual interactions keep working on them.
                let slot = self.view_data_mut();
                slot.set_tracks(tracks);
                slot.selection.clear();
            }
            return;
        }
        if section_kind == ArtistSectionKind::Popular {
            self.view_data_mut().content = LoadState::Loading;
        }
        // A fresh attempt must be allowed to toast its own failure.
        self.artist_error_dedup = None;
        let rid = self.slot_request_id();
        self.load_artist_page(rid, &name, provider, section_kind.data_kinds());
    }

    /// Cache a freshly resolved per-provider artist id on the page that
    /// requested it.
    pub fn apply_artist_id_resolved(&mut self, rid: u64, provider: ProviderId, resolved_id: &str) {
        let Some(idx) = self.slot_for_request(rid) else {
            return;
        };
        if let ViewKind::Artist(entry) = &mut self.nav_history[idx].kind {
            entry
                .page
                .provider_ids
                .insert(provider, resolved_id.to_string());
        }
    }

    /// Merge one finished artist-section fetch into its view slot: cache it
    /// under `provider`, mark the owning section ready (or failed), and —
    /// for popular tracks — refresh the view's track list.
    pub fn apply_artist_section(
        &mut self,
        rid: u64,
        provider: ProviderId,
        kind: ArtistDataKind,
        result: Result<ArtistKindData, String>,
    ) {
        let Some(idx) = self.slot_for_request(rid) else {
            return;
        };
        let ViewKind::Artist(entry) = &mut self.nav_history[idx].kind else {
            return;
        };
        let page = &mut entry.page;
        let succeeded = result.is_ok();
        match result {
            Ok(data) => {
                page.pages
                    .entry(provider)
                    .or_default()
                    .merge_kind(kind, &data);
                if let ArtistKindData::Header(header) = &data {
                    page.merge_header(provider, Some(header));
                } else {
                    let section_kind = ArtistSectionKind::ALL
                        .into_iter()
                        .find(|k| k.data_kind() == kind);
                    if let Some(section_kind) = section_kind {
                        if page.section(section_kind).provider == Some(provider) {
                            page.section_mut(section_kind).state =
                                LoadState::Ready(data.to_section_content());
                            if section_kind == ArtistSectionKind::Popular {
                                if let ArtistKindData::Popular(tracks) = data {
                                    let slot = &mut self.nav_history[idx];
                                    Self::install_popular_tracks(slot, tracks, provider);
                                }
                            }
                        }
                    }
                }
            }
            Err(ref msg) => {
                tracing::warn!("artist {kind:?} load failed: {msg}");
                page.fail_section(provider, kind, msg);
                if kind == ArtistDataKind::Popular
                    && page.section(ArtistSectionKind::Popular).provider == Some(provider)
                    && self.nav_history[idx].content.is_loading()
                {
                    self.nav_history[idx].content = LoadState::Failed(msg.clone());
                }
                // One logical failure fans out to one message per kind;
                // surface only the first as a toast.
                if self.artist_error_dedup != Some((rid, provider)) {
                    self.artist_error_dedup = Some((rid, provider));
                    self.notify_error(format!("{}: {msg}", provider.label()));
                }
            }
        }
        if succeeded {
            if self.artist_error_dedup == Some((rid, provider)) {
                self.artist_error_dedup = None;
            }
            self.finalize_view(idx);
            let view = self.nav_history[idx].clone();
            self.seed_artist_thumbnails(&view);
        }
    }

    /// Put freshly loaded popular tracks into the view slot. An enrichment
    /// resend of the rows already on screen backfills metadata in place so a
    /// selection made while the fetch ran survives.
    fn install_popular_tracks(
        slot: &mut ViewData,
        tracks: Vec<crate::types::Track>,
        provider: ProviderId,
    ) {
        let enriched = match slot.tracks_mut() {
            Some(existing) if same_popular_ids(existing, &tracks, provider) => {
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

    /// Switch which provider supplies the header block (picture, bio). If
    /// that provider's header data is already cached it is applied instantly;
    /// otherwise a header-only fetch is kicked off.
    pub fn handle_artist_header_provider_changed(&mut self, provider: ProviderId) {
        let (name, cache_hit) = {
            let ViewKind::Artist(entry) = &mut self.view_data_mut().kind else {
                return;
            };
            let (name, page) = (&entry.name, &mut entry.page);
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
        let ViewKind::Artist(entry) = &view.kind else {
            return;
        };
        let (id, page) = (&entry.id, &entry.page);
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
