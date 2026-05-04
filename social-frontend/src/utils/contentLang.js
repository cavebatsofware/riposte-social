/// Map a UI locale code (en/es/fr/zh/de) to the Postgres FTS configuration
/// the backend uses to tokenize post bodies. The five values here mirror
/// `src/posts/mod.rs::fts_config_for_locale` and the entity allowlist —
/// keep both in sync when adding a new language.
const LOCALE_TO_FTS = {
  en: "english",
  es: "spanish",
  fr: "french",
  zh: "chinese",
  de: "german",
};

export function ftsConfigForLocale(locale) {
  if (!locale) return "english";
  const base = locale.split("-")[0];
  return LOCALE_TO_FTS[base] || "english";
}

export const SUPPORTED_CONTENT_LANGS = Object.values(LOCALE_TO_FTS);
