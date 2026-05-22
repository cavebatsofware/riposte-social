import i18n from "i18next";
import HttpBackend from "i18next-http-backend";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";

/// Supported launch locales. Adding a sixth language is a JSON-only PR:
/// drop a new directory under `public/locales/<code>/` mirroring the en
/// catalogs, then add the code here so the LanguagePicker surfaces it.
/// Keep this list in sync with `LOCALE_NATIVE_NAMES` below and the
/// backend allowlist in `src/profile/locale.rs`.
export const SUPPORTED_LOCALES = ["en", "es", "fr", "zh", "de"];

/// Native-script display names for the LanguagePicker. Using the
/// language's own name (rather than translating it) is the standard
/// convention  a user who reads German is far more likely to recognize
/// "Deutsch" than the localized translation of "German" in whatever
/// language they're currently reading.
export const LOCALE_NATIVE_NAMES = {
  en: "English",
  es: "Español",
  fr: "Français",
  zh: "中文",
  de: "Deutsch",
};

/// Namespaces map roughly to the page/feature surfaces. Each namespace
/// is a separate JSON file under `public/locales/<lng>/<ns>.json`. The
/// HTTP backend lazy-loads them on first reference, so a visitor who
/// never opens Settings doesn't pay for the settings catalog.
///
/// `common` is loaded eagerly via `defaultNS` because Layout, ThemePicker,
/// LanguagePicker, MobileDrawer, and the cookie banner all read from it
/// and they're on every route.
export const NAMESPACES = [
  "common",
  "feed",
  "compose",
  "settings",
  "auth",
  "browse",
  "articles",
];

i18n
  .use(HttpBackend)
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    fallbackLng: "en",
    supportedLngs: SUPPORTED_LOCALES,
    // Strip region subtags so `en-US`, `zh-CN`, `zh-TW`, etc. all resolve
    // to a base code we actually have a catalog for. Users on
    // `zh-Hant` get our `zh` (Simplified) until a Traditional catalog
    // ships; preferable to a missing-key fallback to English.
    load: "languageOnly",
    ns: NAMESPACES,
    defaultNS: "common",
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "rs_locale_v1",
      caches: ["localStorage"],
    },
    backend: {
      loadPath: "/locales/{{lng}}/{{ns}}.json",
    },
    interpolation: {
      // React already escapes interpolated values in JSX.
      escapeValue: false,
    },
    react: {
      // Suspense lets us render a placeholder while a namespace fetches.
      // The wrapper in `main.jsx` is responsible for the fallback markup.
      useSuspense: true,
    },
    // Don't fall back to a key when a translation is missing  return
    // empty so we can spot gaps visually rather than rendering raw
    // dot-paths in the UI. The catalog-completeness CI gate keeps
    // production catalogs in sync; this is dev-time defense.
    returnEmptyString: false,
  });

// Keep `<html lang>` in sync with the active locale so the browser's
// font shaping, hyphenation, and screen-reader pronunciation pick the
// right script. The default `lang="en"` in index.html is the bootstrap
// value; this updates it on init and on every `changeLanguage` call.
function syncDocumentLang(lng) {
  if (typeof document === "undefined") return;
  const base = (lng || "en").split("-")[0];
  document.documentElement.lang = SUPPORTED_LOCALES.includes(base) ? base : "en";
}
i18n.on("languageChanged", syncDocumentLang);
syncDocumentLang(i18n.resolvedLanguage || i18n.language);

export default i18n;
