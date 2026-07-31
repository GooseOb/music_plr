#!/usr/bin/env python3
"""Migrate Spotify Liked Songs CSV to music_plr playlist.json via YouTube Music."""

import csv
import json
import os
import sys
import time
import html

from ytmusicapi import YTMusic

CSV_PATH = os.path.expanduser("~/projects/Liked_Songs.csv")
PLAYLIST_PATH = os.path.expanduser("~/.config/music_plr/playlists.json")

yt = YTMusic()


def search_track(track_name, artist_name):
    query = f"{artist_name} - {track_name}"
    try:
        results = yt.search(query, filter="songs", limit=3)
        if not results:
            results = yt.search(f"{artist_name} {track_name}", limit=3)
        if results:
            for r in results:
                if r.get("resultType") != "song":
                    continue
                vid_id = r.get("videoId")
                if not vid_id:
                    continue
                title = html.unescape(r.get("title", track_name))
                artists_raw = r.get("artists")
                if artists_raw and isinstance(artists_raw, list):
                    artist = html.unescape(
                        ", ".join(
                            a.get("name", "")
                            for a in artists_raw
                            if isinstance(a, dict)
                        )
                    )
                elif isinstance(artists_raw, dict):
                    artist = html.unescape(artists_raw.get("name", artist_name))
                else:
                    artist = artist_name
                duration = r.get("duration", None)
                if duration is None:
                    duration_seconds = 0
                elif isinstance(duration, str) and ":" in duration:
                    parts = duration.split(":")
                    if len(parts) == 2:
                        duration_seconds = int(parts[0]) * 60 + int(parts[1])
                    elif len(parts) == 3:
                        duration_seconds = (
                            int(parts[0]) * 3600 + int(parts[1]) * 60 + int(parts[2])
                        )
                    else:
                        duration_seconds = 0
                else:
                    duration_seconds = int(duration) if duration else 0

                return {
                    "id": vid_id,
                    "title": title,
                    "artist": artist,
                    "duration": duration_seconds,
                    "url": f"https://youtube.com/watch?v={vid_id}",
                    "source": "YouTube",
                }
        return None
    except Exception as e:
        print(f"  Error searching: {e}", file=sys.stderr)
        return None


def main():
    print(f"Reading {CSV_PATH}...")
    with open(CSV_PATH, "r", encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        rows = list(reader)

    print(f"Found {len(rows)} tracks. Searching YouTube Music...")

    tracks = []
    failed = []

    for i, row in enumerate(rows):
        track_name = row.get("Track Name", "").strip()
        artist_name = row.get("Artist Name(s)", "").strip()
        spotify_uri = row.get("Track URI", "").strip()

        if not track_name or not artist_name:
            failed.append((track_name, artist_name, "Missing name/artist"))
            continue

        print(
            f"  [{i + 1}/{len(rows)}] {artist_name} - {track_name}", end="", flush=True
        )

        result = search_track(track_name, artist_name)
        if result:
            tracks.append(result)
            print(f" -> {result['id']}")
        else:
            failed.append((track_name, artist_name, "No YouTube result"))
            print(" -> NOT FOUND")

        if (i + 1) % 10 == 0:
            time.sleep(1)

    print(f"\nMatched {len(tracks)} / {len(rows)} tracks.")

    existing = {"playlists": []}
    if os.path.exists(PLAYLIST_PATH):
        with open(PLAYLIST_PATH, "r") as f:
            existing = json.load(f)

    existing["playlists"] = [
        p for p in existing["playlists"] if p.get("name") != "Spotify Liked Songs"
    ]
    existing["playlists"].insert(0, {"name": "Spotify Liked Songs", "tracks": tracks})

    os.makedirs(os.path.dirname(PLAYLIST_PATH), exist_ok=True)
    with open(PLAYLIST_PATH, "w") as f:
        json.dump(existing, f, indent=2, ensure_ascii=False)

    print(f"\nWritten to {PLAYLIST_PATH}")
    print(f"Playlist now has {len(tracks)} tracks.")

    if failed:
        print(f"\nFailed to find {len(failed)} tracks:")
        for name, artist, reason in failed:
            print(f"  {artist} - {name}: {reason}")


if __name__ == "__main__":
    main()
