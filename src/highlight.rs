//! Splits a log line into colored parts, the way `nh os switch` (through
//! nix-output-monitor) colors its output: verbs, store paths with a dimmed
//! hash and a bright name, caches, sizes and counts, hashes, and results.
//!
//! This module only finds the parts; the GUI maps [`Token`]s to colors.

use std::ops::Range;

use crate::build_log::LineKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Plain,
    /// Text of an announcement line, such as `these 27 derivations will be
    /// built:`.
    Header,
    /// `building` — nom shows builds in yellow.
    Build,
    /// `copying path`, `unpacking` — downloads.
    Fetch,
    /// Comin's component prefix, such as `nix:` or `deployer:`.
    Component,
    /// `/nix/store/<hash>-` and a trailing `.drv`.
    StoreHash,
    /// The readable name of a store path.
    StoreName,
    /// The `name>` prefix of builder output.
    DrvPrefix,
    Url,
    /// Commit ids, UUIDs, `<repo@1234abcd>`.
    Hash,
    /// Counts and sizes.
    Number,
    Success,
    Error,
    Warning,
    /// Noise, drawn dimmed as a whole.
    Muted,
}

const COMIN_COMPONENTS: &[&str] = &[
    "nix",
    "manager",
    "builder",
    "deployer",
    "store",
    "profile",
    "confirmer",
    "server",
    "fetcher",
    "executor",
];

const SIZE_UNITS: &[&str] = &[" KiB", " MiB", " GiB", " TiB", " B"];

const SUCCESS_WORDS: &[&str] = &["successfully", "succeeded", "deployment ended"];

const STORE_PREFIX: &str = "/nix/store/";

/// The parts of `message`, in order. The ranges cover the whole string and
/// always fall on char boundaries.
pub fn highlight(message: &str, kind: &LineKind, from_comin: bool) -> Vec<(Range<usize>, Token)> {
    let mut spans = Spans::default();
    let len = message.len();

    match kind {
        LineKind::Noise => {
            spans.push(0..len, Token::Muted);
            return spans.finish(len);
        }
        LineKind::Error => {
            spans.push(0..len, Token::Error);
            return spans.finish(len);
        }
        _ => {}
    }

    let mut start = 0;
    let mut base = Token::Plain;

    if let Some(label_end) = ["warning:", "trace:"]
        .iter()
        .find(|label| message.starts_with(*label))
        .map(|label| label.len())
    {
        spans.push(0..label_end, Token::Warning);
        start = label_end;
    } else if let LineKind::BuildOutput(name) = kind {
        let end = name.len() + 1;
        if message
            .get(..end)
            .is_some_and(|prefix| prefix.ends_with('>'))
        {
            spans.push(0..end, Token::DrvPrefix);
            start = end;
        }
    } else if message.starts_with("building ") {
        spans.push(0.."building".len(), Token::Build);
        start = "building".len();
    } else if let Some(verb) = ["copying path", "unpacking", "downloading"]
        .iter()
        .find(|verb| message.starts_with(*verb))
    {
        spans.push(0..verb.len(), Token::Fetch);
        start = verb.len();
    } else if matches!(kind, LineKind::Plan) && !message.starts_with(' ') {
        base = Token::Header;
    } else if from_comin
        && let Some((component, _)) = message.split_once(": ")
        && COMIN_COMPONENTS.contains(&component)
    {
        let end = component.len() + 1;
        spans.push(0..end, Token::Component);
        start = end;
    }

    scan(message, start, base, &mut spans);
    spans.finish(len)
}

/// Collects spans and merges neighbors of the same token.
#[derive(Default)]
struct Spans(Vec<(Range<usize>, Token)>);

impl Spans {
    fn push(&mut self, range: Range<usize>, token: Token) {
        if range.is_empty() {
            return;
        }
        if let Some((last, last_token)) = self.0.last_mut()
            && *last_token == token
            && last.end == range.start
        {
            last.end = range.end;
            return;
        }
        self.0.push((range, token));
    }

    fn finish(self, len: usize) -> Vec<(Range<usize>, Token)> {
        debug_assert_eq!(self.0.last().map_or(0, |(range, _)| range.end), len);
        self.0
    }
}

