/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
//! ZIP walking, JSON manifest decoding, post/photo deserialization, and
//! the mojibake / bare-URL text normalizers. Runs on a blocking thread
//! (no DB or S3 access); the async worker in `mod.rs` consumes the
//! [`ParseOutcome`] this module returns.

use crate::imports;
use blake2::{Blake2b512, Digest};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

/// Per-post body cap. Defends against pathological archives without
/// truncating real posts (FB's own UI limit is ~63K chars).
const BODY_MAX_CHARS: usize = 100_000;

/// Normalized representation of one FB post extracted from the manifest.
/// `attachment_uris` are paths relative to the ZIP root; the worker
/// resolves them against the archive at import time.
#[derive(Debug, Clone)]
pub(crate) struct FacebookPost {
    /// Stable hash used for dedup against existing imports.
    pub external_id: String,
    /// Unix seconds. Mapped to `posts.published_at`.
    pub timestamp: i64,
    /// Markdown body. Empty when the FB post was media-only.
    pub body: String,
    /// In-archive media references. Each item resolves to a single ZIP
    /// entry the worker will upload to S3.
    pub attachment_uris: Vec<String>,
}

/// Normalized representation of one FB album manifest extracted from
/// `your_facebook_activity/posts/album/*.json`. Phase 9d: albums are now
/// imported as `albums` entities, NOT synthesized into posts. Each
/// `attachment_uris` resolves to one `album_media` row.
#[derive(Debug, Clone)]
pub(crate) struct FacebookAlbum {
    /// Stable hash used for dedup against existing imports in the
    /// `albums` table.
    pub external_id: String,
    /// Unix seconds. Mapped to `albums.published_at`.
    pub timestamp: i64,
    /// Album display name (mapped to `albums.name`). Required: empty-name
    /// FB albums fall back to a synthesized "Untitled album <ts>".
    pub name: String,
    /// Optional album description (`albums.description`).
    pub description: Option<String>,
    /// Ordered list of `(uri, optional_caption)` pairs. The current FB
    /// schema has no per-photo caption field, so caption is always None
    /// today; the field is kept for the wire so a future FB export
    /// version with captions can populate it without a re-shape.
    pub attachments: Vec<(String, Option<String>)>,
}

/// Bundled result from `parse_archive`: the flat post list, the album
/// list, plus a vec of log events that the async caller flushes via
/// `append_log` after the blocking thread returns. Avoids passing a DB
/// handle into the blocking closure.
#[derive(Default)]
pub(crate) struct ParseOutcome {
    pub posts: Vec<FacebookPost>,
    pub albums: Vec<FacebookAlbum>,
    pub log_events: Vec<ParseLogEvent>,
}

pub(crate) struct ParseLogEvent {
    pub level: &'static str,
    pub msg: String,
    pub ctx: Option<serde_json::Value>,
}

