import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";
import { useAuth } from "../contexts/AuthContext";
import { LOCALE_NATIVE_NAMES, SUPPORTED_LOCALES } from "../i18n";
import PopoverPicker from "./PopoverPicker";
import useRovingFocus from "../utils/useRovingFocus";

/// Language picker for the social-frontend.
///
/// Two render modes parallel ThemePicker:
/// - `variant="popover"` (default): a globe-icon button in the header that
///   opens a dropdown listing every supported language by its native name.
/// - `variant="inline"`: same list rendered directly without a toggle, for
///   the mobile drawer where vertical space is abundant.
///
/// Switching always writes to localStorage (via i18next-browser-language-
/// detector's `caches`). For authenticated users it additionally fires a
/// fire-and-forget `PATCH /api/me/locale` so the choice follows the user
/// to other devices. Failure of the server-side persist does not undo
/// the local switch; the active session honors the choice immediately
/// and the next login will re-sync from server state.
export default function LanguagePicker({ variant = "popover" }) {
  const { i18n, t } = useTranslation("common");
  const { user } = useAuth();
  const [open, setOpen] = useState(false);
  const popoverRef = useRef(null);
  const inlineRef = useRef(null);

  useRovingFocus(popoverRef, variant === "popover" && open);
  useRovingFocus(inlineRef, variant === "inline", { autoFocus: false });

  // i18n.language can include a region subtag (e.g. "en-US") that we
  // strip when persisting. Compare against the canonical list.
  const active = (i18n.resolvedLanguage || i18n.language || "en").split("-")[0];

  async function pick(lng) {
    if (!SUPPORTED_LOCALES.includes(lng)) return;
    if (lng === active) {
      if (variant === "popover") setOpen(false);
      return;
    }
    await i18n.changeLanguage(lng);
    if (variant === "popover") setOpen(false);

    // Server-side persist: only when authenticated. Fire-and-forget.
    // Failure is silent; the local switch already worked, and a future
    // login will re-sync if the server eventually saves.
    if (user) {
      try {
        await fetchApi("/api/me/locale", {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ locale: lng }),
        });
      } catch {
        // ignore; local switch stands
      }
    }
  }

  const list = (
    <div className="language-picker-list" role="menu" aria-label={t("language.menuAria")}>
      <div className="language-picker-title">{t("language.title")}</div>
      {SUPPORTED_LOCALES.map((lng) => {
        const isActive = lng === active;
        return (
          <button
            key={lng}
            type="button"
            role="menuitemradio"
            aria-checked={isActive}
            className={`language-picker-item ${isActive ? "active" : ""}`}
            onClick={() => pick(lng)}
          >
            <span className="language-picker-name">
              {LOCALE_NATIVE_NAMES[lng]}
            </span>
            {isActive && (
              <span className="language-picker-check" aria-hidden="true">
                ✓
              </span>
            )}
          </button>
        );
      })}
    </div>
  );

  return (
    <PopoverPicker
      variant={variant}
      open={open}
      onOpenChange={setOpen}
      className="language-picker"
      toggleAriaLabel={t("language.toggleAria")}
      popoverAriaLabel={t("language.title")}
      popoverRef={popoverRef}
      inlineRef={inlineRef}
      toggleIcon={
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="10" />
          <path d="M2 12h20" />
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
        </svg>
      }
    >
      {list}
    </PopoverPicker>
  );
}
