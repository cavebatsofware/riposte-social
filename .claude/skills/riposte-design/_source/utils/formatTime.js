/// Locale-aware relative-time formatter shared by PostCard and
/// CommentThread (and any future card-style component).
///
/// Replaces the duplicated inline `formatRelativeTime` that hardcoded
/// English plurals ("X minutes ago"). `Intl.RelativeTimeFormat` with
/// `numeric: "auto"` handles both the plural agreement and the
/// special-case strings ("yesterday", "ayer", "hier") without any
/// per-language branching here. CLDR data ships with the runtime, so a
/// new locale is supported the moment it's added to `SUPPORTED_LOCALES`
/// in `src/i18n.js`  no per-language code paths.
///
/// Bucket boundaries match the previous hand-rolled implementation:
///  - < 60 s         → "now" / "ahora" / "maintenant" / "现在" / "jetzt"
///  - < 1 hour       → minute-granularity
///  - < 1 day        → hour-granularity
///  - < 1 week       → day-granularity
///  - >= 1 week      → absolute date in locale-default short format
export function formatRelativeTime(iso, locale) {
  if (!iso) return "";
  const d = new Date(iso);
  const now = new Date();
  const diffSec = Math.floor((now - d) / 1000);
  const lng = locale || "en";
  const rtf = new Intl.RelativeTimeFormat(lng, { numeric: "auto" });

  if (diffSec < 60) {
    // "0 seconds ago" reads worse than the present-tense form. Use the
    // RTF special-case for the smallest bucket so each locale gets its
    // natural "now" word.
    return rtf.format(0, "second");
  }
  if (diffSec < 3600) {
    return rtf.format(-Math.floor(diffSec / 60), "minute");
  }
  if (diffSec < 86400) {
    return rtf.format(-Math.floor(diffSec / 3600), "hour");
  }
  if (diffSec < 7 * 86400) {
    return rtf.format(-Math.floor(diffSec / 86400), "day");
  }
  // Past a week, the relative phrasing ("3 weeks ago") starts to feel
  // less precise than a date for archival content. Keep parity with
  // the previous implementation: drop to a locale-default short date.
  return new Intl.DateTimeFormat(lng).format(d);
}

/// Convenience wrapper around Intl.DateTimeFormat for cases that need
/// an absolute date/time regardless of recency. Callers pass through
/// the standard `options` object.
export function formatAbsolute(iso, locale, options) {
  if (!iso) return "";
  const lng = locale || "en";
  return new Intl.DateTimeFormat(lng, options).format(new Date(iso));
}