/// Walk the archive central directory, find every post manifest and album
/// manifest, parse them, and return a single flat list. Manifest decoding
/// errors are recorded as warn events and skipped per-file rather than
/// failing the whole import; this keeps a malformed sibling from blocking
/// the rest. Operates on a blocking-thread file handle; the caller wraps
/// in `spawn_blocking`.
///
/// Returns regular posts first, then album posts. After the two passes
/// an "albums win" rule drops regular posts whose body is empty and whose
/// only attachments are URIs already claimed by an album, so auto-generated
/// "added a new photo." entries don't double up the gallery they came from.
pub(crate) fn parse_archive(path: &Path) -> Result<ParseOutcome, anyhow::Error> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut post_manifest_names: Vec<String> = Vec::new();
    let mut album_manifest_names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if is_post_manifest(&name) {
            post_manifest_names.push(name);
        } else if is_album_manifest(&name) {
            album_manifest_names.push(name);
        }
    }

    let mut log_events: Vec<ParseLogEvent> = Vec::new();
    log_events.push(ParseLogEvent {
        level: imports::LOG_LEVEL_INFO,
        msg: format!(
            "Found {} post manifest(s) and {} album manifest(s) in archive",
            post_manifest_names.len(),
            album_manifest_names.len()
        ),
        ctx: Some(serde_json::json!({
            "post_manifests": post_manifest_names.clone(),
            "album_manifests": album_manifest_names.clone(),
        })),
    });

    let mut regular_posts: Vec<FacebookPost> = Vec::new();
    for name in post_manifest_names {
        let before = regular_posts.len();
        let mut entry = archive.by_name(&name)?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        match serde_json::from_str::<Vec<RawFacebookPost>>(&json) {
            Ok(parsed) => {
                let raw_count = parsed.len();
                for raw in parsed {
                    if let Some(post) = normalize_post(raw) {
                        regular_posts.push(post);
                    }
                }
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_INFO,
                    msg: format!(
                        "Parsed post manifest {}: {} of {} entries normalized",
                        name,
                        regular_posts.len() - before,
                        raw_count
                    ),
                    ctx: None,
                });
            }
            Err(e) => {
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_WARN,
                    msg: format!(
                        "Skipping {} (matched post-manifest pattern but failed to decode as post list: {})",
                        name, e
                    ),
                    ctx: None,
                });
            }
        }
    }

    let mut albums: Vec<FacebookAlbum> = Vec::new();
    for name in album_manifest_names {
        let mut entry = archive.by_name(&name)?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        match serde_json::from_str::<RawAlbum>(&json) {
            Ok(raw) => {
                let album_name = raw.name.clone();
                let photo_count = raw.photos.len();
                if let Some(parsed_album) = normalize_album(raw) {
                    albums.push(parsed_album);
                    log_events.push(ParseLogEvent {
                        level: imports::LOG_LEVEL_INFO,
                        msg: format!(
                            "Parsed album manifest {}: '{}' with {} photo(s)",
                            name,
                            album_name.as_deref().unwrap_or("(unnamed)"),
                            photo_count
                        ),
                        ctx: None,
                    });
                } else {
                    log_events.push(ParseLogEvent {
                        level: imports::LOG_LEVEL_WARN,
                        msg: format!(
                            "Album {} produced no importable album (no photos or no timestamps)",
                            name
                        ),
                        ctx: None,
                    });
                }
            }
            Err(e) => {
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_WARN,
                    msg: format!(
                        "Skipping {} (matched album-manifest pattern but failed to decode as album dict: {})",
                        name, e
                    ),
                    ctx: None,
                });
            }
        }
    }

    // Albums-win dedup: any regular post that contributes no body text and
    // whose attachment URIs are entirely claimed by an album is suppressed
    // so the same image doesn't appear twice on the timeline.
    let album_uris: HashSet<String> = albums
        .iter()
        .flat_map(|a| a.attachments.iter().map(|(u, _)| u.clone()))
        .collect();
    let mut suppressed = 0usize;
    regular_posts.retain(|p| {
        if p.body.is_empty()
            && !p.attachment_uris.is_empty()
            && p.attachment_uris.iter().all(|u| album_uris.contains(u))
        {
            suppressed += 1;
            false
        } else {
            true
        }
    });
    if suppressed > 0 {
        log_events.push(ParseLogEvent {
            level: imports::LOG_LEVEL_INFO,
            msg: format!(
                "Albums-win dedup suppressed {} body-less regular post(s) whose media is owned by an album",
                suppressed
            ),
            ctx: None,
        });
    }

    Ok(ParseOutcome {
        posts: regular_posts,
        albums,
        log_events,
    })
}