fn scan(message: &str, from: usize, base: Token, spans: &mut Spans) {
    let bytes = message.as_bytes();
    let mut i = from;
    let mut plain_start = from;

    let flush = |spans: &mut Spans, plain_start: usize, at: usize| {
        spans.push(plain_start..at, base);
    };

    while i < message.len() {
        let rest = &message[i..];
        let at_word_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();

        // Store paths: dim hash, bright name, dim `.drv`.
        if let Some(after) = rest.strip_prefix(STORE_PREFIX)
            && let Some(hash_len) = store_hash_len(after)
        {
            flush(spans, plain_start, i);
            let hash_end = i + STORE_PREFIX.len() + hash_len;
            spans.push(i..hash_end, Token::StoreHash);
            let name_len = message[hash_end..]
                .find([' ', '\'', '"', '/', ')', ',', ':', '^'])
                .unwrap_or(message.len() - hash_end);
            let name_end = hash_end + name_len;
            let name = &message[hash_end..name_end];
            if let Some(stem) = name.strip_suffix(".drv") {
                spans.push(hash_end..hash_end + stem.len(), Token::StoreName);
                spans.push(hash_end + stem.len()..name_end, Token::StoreHash);
            } else {
                spans.push(hash_end..name_end, Token::StoreName);
            }
            i = name_end;
            plain_start = i;
            continue;
        }

        // URLs and flake references.
        if at_word_start
            && ["https://", "http://", "file://", "github:", "git+"]
                .iter()
                .any(|scheme| rest.starts_with(scheme))
        {
            flush(spans, plain_start, i);
            let end = i + rest.find([' ', '\'', '"', ')']).unwrap_or(rest.len());
            spans.push(i..end, Token::Url);
            i = end;
            plain_start = i;
            continue;
        }

        // `<repo@1234abcd>` from the shortened nix commands.
        if rest.starts_with("<repo@")
            && let Some(close) = rest.find('>')
        {
            flush(spans, plain_start, i);
            spans.push(i..i + close + 1, Token::Hash);
            i += close + 1;
            plain_start = i;
            continue;
        }

        if at_word_start {
            // UUIDs and full commit ids.
            let word_len = rest
                .find(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
                .unwrap_or(rest.len());
            let word = &rest[..word_len];
            if is_uuid(word) || (word.len() == 40 && word.bytes().all(|b| b.is_ascii_hexdigit())) {
                flush(spans, plain_start, i);
                spans.push(i..i + word_len, Token::Hash);
                i += word_len;
                plain_start = i;
                continue;
            }

            // Counts and sizes: `27`, `3.2 GiB`.
            let digits = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let after_digits = &rest[digits..];
            // Only free-standing numbers: not versions such as `3.14-modal`.
            let starts_number = i == 0 || matches!(bytes[i - 1], b' ' | b'(' | b'[' | b',');
            let ends_number = after_digits
                .chars()
                .next()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            if digits > 0 && bytes[i].is_ascii_digit() && starts_number && ends_number {
                let unit = SIZE_UNITS
                    .iter()
                    .find(|unit| after_digits.starts_with(*unit))
                    .map_or(0, |unit| unit.len());
                flush(spans, plain_start, i);
                spans.push(i..i + digits + unit, Token::Number);
                i += digits + unit;
                plain_start = i;
                continue;
            }

            if let Some(word) = SUCCESS_WORDS.iter().find(|word| rest.starts_with(*word)) {
                flush(spans, plain_start, i);
                spans.push(i..i + word.len(), Token::Success);
                i += word.len();
                plain_start = i;
                continue;
            }
        }

        i += rest.chars().next().map_or(1, char::len_utf8);
    }

    flush(spans, plain_start, message.len());
}

/// The length of `<hash>-` at the start of a store path, for a full
/// 32-character hash or one shortened by [`compact_store_paths`].
fn store_hash_len(after_prefix: &str) -> Option<usize> {
    let hash = after_prefix
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(after_prefix.len());
    let rest = &after_prefix[hash..];
    if hash == 32 && rest.starts_with('-') {
        Some(33)
    } else if (1..32).contains(&hash) && rest.starts_with("…-") {
        Some(hash + "…-".len())
    } else {
        None
    }
}

/// Shortens every store hash to its first 7 characters (`/nix/store/1hcxlkv…-name`),
/// so the readable name stays visible on narrow rows.
pub fn compact_store_paths(text: &str) -> String {
    let mut compact = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(STORE_PREFIX) {
        let after = &rest[start + STORE_PREFIX.len()..];
        compact.push_str(&rest[..start + STORE_PREFIX.len()]);
        if after.len() > 32
            && after.as_bytes()[..32].iter().all(u8::is_ascii_alphanumeric)
            && after.as_bytes()[32] == b'-'
        {
            compact.push_str(&after[..7]);
            compact.push('…');
            rest = &after[32..];
        } else {
            rest = after;
        }
    }
    compact.push_str(rest);
    compact
}

fn is_uuid(word: &str) -> bool {
    word.len() == 36
        && word.char_indices().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(message: &str, kind: LineKind, from_comin: bool) -> Vec<(&str, Token)> {
        let spans = highlight(message, &kind, from_comin);
        // The spans cover the message without gaps or overlaps.
        let mut expected_start = 0;
        for (range, _) in &spans {
            assert_eq!(range.start, expected_start);
            expected_start = range.end;
        }
        assert_eq!(expected_start, message.len());
        spans
            .into_iter()
            .map(|(range, token)| (&message[range], token))
            .collect()
    }

    #[test]
    fn building_lines_split_the_store_path() {
        assert_eq!(
            parts(
                "building '/nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm211p2-home-manager.drv'...",
                LineKind::Building("home-manager".into()),
                false,
            ),
            vec![
                ("building", Token::Build),
                (" '", Token::Plain),
                (
                    "/nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm211p2-",
                    Token::StoreHash
                ),
                ("home-manager", Token::StoreName),
                (".drv", Token::StoreHash),
                ("'...", Token::Plain),
            ]
        );
    }

    #[test]
    fn fetch_lines_color_the_cache() {
        let spans = parts(
            "copying path '/nix/store/k39ck8k9ddxy84bihnacckwp736nwkzn-codex-0.156.1' from 'https://cache.numtide.com'...",
            LineKind::Fetching {
                name: "codex-0.156.1".into(),
                cache: "https://cache.numtide.com".into(),
            },
            false,
        );
        assert_eq!(spans[0], ("copying path", Token::Fetch));
        assert!(spans.contains(&("codex-0.156.1", Token::StoreName)));
        assert!(spans.contains(&("https://cache.numtide.com", Token::Url)));
    }

    #[test]
    fn plan_headers_highlight_counts_and_sizes() {
        let spans = parts(
            "these 13 paths will be fetched (0.0 KiB download, 3.2 GiB unpacked):",
            LineKind::Plan,
            false,
        );
        assert_eq!(spans[0], ("these ", Token::Header));
        assert!(spans.contains(&("13", Token::Number)));
        assert!(spans.contains(&("0.0 KiB", Token::Number)));
        assert!(spans.contains(&("3.2 GiB", Token::Number)));
    }

    #[test]
    fn builder_output_keeps_its_prefix() {
        let spans = parts(
            "system-path> created 47172 symlinks in user environment",
            LineKind::BuildOutput("system-path".into()),
            false,
        );
        assert_eq!(spans[0], ("system-path>", Token::DrvPrefix));
        assert!(spans.contains(&("47172", Token::Number)));
    }

    #[test]
    fn comin_lines_color_component_hashes_and_results() {
        let spans = parts(
            "nix: command 'nix eval <repo@8120812c>#nixosConfigurations' successfully executed",
            LineKind::Step,
            true,
        );
        assert_eq!(spans[0], ("nix:", Token::Component));
        assert!(spans.contains(&("<repo@8120812c>", Token::Hash)));
        assert!(spans.contains(&("successfully", Token::Success)));

        let spans = parts(
            "manager: a generation is evaluating for commit 8120812c1c0e3bc731a814d8da26a1f38e27996d",
            LineKind::Step,
            true,
        );
        assert_eq!(
            spans.last(),
            Some(&("8120812c1c0e3bc731a814d8da26a1f38e27996d", Token::Hash))
        );

        let spans = parts(
            "builder: build of generation d558cf65-2df9-4869-8855-74165854ddf9 is starting",
            LineKind::Step,
            true,
        );
        assert!(spans.contains(&("d558cf65-2df9-4869-8855-74165854ddf9", Token::Hash)));
    }

    #[test]
    fn errors_warnings_and_noise_are_whole_line_colors() {
        assert_eq!(
            parts("error: builder failed", LineKind::Error, false),
            vec![("error: builder failed", Token::Error)]
        );
        assert_eq!(
            parts("remote: Total 4491", LineKind::Noise, false),
            vec![("remote: Total 4491", Token::Muted)]
        );
        assert_eq!(
            parts("warning: no info dir", LineKind::Warning, false)[0],
            ("warning:", Token::Warning)
        );
    }

    #[test]
    fn compacted_store_paths_keep_their_colors() {
        let compact =
            compact_store_paths("  /nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm211p2-home-manager.drv");
        assert_eq!(compact, "  /nix/store/1hcxlkv…-home-manager.drv");
        assert_eq!(
            parts(&compact, LineKind::Plan, false),
            vec![
                ("  ", Token::Plain),
                ("/nix/store/1hcxlkv…-", Token::StoreHash),
                ("home-manager", Token::StoreName),
                (".drv", Token::StoreHash),
            ]
        );
        assert_eq!(compact_store_paths("/nix/store/short"), "/nix/store/short");
    }

    #[test]
    fn numbers_inside_words_are_left_alone() {
        let spans = parts("python3.14-modal v2 x86_64", LineKind::Other, false);
        assert_eq!(spans, vec![("python3.14-modal v2 x86_64", Token::Plain)]);
    }

    #[test]
    fn truncated_and_multibyte_lines_stay_on_char_boundaries() {
        let message = "building '/nix/store/1hcxlkvd7hmp5r0kvfz6mn12zpm2…";
        let _ = parts(message, LineKind::Building("x".into()), false);
        let _ = parts("é 12 ⏎ /nix/store/…", LineKind::Other, false);
    }
}
