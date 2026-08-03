import sys, json
try:
    from ytmusicapi import YTMusic
except ImportError:
    print(json.dumps({"error": "ytmusicapi not installed"}))
    sys.exit(1)
query = sys.argv[1] if len(sys.argv) > 1 else ""
limit = int(sys.argv[2]) if len(sys.argv) > 2 else 10
if not query:
    print(json.dumps([]))
    sys.exit(0)
results = YTMusic().search(query, filter="songs", limit=limit)
out = []
for r in results:
    vid = r.get("videoId", "")
    if not vid:
        continue
    artists = r.get("artists", [])
    artist = artists[0].get("name", "") if artists else ""
    duration = r.get("duration_seconds", 0) or 0
    thumbs = r.get("thumbnails") or []
    thumb = thumbs[-1].get("url", "") if thumbs else ""
    out.append({
        "id": vid,
        "title": r.get("title", ""),
        "url": f"https://youtube.com/watch?v={vid}",
        "duration": duration,
        "thumbnail": thumb,
        "channel": artist,
    })
print(json.dumps(out))
