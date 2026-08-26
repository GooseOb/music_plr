use super::Strings;
use crate::providers::ProviderId;

pub const STRINGS: Strings = Strings {
    search: "Szukaj",
    downloads: "Pobieranie",
    settings: "Ustawienia",
    library: "Biblioteka",
    nothing_saved_yet: "Nic jeszcze nie zapisano",
    new_playlist_name: "Nazwa nowej playlisty",

    queue: "Kolejka",
    recently_played: "Ostatnio odtwarzane",
    now_playing_from: "Teraz odtwarzane z",
    now_playing: "Teraz odtwarzane",
    up_next: "Następne",
    no_track_playing: "Nic nie jest odtwarzane",
    no_more_tracks_in_queue: "Brak więcej utworów w kolejce",
    no_recently_played_tracks: "Brak ostatnio odtwarzanych utworów",

    not_playing: "Nie odtwarza się",
    no_tracks_found: "Nie znaleziono utworów",

    searching: "Wyszukiwanie...",
    loading: "Wczytywanie...",
    load_more: "Wczytaj więcej",
    no_results_found: "Brak wyników",
    no_recent_searches: "Brak ostatnich wyszukiwań",
    generating_radio: "Generowanie radia...",

    not_an_artist_page: "To nie jest strona wykonawcy",
    provided_by: "Dostarcza",
    retry: "Ponów",
    nothing_here: "Tu nic nie ma",
    most_popular_songs: "Najpopularniejsze utwory",
    albums: "Albumy",
    playlists: "Playlisty",
    fans_also_like: "Fani lubią też",

    looking_up_lyrics: "Szukanie tekstu…",
    play_a_track_for_lyrics: "Odtwórz utwór, aby zobaczyć jego tekst.",
    lyrics_selectable: "Zaznaczany",
    lyrics_synced: "Synchronizowany",
    lyrics_plain: "Zwykły",

    playlist_not_found: "Nie znaleziono playlisty",
    add_local: "Dodaj lokalne",
    downloaded_tracks: "Pobrane utwory",
    no_downloaded_tracks: "Brak pobranych utworów",

    ctx_play: "Odtwórz",
    ctx_edit: "Edytuj",
    ctx_go_to_artist: "Przejdź do wykonawcy",
    ctx_add_to_playlist: "Dodaj do playlisty",
    ctx_download: "Pobierz",
    ctx_song_radio: "Radio utworu",
    ctx_artist_radio: "Radio wykonawcy",
    ctx_remove_from_queue: "Usuń z kolejki",
    ctx_remove_from_playlist: "Usuń z playlisty",
    sub_play_via: "Odtwórz przez",
    sub_download_from: "Pobierz z",
    sub_via: "Przez",
    sub_on: "Na",
    via_search_suffix: "(szukaj)",

    current: "(bieżący)",
    select: "wybierz",
    providers: "Dostawcy",
    no_provider_data: "Ten utwór nie ma danych dostawców.",
    save: "Zapisz",
    cancel: "Anuluj",
    edit_track: "Edytuj utwór",
    lbl_title: "Tytuł",
    lbl_artist: "Wykonawca",
    ph_track_title: "Tytuł utworu",
    ph_track_artist: "Wykonawca utworu",
    delete: "Usuń",
    delete_playlist_q: "Usunąć playlistę?",
    tracks_wont_be_deleted: "Utwory nie zostaną usunięte.",
    lbl_id: "Id",
    lbl_url: "Url",
    lbl_artist_id: "ID wykonawcy",
    lbl_duration_secs: "Czas trwania (sekundy)",
    lbl_thumbnail: "Miniatura",
    lbl_album: "Album",

    sec_playback: "Odtwarzanie",
    sec_storage: "Pamięć",
    sec_history: "Historia",
    language_lbl: "Język",
    default_provider_lbl: "Domyślny dostawca streamingu i pobierania",
    normalize_volume_lbl: "Normalizuj głośność między utworami",
    download_dir_lbl: "Katalog pobierania",
    cache_size_lbl: "Maks. rozmiar cache streamingu (MB)",
    hist_rows_lbl: "Widoczne wiersze historii wyszukiwania",
    hist_entries_lbl: "Przechowywane wpisy historii wyszukiwania",
    recent_kept_lbl: "Przechowywane ostatnio odtwarzane utwory",
    reset_defaults: "Przywróć domyślne",

    find_in_list: "Znajdź na liście…",
    scope_songs: "Utwory",
    scope_videos: "Filmy",
    scope_artists: "Wykonawcy",
    scope_albums: "Albumy",
    scope_playlists: "Playlisty",
    radio_word_song: "utworu",
    radio_word_artist: "wykonawcy",

    lyrics_copied: "Tekst skopiowany do schowka",
    saved_to_library: "Zapisano w bibliotece",
    removed_from_library: "Usunięto z biblioteki",
    no_lyrics_found: "Nie znaleziono tekstu dla tego utworu.",
    select_playlist_drop: "Wybierz playlistę, aby upuścić utwory",
    reordered_playlist: "Zmieniono kolejność playlisty",

    n_saved: |n| format!("zapisano: {n}"),
    n_plays: |n| format!("{} odtworzeń", crate::util::format_count(n as u64)),
    search_placeholder: |p| match p {
        ProviderId::YouTube => "Szukaj w YouTube Music...".into(),
        ProviderId::Local => "Szukaj...".into(),
        other => format!("Szukaj w {}...", other.label()),
    },
    added: |n| format!("Dodano {}", pl_tracks(n)),
    added_to: |n, to| format!("Dodano {} do: {to}", pl_tracks(n)),
    removed_n: |n| format!("Usunięto {}", pl_tracks(n)),
    removed_from: |n, from| format!("Usunięto {} z: {from}", pl_tracks(n)),
    pasted_into: |n, into| format!("Wklejono {} do: {into}", pl_tracks(n)),
    saved_title: |t| format!("Zapisano \"{t}\" w bibliotece"),
    playlist_created: |n| format!("Playlista \"{n}\" utworzona"),
    added_local: |n| {
        format!(
            "Dodano {} (wybierz playlistę, aby uporządkować)",
            pl_tracks(n)
        )
    },
    downloading_n: |n| format!("Pobieranie {}...", pl_gen_tracks(n)),
    resolving_on: |title, p| format!("Rozwiązywanie \"{title}\" na {p}..."),
    provider_no_radio: |p| format!("{p} nie obsługuje radia"),
    generating_radio_for: |w, name| format!("Generowanie radia ({w}): {name}..."),
    could_not_find_on: |name, p| format!("Nie znaleziono \"{name}\" na {p}"),
    opening: |l| format!("Otwieranie: {l}..."),
    opening_artist: |name| format!("Otwieranie wykonawcy: {name}..."),
    download_complete: |path| format!("Pobieranie ukończone! Zapisano w {path}"),
    failed_resolve_on: |title, p, e| format!("Nie udało się rozwiązać \"{title}\" na {p}: {e}"),
    couldnt_load: |e| format!("Nie udało się wczytać: {e}"),
    couldnt_load_lyrics: |e| format!("Nie udało się wczytać tekstu: {e}"),
    search_failed: |e| format!("Wyszukiwanie nie powiodło się: {e}"),
    radio_label: |w, name| format!("Radio ({w}): {name}"),
};

fn pl_tracks(n: usize) -> String {
    match n {
        1 => "1 utwór".into(),
        2..=4 => format!("{n} utwory"),
        _ => format!("{n} utworów"),
    }
}

fn pl_gen_tracks(n: usize) -> String {
    match n {
        1 => "1 utworu".into(),
        _ => format!("{n} utworów"),
    }
}
