import sys, json
try:
    from ytmusicapi import YTMusic
except ImportError:
    print(json.dumps({"error": "ytmusicapi not installed"}))
    sys.exit(1)

# Scope -> ytmusicapi filter argument. "all" uses the general search (no
# filter) so it blends songs/videos/artists/albums/playlists by overall
# relevance — this is what matches the real YouTube Music site behavior and is
# what surfaced "Karolinerna - November 1700" as the top hit.
SCOPE_FILTER = {
    "all": None,
    "songs": "songs",
    "videos": "videos",
    "artists": "artists",
    "albums": "albums",
    "playlists": "playlists",
}

# The resultType ytmusicapi reports for each filtered scope (always singular,
# even though the scope names are plural).
EXPECTED_TYPE = {
    "songs": "song",
    "videos": "video",
    "artists": "artist",
    "albums": "album",
    "playlists": "playlist",
}


def _make_track(vid, title, artist, *, duration=0, thumbnail="",
                album=None, artist_id=None, views=""):
    return {
        "kind": "track",
        "resultType": "song",
        "id": vid,
        "title": title,
        "subtitle": artist,
        "url": f"https://youtube.com/watch?v={vid}",
        # Either a number (search's duration_seconds) or a raw "M:SS" /
        # "H:MM:SS" string; the Rust side parses both.
        "duration": duration,
        "thumbnail": thumbnail,
        "channel": artist,
        "artist_id": artist_id,
        "album": album,
        "views": views,
    }


def _yt():
    return YTMusic()


def search(query, scope="all", limit=20):
    filt = SCOPE_FILTER.get(scope, None)
    if filt is None:
        # General search: over-fetch so we can still trim to `limit` after we
        # keep only the playable/known types.
        results = _yt().search(query, limit=limit * 3)
    else:
        # Filtered endpoints return only that result type and rank within it.
        results = _yt().search(query, filter=filt, limit=limit)
    out = []
    for r in results:
        rt = r.get("resultType")
        # Paranoia guard: if the endpoint ever returns mixed types, drop
        # anything that doesn't match the requested scope.
        if scope != "all" and rt != EXPECTED_TYPE.get(scope):
            continue
        item = result_to_item(r, rt)
        if item is not None:
            out.append(item)
        if len(out) >= limit:
            break
    return out


def result_to_item(r, rt):
    thumbs = r.get("thumbnails") or []
    thumb = thumbs[-1].get("url", "") if thumbs else ""
    if rt in ("song", "video"):
        vid = r.get("videoId", "")
        if not vid:
            return None
        artists = r.get("artists", [])
        artist = artists[0].get("name", "") if artists else ""
        artist_id = artists[0].get("id") if artists else None
        duration = r.get("duration_seconds", 0) or 0
        album = r.get("album") or {}
        album_name = album.get("name", "") or ""
        album_id = album.get("id", "") or ""
        album_obj = (
            {"name": album_name, "id": album_id}
            if album_name and album_id
            else None
        )
        return _make_track(
            vid, r.get("title", ""), artist,
            duration=duration, thumbnail=thumb, album=album_obj,
            artist_id=artist_id, views=r.get("views") or "",
        )
    if rt == "artist":
        bid = r.get("browseId", "")
        if not bid:
            return None
        name = r.get("artist") or r.get("name") or r.get("title") or ""
        return {
            "kind": "artist",
            "resultType": rt,
            "id": bid,
            "title": name,
            "subtitle": "",
            "url": f"https://music.youtube.com/channel/{bid}",
            "duration": 0,
            "thumbnail": thumb,
            "channel": name,
        }
    if rt == "album":
        bid = r.get("browseId", "")
        if not bid:
            return None
        artists = r.get("artists", [])
        artist = artists[0].get("name", "") if artists else ""
        return {
            "kind": "album",
            "resultType": rt,
            "id": bid,
            "title": r.get("title", ""),
            "subtitle": artist,
            "url": f"https://music.youtube.com/browse/{bid}",
            "duration": 0,
            "thumbnail": thumb,
            "channel": artist,
        }
    if rt == "playlist":
        pid = r.get("browseId", "")
        if not pid:
            return None
        author = r.get("author", "")
        if isinstance(author, list):
            author = ", ".join(a.get("name", "") for a in author if isinstance(a, dict))
        return {
            "kind": "playlist",
            "resultType": rt,
            "id": pid,
            "title": r.get("title", ""),
            "subtitle": author or "",
            "url": f"https://music.youtube.com/playlist?list={pid}",
            "duration": 0,
            "thumbnail": thumb,
            "channel": author or "",
        }
    return None


