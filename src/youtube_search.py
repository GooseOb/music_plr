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


def _yt():
    return YTMusic()


def search(query, scope="all", limit=20):
    filt = SCOPE_FILTER.get(scope, None)
    if filt is not None:
        # Filtered endpoints return only that result type and rank within it.
        results = _yt().search(query, filter=filt, limit=limit)
    else:
        # General search: over-fetch so we can still trim to `limit` after we
        # keep only the playable/known types.
        results = _yt().search(query, limit=limit * 3)
    out = []
    for r in results:
        rt = r.get("resultType")
        # Paranoia guard: if the endpoint ever returns mixed types, drop
        # anything that doesn't match the requested scope.
        if scope != "all" and rt != scope.rstrip("s"):
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
        duration = r.get("duration_seconds", 0) or 0
        return {
            "kind": "track",
            "resultType": rt,
            "id": vid,
            "title": r.get("title", ""),
            "subtitle": artist,
            "url": f"https://youtube.com/watch?v={vid}",
            "duration": duration,
            "thumbnail": thumb,
            "channel": artist,
        }
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


def _track_from_album_entry(e):
    vid = e.get("videoId") or ""
    if not vid:
        return None
    return {
        "kind": "track",
        "resultType": "song",
        "id": vid,
        "title": e.get("title", ""),
        "subtitle": e.get("artists", [{}])[0].get("name", "")
        if e.get("artists")
        else (e.get("artist", "") or ""),
        "url": f"https://youtube.com/watch?v={vid}",
        "duration": _dur(e.get("duration")),
        "thumbnail": (e.get("thumbnails") or [{}])[-1].get("url", ""),
        "channel": e.get("artists", [{}])[0].get("name", "")
        if e.get("artists")
        else (e.get("artist", "") or ""),
    }


def _dur(d):
    if not d:
        return 0
    parts = [int(x) for x in str(d).split(":")]
    secs = 0
    for p in parts:
        secs = secs * 60 + p
    return secs


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
        for e in album.get("tracks", []):
            t = _track_from_album_entry(e)
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
    # artist
    artist = yt.get_artist(browse_id)
    songs = artist.get("songs", {})
    for e in songs.get("browseId") and [] or []:
        pass
    # get_artist's "songs" is just a shelf with a browseId; fetch the full
    # song list via the artist's songs endpoint.
    try:
        songs_list = yt.get_artist(browse_id)
        # The artist's top songs are in artist["songs"]["results"].
        for e in songs_list.get("songs", {}).get("results", []):
            vid = e.get("videoId") or ""
            if vid:
                out.append(
                    {
                        "kind": "track",
                        "resultType": "song",
                        "id": vid,
                        "title": e.get("title", ""),
                        "subtitle": e.get("artist", ""),
                        "url": f"https://youtube.com/watch?v={vid}",
                        "duration": _dur(e.get("duration")),
                        "thumbnail": (e.get("thumbnails") or [{}])[-1].get("url", ""),
                        "channel": e.get("artist", ""),
                    }
                )
            if len(out) >= limit:
                break
    except Exception:
        pass
    return out


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "browse":
        bid = sys.argv[2] if len(sys.argv) > 2 else ""
        limit = int(sys.argv[3]) if len(sys.argv) > 3 else 50
        kind = sys.argv[4] if len(sys.argv) > 4 else None
        print(json.dumps(browse(bid, limit, kind)))
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
