//! LRC timestamp parsing and formatting, shared by the LRCLib-compatible
//! providers and user-edited lyrics.

pub(super) fn format_timestamp(secs: f32) -> String {
    let total = secs.max(0.0);
    let min = (total / 60.0).floor() as u32;
    let sec = total - min as f32 * 60.0;
    format!("{min:02}:{sec:05.2}")
}

pub(super) fn parse_lrc(text: &str) -> Vec<(f32, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((secs, content)) = parse_lrc_line(line) {
            out.push((secs, content));
        }
    }
    out
}

pub(super) fn parse_lrc_line(line: &str) -> Option<(f32, String)> {
    let open = line.find('[')?;
    let close = line[open..].find(']')? + open;
    let stamp = &line[open + 1..close];
    let content = line[close + 1..].trim().to_string();
    let secs = parse_timestamp(stamp)?;
    Some((secs, content))
}

fn parse_timestamp(stamp: &str) -> Option<f32> {
    let mut parts = stamp.split(':');
    let min: f32 = parts.next()?.parse().ok()?;
    let sec: f32 = parts.next()?.parse().ok()?;
    Some(min * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc() {
        let text = "[00:12.34]First line\n[00:16.80]Second line\n[00:21.05]Chorus";
        let timed = parse_lrc(text);
        assert_eq!(timed.len(), 3);
        assert!((timed[0].0 - 12.34).abs() < 0.001);
        assert_eq!(timed[0].1, "First line");
        assert!((timed[2].0 - 21.05).abs() < 0.001);
    }

    #[test]
    fn skips_lines_without_timestamp() {
        let text = "intro\n[00:01.00]Real line";
        let timed = parse_lrc(text);
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0].1, "Real line");
    }
}