def _track_from_album_entry(e, album=None):
    vid = e.get("videoId") or ""
    if not vid:
        return None
    artist = e.get("artists", [{}])[0].get("name", "") if e.get("artists") \
        else (e.get("artist", "") or "")
    thumb = (e.get("thumbnails") or [{}])[-1].get("url", "")
    # get_album reports an exact integer view count; normalize to string so
    # the Rust side can parse it like the abbreviated shelf counts.
    views = e.get("views")
    return _make_track(
        vid, e.get("title", ""), artist,
        duration=e.get("duration") or 0, thumbnail=thumb, album=album,
        views=str(views) if views else "",
    )

def browse(browse_id, limit=50, kind=None):
    """Return the tracks inside an artist/album/playlist.

    `kind` is optional but lets us pick the right endpoint. For a playlist the
    id is a playlistId (usually starts with PL or OL); for albums it is an
    album browseId (starts with MPRE); for artists it is a channelId
    (starts with UC). We sniff from the id when `kind` is not given.
    """
    yt = _yt()
    out = []
    if kind == "album" or (kind is None and browse_id.startswith("MPRE")):
        album = yt.get_album(browse_id)
        album_name = album.get("title", "") or ""
        album_id = album.get("albumId") or album.get("browseId", "") or browse_id
        album_obj = {"name": album_name, "id": album_id}
        for e in album.get("tracks", []):
            t = _track_from_album_entry(e, album_obj)
            if t:
                out.append(t)
            if len(out) >= limit:
                break
        return out
    if kind == "playlist" or (kind is None and (browse_id.startswith("PL") or browse_id.startswith("OL"))):
        pl = yt.get_playlist(browse_id, limit=limit)
        for e in pl.get("tracks", []):
            vid = e.get("videoId") or e.get("browseId") or ""
            if not vid.startswith("PL") and not vid.startswith("OL"):
                t = _track_from_album_entry(e)
                if t:
                    out.append(t)
        return out
    # artist: get_artist's "songs" is just a shelf with a browseId; the real
    # tracks live in artist["songs"]["results"], so fetch once and read that.
    try:
        artist = yt.get_artist(browse_id)
        for e in artist.get("songs", {}).get("results", []):
            vid = e.get("videoId") or ""
            if not vid:
                continue
            thumb = (e.get("thumbnails") or [{}])[-1].get("url", "")
            out.append(
                _make_track(
                    vid, e.get("title", ""), e.get("artist", ""),
                    duration=e.get("duration") or 0, thumbnail=thumb,
                    artist_id=browse_id,
                )
            )
            if len(out) >= limit:
                break
    except Exception:
        pass
    return out


def artist_page(browse_id):
    """Return the full artist page: header stats, popular tracks and the
    albums/singles/playlists/related-artists shelves."""
    yt = _yt()
    a = yt.get_artist(browse_id)

    def thumb(e):
        return (e.get("thumbnails") or [{}])[-1].get("url", "")

    header = {
        "image": thumb(a),
        "stats": [],
        "description": a.get("description", "") or "",
    }
    if a.get("monthlyListeners"):
        header["stats"].append(["Monthly listeners", a["monthlyListeners"]])
    if a.get("subscribers"):
        header["stats"].append(["YouTube Subscribers", a["subscribers"]])

    popular = []
    for e in a.get("songs", {}).get("results", []):
        vid = e.get("videoId") or ""
        if not vid:
            continue
        artists = e.get("artists") or []
        # The songs shelf carries the track's album; keep it so rows and
        # drill-downs can show it.
        album = e.get("album") or {}
        album_obj = (
            {"name": album.get("name", ""), "id": album.get("id", "")}
            if album.get("name") and album.get("id")
            else None
        )
        t = _make_track(
            vid, e.get("title", ""),
            artists[0].get("name", "") if artists else "",
            duration=e.get("duration") or 0, thumbnail=thumb(e),
            album=album_obj,
            artist_id=browse_id,
            views=e.get("views") or "",
        )
        popular.append(t)

    albums = []
    for badge in (None, "Single"):
        shelf = a.get("albums" if badge is None else "singles", {})
        for e in shelf.get("results", []):
            bid = e.get("browseId") or ""
            if not bid:
                continue
            label = e.get("type", "") or badge or ""
            albums.append({
                "id": bid,
                "title": e.get("title", ""),
                "date": e.get("year", "") or "",
                "badge": label,
                "thumbnail": thumb(e),
            })

    playlists = []
    for e in a.get("playlists", {}).get("results", []):
        pid = e.get("playlistId") or e.get("browseId") or ""
        if not pid:
            continue
        playlists.append({
            "id": pid,
            "title": e.get("title", ""),
            "subtitle": "",
            "thumbnail": thumb(e),
        })

    related = []
    for e in a.get("related", {}).get("results", []):
        rid = e.get("browseId") or ""
        if not rid:
            continue
        related.append({
            "id": rid,
            "name": e.get("title", ""),
            "stat": e.get("subscribers", "") or "",
            "thumbnail": thumb(e),
        })

    return {
        "header": header,
        "popular": popular,
        "albums": albums,
        "playlists": playlists,
        "related": related,
    }


