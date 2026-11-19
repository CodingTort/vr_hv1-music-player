//! Playlists as tiny CSV files of YouTube ids, to carry them to another
//! machine: `youtube_id,title,artist`, one song per row. The id is what
//! matters; title and artist keep the file readable and let songs that
//! didn't come from YouTube still be matched by name.

use crate::library::models::Track;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub artist: String,
}

pub const HEADER: &str = "youtube_id,title,artist";

pub fn to_csv<'a>(tracks: impl IntoIterator<Item = &'a Track>) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for t in tracks {
        let fields = [t.yt_id.as_deref().unwrap_or(""), t.title.as_str(), t.artist.as_str()];
        out.push_str(&fields.map(quote).join(","));
        out.push('\n');
    }
    out
}

/// Reads rows, skipping the header and blank lines. Lenient: a file of bare
/// ids (one per line) works too.
pub fn parse_csv(text: &str) -> Vec<Row> {
    text.lines()
        .map(|l| l.trim_start_matches('\u{feff}'))
        .filter(|l| !l.trim().is_empty() && l.trim() != HEADER)
        .map(|l| {
            let mut fields = split_fields(l).into_iter();
            let mut next = || fields.next().unwrap_or_default().trim().to_string();
            Row { id: next(), title: next(), artist: next() }
        })
        .filter(|r| !r.id.is_empty() || !r.title.is_empty())
        .collect()
}

fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn split_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().unwrap().push('"');
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(String::new()),
            (c, _) => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: Option<&str>, title: &str, artist: &str) -> Track {
        Track {
            id: 0,
            path: Default::default(),
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            track_number: None,
            disc_number: None,
            duration: Default::default(),
            cover_data: None,
            genre: String::new(),
            year: None,
            play_count: 0,
            liked: false,
            date_played: None,
            yt_id: id.map(Into::into),
        }
    }

    #[test]
    fn round_trips_awkward_titles() {
        let tracks = [
            track(Some("abcDEF12345"), "Rain, Again", "Some \"Band\""),
            track(None, "夜の歌", "テスト歌手"),
        ];
        let csv = to_csv(&tracks);
        assert!(csv.starts_with("youtube_id,title,artist\n"));
        assert_eq!(
            parse_csv(&csv),
            vec![
                Row { id: "abcDEF12345".into(), title: "Rain, Again".into(), artist: "Some \"Band\"".into() },
                Row { id: "".into(), title: "夜の歌".into(), artist: "テスト歌手".into() },
            ]
        );
    }

    #[test]
    fn accepts_bare_ids() {
        assert_eq!(parse_csv("abcDEF12345\n\nxyz_-987654\n").len(), 2);
    }

    #[test]
    fn finds_youtube_ids() {
        use crate::library::scanner::youtube_id;
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=abcDEF12345").as_deref(), Some("abcDEF12345"));
        assert_eq!(youtube_id("https://music.youtube.com/watch?v=abcDEF12345&list=x").as_deref(), Some("abcDEF12345"));
        assert_eq!(youtube_id("https://youtu.be/abcDEF12345?t=3").as_deref(), Some("abcDEF12345"));
        assert_eq!(youtube_id("https://example.com/watch?v=abcDEF12345"), None);
        assert_eq!(youtube_id("no link here"), None);
    }
}