/// Accept any post-list manifest under the archive. FB names these
/// inconsistently across export versions:
/// - older exports: `your_posts_1.json`
/// - newer exports: `your_posts__check_ins__photos_and_videos_1.json`
///
/// The basename must start with `your_posts_` AND end with `_<digits>.json`.
/// That admits both shapes and rejects siblings like `your_posts_check_ins.json`
/// (no digit suffix), `shared_memories.json` (wrong prefix), or
/// `your_uncategorized_photos.json` (wrong prefix). If a future archive
/// surfaces a new shape under the same prefix, the JSON decode fallback
/// in `parse_archive` skips it with a warning rather than failing the
/// whole import.
fn is_post_manifest(name: &str) -> bool {
    let basename = name.rsplit('/').next().unwrap_or(name);
    if !basename.starts_with("your_posts_") || !basename.ends_with(".json") {
        return false;
    }
    let stem = &basename[..basename.len() - ".json".len()];
    let Some(last_us) = stem.rfind('_') else {
        return false;
    };
    let suffix = &stem[last_us + 1..];
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

/// Album manifests live at `posts/album/<basename>.json` (one album per
/// file, dict shape, not a list of posts).
fn is_album_manifest(name: &str) -> bool {
    let lower = name;
    if !lower.ends_with(".json") {
        return false;
    }
    if let Some(idx) = lower.find("posts/album/") {
        let rest = &lower[idx + "posts/album/".len()..];
        return !rest.is_empty() && !rest.contains('/');
    }
    false
}

#[derive(Debug, Deserialize)]
struct RawFacebookPost {
    #[serde(default)]
    timestamp: Option<i64>,
    #[serde(default)]
    data: Vec<RawDataEntry>,
    #[serde(default)]
    attachments: Vec<RawAttachmentBlock>,
    // FB's `title` field carries auto-generated activity descriptions like
    // "X updated his status." which are noise rather than content. The
    // parser intentionally does not deserialize it.
}

#[derive(Debug, Deserialize)]
struct RawDataEntry {
    #[serde(default)]
    post: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAttachmentBlock {
    #[serde(default)]
    data: Vec<RawAttachmentItem>,
}

#[derive(Debug, Deserialize)]
struct RawAttachmentItem {
    #[serde(default)]
    media: Option<RawMedia>,
}

#[derive(Debug, Deserialize)]
struct RawMedia {
    #[serde(default)]
    uri: Option<String>,
}

/// FB album manifest shape (`posts/album/<n>.json`). One album per file,
/// dict-shaped, not a list of posts.
#[derive(Debug, Deserialize)]
struct RawAlbum {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    photos: Vec<RawAlbumPhoto>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    last_modified_timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawAlbumPhoto {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    creation_timestamp: Option<i64>,
}

/// Normalize one album manifest into a `FacebookAlbum` (Phase 9d).
/// Albums are imported as `albums` rows with `album_media` items, not
/// synthesized as posts with photos crammed into the body. The timestamp
/// prefers the earliest `creation_timestamp` across the photos so
/// re-exports don't shift the album date when an album is touched in FB;
/// `last_modified_timestamp` is a fallback when no photo carries a
/// creation timestamp.
fn normalize_album(raw: RawAlbum) -> Option<FacebookAlbum> {
    let attachments: Vec<(String, Option<String>)> = raw
        .photos
        .iter()
        .filter_map(|p| p.uri.as_ref().map(|u| (u.clone(), None)))
        .collect();
    if attachments.is_empty() {
        return None;
    }

    let earliest_creation = raw.photos.iter().filter_map(|p| p.creation_timestamp).min();
    let timestamp = earliest_creation
        .or(raw.last_modified_timestamp)
        // No timestamps anywhere — drop. The album entity requires a
        // published_at and we have no defensible value to set.
        ?;

    let raw_name = raw
        .name
        .map(|s| fix_facebook_mojibake(s.trim()).into_owned())
        .filter(|s| !s.is_empty());
    // FB albums often lack a name; synthesize a stable placeholder so we
    // never fall back on storing an empty string in `albums.name`.
    let name = raw_name.unwrap_or_else(|| format!("Untitled album {}", timestamp));

    let description = raw
        .description
        .map(|s| fix_facebook_mojibake(s.trim()).into_owned())
        .filter(|s| !s.is_empty());

    // External id is hashed over (timestamp, name, first uri) so the same
    // album re-exported produces the same id even if the description is
    // touched later.
    let first_uri = attachments.first().map(|(u, _)| u.as_str());
    let external_id = compute_external_id(timestamp, &name, first_uri);
    Some(FacebookAlbum {
        external_id,
        timestamp,
        name,
        description,
        attachments,
    })
}

fn normalize_post(raw: RawFacebookPost) -> Option<FacebookPost> {
    let timestamp = raw.timestamp?;
    // Concatenate body text from each data entry; FB sometimes splits a
    // post across multiple entries (e.g. text + a "feeling" annotation).
    // FB's auto-generated titles are activity descriptions, not content,
    // and are deliberately ignored. A post with no `data[].post` text and
    // no attachments is dropped further down.
    let body_parts: Vec<String> = raw
        .data
        .into_iter()
        .filter_map(|d| d.post)
        .map(|s| {
            let fixed = fix_facebook_mojibake(s.trim());
            match wrap_bare_urls(fixed.as_ref()) {
                Cow::Owned(wrapped) => wrapped,
                Cow::Borrowed(_) => fixed.into_owned(),
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    let mut body = body_parts.join("\n\n");
    if body.chars().count() > BODY_MAX_CHARS {
        body = body.chars().take(BODY_MAX_CHARS).collect::<String>() + "\n\n[truncated]";
    }

    let attachment_uris: Vec<String> = raw
        .attachments
        .into_iter()
        .flat_map(|b| b.data.into_iter())
        .filter_map(|a| a.media.and_then(|m| m.uri))
        .collect();

    // A media-only post with no body should still import (the media is the
    // content). A post with neither body nor media is dropped.
    if body.is_empty() && attachment_uris.is_empty() {
        return None;
    }

    let external_id = compute_external_id(
        timestamp,
        &body,
        attachment_uris.first().map(|s| s.as_str()),
    );
    Some(FacebookPost {
        external_id,
        timestamp,
        body,
        attachment_uris,
    })
}

/// Best-effort fixup for Facebook's latin1-as-UTF8 double-encoding.
///
/// FB's exporter JSON-stringifies UTF-8 byte sequences as latin1
/// codepoints, so each byte of a multi-byte UTF-8 sequence shows up as
/// its own `\u00XX` escape. Examples:
/// - `«` (U+00AB, UTF-8 `C2 AB`) lands as `Â«` (U+00C2 U+00AB).
/// - `'` (U+2019, UTF-8 `E2 80 99`) lands as three chars `â\u{0080}\u{0099}`.
/// - 🦢 (U+1F9A2, UTF-8 `F0 9F A6 A2`) lands as four chars.
///
/// Fix:
/// 1. Quick-reject strings with no chars in the latin-1 high-byte range
///    (U+0080..=U+00FF). Those can't be hiding misinterpreted UTF-8.
/// 2. Re-collect `s.chars()` as latin1 bytes; if any char is ≥ 256 the
///    string contains genuine multi-codepoint UTF-8 mixed in, so leave it
///    alone.
/// 3. Decode those bytes as UTF-8. Accept the decoded result only if it
///    has strictly fewer chars than the input (the fixup collapsed at
///    least one multi-byte sequence into one codepoint). Otherwise return
///    the input untouched.
///
/// The strict "fewer chars" gate prevents false positives: a clean ASCII
/// string with a stray `Â` (someone literally typed it) re-decodes to the
/// same length and is left alone.
pub(crate) fn fix_facebook_mojibake(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| ('\u{0080}'..='\u{00FF}').contains(&c)) {
        return Cow::Borrowed(s);
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let code = ch as u32;
        if code >= 256 {
            return Cow::Borrowed(s);
        }
        bytes.push(code as u8);
    }
    let original_chars = s.chars().count();
    match std::str::from_utf8(&bytes) {
        Ok(decoded) if decoded.chars().count() < original_chars => Cow::Owned(decoded.to_string()),
        _ => Cow::Borrowed(s),
    }
}

/// Wraps bare http(s) URLs in markdown autolink syntax `<...>`. FB exports
/// store URLs as plain text; without the wrap a markdown renderer either
/// silently shows them as text or relies on a non-standard autolink
/// extension. The autolink form is canonical markdown and works in every
/// renderer.
///
/// Sentence punctuation (`.,;!?`) at the tail of a captured URL is always
/// peeled off so "visit https://example.com." becomes "visit
/// <https://example.com>.". Closing brackets (`)` `]`) are peeled only when
/// their tally exceeds the matching opener inside the captured URL, so
/// "(see https://example.com)" trims the trailing `)` while a
/// Wikipedia-style permalink such as
/// "https://en.wikipedia.org/wiki/Foo_(mathematics)" keeps it.
pub(crate) fn wrap_bare_urls(s: &str) -> Cow<'_, str> {
    use std::sync::OnceLock;
    static URL_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re =
        URL_RE.get_or_init(|| regex::Regex::new(r"https?://[^\s<>]+").expect("valid url regex"));
    if !re.is_match(s) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        re.replace_all(s, |caps: &regex::Captures<'_>| {
            let raw = &caps[0];
            let trimmed = trim_url_punctuation(raw);
            let trail = &raw[trimmed.len()..];
            format!("<{trimmed}>{trail}")
        })
        .into_owned(),
    )
}

/// Walks back from the end of `raw` peeling off characters that are
/// almost certainly surrounding-text punctuation rather than URL content.
/// Sentence punctuation is unconditional. A trailing `)` or `]` is peeled
/// only when the remaining URL has more closers than matching openers, so
/// URLs whose path legitimately ends in a bracket (Wikipedia, MDN's
/// `Array.prototype[Symbol.iterator]`, etc.) keep that final character.
fn trim_url_punctuation(raw: &str) -> &str {
    // Pre-count brackets in one pass so the trimming loop stays linear:
    // each peel updates the running counter for the closer it removes
    // instead of re-scanning the prefix.
    let (mut opens_p, mut closes_p, mut opens_b, mut closes_b) = (0usize, 0usize, 0usize, 0usize);
    for c in raw.chars() {
        match c {
            '(' => opens_p += 1,
            ')' => closes_p += 1,
            '[' => opens_b += 1,
            ']' => closes_b += 1,
            _ => {}
        }
    }
    let mut end = raw.len();
    while let Some(last) = raw[..end].chars().next_back() {
        let strip = match last {
            '.' | ',' | ';' | '!' | '?' => true,
            ')' => closes_p > opens_p,
            ']' => closes_b > opens_b,
            _ => false,
        };
        if !strip {
            break;
        }
        match last {
            ')' => closes_p -= 1,
            ']' => closes_b -= 1,
            _ => {}
        }
        end -= last.len_utf8();
    }
    &raw[..end]
}

/// Stable dedup key. FB's internal post ids aren't exposed in the export,
/// so the key is a content hash. The first attachment URI keeps two
/// same-day posts with different media from colliding on identical text
/// bodies.
pub(crate) fn compute_external_id(
    timestamp: i64,
    body: &str,
    first_uri: Option<&str>,
) -> String {
    let mut hasher = Blake2b512::new();
    hasher.update(timestamp.to_le_bytes());
    hasher.update(b"\x00");
    hasher.update(body.as_bytes());
    hasher.update(b"\x00");
    if let Some(uri) = first_uri {
        hasher.update(uri.as_bytes());
    }
    let digest = hasher.finalize();
    // Truncate to 32 hex chars; collision probability remains negligible
    // and keeps the column compact.
    hex::encode(&digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- manifest matchers -----

    #[test]
    fn test_post_manifest_matcher_accepts_real_archive_filename() {
        assert!(is_post_manifest(
            "your_facebook_activity/posts/your_posts__check_ins__photos_and_videos_1.json"
        ));
        assert!(is_post_manifest(
            "your_facebook_activity/posts/your_posts_1.json"
        ));
    }

    #[test]
    fn test_post_manifest_matcher_rejects_siblings() {
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/shared_memories.json"
        ));
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/places_you_have_been_tagged_in.json"
        ));
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/your_uncategorized_photos.json"
        ));
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/your_posts_check_ins.json"
        ));
        assert!(!is_post_manifest("your_posts_1.txt"));
    }

    #[test]
    fn test_album_manifest_matcher() {
        assert!(is_album_manifest(
            "your_facebook_activity/posts/album/0.json"
        ));
        assert!(is_album_manifest(
            "your_facebook_activity/posts/album/profile_pictures.json"
        ));
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/your_posts_1.json"
        ));
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/album/sub/0.json"
        ));
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/album/0.txt"
        ));
    }

    // ----- mojibake fixup -----

    #[test]
    fn test_mojibake_fixup_repairs_known_pattern() {
        let mangled = "got a baby \u{00C2}\u{00AB}hunger\u{00C2}\u{00BB} two weeks ago";
        let fixed = fix_facebook_mojibake(mangled);
        assert_eq!(fixed, "got a baby «hunger» two weeks ago");
    }

    #[test]
    fn test_mojibake_fixup_repairs_three_byte_utf8() {
        let mangled = "you\u{00e2}\u{0080}\u{0099}re here";
        assert_eq!(fix_facebook_mojibake(mangled), "you\u{2019}re here");

        let mangled2 = "say \u{00e2}\u{0080}\u{009c}hi\u{00e2}\u{0080}\u{009d}";
        assert_eq!(fix_facebook_mojibake(mangled2), "say \u{201c}hi\u{201d}");
    }

    #[test]
    fn test_mojibake_fixup_repairs_four_byte_utf8() {
        let mangled = "two \u{00f0}\u{009f}\u{00a6}\u{00a2} watch";
        assert_eq!(fix_facebook_mojibake(mangled), "two \u{1f9a2} watch");
    }

    #[test]
    fn test_mojibake_fixup_leaves_clean_strings_alone() {
        assert_eq!(
            fix_facebook_mojibake("hello world"),
            std::borrow::Cow::Borrowed("hello world")
        );
        let already_clean = "hola — té";
        assert_eq!(fix_facebook_mojibake(already_clean).as_ref(), already_clean);
        let stray_c2 = "stray Â at end";
        assert_eq!(fix_facebook_mojibake(stray_c2).as_ref(), stray_c2);
    }

    // ----- url auto-link -----

    #[test]
    fn test_wrap_bare_urls_basic() {
        let input = "see https://example.com for more";
        assert_eq!(wrap_bare_urls(input), "see <https://example.com> for more");
    }

    #[test]
    fn test_wrap_bare_urls_strips_trailing_punctuation() {
        let input = "visit https://example.com.";
        assert_eq!(wrap_bare_urls(input), "visit <https://example.com>.");
    }

    #[test]
    fn test_wrap_bare_urls_preserves_query_params() {
        let url = "https://twitter.com/u/status/1?ref_src=twsrc%5Etfw&ref_url=foo";
        let input = format!("see {url}");
        let want = format!("see <{url}>");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_leaves_text_without_urls_alone() {
        let input = "no urls here";
        assert_eq!(wrap_bare_urls(input), input);
    }

    #[test]
    fn test_wrap_bare_urls_keeps_balanced_parens_in_url() {
        let url = "https://en.wikipedia.org/wiki/Pi_(letter)";
        let input = format!("see {url}");
        let want = format!("see <{url}>");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_strips_unbalanced_trailing_paren() {
        let input = "(see https://example.com)";
        assert_eq!(wrap_bare_urls(input), "(see <https://example.com>)");
    }

    #[test]
    fn test_wrap_bare_urls_strips_extra_paren_after_balanced_url() {
        let url = "https://en.wikipedia.org/wiki/Pi_(letter)";
        let input = format!("(see {url})");
        let want = format!("(see <{url}>)");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_keeps_balanced_brackets_in_url() {
        let url = "https://example.com/Array.prototype[Symbol.iterator]";
        let input = format!("see {url} now");
        let want = format!("see <{url}> now");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_multiple() {
        let input = "first https://a.example then https://b.example end";
        assert_eq!(
            wrap_bare_urls(input),
            "first <https://a.example> then <https://b.example> end"
        );
    }

    // ----- album normalization (Phase 9d: FacebookAlbum, not FacebookPost) -----

    #[test]
    fn test_album_normalizes_with_description() {
        let raw = RawAlbum {
            name: Some("Phone Pics".to_string()),
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/PhonePics/a.jpg".to_string()),
                creation_timestamp: Some(1262498419),
            }],
            description: Some("Captured on the go.".to_string()),
            last_modified_timestamp: Some(1274792706),
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert_eq!(album.name, "Phone Pics");
        assert_eq!(album.description.as_deref(), Some("Captured on the go."));
        // Earliest creation_timestamp wins over last_modified_timestamp.
        assert_eq!(album.timestamp, 1262498419);
        assert_eq!(album.attachments.len(), 1);
        assert_eq!(album.attachments[0].0, "posts/media/PhonePics/a.jpg");
    }

    #[test]
    fn test_album_normalizes_without_description() {
        let raw = RawAlbum {
            name: Some("Profile pictures".to_string()),
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/p/a.jpg".to_string()),
                creation_timestamp: Some(1256877040),
            }],
            description: None,
            last_modified_timestamp: Some(1732132304),
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert_eq!(album.name, "Profile pictures");
        assert!(album.description.is_none());
    }

    #[test]
    fn test_album_synthesizes_name_when_missing() {
        let raw = RawAlbum {
            name: None,
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/x/a.jpg".to_string()),
                creation_timestamp: Some(123456),
            }],
            description: None,
            last_modified_timestamp: None,
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert!(album.name.starts_with("Untitled album"));
    }

    #[test]
    fn test_album_drops_when_no_photos() {
        let raw = RawAlbum {
            name: Some("Empty".to_string()),
            photos: vec![],
            description: None,
            last_modified_timestamp: Some(123),
        };
        assert!(normalize_album(raw).is_none());
    }

    // ----- regular post normalization -----

    #[test]
    fn test_normalize_post_drops_auto_title_only_entries() {
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry { post: None }, RawDataEntry { post: None }],
            attachments: vec![],
        };
        assert!(normalize_post(raw).is_none());
    }

    #[test]
    fn test_normalize_post_keeps_real_body_text() {
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry {
                post: Some("Hello world".to_string()),
            }],
            attachments: vec![],
        };
        let post = normalize_post(raw).expect("should retain real body");
        assert_eq!(post.body, "Hello world");
    }

    #[test]
    fn test_normalize_post_applies_mojibake_fixup() {
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry {
                post: Some("baby \u{00C2}\u{00AB}hunger\u{00C2}\u{00BB}".to_string()),
            }],
            attachments: vec![],
        };
        let post = normalize_post(raw).expect("should retain");
        assert_eq!(post.body, "baby «hunger»");
    }
}