def watch_playlist(video_id=None, playlist_id=None, radio=True, limit=50):
    """Return the tracks of a YouTube Music radio/mix playlist.

    This is the same engine that powers the site's "Start radio" button.
    For a song radio pass `video_id`; for an artist/playlist radio pass the
    `playlist_id`. A raw artist channel browseId (starts with `UC`) is
    resolved to its generated `radioId` via `get_artist`, since
    `get_watch_playlist` only accepts radio/mix playlist ids (`RD...`) or a
    videoId. `radio=True` seeds the autoplay continuation so the returned
    list is the generated mix rather than a static playlist.
    """
    yt = _yt()
    # A channel browseId can't be fed straight to get_watch_playlist; resolve
    # it to the artist's radio playlist id first.
    if playlist_id and playlist_id.startswith("UC"):
        try:
            artist = yt.get_artist(playlist_id)
            radio_id = artist.get("radioId")
            if radio_id:
                playlist_id = radio_id
        except Exception:
            pass
    try:
        wp = yt.get_watch_playlist(
            videoId=video_id, playlistId=playlist_id, radio=radio
        )
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)
    tracks = wp.get("tracks", [])
    out = []
    for t in tracks:
        vid = t.get("videoId") or ""
        if not vid:
            continue
        artists = t.get("artists", [])
        artist = artists[0].get("name", "") if artists else ""
        artist_id = artists[0].get("id") if artists else None
        # watch_playlist omits numeric duration and thumbnails, so pass the
        # raw length string (parsed Rust-side) and synthesize the standard
        # thumbnail URL. This avoids a slow per-track yt-dlp
        # metadata pass (which added ~100s for a 50-track radio).
        thumbs = t.get("thumbnails") or []
        thumbnail = thumbs[-1].get("url", "") if thumbs else f"https://i.ytimg.com/vi/{vid}/mqdefault.jpg"
        out.append(
            _make_track(
                vid, t.get("title", ""), artist,
                duration=t.get("length") or 0, thumbnail=thumbnail,
                artist_id=artist_id,
            )
        )
        if len(out) >= limit:
            break
    return out


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "browse":
        bid = sys.argv[2] if len(sys.argv) > 2 else ""
        limit = int(sys.argv[3]) if len(sys.argv) > 3 else 50
        kind = sys.argv[4] if len(sys.argv) > 4 else None
        print(json.dumps(browse(bid, limit, kind)))
        return
    if cmd == "artist_page":
        bid = sys.argv[2] if len(sys.argv) > 2 else ""
        print(json.dumps(artist_page(bid)))
        return
    if cmd == "watch":
        # watch <videoId|''> <playlistId|''> [limit]
        video_id = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else None
        playlist_id = sys.argv[3] if len(sys.argv) > 3 and sys.argv[3] else None
        limit = int(sys.argv[4]) if len(sys.argv) > 4 else 50
        print(json.dumps(watch_playlist(video_id, playlist_id, True, limit)))
        return
    # default: search <query> [scope] [limit]
    query = sys.argv[2] if len(sys.argv) > 2 else ""
    scope = sys.argv[3] if len(sys.argv) > 3 else "all"
    limit = int(sys.argv[4]) if len(sys.argv) > 4 else 20
    if not query:
        print(json.dumps([]))
        return
    print(json.dumps(search(query, scope, limit)))


if __name__ == "__main__":
    main()
