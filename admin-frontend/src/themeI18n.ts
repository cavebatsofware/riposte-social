/**
 * Isolated i18next instance for the design-system pickers ONLY. The admin app's
 * own UI strings are hardcoded English (no react-i18next), but the shared
 * ThemePicker speaks react-i18next, so we spin up a dedicated instance carrying
 * just the bundled `theme.*` strings (merged into the `common` namespace the
 * picker expects) and hand it to the tree via I18nextProvider. Keeping it
 * separate means react-i18next never owns the admin's strings.
 */
import i18next, { type i18n as I18nInstance } from "i18next";
import { initReactI18next } from "react-i18next";
import { themeResources } from "@cavebatsofware/riposte-pickers/i18n";

// themeResources is { [lng]: { theme: {...} } }; nest each under the `common` ns.
const resources = Object.fromEntries(
  Object.entries(themeResources).map(([lng, res]) => [lng, { common: res }]),
);

const themeI18n: I18nInstance = i18next.createInstance();
themeI18n.use(initReactI18next).init({
  lng: "en",
  fallbackLng: "en",
  defaultNS: "common",
  ns: ["common"],
  resources,
  interpolation: { escapeValue: false },
  react: { useSuspense: false },
});

export default themeI18n;
