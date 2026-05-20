import { useState } from "react";
import { useTranslation } from "react-i18next";

const STORAGE_KEY = "rs_cookie_ack_v1";

/// Lightweight cookie disclosure banner. Riposte Social only uses cookies
/// that are strictly necessary for authentication, CSRF protection, and
/// (for invited users) carrying invite state across sign-in. There are no
/// tracking, analytics, or advertising cookies.
///
/// Acknowledgment is persisted in localStorage so the banner shows once
/// per device. The banner does not gate the site itself; the strictly-
/// necessary cookies still flow whether or not the visitor clicks OK,
/// per ePrivacy guidance for essential cookies. Tracking-grade cookies
/// (none yet) would need an opt-in here.
export default function CookieBanner() {
  // Resolve from localStorage during initial render so the banner state
  // is correct on first paint and never flashes in after mount.
  const [acknowledged, setAcknowledged] = useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) === "1";
    } catch {
      // Browsers in private mode without storage access. Don't block.
      return true;
    }
  });
  const { t } = useTranslation("common");

  function handleAck() {
    try {
      localStorage.setItem(STORAGE_KEY, "1");
    } catch {
      // ignore; banner just won't persist
    }
    setAcknowledged(true);
  }

  if (acknowledged) {
    return null;
  }

  return (
    <section className="cookie-banner" aria-label={t("cookieBanner.regionLabel")}>
      <p>{t("cookieBanner.text")}</p>
      <button type="button" className="btn-primary" onClick={handleAck}>
        {t("cookieBanner.ack")}
      </button>
    </section>
  );
}
