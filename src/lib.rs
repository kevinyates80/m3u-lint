//! Parsing for M3U / M3U8 playlist files.
//!
//! The M3U format was never formally specified, and most files found in
//! the wild bend the informal rules in some way: a missing `#EXTM3U`
//! header, an `#EXTINF` line with no comma, a stray directive nobody
//! recognizes. By default this parser refuses all of that so a caller
//! can trust the result. Pass a lenient [`ParseOptions`] to get a
//! best-effort parse instead, where unrecognized or broken lines are
//! dropped rather than causing a hard failure.

use std::fmt;

/// One track entry in a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 1-based source line number of the path, for error reporting.
    pub line: usize,
    /// Duration in seconds from the preceding `#EXTINF` tag, if any.
    /// A value of -1 means "unknown length", per the original spec.
    pub duration_secs: Option<i64>,
    /// Free-text title from the preceding `#EXTINF` tag, if any.
    pub title: Option<String>,
    /// The path or URL on the entry's own line.
    pub path: String,
}

/// A parsed playlist: just an ordered list of entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub entries: Vec<Entry>,
}

impl Playlist {
    /// Sum of all entry durations, or `None` if any entry has an
    /// unknown or missing length.
    pub fn total_duration_secs(&self) -> Option<i64> {
        let mut total = 0i64;
        for entry in &self.entries {
            match entry.duration_secs {
                Some(secs) if secs >= 0 => total += secs,
                _ => return None,
            }
        }
        Some(total)
    }
}

/// Serializes back to M3U text. Always starts with `#EXTM3U`, and emits
/// an `#EXTINF` line before an entry only if it actually carries
/// duration metadata, so an entry with no metadata round-trips as a
/// bare path rather than gaining a fake `#EXTINF:-1,` line.
impl fmt::Display for Playlist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "#EXTM3U")?;
        for entry in &self.entries {
            if let Some(duration) = entry.duration_secs {
                match &entry.title {
                    Some(title) => writeln!(f, "#EXTINF:{duration},{title}")?,
                    None => writeln!(f, "#EXTINF:{duration}")?,
                }
            }
            writeln!(f, "{}", entry.path)?;
        }
        Ok(())
    }
}

