# m3u-lint

A parser for M3U/M3U8 playlist files, plus a small CLI to check one.

## Why

M3U has no real spec. Every media player writes it slightly differently,
and every player reads it slightly more forgivingly than the last, which
is how you end up with files that "work" everywhere except the one tool
that actually cares about correctness. This library goes the other way:
by default it treats a playlist as broken if the header is missing, an
`#EXTINF` line doesn't parse, or a directive it doesn't recognize shows
up. If you need to process real-world files that bend the rules, you can
opt into a lenient mode explicitly instead of the parser silently
guessing on your behalf.

## What counts as strict

Given this playlist:

```
#EXTM3U
#EXTINF:213,Boards of Canada - Roygbiv
music/boc/roygbiv.flac
#EXTINF:-1,Live Radio
http://stream.example/live
```

By default, `parse` will reject the file if:

- it doesn't start with `#EXTM3U`
- an `#EXTINF` line has a duration that isn't an integer
- an `#EXTINF` line is never followed by a path
- any other `#`-prefixed directive shows up (only `#EXTM3U` and
  `#EXTINF` are understood right now)

A duration of `-1` is treated as "unknown length" per the original
format, not an error — but it does mean `total_duration_secs()` can no
longer give you an exact total.

## CLI usage

```
$ m3u-lint my_mix.m3u
my_mix.m3u: 2 tracks
total duration: unknown (some tracks missing length)

$ m3u-lint broken.m3u
broken.m3u: line 3: malformed #EXTINF line: "#EXTINF:oops,Title"
(retry with --lenient to parse past this)

$ m3u-lint --lenient broken.m3u
broken.m3u: 4 tracks
total duration: 12m 40s

$ m3u-lint --check-files my_mix.m3u
my_mix.m3u: 2 tracks
total duration: unknown (some tracks missing length)
line 3: file not found: music/boc/roygbiv.flac
```

`--check-files` resolves relative paths against the playlist's own
directory, skips entries that are URLs, and exits non-zero if anything
is missing.

## Library usage

```rust
use m3u_lint::{parse, ParseOptions};

let text = std::fs::read_to_string("my_mix.m3u")?;

// Strict by default.
let playlist = parse(&text, &ParseOptions::default())?;

// Or explicitly tolerate malformed input.
let playlist = parse(&text, &ParseOptions { lenient: true })?;

for entry in &playlist.entries {
    println!("{:?} — {}", entry.title, entry.path);
}

// Playlist implements Display, so it serializes back to M3U text.
// An entry only gets an #EXTINF line if it actually has duration
// metadata; a bare path round-trips as a bare path.
let text = playlist.to_string();
```

## Status

Early. Parsing and writing both work and are tested; there's no
support yet for extended tags beyond `#EXTINF`, and duplicate tracks
are not reported. See the issues for what's planned next.

## License

MIT, see [LICENSE](LICENSE).