/// Everything that can go wrong while parsing, in strict mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The file's first non-blank line was not `#EXTM3U`.
    MissingHeader,
    /// An `#EXTINF` line's duration or format couldn't be parsed.
    MalformedExtinf { line: usize, text: String },
    /// A directive other than `#EXTM3U` / `#EXTINF` was encountered.
    UnknownDirective { line: usize, text: String },
    /// An `#EXTINF` tag was never followed by a path line.
    DanglingExtinf { line: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingHeader => write!(f, "file does not start with #EXTM3U"),
            ParseError::MalformedExtinf { line, text } => {
                write!(f, "line {line}: malformed #EXTINF line: {text:?}")
            }
            ParseError::UnknownDirective { line, text } => {
                write!(f, "line {line}: unrecognized directive: {text:?}")
            }
            ParseError::DanglingExtinf { line } => {
                write!(f, "line {line}: #EXTINF has no path after it")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Controls how tolerant [`parse`] is of malformed input.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseOptions {
    /// When true, problems that would otherwise abort parsing are
    /// instead dropped: a missing header is ignored, an unrecognized
    /// directive is skipped, a broken `#EXTINF` line is treated as if
    /// it carried no metadata at all.
    pub lenient: bool,
}

/// Parse the text of an M3U/M3U8 playlist.
pub fn parse(input: &str, opts: &ParseOptions) -> Result<Playlist, ParseError> {
    let mut lines = input.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));

    let mut first_content: Option<(usize, &str)> = None;
    for (n, line) in lines.by_ref() {
        if !line.is_empty() {
            first_content = Some((n, line));
            break;
        }
    }

    let header_present = matches!(first_content, Some((_, "#EXTM3U")));
    if !header_present && !opts.lenient {
        return Err(ParseError::MissingHeader);
    }

    // If the first content line wasn't actually the header, it still
    // needs to be parsed as a normal line (only reachable in lenient
    // mode, since strict mode already returned above).
    let remaining: Vec<(usize, &str)> = match first_content {
        Some((n, line)) if line != "#EXTM3U" => {
            std::iter::once((n, line)).chain(lines).collect()
        }
        _ => lines.collect(),
    };

    let mut entries = Vec::new();
    let mut pending: Option<(usize, Option<i64>, Option<String>)> = None;

    for (n, line) in remaining {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            if let Some((pending_line, _, _)) = pending {
                if !opts.lenient {
                    return Err(ParseError::DanglingExtinf { line: pending_line });
                }
            }
            match parse_extinf(rest) {
                Some((duration, title)) => pending = Some((n, duration, title)),
                None if opts.lenient => pending = Some((n, None, None)),
                None => {
                    return Err(ParseError::MalformedExtinf {
                        line: n,
                        text: line.to_string(),
                    })
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            if !opts.lenient {
                return Err(ParseError::UnknownDirective {
                    line: n,
                    text: rest.trim().to_string(),
                });
            }
            continue;
        }
        let (duration_secs, title) = match pending.take() {
            Some((_, duration, title)) => (duration, title),
            None => (None, None),
        };
        entries.push(Entry {
            line: n,
            duration_secs,
            title,
            path: line.to_string(),
        });
    }

    if let Some((pending_line, _, _)) = pending {
        if !opts.lenient {
            return Err(ParseError::DanglingExtinf { line: pending_line });
        }
    }

    Ok(Playlist { entries })
}

/// Parse the part of an `#EXTINF:` line after the colon: a duration in
/// seconds, an optional comma, and an optional title.
fn parse_extinf(rest: &str) -> Option<(Option<i64>, Option<String>)> {
    let (duration_str, title) = match rest.split_once(',') {
        Some((d, t)) => (d, Some(t.to_string())),
        None => (rest, None),
    };
    let duration = duration_str.trim().parse::<i64>().ok()?;
    Some((Some(duration), title.filter(|t| !t.is_empty())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_playlist() {
        let input = "#EXTM3U\n#EXTINF:123,Artist - Title\nsongs/track1.mp3\n";
        let playlist = parse(input, &ParseOptions::default()).unwrap();
        assert_eq!(playlist.entries.len(), 1);
        assert_eq!(playlist.entries[0].duration_secs, Some(123));
        assert_eq!(playlist.entries[0].title.as_deref(), Some("Artist - Title"));
        assert_eq!(playlist.entries[0].path, "songs/track1.mp3");
    }

    #[test]
    fn rejects_missing_header_by_default() {
        let input = "songs/track1.mp3\n";
        let err = parse(input, &ParseOptions::default()).unwrap_err();
        assert_eq!(err, ParseError::MissingHeader);
    }

    #[test]
    fn lenient_mode_accepts_missing_header() {
        let input = "songs/track1.mp3\n";
        let playlist = parse(input, &ParseOptions { lenient: true }).unwrap();
        assert_eq!(playlist.entries.len(), 1);
        assert_eq!(playlist.entries[0].duration_secs, None);
    }

    #[test]
    fn rejects_dangling_extinf_by_default() {
        let input = "#EXTM3U\n#EXTINF:10,Only Metadata\n";
        let err = parse(input, &ParseOptions::default()).unwrap_err();
        assert!(matches!(err, ParseError::DanglingExtinf { line: 2 }));
    }

    #[test]
    fn rejects_malformed_extinf_by_default() {
        let input = "#EXTM3U\n#EXTINF:not-a-number,Title\nsongs/a.mp3\n";
        let err = parse(input, &ParseOptions::default()).unwrap_err();
        assert!(matches!(err, ParseError::MalformedExtinf { line: 2, .. }));
    }

    #[test]
    fn total_duration_is_none_when_any_entry_is_unknown() {
        let input = "#EXTM3U\n#EXTINF:-1,Live Stream\nhttp://example.com/stream\n";
        let playlist = parse(input, &ParseOptions::default()).unwrap();
        assert_eq!(playlist.total_duration_secs(), None);
    }

    #[test]
    fn writes_extinf_line_only_when_metadata_present() {
        let playlist = Playlist {
            entries: vec![
                Entry {
                    line: 0,
                    duration_secs: Some(213),
                    title: Some("Boards of Canada - Roygbiv".to_string()),
                    path: "music/boc/roygbiv.flac".to_string(),
                },
                Entry {
                    line: 0,
                    duration_secs: Some(120),
                    title: None,
                    path: "music/untitled.mp3".to_string(),
                },
                Entry {
                    line: 0,
                    duration_secs: None,
                    title: None,
                    path: "music/plain.mp3".to_string(),
                },
            ],
        };
        let expected = "#EXTM3U\n\
#EXTINF:213,Boards of Canada - Roygbiv\n\
music/boc/roygbiv.flac\n\
#EXTINF:120\n\
music/untitled.mp3\n\
music/plain.mp3\n";
        assert_eq!(playlist.to_string(), expected);
    }

    #[test]
    fn writer_output_round_trips_through_parser() {
        let input = "#EXTM3U\n#EXTINF:213,Boards of Canada - Roygbiv\nmusic/boc/roygbiv.flac\n#EXTINF:-1,Live Radio\nhttp://stream.example/live\nplain/no_metadata.mp3\n";
        let playlist = parse(input, &ParseOptions::default()).unwrap();
        let reparsed = parse(&playlist.to_string(), &ParseOptions::default()).unwrap();

        assert_eq!(playlist.entries.len(), reparsed.entries.len());
        for (original, roundtripped) in playlist.entries.iter().zip(reparsed.entries.iter()) {
            assert_eq!(original.duration_secs, roundtripped.duration_secs);
            assert_eq!(original.title, roundtripped.title);
            assert_eq!(original.path, roundtripped.path);
        }
    }
}
